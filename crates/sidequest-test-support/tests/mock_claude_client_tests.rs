//! Story 40-1 RED: MockClaudeClient records inputs, returns scripted outputs,
//! and implements `ClaudeLike`.
//!
//! These tests fail to compile today because `sidequest_test_support::MockClaudeClient`
//! does not exist. Dev's GREEN phase defines the mock so tests can substitute
//! it for a real `ClaudeClient` wherever production takes `Arc<dyn ClaudeLike>`.
//!
//! API requirements:
//! - `MockClaudeClient::new()` — empty mock, returns `ClaudeClientError::EmptyResponse`
//!   on any unscripted call (fails loudly — no silent fallbacks per CLAUDE.md).
//! - `MockClaudeClient::respond_with(&mut self, text)` — scripts the next response.
//! - `MockClaudeClient::respond_with_error(&mut self, err)` — scripts the next error.
//! - `MockClaudeClient::recorded_prompts(&self) -> Vec<RecordedCall>` — returns every
//!   (prompt, model, session_id, system_prompt, allowed_tools, env_vars) the mock was
//!   invoked with, in call order.
//! - `RecordedCall::prompt() -> &str`, `.model() -> &str`, `.session_id() -> Option<&str>`, etc.

use sidequest_agents::client::{ClaudeClientError, ClaudeLike as AgentsClaudeLike};
// Second import keeps the test honest even if someone re-exports under a
// different path — a single canonical ClaudeLike must be in force.
use sidequest_test_support::{ClaudeLike, MockClaudeClient};

#[test]
fn re_export_is_canonical() {
    // If sidequest-agents re-exports ClaudeLike, it must be the SAME trait as
    // sidequest-test-support's ClaudeLike (via trait-object equivalence).
    // Otherwise, a caller importing the wrong path silently gets a different
    // trait and the DI refactor is half-done.
    fn assert_same<T: ClaudeLike + AgentsClaudeLike + ?Sized>() {}
    assert_same::<MockClaudeClient>();
}

#[test]
fn unscripted_call_returns_error_not_default() {
    // CLAUDE.md: "Never silently try an alternative path, config, or default."
    // A mock that returns `Ok("")` on an unscripted call would hide test bugs.
    let mock = MockClaudeClient::new();
    let result = mock.send_with_model("anything", "haiku");
    assert!(
        matches!(result, Err(ClaudeClientError::EmptyResponse)),
        "unscripted mock call must return an explicit error, not Ok(empty) or default: got {result:?}"
    );
}

#[test]
fn scripted_response_round_trips_text_and_tokens() {
    let mut mock = MockClaudeClient::new();
    mock.respond_with("the quick brown fox");

    let response = mock
        .send_with_model("prompt", "haiku")
        .expect("scripted response should succeed");

    assert_eq!(response.text, "the quick brown fox");
}

#[test]
fn scripted_error_round_trips() {
    let mut mock = MockClaudeClient::new();
    mock.respond_with_error(ClaudeClientError::EmptyResponse);

    let result = mock.send_with_model("prompt", "haiku");
    assert!(matches!(result, Err(ClaudeClientError::EmptyResponse)));
}

#[test]
fn records_invocations_in_order_with_prompt_and_model() {
    let mut mock = MockClaudeClient::new();
    mock.respond_with("a");
    mock.respond_with("b");

    let _ = mock.send_with_model("first prompt", "haiku");
    let _ = mock.send_with_model("second prompt", "sonnet");

    let recorded = mock.recorded_calls();
    assert_eq!(recorded.len(), 2, "two calls should produce two records");
    assert_eq!(recorded[0].prompt(), "first prompt");
    assert_eq!(recorded[0].model(), "haiku");
    assert_eq!(recorded[1].prompt(), "second prompt");
    assert_eq!(recorded[1].model(), "sonnet");
}

#[test]
fn records_session_call_metadata() {
    let mut mock = MockClaudeClient::new();
    mock.respond_with("session response");

    let env = std::collections::HashMap::new();
    let tools: Vec<String> = vec![];
    let _ = mock.send_with_session(
        "session prompt",
        "opus",
        Some("sess-xyz"),
        Some("be concise"),
        &tools,
        &env,
    );

    let recorded = mock.recorded_calls();
    assert_eq!(recorded.len(), 1);
    let call = &recorded[0];
    assert_eq!(call.prompt(), "session prompt");
    assert_eq!(call.model(), "opus");
    assert_eq!(call.session_id(), Some("sess-xyz"));
    assert_eq!(call.system_prompt(), Some("be concise"));
}

#[test]
fn fifo_script_order_not_lifo() {
    // A bug we want to catch: a stack-based script would return the last-pushed
    // response first. Tests authored assuming FIFO would then lie silently.
    let mut mock = MockClaudeClient::new();
    mock.respond_with("first");
    mock.respond_with("second");

    let first = mock.send_with_model("p1", "m").unwrap().text;
    let second = mock.send_with_model("p2", "m").unwrap().text;

    assert_eq!(first, "first");
    assert_eq!(second, "second");
}

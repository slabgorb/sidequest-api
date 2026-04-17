//! Story 40-1: pins the `MockClaudeClient` scripted-response and call-recording
//! contract, plus the canonical `ClaudeLike` re-export path.
//!
//! The mock is the test-side half of Epic 40's DI pattern: production sites
//! take `Arc<dyn ClaudeLike>`, tests substitute a `MockClaudeClient` that
//! returns scripted responses and records every invocation for post-hoc
//! assertion.
//!
//! API contract pinned by these tests:
//! - `MockClaudeClient::new()` — empty mock. Unscripted calls return
//!   `ClaudeClientError::EmptyResponse` (fails loudly — no silent fallbacks per
//!   CLAUDE.md).
//! - `MockClaudeClient::respond_with(&mut self, text)` — queues the next `Ok`
//!   response; FIFO order.
//! - `MockClaudeClient::respond_with_error(&mut self, err)` — queues the next
//!   `Err`.
//! - `MockClaudeClient::recorded_calls(&self) -> Vec<RecordedCall>` — returns
//!   every invocation (prompt, model, session_id, system_prompt, allowed_tools,
//!   env_vars) in call order. Available on `&self` so the mock can be queried
//!   through `Arc<dyn ClaudeLike>`.
//! - `RecordedCall::prompt() -> &str`, `.model() -> &str`,
//!   `.session_id() -> Option<&str>`, `.system_prompt() -> Option<&str>`,
//!   `.allowed_tools() -> &[String]`, `.env_vars() -> &HashMap<String, String>`.

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
    assert_eq!(
        result,
        Err(ClaudeClientError::EmptyResponse),
        "unscripted mock call must return an explicit error, not Ok(empty) or default"
    );
}

#[test]
fn scripted_response_round_trips_text() {
    // Tokens are not covered here — `respond_with(text)` hardcodes
    // `input_tokens: None` and `output_tokens: None` on the scripted
    // `ClaudeResponse`. When token-accounting assertions are needed, add a
    // `respond_with_tokens(text, input, output)` builder on `MockClaudeClient`
    // and a dedicated test — do not let the name of this test mislead future
    // authors into thinking tokens are pinned here.
    let mut mock = MockClaudeClient::new();
    mock.respond_with("the quick brown fox");

    let response = mock
        .send_with_model("prompt", "haiku")
        .expect("scripted response should succeed");

    assert_eq!(response.text, "the quick brown fox");
    assert_eq!(
        response.input_tokens, None,
        "respond_with() must default input_tokens to None; change both this assertion \
         and the mock's API when a respond_with_tokens variant is added"
    );
    assert_eq!(
        response.output_tokens, None,
        "respond_with() must default output_tokens to None; change both this assertion \
         and the mock's API when a respond_with_tokens variant is added"
    );
}

#[test]
fn scripted_error_round_trips() {
    let mut mock = MockClaudeClient::new();
    mock.respond_with_error(ClaudeClientError::EmptyResponse);

    let result = mock.send_with_model("prompt", "haiku");
    assert_eq!(result, Err(ClaudeClientError::EmptyResponse));
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
    // Empty tools and env must still round-trip as empty (not None, not missing).
    // A bug that silently dropped the empty slices would not be caught by the
    // other assertions, so pin them explicitly.
    assert!(
        call.allowed_tools().is_empty(),
        "empty allowed_tools must round-trip as an empty slice"
    );
    assert!(
        call.env_vars().is_empty(),
        "empty env_vars must round-trip as an empty map"
    );
}

#[test]
fn records_session_call_with_non_empty_tools_and_env() {
    // Companion to records_session_call_metadata: the empty-case test above
    // cannot detect a bug where a non-empty slice silently becomes empty
    // during the to_vec() / clone() inside the mock's record() path.
    let mut mock = MockClaudeClient::new();
    mock.respond_with("session response");

    let mut env = std::collections::HashMap::new();
    env.insert(
        "SIDEQUEST_GENRE".to_string(),
        "caverns_and_claudes".to_string(),
    );
    env.insert(
        "SIDEQUEST_CONTENT_PATH".to_string(),
        "/tmp/content".to_string(),
    );
    let tools = vec!["Read".to_string(), "Bash".to_string()];

    let _ = mock.send_with_session(
        "session prompt with tools",
        "opus",
        Some("sess-with-tools"),
        Some("tool-aware system prompt"),
        &tools,
        &env,
    );

    let recorded = mock.recorded_calls();
    assert_eq!(recorded.len(), 1);
    let call = &recorded[0];
    assert_eq!(
        call.allowed_tools(),
        &["Read".to_string(), "Bash".to_string()],
        "allowed_tools must round-trip in order and without loss"
    );
    assert_eq!(call.env_vars().len(), 2);
    assert_eq!(
        call.env_vars().get("SIDEQUEST_GENRE").map(String::as_str),
        Some("caverns_and_claudes")
    );
    assert_eq!(
        call.env_vars()
            .get("SIDEQUEST_CONTENT_PATH")
            .map(String::as_str),
        Some("/tmp/content")
    );
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

//! Story 40-1: pins the wire-first gate — at least one production call site
//! accepts `Arc<dyn ClaudeLike>`, and the mock actually sees the call.
//!
//! CLAUDE.md: "a trait with zero non-test consumers is a skeleton, and
//! skeletons violate wire-first discipline." These tests prove the
//! sidequest-agents preprocessor is a real non-test consumer, and that error
//! paths through the DI boundary round-trip cleanly.
//!
//! # Why `preprocessor::preprocess_action_with_client`?
//!
//! - Its sole LLM dependency is a single `send_with_model` call, which keeps
//!   the wiring test focused on the trait-object boundary rather than the
//!   surrounding business logic.
//! - `preprocess_action` (the original entry point) delegates to
//!   `preprocess_action_with_client` with a real `ClaudeClient` wrapped in
//!   `Arc`, so the migration didn't require visibility changes anywhere else.
//! - The haiku-tier call is deterministic enough that a `MockClaudeClient`
//!   can return a single scripted JSON string and the function succeeds end
//!   to end.
//!
//! Stories 40-2 through 40-6 migrate the remaining seven call sites
//! (catch_up.rs, server/lib.rs create_claude_client, orchestrator, inventory
//! extractor, continuity validator, resonator×4) onto the same pattern.

use std::sync::Arc;

use sidequest_agents::client::ClaudeClientError;
use sidequest_agents::preprocessor::PreprocessError;
use sidequest_test_support::{ClaudeLike, MockClaudeClient};

/// Expected Dev-facing API: a `preprocess_action_with_client` function that
/// takes the client as a trait object. If Dev renames this, update the
/// import and the assertion — the shape of the test remains: "a production
/// function accepts `Arc<dyn ClaudeLike>`."
///
/// The prompt format is defined in `sidequest_agents::preprocessor::build_prompt`
/// and expects JSON output with `you`, `named`, `intent` fields.
#[test]
fn preprocessor_accepts_arc_dyn_claude_like() {
    let mut mock = MockClaudeClient::new();
    // Set every field, including non-default booleans, so this test pins the
    // whole `PreprocessedAction` round-trip — not just the three string fields.
    // A future change that makes the booleans required (removes
    // `#[serde(default)]`) must not regress this path silently.
    mock.respond_with(
        r#"{
            "you":"you draw your sword",
            "named":"Rux draws his sword",
            "intent":"draw weapon",
            "is_power_grab":false,
            "references_inventory":true,
            "references_npc":false,
            "references_ability":true,
            "references_location":false
        }"#,
    );

    let client: Arc<dyn ClaudeLike> = Arc::new(mock);

    let result = sidequest_agents::preprocessor::preprocess_action_with_client(
        client,
        "i, uh, draw my sword",
        "Rux",
    );

    assert!(
        result.is_ok(),
        "preprocessor must succeed when mock returns a valid JSON response: {result:?}"
    );
    let action = result.unwrap();
    assert_eq!(action.you, "you draw your sword");
    assert_eq!(action.named, "Rux draws his sword");
    assert_eq!(action.intent, "draw weapon");
    assert!(
        action.references_inventory,
        "references_inventory must round-trip through the preprocessor"
    );
    assert!(
        action.references_ability,
        "references_ability must round-trip through the preprocessor"
    );
    assert!(
        !action.is_power_grab,
        "is_power_grab must round-trip as false"
    );
    assert!(
        !action.references_npc,
        "references_npc must round-trip as false"
    );
    assert!(
        !action.references_location,
        "references_location must round-trip as false"
    );
}

/// A scripted `Err` from the injected client must propagate through the
/// preprocessor as `PreprocessError::LlmFailed(_)` — Epic 40's whole premise
/// is that error paths through the DI boundary are observable and pinned.
/// A refactor that swallowed the error (or converted it to `Ok("")`) would
/// otherwise be undetectable by the test suite.
#[test]
fn preprocessor_propagates_client_error_as_llm_failed() {
    let mut mock = MockClaudeClient::new();
    mock.respond_with_error(ClaudeClientError::EmptyResponse);
    let client: Arc<dyn ClaudeLike> = Arc::new(mock);

    let result = sidequest_agents::preprocessor::preprocess_action_with_client(
        client,
        "some player input",
        "Rux",
    );

    assert!(
        matches!(result, Err(PreprocessError::LlmFailed(_))),
        "a scripted client error must propagate as PreprocessError::LlmFailed, got {result:?}"
    );
}

/// Same propagation discipline for `ClaudeClientError::Timeout` — a timeout
/// from the trait object must not be silently converted to any other error
/// variant or to Ok.
#[test]
fn preprocessor_propagates_client_timeout_as_llm_failed() {
    let mut mock = MockClaudeClient::new();
    mock.respond_with_error(ClaudeClientError::Timeout {
        elapsed: std::time::Duration::from_secs(30),
    });
    let client: Arc<dyn ClaudeLike> = Arc::new(mock);

    let result = sidequest_agents::preprocessor::preprocess_action_with_client(
        client,
        "another input",
        "Rux",
    );

    assert!(
        matches!(result, Err(PreprocessError::LlmFailed(_))),
        "a scripted timeout must propagate as PreprocessError::LlmFailed, got {result:?}"
    );
    // The message must carry the upstream detail so GM-panel debugging can see it.
    match result {
        Err(PreprocessError::LlmFailed(msg)) => {
            assert!(
                msg.to_lowercase().contains("timed out") || msg.to_lowercase().contains("timeout"),
                "LlmFailed message should carry timeout information; got: {msg}"
            );
        }
        other => panic!("expected LlmFailed, got {other:?}"),
    }
}

#[test]
fn preprocessor_records_prompt_through_mock() {
    // This test proves the DI is real — the mock saw the call, which means the
    // function did not silently construct its own ClaudeClient internally.
    // A production site that accepts the trait but ignores it would pass
    // `preprocessor_accepts_arc_dyn_claude_like` while failing this one.
    let mut mock = MockClaudeClient::new();
    mock.respond_with(r#"{"you":"you look around","named":"Rux looks around","intent":"observe"}"#);
    // We need to clone the Arc so we can query the mock after the call — but
    // the mock is owned by the Arc. The test API exposes a query handle so
    // that after the call, we can ask "what did the production code send?".
    let mock_arc = Arc::new(mock);
    let client: Arc<dyn ClaudeLike> = mock_arc.clone();

    let _ = sidequest_agents::preprocessor::preprocess_action_with_client(
        client,
        "i look around, you know?",
        "Rux",
    );

    let recorded = mock_arc.recorded_calls();
    assert_eq!(
        recorded.len(),
        1,
        "preprocessor must invoke the injected client exactly once"
    );
    assert_eq!(
        recorded[0].model(),
        "haiku",
        "preprocessor must use haiku tier (HAIKU_MODEL constant in preprocessor.rs)"
    );
    // Pin the exact template slots — not just any occurrence of "Rux" (which
    // also appears in the JSON example line `{char_name} draws their sword`)
    // or "i look around" (which doesn't appear elsewhere but would if the
    // template grew more examples). Epic 40's whole mission is eliminating
    // loose substring matching; this foundation test must not regress that.
    let prompt = recorded[0].prompt();
    assert!(
        prompt.contains("Character name: Rux"),
        "prompt must inject char_name into the `Character name:` slot; got: {prompt}"
    );
    assert!(
        prompt.contains("Player input: \"i look around, you know?\""),
        "prompt must inject raw_input into the `Player input:` slot verbatim; got: {prompt}"
    );
}

/// A non-JSON scripted response must propagate as
/// `PreprocessError::ParseFailed(response)` — the malformed text is carried
/// through for debugging, and no variant is silently swallowed.
#[test]
fn preprocessor_propagates_parse_failure() {
    let mut mock = MockClaudeClient::new();
    mock.respond_with("not json at all");
    let client: Arc<dyn ClaudeLike> = Arc::new(mock);

    let result =
        sidequest_agents::preprocessor::preprocess_action_with_client(client, "some input", "Rux");

    match result {
        Err(PreprocessError::ParseFailed(text)) => {
            assert_eq!(
                text, "not json at all",
                "ParseFailed must carry the malformed response text verbatim so GM-panel \
                 debugging can see what Haiku actually returned"
            );
        }
        other => panic!("expected Err(PreprocessError::ParseFailed(_)), got {other:?}"),
    }
}

/// A scripted JSON response whose rewrite fields exceed 2x the raw input
/// length must propagate as `PreprocessError::OutputTooLong { .. }` — a
/// hallucinated or runaway expansion must not reach the dispatch layer as
/// `Ok(PreprocessedAction)`. The struct fields must carry actual lengths so
/// the GM panel can see which field blew the budget.
#[test]
fn preprocessor_propagates_output_too_long() {
    // raw = 6 chars; 2x budget = 12 chars. "you" field at 54 chars must
    // exceed the budget.
    let raw = "i draw";
    let long_you = "you draw your sword and swing it wildly for a long time";
    assert!(
        long_you.len() > raw.len() * 2,
        "test data must exceed 2x budget"
    );

    let json = format!(r#"{{"you":"{long_you}","named":"Rux","intent":"x"}}"#);
    let mut mock = MockClaudeClient::new();
    mock.respond_with(json);
    let client: Arc<dyn ClaudeLike> = Arc::new(mock);

    let result = sidequest_agents::preprocessor::preprocess_action_with_client(client, raw, "Rux");

    match result {
        Err(PreprocessError::OutputTooLong {
            raw_len,
            you_len,
            named_len,
            intent_len,
        }) => {
            assert_eq!(raw_len, raw.len(), "raw_len must equal input length");
            assert_eq!(
                you_len,
                long_you.len(),
                "you_len must equal the over-budget field length"
            );
            assert_eq!(
                named_len, 3,
                "named_len must equal the actual response field length"
            );
            assert_eq!(
                intent_len, 1,
                "intent_len must equal the actual response field length"
            );
        }
        other => panic!("expected Err(PreprocessError::OutputTooLong {{ .. }}), got {other:?}"),
    }
}

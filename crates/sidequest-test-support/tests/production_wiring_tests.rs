//! Story 40-1 RED: at least one production call site accepts `Arc<dyn ClaudeLike>`.
//!
//! This test fails to compile today because no production site exposes the trait
//! form. Dev's GREEN phase refactors the simplest pub entry point
//! (`sidequest_agents::preprocessor`) to take `Arc<dyn ClaudeLike>`, proving the
//! DI pattern works end-to-end.
//!
//! This is the "wiring-first" gate: a trait with zero non-test consumers is a
//! skeleton, and skeletons violate CLAUDE.md. The test below is the single
//! non-test consumer required by story 40-1. Subsequent stories (40-2 in
//! particular) migrate the remaining seven call sites.
//!
//! # Why preprocessor?
//!
//! - Its sole dependency on the LLM is a single `send_with_model` call.
//! - `preprocess_action` is already `pub`, so an analogous
//!   `preprocess_action_with_client` can be exposed without changing the
//!   visibility policy of other modules.
//! - The haiku-tier call is deterministic enough that a MockClaudeClient can
//!   return a single scripted JSON string and the function succeeds.
//!
//! Dev may choose a different production site if that proves cleaner; the
//! assertion shape stays the same — the point is that some function takes
//! `Arc<dyn ClaudeLike>`.

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
    assert!(!action.is_power_grab, "is_power_grab must round-trip as false");
    assert!(!action.references_npc, "references_npc must round-trip as false");
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
        "preprocessor must use haiku tier per PREPROCESS_TIMEOUT comment"
    );
    assert!(
        recorded[0].prompt().contains("Rux"),
        "the prompt must carry the character name through to the injected client"
    );
    assert!(
        recorded[0].prompt().contains("i look around"),
        "the prompt must carry the raw input through to the injected client"
    );
}

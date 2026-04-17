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
    mock.respond_with(
        r#"{"you":"you draw your sword","named":"Rux draws his sword","intent":"draw weapon"}"#,
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
}

#[test]
fn preprocessor_records_prompt_through_mock() {
    // This test proves the DI is real — the mock saw the call, which means the
    // function did not silently construct its own ClaudeClient internally.
    // A production site that accepts the trait but ignores it would pass
    // `preprocessor_accepts_arc_dyn_claude_like` while failing this one.
    let mut mock = MockClaudeClient::new();
    mock.respond_with(
        r#"{"you":"you look around","named":"Rux looks around","intent":"observe"}"#,
    );
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

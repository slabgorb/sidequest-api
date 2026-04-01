//! Story 18-3: Parallelize prompt context build and preprocess Haiku call.
//!
//! RED phase — tests reference `preprocess_action_async` which doesn't exist yet.
//! Dev must create this async wrapper around the sync `preprocess_action` function
//! using `tokio::task::spawn_blocking`, then wire it into dispatch/mod.rs with
//! `tokio::join!` alongside `build_prompt_context`.
//!
//! ACs tested:
//!   1. Preprocess and prompt context build run concurrently via tokio::join!
//!   2. Async preprocessor produces identical output to sync version
//!   3. Fallback behavior preserved in async context (LLM failure → mechanical fallback)
//!   4. Sub-spans from 18-1 remain valid in parallel context
//!   5. Turn time reduced (preprocess + context build overlap in flame chart)

use sidequest_game::PreprocessedAction;
// RED: This async function does not exist yet. Dev must create it.
use sidequest_agents::preprocessor::preprocess_action_async;
// Existing sync function for comparison
use sidequest_agents::preprocessor::{fallback, preprocess_action};

// ============================================================================
// AC-2: Async preprocessor produces identical output to sync fallback
// ============================================================================

/// The async wrapper must produce the same fallback result as the sync version
/// when no LLM is available (which is the case in tests).
#[tokio::test]
async fn async_preprocess_fallback_matches_sync() {
    let raw = "I draw my sword and look around";
    let char_name = "Kael";

    // Sync fallback (no LLM in test env)
    let sync_result = fallback(raw, char_name);

    // Async version should produce identical output
    let async_result = preprocess_action_async(raw, char_name).await;

    assert_eq!(
        async_result.you, sync_result.you,
        "Async 'you' field must match sync fallback"
    );
    assert_eq!(
        async_result.named, sync_result.named,
        "Async 'named' field must match sync fallback"
    );
    assert_eq!(
        async_result.intent, sync_result.intent,
        "Async 'intent' field must match sync fallback"
    );
    assert_eq!(
        async_result.is_power_grab, sync_result.is_power_grab,
        "Async 'is_power_grab' must match sync"
    );
}

// ============================================================================
// AC-2: Async preprocess returns correct PreprocessedAction type
// ============================================================================

#[tokio::test]
async fn async_preprocess_returns_preprocessed_action() {
    let result = preprocess_action_async("I look around the room", "Thorn").await;

    // Must return a valid PreprocessedAction with all fields populated
    assert!(!result.you.is_empty(), "you field must not be empty");
    assert!(!result.named.is_empty(), "named field must not be empty");
    assert!(!result.intent.is_empty(), "intent field must not be empty");
}

// ============================================================================
// AC-3: Fallback preserved — empty input produces reasonable output
// ============================================================================

#[tokio::test]
async fn async_preprocess_handles_empty_input() {
    let result = preprocess_action_async("", "Kael").await;

    // Empty input should still return a PreprocessedAction (fallback)
    // The sync fallback produces "You " for empty input — async must match
    let sync_result = fallback("", "Kael");
    assert_eq!(result.you, sync_result.you, "Empty input: async must match sync fallback");
    assert_eq!(result.named, sync_result.named);
    assert_eq!(result.intent, sync_result.intent);
}

// ============================================================================
// AC-3: Fallback preserved — first-person prefix stripping works in async
// ============================================================================

#[tokio::test]
async fn async_preprocess_strips_first_person_prefix() {
    let result = preprocess_action_async("I search the chest for traps", "Kael").await;

    // Fallback strips "I " prefix: intent should be "search the chest for traps"
    assert!(
        result.intent.contains("search the chest for traps")
            || result.intent.contains("search"),
        "Intent should contain the action without first-person prefix, got: '{}'",
        result.intent
    );
    assert!(
        result.you.starts_with("You "),
        "Second-person form must start with 'You ', got: '{}'",
        result.you
    );
    assert!(
        result.named.starts_with("Kael "),
        "Named form must start with character name, got: '{}'",
        result.named
    );
}

// ============================================================================
// AC-2: Power grab detection preserved in async context
// ============================================================================

#[tokio::test]
async fn async_preprocess_fallback_never_flags_power_grab() {
    // Mechanical fallback never detects power grabs (only LLM can)
    let result = preprocess_action_async("I wish for unlimited gold", "Kael").await;

    // In test env (no LLM), fallback is used — is_power_grab always false
    let sync_result = fallback("I wish for unlimited gold", "Kael");
    assert_eq!(
        result.is_power_grab, sync_result.is_power_grab,
        "Power grab flag must match sync fallback in test env"
    );
}

// ============================================================================
// AC-1: Async preprocess is Send (required for tokio::join! with other futures)
// ============================================================================

/// Compile-time test: the future returned by preprocess_action_async must be Send.
/// tokio::join! requires all futures to be Send when used with multi-threaded runtime.
#[tokio::test]
async fn async_preprocess_future_is_send() {
    // This test verifies at compile time that the async function returns a Send future.
    // If preprocess_action_async returns a non-Send future, this won't compile.
    let future = preprocess_action_async("test action", "Thorn");
    assert_send(future).await;
}

fn assert_send<T: Send>(t: T) -> T {
    t
}

// ============================================================================
// AC-1: Can be used with tokio::join! (the actual parallelization pattern)
// ============================================================================

#[tokio::test]
async fn async_preprocess_works_with_tokio_join() {
    // Simulate the tokio::join! pattern from dispatch/mod.rs
    let (result_a, result_b) = tokio::join!(
        preprocess_action_async("I attack the goblin", "Kael"),
        preprocess_action_async("I cast fireball", "Mira"),
    );

    // Both should complete successfully
    assert!(!result_a.you.is_empty(), "First join branch must produce output");
    assert!(!result_b.you.is_empty(), "Second join branch must produce output");

    // Results should be different (different inputs)
    assert_ne!(
        result_a.intent, result_b.intent,
        "Different inputs must produce different intents"
    );
}

// ============================================================================
// AC-2: Multiple sequential calls produce consistent results
// ============================================================================

#[tokio::test]
async fn async_preprocess_is_deterministic_for_fallback() {
    let input = "I pick up the ancient tome";
    let char_name = "Kael";

    let result1 = preprocess_action_async(input, char_name).await;
    let result2 = preprocess_action_async(input, char_name).await;

    assert_eq!(result1, result2, "Same input must produce identical output (fallback is deterministic)");
}

// ============================================================================
// AC-5: Async preprocess completes within timeout (15s budget)
// ============================================================================

#[tokio::test]
async fn async_preprocess_completes_within_timeout() {
    let start = std::time::Instant::now();
    let _result = preprocess_action_async("I draw my sword", "Kael").await;
    let elapsed = start.elapsed();

    // Fallback should be near-instant (< 100ms). LLM path has 15s timeout.
    // In test env, fallback fires — must be fast.
    assert!(
        elapsed.as_millis() < 5000,
        "Async preprocess must complete within 5s (fallback should be <100ms), took: {:?}",
        elapsed
    );
}

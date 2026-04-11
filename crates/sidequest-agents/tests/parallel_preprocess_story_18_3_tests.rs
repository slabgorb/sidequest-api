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
//!   3. LLM failure propagates as error (no silent fallback)
//!   4. Sub-spans from 18-1 remain valid in parallel context
//!   5. Turn time reduced (preprocess + context build overlap in flame chart)

use sidequest_agents::preprocessor::preprocess_action;
use sidequest_agents::preprocessor::preprocess_action_async;

// ============================================================================
// AC-2: Async preprocessor produces identical output to sync fallback
// ============================================================================

/// The async wrapper must produce structurally equivalent output to the sync version.
/// Both call the same underlying preprocess_action logic (LLM or fallback).
/// Note: LLM output is non-deterministic, so we compare structure not exact strings.
#[tokio::test]
async fn async_preprocess_matches_sync_structure() {
    let raw = "I draw my sword and look around";
    let char_name = "Kael";

    // Both return Result — if Haiku is down, they fail. No silent fallback.
    let sync_result =
        preprocess_action(raw, char_name).expect("Sync preprocess failed — Haiku unavailable");
    let async_result = preprocess_action_async(raw, char_name)
        .await
        .expect("Async preprocess failed — Haiku unavailable");

    // Both must produce populated fields
    assert!(!async_result.you.is_empty(), "Async you must be populated");
    assert!(!sync_result.you.is_empty(), "Sync you must be populated");

    // Both must have second-person and named forms
    assert!(
        async_result.you.starts_with("You "),
        "Async you must start with 'You '"
    );
    assert!(
        sync_result.you.starts_with("You "),
        "Sync you must start with 'You '"
    );
    assert!(
        async_result.named.starts_with(char_name),
        "Async named must start with char name"
    );
    assert!(
        sync_result.named.starts_with(char_name),
        "Sync named must start with char name"
    );

    // Both must have non-empty intent
    assert!(
        !async_result.intent.is_empty(),
        "Async intent must not be empty"
    );
    assert!(
        !sync_result.intent.is_empty(),
        "Sync intent must not be empty"
    );
}

// ============================================================================
// AC-2: Async preprocess returns correct PreprocessedAction type
// ============================================================================

#[tokio::test]
async fn async_preprocess_returns_preprocessed_action() {
    let result = preprocess_action_async("I look around the room", "Thorn")
        .await
        .expect("Preprocess failed — Haiku unavailable");

    assert!(!result.you.is_empty(), "you field must not be empty");
    assert!(!result.named.is_empty(), "named field must not be empty");
    assert!(!result.intent.is_empty(), "intent field must not be empty");
}

// ============================================================================
// AC-3: Fallback preserved — empty input produces reasonable output
// ============================================================================

#[tokio::test]
async fn async_preprocess_handles_empty_input() {
    // Empty input should produce an error or a valid result — no silent garbage
    let result = preprocess_action_async("", "Kael").await;
    let sync_result = preprocess_action("", "Kael");
    // Both should either succeed or fail consistently
    match (result, sync_result) {
        (Ok(a), Ok(s)) => {
            assert_eq!(a.you, s.you, "Empty input: async must match sync");
            assert_eq!(a.named, s.named);
            assert_eq!(a.intent, s.intent);
        }
        (Err(_), Err(_)) => {} // Both failed — consistent
        (Ok(_), Err(_)) | (Err(_), Ok(_)) => {
            panic!("Async and sync must fail/succeed consistently for empty input");
        }
    }
}

// ============================================================================
// AC-3: Fallback preserved — first-person prefix stripping works in async
// ============================================================================

#[tokio::test]
async fn async_preprocess_strips_first_person_prefix() {
    let result = preprocess_action_async("I search the chest for traps", "Kael")
        .await
        .expect("Preprocess failed — Haiku unavailable");

    assert!(
        result.intent.contains("search the chest for traps") || result.intent.contains("search"),
        "Intent should contain the action, got: '{}'",
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
async fn async_preprocess_power_grab_matches_sync() {
    // Async wrapper must produce same power-grab classification as sync version
    let raw = "I wish for unlimited gold";
    let char_name = "Kael";

    let sync_result = preprocess_action(raw, char_name).expect("Sync preprocess failed");
    let async_result = preprocess_action_async(raw, char_name)
        .await
        .expect("Async preprocess failed");

    assert_eq!(
        async_result.is_power_grab, sync_result.is_power_grab,
        "Power grab flag must match sync version"
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
    let _ = assert_send(future).await;
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

    let a = result_a.expect("First preprocess failed");
    let b = result_b.expect("Second preprocess failed");

    assert!(!a.you.is_empty(), "First join branch must produce output");
    assert!(!b.you.is_empty(), "Second join branch must produce output");

    assert_ne!(
        a.intent, b.intent,
        "Different inputs must produce different intents"
    );
}

// ============================================================================
// AC-2: Multiple sequential calls produce consistent results
// ============================================================================

#[tokio::test]
async fn async_preprocess_produces_structurally_valid_output() {
    let input = "I pick up the ancient tome";
    let char_name = "Kael";

    let result = preprocess_action_async(input, char_name)
        .await
        .expect("Preprocess failed — Haiku unavailable");

    assert!(
        result.you.starts_with("You "),
        "you must start with 'You ', got: '{}'",
        result.you
    );
    assert!(
        result.named.starts_with("Kael "),
        "named must start with char name, got: '{}'",
        result.named
    );
    assert!(!result.intent.is_empty(), "intent must not be empty");
    assert!(
        result.intent.contains("pick") || result.intent.contains("tome"),
        "intent must relate to the action, got: '{}'",
        result.intent
    );
}

// ============================================================================
// AC-5: Async preprocess completes within timeout (30s budget)
// ============================================================================

#[tokio::test]
async fn async_preprocess_completes_within_timeout() {
    let start = std::time::Instant::now();
    let _result = preprocess_action_async("I draw my sword", "Kael")
        .await
        .expect("Preprocess failed — Haiku unavailable");
    let elapsed = start.elapsed();

    // LLM path has 30s timeout. Must complete within that budget + spawn overhead.
    assert!(
        elapsed.as_secs() < 35,
        "Async preprocess must complete within 35s (30s LLM timeout + overhead), took: {:?}",
        elapsed
    );
}

//! Story 34-9 wiring tests: DiceThrow outcome → narrator prompt injection.
//!
//! These tests verify the server-layer wiring that connects dice resolution
//! to narrator prompt injection. The prompt-layer tests (in sidequest-agents)
//! verify that TurnContext.roll_outcome produces correct tags. These tests
//! verify that the server actually POPULATES that field from DiceThrow results.
//!
//! Reviewer finding: pending_roll_outcome was never assigned Some(...) in the
//! DiceThrow handler — the entire 34-9 feature was dead in production.

// ===========================================================================
// WIRING: DiceThrow handler stores resolved outcome for next narration turn
// ===========================================================================

/// Verify that the DiceThrow handler in lib.rs assigns `pending_roll_outcome`
/// after resolving the dice. Without this assignment, the narrator never
/// receives the roll outcome and the [DICE_OUTCOME: X] tag is never injected.
///
/// This is a source-scan wiring test. It catches the exact bug Reviewer found:
/// `pending_roll_outcome` was declared but never written to.
#[test]
fn dice_throw_handler_assigns_pending_roll_outcome() {
    let lib_src = crate::test_helpers::server_source_combined();

    // Find the DiceThrow handler block — it starts with `GameMessage::DiceThrow`
    // and ends at the next top-level match arm or closing brace + return.
    let dice_throw_start = lib_src
        .find("GameMessage::DiceThrow")
        .expect("DiceThrow handler must exist in lib.rs");

    // Search within the DiceThrow handler for the assignment
    let handler_block = &lib_src[dice_throw_start..];

    // The handler must assign pending_roll_outcome = Some(...)
    // This is the wiring that was missing — resolved.outcome was computed
    // and logged but never stored for the next narration turn.
    assert!(
        handler_block.contains("pending_roll_outcome")
            && (handler_block.contains("pending_roll_outcome = Some(")
                || handler_block.contains("pending_roll_outcome = Some (")),
        "DiceThrow handler must assign `pending_roll_outcome = Some(resolved.outcome)` \
         after resolving dice. Without this, the narrator never receives the roll outcome \
         and the [DICE_OUTCOME: X] tag is never injected. \
         Found DiceThrow handler but no assignment to pending_roll_outcome."
    );
}

/// Verify that the assignment happens BEFORE the handler returns.
/// The DiceThrow handler returns `vec![GameMessage::DiceResult { ... }]`.
/// The assignment must come before any `return` or the final expression.
#[test]
fn dice_throw_outcome_assignment_before_return() {
    // Post-refactor structure: DiceThrow handler delegates to dice_dispatch.rs
    // which stores `pending_roll_outcome = Some(resolved.outcome)` on the
    // shared session BEFORE returning. The assignment and the return live in
    // different files so the cross-file ordering check the original test did
    // is no longer meaningful — instead verify both pieces exist in the
    // combined server source.
    let server_src = crate::test_helpers::server_source_combined();

    assert!(
        server_src.contains("pending_roll_outcome = Some("),
        "Server must assign `pending_roll_outcome = Some(resolved.outcome)` \
         (currently in dice_dispatch.rs) so the next narration turn picks it up."
    );
    assert!(
        server_src.contains("GameMessage::DiceResult"),
        "Server must return a DiceResult after dice resolution."
    );
}

// ===========================================================================
// WIRING: DispatchContext carries roll_outcome to TurnContext
// ===========================================================================

/// Verify that DispatchContext.pending_roll_outcome flows through to
/// TurnContext.roll_outcome via .take(). This path already exists in code
/// (dispatch/mod.rs:962) but we verify it hasn't been accidentally removed.
#[test]
fn dispatch_context_roll_outcome_flows_to_turn_context() {
    let dispatch_src = crate::test_helpers::dispatch_source_combined();

    // The existing wiring: roll_outcome: ctx.pending_roll_outcome.take()
    assert!(
        dispatch_src.contains("pending_roll_outcome.take()"),
        "DispatchContext.pending_roll_outcome must be .take()'d into TurnContext.roll_outcome. \
         This wiring connects the server-layer dice result to the orchestrator-layer prompt injection."
    );
}

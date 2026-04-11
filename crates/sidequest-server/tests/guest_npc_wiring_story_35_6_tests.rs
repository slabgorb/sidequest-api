//! Story 35-6: Wire guest_npc permission gating into dispatch pipeline.
//!
//! Epic 35 wiring remediation. `sidequest-game::guest_npc` has been fully
//! built since story 8-7 with `PlayerRole`, `ActionCategory`,
//! `GuestNpcContext`, `validate_action()`, and `ActionError::RestrictedAction`
//! — all with proper unit tests in
//! `sidequest-game/tests/guest_npc_story_8_7_tests.rs`. Story 35-6 wires
//! the permission gate into `dispatch_player_action()` so guest NPCs can
//! only perform their allowed action categories.
//!
//! ## What this test file does NOT do
//!
//! Earlier iterations of this file contained ~15 source-grep "wiring tests"
//! that verified the production code by `include_str!`-ing the dispatch
//! source and searching for substrings (`"guest_npc"`, `PlayerRole::GuestNpc`,
//! `.can_perform(`, etc.) within byte-position windows. These tests were
//! removed during the review pass because they were:
//!
//! - **Too loose** when they passed: matching bare English words like
//!   `"Combat"` or `"category"` that appear pervasively in unrelated code
//! - **Too brittle** when they failed: byte-position windows broke every
//!   time a comment in the gate code expanded by a few lines
//! - **Not actually testing what they claimed**: a source grep for
//!   `ValidationWarning` near `"guest_npc"` cannot distinguish "the deny
//!   path emits ValidationWarning" from "an unrelated nearby block uses
//!   ValidationWarning"
//!
//! The architecturally correct wiring test would construct a real
//! `DispatchContext` and call `dispatch_player_action()` with a guest
//! player attempting a restricted action. That requires building
//! ~80 `DispatchContext` fields plus mocking `ClaudeClient` — out of
//! scope for 35-6 (and arguably out of scope for any single story until
//! a `DispatchContext::for_testing()` builder exists).
//!
//! For now, the wiring is verified by:
//!   1. The build (the gate code compiles, references the right types)
//!   2. The unit-level contract tests in this file (the underlying
//!      `guest_npc` API behaves as the gate assumes)
//!   3. Manual review (the gate is visibly inside `dispatch_player_action`
//!      after `process_action()`, inside an `if let Some(GuestNpc {...})`
//!      branch, with `WatcherEventBuilder::new("guest_npc", ...)` calls
//!      on both allow and deny paths)
//!
//! When a `DispatchContext` test builder lands, this file should grow a
//! real integration test that exercises the gate end-to-end. Until then,
//! the contract tests below are honest about what they verify.
//!
//! ## What this file DOES test
//!
//! Pure contract tests on the `sidequest-game::guest_npc` module — the
//! invariants the gate depends on. These would pass even without the gate
//! being wired; their job is to pin the API surface so a future change to
//! the underlying module can't silently break the gate. If `can_perform()`
//! changes semantics, or `ActionError::RestrictedAction` loses the
//! `category` field, or `default_guest_actions()` returns a different set,
//! these tests catch the regression at the contract level — separate from
//! whatever integration testing eventually happens at the dispatch layer.

use std::collections::HashSet;

use sidequest_game::guest_npc::{
    ActionCategory, ActionError, GuestNpcContext, PlayerRole,
};

// ============================================================================
// Contract tests on the existing guest_npc module
// ============================================================================

#[test]
fn full_player_permits_every_action_category() {
    let full = PlayerRole::Full;
    assert!(full.can_perform(&ActionCategory::Dialogue));
    assert!(full.can_perform(&ActionCategory::Movement));
    assert!(full.can_perform(&ActionCategory::Examine));
    assert!(full.can_perform(&ActionCategory::Combat));
    assert!(full.can_perform(&ActionCategory::Inventory));
}

#[test]
fn default_guest_permits_only_dialogue_movement_examine() {
    let guest = PlayerRole::GuestNpc {
        npc_name: "Marta".to_string(),
        allowed_actions: PlayerRole::default_guest_actions(),
    };
    assert!(guest.can_perform(&ActionCategory::Dialogue));
    assert!(guest.can_perform(&ActionCategory::Movement));
    assert!(guest.can_perform(&ActionCategory::Examine));
    assert!(
        !guest.can_perform(&ActionCategory::Combat),
        "Default guest NPC must not be able to initiate Combat — \
         this is the load-bearing restriction for the mode"
    );
    assert!(
        !guest.can_perform(&ActionCategory::Inventory),
        "Default guest NPC must not be able to perform Inventory actions"
    );
}

#[test]
fn guest_context_validate_action_returns_restricted_action_variant() {
    let ctx = GuestNpcContext::new(
        "player-2".to_string(),
        "Marta".to_string(),
        PlayerRole::default_guest_actions(),
    );

    let result = ctx.validate_action(&ActionCategory::Combat);
    match result {
        Err(ActionError::RestrictedAction { category }) => {
            assert_eq!(
                category,
                ActionCategory::Combat,
                "RestrictedAction error must carry the attempted category, \
                 not a placeholder — the gate's OTEL event and the client \
                 error message depend on this field"
            );
        }
        Err(other) => panic!(
            "Expected ActionError::RestrictedAction, got {:?}. The gate \
             depends on this specific variant being returned for disallowed \
             categories.",
            other
        ),
        Ok(()) => panic!(
            "validate_action(Combat) on a default guest must return Err — \
             the contract underlying Story 35-6 is broken"
        ),
    }
}

#[test]
fn guest_context_validate_action_returns_ok_for_allowed() {
    let ctx = GuestNpcContext::new(
        "player-2".to_string(),
        "Marta".to_string(),
        PlayerRole::default_guest_actions(),
    );
    assert!(ctx.validate_action(&ActionCategory::Dialogue).is_ok());
    assert!(ctx.validate_action(&ActionCategory::Movement).is_ok());
    assert!(ctx.validate_action(&ActionCategory::Examine).is_ok());
}

#[test]
fn custom_guest_allowed_set_is_respected() {
    // A guest with a custom (non-default) allowed set — e.g., a scenario
    // author gives a combat-capable NPC Combat+Dialogue. The gate must
    // respect the HashSet stored on the role, not the default.
    let mut custom = HashSet::new();
    custom.insert(ActionCategory::Combat);
    custom.insert(ActionCategory::Dialogue);
    let ctx = GuestNpcContext::new(
        "player-3".to_string(),
        "Razortooth".to_string(),
        custom,
    );
    assert!(ctx.validate_action(&ActionCategory::Combat).is_ok());
    assert!(ctx.validate_action(&ActionCategory::Dialogue).is_ok());
    assert!(ctx.validate_action(&ActionCategory::Movement).is_err());
    assert!(ctx.validate_action(&ActionCategory::Examine).is_err());
    assert!(ctx.validate_action(&ActionCategory::Inventory).is_err());
}

#[test]
fn empty_allowed_actions_denies_everything() {
    // Edge case flagged by reviewer-test-analyzer: a guest constructed with
    // an empty HashSet should deny every category, including the three
    // defaults. This is a degenerate but valid construction — a scenario
    // author can pass HashSet::new() and the gate must respect it without
    // silently substituting the defaults.
    let ctx = GuestNpcContext::new(
        "player-4".to_string(),
        "Silent Witness".to_string(),
        HashSet::new(),
    );
    assert!(matches!(
        ctx.validate_action(&ActionCategory::Dialogue),
        Err(ActionError::RestrictedAction { .. })
    ));
    assert!(matches!(
        ctx.validate_action(&ActionCategory::Movement),
        Err(ActionError::RestrictedAction { .. })
    ));
    assert!(matches!(
        ctx.validate_action(&ActionCategory::Examine),
        Err(ActionError::RestrictedAction { .. })
    ));
    assert!(matches!(
        ctx.validate_action(&ActionCategory::Combat),
        Err(ActionError::RestrictedAction { .. })
    ));
    assert!(matches!(
        ctx.validate_action(&ActionCategory::Inventory),
        Err(ActionError::RestrictedAction { .. })
    ));
}

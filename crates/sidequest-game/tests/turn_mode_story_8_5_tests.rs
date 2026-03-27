//! RED tests for Story 8-5: Turn modes.
//!
//! Tests the turn mode state machine: FreePlay, Structured, Cinematic.
//! Each mode changes how the barrier and batching behave.
//!
//! Types under test:
//!   - `TurnMode` — enum with FreePlay, Structured, Cinematic variants
//!   - `TurnModeTransition` — enum for valid state changes
//!   - `TurnMode::apply()` — pure state machine transition
//!   - `TurnMode::should_use_barrier()` — barrier gating per mode

use sidequest_game::turn_mode::{TurnMode, TurnModeTransition};

// ===========================================================================
// AC: FreePlay default — new sessions start in FreePlay mode
// ===========================================================================

#[test]
fn default_mode_is_free_play() {
    let mode = TurnMode::default();
    assert_eq!(mode, TurnMode::FreePlay);
}

// ===========================================================================
// AC: Combat transition — CombatStarted switches FreePlay to Structured
// ===========================================================================

#[test]
fn combat_started_from_free_play_gives_structured() {
    let mode = TurnMode::FreePlay;
    let next = mode.apply(TurnModeTransition::CombatStarted);
    assert_eq!(next, TurnMode::Structured);
}

// ===========================================================================
// AC: Cutscene transition — CutsceneStarted switches FreePlay to Cinematic
// ===========================================================================

#[test]
fn cutscene_started_from_free_play_gives_cinematic() {
    let mode = TurnMode::FreePlay;
    let next = mode.apply(TurnModeTransition::CutsceneStarted {
        prompt: "The vampire lord rises from his throne...".to_string(),
    });
    assert!(
        matches!(next, TurnMode::Cinematic { .. }),
        "expected Cinematic, got {next:?}"
    );
}

#[test]
fn cutscene_stores_prompt() {
    let mode = TurnMode::FreePlay;
    let next = mode.apply(TurnModeTransition::CutsceneStarted {
        prompt: "A portal opens before you.".to_string(),
    });
    match &next {
        TurnMode::Cinematic { prompt } => {
            assert_eq!(
                prompt.as_deref(),
                Some("A portal opens before you."),
                "Cinematic should store the prompt"
            );
        }
        other => panic!("expected Cinematic, got {other:?}"),
    }
}

// ===========================================================================
// AC: Return to FreePlay — CombatEnded and SceneEnded return to FreePlay
// ===========================================================================

#[test]
fn combat_ended_from_structured_gives_free_play() {
    let mode = TurnMode::Structured;
    let next = mode.apply(TurnModeTransition::CombatEnded);
    assert_eq!(next, TurnMode::FreePlay);
}

#[test]
fn scene_ended_from_cinematic_gives_free_play() {
    let mode = TurnMode::Cinematic {
        prompt: Some("dramatic scene".to_string()),
    };
    let next = mode.apply(TurnModeTransition::SceneEnded);
    assert_eq!(next, TurnMode::FreePlay);
}

#[test]
fn scene_ended_clears_cinematic_prompt() {
    let mode = TurnMode::Cinematic {
        prompt: Some("the world shakes".to_string()),
    };
    let next = mode.apply(TurnModeTransition::SceneEnded);
    // After SceneEnded, we're back to FreePlay — no prompt data carried
    assert_eq!(next, TurnMode::FreePlay);
    assert!(!matches!(next, TurnMode::Cinematic { .. }));
}

// ===========================================================================
// AC: Barrier gating — Structured and Cinematic use barrier; FreePlay does not
// ===========================================================================

#[test]
fn free_play_should_not_use_barrier() {
    let mode = TurnMode::FreePlay;
    assert!(
        !mode.should_use_barrier(),
        "FreePlay should NOT use barrier"
    );
}

#[test]
fn structured_should_use_barrier() {
    let mode = TurnMode::Structured;
    assert!(mode.should_use_barrier(), "Structured SHOULD use barrier");
}

#[test]
fn cinematic_should_use_barrier() {
    let mode = TurnMode::Cinematic {
        prompt: Some("test".to_string()),
    };
    assert!(mode.should_use_barrier(), "Cinematic SHOULD use barrier");
}

#[test]
fn cinematic_without_prompt_should_use_barrier() {
    let mode = TurnMode::Cinematic { prompt: None };
    assert!(
        mode.should_use_barrier(),
        "Cinematic with no prompt should still use barrier"
    );
}

// ===========================================================================
// AC: Invalid no-op — invalid transitions leave mode unchanged
// ===========================================================================

#[test]
fn combat_ended_from_free_play_is_no_op() {
    let mode = TurnMode::FreePlay;
    let next = mode.apply(TurnModeTransition::CombatEnded);
    assert_eq!(
        next,
        TurnMode::FreePlay,
        "CombatEnded from FreePlay should be no-op"
    );
}

#[test]
fn scene_ended_from_free_play_is_no_op() {
    let mode = TurnMode::FreePlay;
    let next = mode.apply(TurnModeTransition::SceneEnded);
    assert_eq!(
        next,
        TurnMode::FreePlay,
        "SceneEnded from FreePlay should be no-op"
    );
}

#[test]
fn combat_started_from_structured_is_no_op() {
    let mode = TurnMode::Structured;
    let next = mode.apply(TurnModeTransition::CombatStarted);
    assert_eq!(
        next,
        TurnMode::Structured,
        "CombatStarted from Structured should be no-op"
    );
}

#[test]
fn cutscene_from_structured_is_no_op() {
    let mode = TurnMode::Structured;
    let next = mode.apply(TurnModeTransition::CutsceneStarted {
        prompt: "should not transition".to_string(),
    });
    assert_eq!(
        next,
        TurnMode::Structured,
        "CutsceneStarted from Structured should be no-op"
    );
}

#[test]
fn combat_started_from_cinematic_is_no_op() {
    let mode = TurnMode::Cinematic {
        prompt: Some("mid-scene".to_string()),
    };
    let next = mode.apply(TurnModeTransition::CombatStarted);
    assert!(
        matches!(next, TurnMode::Cinematic { .. }),
        "CombatStarted from Cinematic should be no-op, got {next:?}"
    );
}

#[test]
fn combat_ended_from_cinematic_is_no_op() {
    let mode = TurnMode::Cinematic {
        prompt: Some("still in cutscene".to_string()),
    };
    let next = mode.apply(TurnModeTransition::CombatEnded);
    assert!(
        matches!(next, TurnMode::Cinematic { .. }),
        "CombatEnded from Cinematic should be no-op, got {next:?}"
    );
}

// ===========================================================================
// Full transition cycle — walk through all three modes
// ===========================================================================

#[test]
fn full_transition_cycle() {
    // Start in FreePlay (default)
    let mode = TurnMode::default();
    assert_eq!(mode, TurnMode::FreePlay);

    // Combat starts → Structured
    let mode = mode.apply(TurnModeTransition::CombatStarted);
    assert_eq!(mode, TurnMode::Structured);
    assert!(mode.should_use_barrier());

    // Combat ends → FreePlay
    let mode = mode.apply(TurnModeTransition::CombatEnded);
    assert_eq!(mode, TurnMode::FreePlay);
    assert!(!mode.should_use_barrier());

    // Cutscene starts → Cinematic
    let mode = mode.apply(TurnModeTransition::CutsceneStarted {
        prompt: "The dragon descends.".to_string(),
    });
    assert!(matches!(mode, TurnMode::Cinematic { .. }));
    assert!(mode.should_use_barrier());

    // Scene ends → FreePlay
    let mode = mode.apply(TurnModeTransition::SceneEnded);
    assert_eq!(mode, TurnMode::FreePlay);
    assert!(!mode.should_use_barrier());
}

// ===========================================================================
// Edge cases
// ===========================================================================

#[test]
fn apply_consumes_self_and_returns_new_mode() {
    // Verify apply() takes ownership (self, not &self) — enforces immutable state machine
    let mode = TurnMode::FreePlay;
    let next = mode.apply(TurnModeTransition::CombatStarted);
    // `mode` is consumed — can't use it after apply. This is enforced by the compiler.
    // If apply took &self, this test would still compile but the design would be wrong.
    assert_eq!(next, TurnMode::Structured);
}

#[test]
fn cinematic_with_none_prompt_transitions_to_free_play_on_scene_ended() {
    let mode = TurnMode::Cinematic { prompt: None };
    let next = mode.apply(TurnModeTransition::SceneEnded);
    assert_eq!(next, TurnMode::FreePlay);
}

#[test]
fn turn_mode_implements_debug() {
    // Needed for error messages and logging
    let mode = TurnMode::FreePlay;
    let debug = format!("{mode:?}");
    assert!(!debug.is_empty(), "Debug should produce non-empty output");
}

#[test]
fn turn_mode_implements_clone() {
    let mode = TurnMode::Cinematic {
        prompt: Some("cloneable".to_string()),
    };
    let cloned = mode.clone();
    assert_eq!(mode, cloned);
}

#[test]
fn turn_mode_transition_implements_debug() {
    let transition = TurnModeTransition::CombatStarted;
    let debug = format!("{transition:?}");
    assert!(!debug.is_empty());
}

// ===========================================================================
// Rule #9: Getters where applicable
// ===========================================================================

#[test]
fn turn_mode_exposes_should_use_barrier() {
    // Verify the method exists and returns bool
    let barrier: bool = TurnMode::FreePlay.should_use_barrier();
    assert!(!barrier);
}

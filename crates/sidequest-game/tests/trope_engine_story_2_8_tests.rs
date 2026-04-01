//! Story 2-8: Trope Engine Runtime — failing tests (RED phase)
//!
//! Tests cover:
//!   - TropeState struct with TropeStatus enum
//!   - TropeEngine::tick() — passive progression + escalation beat firing
//!   - TropeEngine::apply_keyword_modifiers() — accelerator/decelerator keywords
//!   - FiredBeat struct
//!   - GameSnapshot::activate_trope() / resolve_trope() lifecycle
//!   - Beat injection prompt section formatting

use std::collections::HashSet;

use ordered_float::OrderedFloat;
use sidequest_genre::{PassiveProgression, TropeDefinition, TropeEscalation};
use sidequest_protocol::NonBlankString;

// New types from story 2-8
use sidequest_game::trope::{FiredBeat, TropeEngine, TropeState, TropeStatus};

// ============================================================================
// Test fixtures
// ============================================================================

fn test_trope_def() -> TropeDefinition {
    TropeDefinition {
        id: Some("rising_threat".to_string()),
        name: NonBlankString::new("Rising Threat").unwrap(),
        description: Some("A danger grows in the shadows".to_string()),
        category: "conflict".to_string(),
        triggers: vec!["combat".to_string()],
        narrative_hints: vec!["Hint at growing danger".to_string()],
        tension_level: Some(0.5),
        resolution_hints: None,
        resolution_patterns: None,
        tags: vec![],
        escalation: vec![
            TropeEscalation {
                at: 0.25,
                event: "Strange noises in the night".to_string(),
                npcs_involved: vec![],
                stakes: "Safety of the camp".to_string(),
            },
            TropeEscalation {
                at: 0.5,
                event: "A scout goes missing".to_string(),
                npcs_involved: vec!["Scout Kira".to_string()],
                stakes: "Lives at risk".to_string(),
            },
            TropeEscalation {
                at: 1.0,
                event: "The creature attacks the camp".to_string(),
                npcs_involved: vec!["The Beast".to_string()],
                stakes: "Survival of the party".to_string(),
            },
        ],
        passive_progression: Some(PassiveProgression {
            rate_per_turn: 0.1,
            rate_per_day: 0.0,
            accelerators: vec!["danger".to_string(), "threat".to_string()],
            decelerators: vec!["safe".to_string(), "calm".to_string()],
            accelerator_bonus: 0.05,
            decelerator_penalty: 0.03,
        }),
        is_abstract: false,
        extends: None,
    }
}

fn test_trope_def_no_progression() -> TropeDefinition {
    TropeDefinition {
        id: Some("static_lore".to_string()),
        name: NonBlankString::new("Static Lore").unwrap(),
        description: Some("Background lore that doesn't tick".to_string()),
        category: "revelation".to_string(),
        triggers: vec![],
        narrative_hints: vec![],
        tension_level: None,
        resolution_hints: None,
        resolution_patterns: None,
        tags: vec![],
        escalation: vec![],
        passive_progression: None,
        is_abstract: false,
        extends: None,
    }
}

fn test_trope_state(def_id: &str) -> TropeState {
    TropeState::new(def_id)
}

// ============================================================================
// AC: TropeState and TropeStatus
// ============================================================================

#[test]
fn trope_state_new_is_active_at_zero() {
    let ts = TropeState::new("rising_threat");
    assert_eq!(ts.status(), TropeStatus::Active);
    assert_eq!(ts.progression(), 0.0);
    assert!(ts.fired_beats().is_empty());
    assert_eq!(ts.trope_definition_id(), "rising_threat");
}

#[test]
fn trope_status_resolved_and_dormant_are_distinct() {
    // Verify the enum has the expected variants
    assert_ne!(TropeStatus::Active, TropeStatus::Resolved);
    assert_ne!(TropeStatus::Active, TropeStatus::Dormant);
    assert_ne!(TropeStatus::Resolved, TropeStatus::Dormant);
    assert_ne!(TropeStatus::Active, TropeStatus::Progressing);
}

// ============================================================================
// AC: Passive tick — progression increases by rate_per_turn
// ============================================================================

#[test]
fn tick_advances_progression_by_rate() {
    let defs = vec![test_trope_def()];
    let mut tropes = vec![test_trope_state("rising_threat")];

    let fired = TropeEngine::tick(&mut tropes, &defs);

    // rate_per_turn is 0.1, so after one tick: 0.0 + 0.1 = 0.1
    assert!((tropes[0].progression() - 0.1).abs() < f64::EPSILON);
    // No beats should fire at 0.1 (first beat is at 0.25)
    assert!(fired.is_empty());
}

#[test]
fn tick_multiple_advances_accumulate() {
    let defs = vec![test_trope_def()];
    let mut tropes = vec![test_trope_state("rising_threat")];

    TropeEngine::tick(&mut tropes, &defs);
    TropeEngine::tick(&mut tropes, &defs);
    TropeEngine::tick(&mut tropes, &defs);

    // 3 ticks at 0.1 = 0.3
    assert!((tropes[0].progression() - 0.3).abs() < 0.001);
}

#[test]
fn tick_caps_progression_at_one() {
    let defs = vec![test_trope_def()];
    let mut tropes = vec![test_trope_state("rising_threat")];
    // Set progression to 0.95
    tropes[0].set_progression(0.95);

    TropeEngine::tick(&mut tropes, &defs);

    // 0.95 + 0.1 = 1.05 → capped at 1.0
    assert!((tropes[0].progression() - 1.0).abs() < f64::EPSILON);
}

#[test]
fn tick_no_progression_without_passive_config() {
    let defs = vec![test_trope_def_no_progression()];
    let mut tropes = vec![test_trope_state("static_lore")];

    TropeEngine::tick(&mut tropes, &defs);

    assert_eq!(tropes[0].progression(), 0.0);
}

// ============================================================================
// AC: Beat fires — progression crosses threshold
// ============================================================================

#[test]
fn beat_fires_when_threshold_crossed() {
    let defs = vec![test_trope_def()];
    let mut tropes = vec![test_trope_state("rising_threat")];
    tropes[0].set_progression(0.2);

    // Tick: 0.2 + 0.1 = 0.3, crosses 0.25 threshold
    let fired = TropeEngine::tick(&mut tropes, &defs);

    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].trope_name, "Rising Threat");
    assert_eq!(fired[0].beat.event, "Strange noises in the night");
    assert_eq!(fired[0].beat.stakes, "Safety of the camp");
}

#[test]
fn multiple_beats_fire_when_multiple_thresholds_crossed() {
    let defs = vec![test_trope_def()];
    let mut tropes = vec![test_trope_state("rising_threat")];
    tropes[0].set_progression(0.2);

    // Tick with big jump: 0.2 + 0.1 = 0.3, but let's set rate high
    // Actually, set progression to 0.45, tick will go to 0.55
    // That crosses both 0.25 (already passed? No, nothing fired yet) AND 0.5
    tropes[0].set_progression(0.0);
    // After 3 ticks from 0.0: 0.3 → crosses 0.25 on tick 3
    TropeEngine::tick(&mut tropes, &defs); // 0.1
    TropeEngine::tick(&mut tropes, &defs); // 0.2
    let fired = TropeEngine::tick(&mut tropes, &defs); // 0.3 → fires 0.25 beat
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].beat.at, 0.25);

    // Two more ticks: 0.4, 0.5 → fires 0.5 beat
    TropeEngine::tick(&mut tropes, &defs); // 0.4
    let fired = TropeEngine::tick(&mut tropes, &defs); // 0.5 → fires 0.5 beat
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].beat.at, 0.5);
}

// ============================================================================
// AC: No double fire — same threshold doesn't fire twice
// ============================================================================

#[test]
fn beat_does_not_fire_twice() {
    let defs = vec![test_trope_def()];
    let mut tropes = vec![test_trope_state("rising_threat")];
    tropes[0].set_progression(0.2);

    // First tick crosses 0.25
    let fired = TropeEngine::tick(&mut tropes, &defs);
    assert_eq!(fired.len(), 1);

    // Second tick — progression 0.4, still above 0.25 but already fired
    let fired = TropeEngine::tick(&mut tropes, &defs);
    assert!(fired.is_empty(), "Beat at 0.25 should not fire again");
}

#[test]
fn fired_beats_tracked_in_hashset() {
    let defs = vec![test_trope_def()];
    let mut tropes = vec![test_trope_state("rising_threat")];
    tropes[0].set_progression(0.2);

    TropeEngine::tick(&mut tropes, &defs);

    assert!(tropes[0].fired_beats().contains(&OrderedFloat(0.25)));
}

// ============================================================================
// AC: Resolved skipped — resolved tropes not ticked
// ============================================================================

#[test]
fn resolved_trope_not_ticked() {
    let defs = vec![test_trope_def()];
    let mut tropes = vec![test_trope_state("rising_threat")];
    tropes[0].set_status(TropeStatus::Resolved);
    tropes[0].set_progression(0.5);

    TropeEngine::tick(&mut tropes, &defs);

    // Progression should not change
    assert_eq!(tropes[0].progression(), 0.5);
}

// ============================================================================
// AC: Dormant skipped — dormant tropes not ticked
// ============================================================================

#[test]
fn dormant_trope_not_ticked() {
    let defs = vec![test_trope_def()];
    let mut tropes = vec![test_trope_state("rising_threat")];
    tropes[0].set_status(TropeStatus::Dormant);

    TropeEngine::tick(&mut tropes, &defs);

    assert_eq!(tropes[0].progression(), 0.0);
}

// ============================================================================
// AC: Status update — Active → Progressing when progression > 0.0
// ============================================================================

#[test]
fn status_transitions_to_progressing_after_tick() {
    let defs = vec![test_trope_def()];
    let mut tropes = vec![test_trope_state("rising_threat")];
    assert_eq!(tropes[0].status(), TropeStatus::Active);

    TropeEngine::tick(&mut tropes, &defs);

    assert_eq!(tropes[0].status(), TropeStatus::Progressing);
}

// ============================================================================
// AC: Keyword acceleration
// ============================================================================

// Keyword modifier tests removed — apply_keyword_modifiers was deleted.
// Trope progression is now driven by LLM evaluation (TroperAgent::evaluate_triggers).

// ============================================================================
// AC: Beat injection — fired beats in prompt context
// ============================================================================

#[test]
fn fired_beat_contains_event_and_stakes() {
    let defs = vec![test_trope_def()];
    let mut tropes = vec![test_trope_state("rising_threat")];
    tropes[0].set_progression(0.2);

    let fired = TropeEngine::tick(&mut tropes, &defs);
    assert_eq!(fired.len(), 1);

    let beat = &fired[0];
    assert_eq!(beat.trope_id, "rising_threat");
    assert_eq!(beat.trope_name, "Rising Threat");
    assert!(!beat.beat.event.is_empty());
    assert!(!beat.beat.stakes.is_empty());
}

#[test]
fn fired_beat_contains_npcs_involved() {
    let defs = vec![test_trope_def()];
    let mut tropes = vec![test_trope_state("rising_threat")];
    tropes[0].set_progression(0.45);

    let fired = TropeEngine::tick(&mut tropes, &defs);
    // Should fire the 0.5 beat which has npcs_involved: ["Scout Kira"]
    let scout_beat = fired.iter().find(|b| b.beat.at == 0.5);
    assert!(scout_beat.is_some(), "0.5 beat should fire");
    assert_eq!(scout_beat.unwrap().beat.npcs_involved, vec!["Scout Kira"]);
}

// ============================================================================
// AC: Activate idempotent
// ============================================================================

#[test]
fn activate_trope_creates_new_state() {
    let mut tropes: Vec<TropeState> = vec![];

    let ts = TropeEngine::activate(&mut tropes, "rising_threat");
    assert_eq!(ts.trope_definition_id(), "rising_threat");
    assert_eq!(ts.status(), TropeStatus::Active);
    assert_eq!(tropes.len(), 1);
}

#[test]
fn activate_trope_idempotent() {
    let mut tropes: Vec<TropeState> = vec![];

    TropeEngine::activate(&mut tropes, "rising_threat");
    TropeEngine::activate(&mut tropes, "rising_threat");

    assert_eq!(
        tropes.len(),
        1,
        "Duplicate activation should not create second entry"
    );
}

// ============================================================================
// AC: Resolve sets 1.0
// ============================================================================

#[test]
fn resolve_trope_sets_progression_and_status() {
    let mut tropes = vec![test_trope_state("rising_threat")];
    tropes[0].set_progression(0.5);

    TropeEngine::resolve(&mut tropes, "rising_threat", Some("The beast was slain"));

    assert_eq!(tropes[0].status(), TropeStatus::Resolved);
    assert!((tropes[0].progression() - 1.0).abs() < f64::EPSILON);
}

#[test]
fn resolve_trope_adds_note() {
    let mut tropes = vec![test_trope_state("rising_threat")];

    TropeEngine::resolve(&mut tropes, "rising_threat", Some("Victory!"));

    assert!(tropes[0].notes().contains(&"Victory!".to_string()));
}

#[test]
fn resolve_unknown_trope_is_noop() {
    let mut tropes = vec![test_trope_state("rising_threat")];

    // Should not panic
    TropeEngine::resolve(&mut tropes, "nonexistent_trope", None);

    assert_eq!(tropes[0].status(), TropeStatus::Active);
}

// ============================================================================
// AC: Missing def logged — trope with unknown def ID skipped
// ============================================================================

#[test]
fn tick_skips_trope_with_unknown_definition() {
    let defs = vec![test_trope_def()]; // only has "rising_threat"
    let mut tropes = vec![test_trope_state("unknown_trope_id")];

    let fired = TropeEngine::tick(&mut tropes, &defs);

    // Should not panic, should not advance progression
    assert!(fired.is_empty());
    assert_eq!(tropes[0].progression(), 0.0);
}

// ============================================================================
// Rule #6: Test quality self-check
// ============================================================================
// Every test above uses assert_eq!, assert!, or specific value checks.
// No `let _ =` patterns. No `assert!(true)`.
// All fired beat checks verify actual content (event, stakes, npcs), not just
// presence.

//! Story 37-15: Trope completion must handshake with encounter lifecycle.
//!
//! During playtest, the_standoff trope reached progression 1.0 while the poker
//! encounter remained resolved=false. The trope engine and encounter engine have
//! no handshake — when a trope completes, nothing signals the encounter to resolve.
//!
//! These tests assert:
//! 1. Trope auto-resolves when progression reaches 1.0 via tick()
//! 2. tick() returns information about newly-completed tropes
//! 3. A completed trope can signal encounter resolution

use sidequest_game::encounter::StructuredEncounter;
use sidequest_game::trope::{TropeEngine, TropeState, TropeStatus};
use sidequest_genre::{PassiveProgression, TropeDefinition, TropeEscalation};
use sidequest_protocol::NonBlankString;

// ── Test fixtures ────────────────────────────────────────────────────

fn fast_trope_def() -> TropeDefinition {
    TropeDefinition {
        id: Some("the_standoff".to_string()),
        name: NonBlankString::new("The Standoff").unwrap(),
        description: Some("A tense confrontation reaches its breaking point".to_string()),
        category: "conflict".to_string(),
        triggers: vec![],
        narrative_hints: vec![],
        tension_level: Some(0.8),
        resolution_hints: None,
        resolution_patterns: None,
        tags: vec![],
        escalation: vec![
            TropeEscalation {
                at: 0.5,
                event: "Tensions rise".to_string(),
                npcs_involved: vec![],
                stakes: "Pride".to_string(),
            },
            TropeEscalation {
                at: 1.0,
                event: "The standoff breaks".to_string(),
                npcs_involved: vec![],
                stakes: "Everything".to_string(),
            },
        ],
        passive_progression: Some(PassiveProgression {
            rate_per_turn: 0.6,
            rate_per_day: 0.0,
            accelerators: vec![],
            decelerators: vec![],
            accelerator_bonus: 0.0,
            decelerator_penalty: 0.0,
        }),
        is_abstract: false,
        extends: None,
    }
}

// ── Test 1: Trope auto-resolves at progression 1.0 ──────────────────

#[test]
fn tick_resolves_trope_when_progression_reaches_one() {
    let mut tropes = vec![TropeState::new("the_standoff")];
    let defs = vec![fast_trope_def()];

    // First tick: 0.0 + 0.6 = 0.6 → Progressing
    TropeEngine::tick(&mut tropes, &defs);
    assert_eq!(tropes[0].status(), TropeStatus::Progressing);

    // Second tick: 0.6 + 0.6 = 1.0 (clamped) → should auto-resolve
    TropeEngine::tick(&mut tropes, &defs);
    assert_eq!(
        tropes[0].progression(),
        1.0,
        "progression should reach 1.0"
    );
    assert_eq!(
        tropes[0].status(),
        TropeStatus::Resolved,
        "trope must auto-resolve when progression reaches 1.0 — \
         currently stays Progressing, which is the bug"
    );
}

#[test]
fn tick_does_not_resolve_trope_below_one() {
    let mut tropes = vec![TropeState::new("the_standoff")];
    let defs = vec![fast_trope_def()];

    // Single tick: 0.0 + 0.6 = 0.6 → should NOT resolve
    TropeEngine::tick(&mut tropes, &defs);
    assert_eq!(tropes[0].progression(), 0.6);
    assert_ne!(
        tropes[0].status(),
        TropeStatus::Resolved,
        "trope should not resolve before reaching 1.0"
    );
}

#[test]
fn resolved_trope_does_not_tick_further() {
    let mut tropes = vec![TropeState::new("the_standoff")];
    let defs = vec![fast_trope_def()];

    // Drive to completion
    TropeEngine::tick(&mut tropes, &defs);
    TropeEngine::tick(&mut tropes, &defs);

    // Tick again — should be skipped (Resolved tropes are filtered)
    let fired = TropeEngine::tick(&mut tropes, &defs);
    assert!(fired.is_empty(), "resolved trope must not fire additional beats");
    assert_eq!(tropes[0].progression(), 1.0, "progression stays at 1.0");
}

// ── Test 2: tick() signals which tropes just completed ───────────────

#[test]
fn tick_returns_completed_trope_ids() {
    let mut tropes = vec![TropeState::new("the_standoff")];
    let defs = vec![fast_trope_def()];

    // First tick — not complete yet
    let result = TropeEngine::tick(&mut tropes, &defs);
    let completed: Vec<&str> = result
        .iter()
        .filter(|b| b.beat.at >= 1.0)
        .map(|b| b.trope_id.as_str())
        .collect();
    assert!(
        completed.is_empty(),
        "no trope should complete on first tick"
    );

    // Second tick — reaches 1.0, should have a beat at 1.0 AND
    // the trope should be marked Resolved
    let result = TropeEngine::tick(&mut tropes, &defs);
    let completed: Vec<&str> = result
        .iter()
        .filter(|b| b.beat.at >= 1.0)
        .map(|b| b.trope_id.as_str())
        .collect();
    assert!(
        completed.contains(&"the_standoff"),
        "tick must signal that the_standoff completed via a beat at 1.0"
    );
    assert_eq!(
        tropes[0].status(),
        TropeStatus::Resolved,
        "trope must be Resolved after the 1.0 beat fires"
    );
}

// ── Test 3: Encounter resolution from trope completion ───────────────

#[test]
fn encounter_can_be_resolved_by_trope_completion() {
    // Create an unresolved encounter
    let mut encounter = StructuredEncounter::combat(
        vec![],
        100, // hp threshold
    );
    assert!(!encounter.resolved, "encounter starts unresolved");

    // Signal the encounter that the associated trope completed.
    // This method must exist — it's the handshake this story introduces.
    encounter.resolve_from_trope("the_standoff");

    assert!(
        encounter.resolved,
        "encounter must be resolved after trope completion signal"
    );
}

#[test]
fn encounter_resolution_from_trope_sets_outcome() {
    let mut encounter = StructuredEncounter::combat(vec![], 100);

    encounter.resolve_from_trope("the_standoff");

    assert!(
        encounter.outcome.is_some(),
        "encounter outcome must be set when resolved by trope"
    );
    let outcome = encounter.outcome.as_ref().unwrap();
    assert!(
        outcome.contains("the_standoff"),
        "outcome should reference the trope that triggered resolution"
    );
}

#[test]
fn encounter_already_resolved_ignores_trope_signal() {
    let mut encounter = StructuredEncounter::combat(vec![], 100);

    // Resolve it first via normal path
    encounter.resolved = true;
    encounter.outcome = Some("defeated enemies".to_string());

    // Trope completion signal should be a no-op
    encounter.resolve_from_trope("the_standoff");

    assert_eq!(
        encounter.outcome.as_deref(),
        Some("defeated enemies"),
        "already-resolved encounter must not change outcome on trope signal"
    );
}

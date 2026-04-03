//! Cross-session trope advancement tests.
//!
//! Covers TropeEngine::advance_between_sessions() — the "living world" mechanic
//! where tropes progress using rate_per_day while players are away.

use ordered_float::OrderedFloat;
use sidequest_genre::{PassiveProgression, TropeDefinition, TropeEscalation};
use sidequest_protocol::NonBlankString;

use sidequest_game::trope::{TropeEngine, TropeState, TropeStatus};

fn def_with_rate_per_day(id: &str, name: &str, rate: f64) -> TropeDefinition {
    TropeDefinition {
        id: Some(id.to_string()),
        name: NonBlankString::new(name).unwrap(),
        description: None,
        category: "conflict".to_string(),
        triggers: vec![],
        narrative_hints: vec![],
        tension_level: Some(0.5),
        resolution_hints: None,
        resolution_patterns: None,
        tags: vec![],
        escalation: vec![
            TropeEscalation {
                at: 0.25,
                event: "Quarter mark".to_string(),
                npcs_involved: vec![],
                stakes: "Low".to_string(),
            },
            TropeEscalation {
                at: 0.5,
                event: "Halfway".to_string(),
                npcs_involved: vec!["NPC A".to_string()],
                stakes: "Medium".to_string(),
            },
            TropeEscalation {
                at: 1.0,
                event: "Climax".to_string(),
                npcs_involved: vec!["NPC B".to_string()],
                stakes: "Critical".to_string(),
            },
        ],
        passive_progression: Some(PassiveProgression {
            rate_per_turn: 0.1,
            rate_per_day: rate,
            accelerators: vec![],
            decelerators: vec![],
            accelerator_bonus: 0.0,
            decelerator_penalty: 0.0,
        }),
        is_abstract: false,
        extends: None,
    }
}

#[test]
fn advance_zero_days_is_noop() {
    let defs = vec![def_with_rate_per_day("threat", "Threat", 0.02)];
    let mut tropes = vec![TropeState::new("threat")];

    let fired = TropeEngine::advance_between_sessions(&mut tropes, &defs, 0.0);

    assert!(fired.is_empty());
    assert!((tropes[0].progression() - 0.0).abs() < f64::EPSILON);
}

#[test]
fn advance_seven_days_at_002_rate() {
    let defs = vec![def_with_rate_per_day("threat", "Threat", 0.02)];
    let mut tropes = vec![TropeState::new("threat")];

    let fired = TropeEngine::advance_between_sessions(&mut tropes, &defs, 7.0);

    // 0.02 * 7 = 0.14
    assert!((tropes[0].progression() - 0.14).abs() < 1e-9);
    assert_eq!(tropes[0].status(), TropeStatus::Progressing);
    // No beats should fire (0.14 < 0.25 threshold)
    assert!(fired.is_empty());
}

#[test]
fn advance_fires_beats_across_gap() {
    let defs = vec![def_with_rate_per_day("threat", "Threat", 0.05)];
    let mut tropes = vec![TropeState::new("threat")];

    // 0.05 * 7 = 0.35 — should cross the 0.25 threshold
    let fired = TropeEngine::advance_between_sessions(&mut tropes, &defs, 7.0);

    assert!((tropes[0].progression() - 0.35).abs() < 1e-9);
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].beat.event, "Quarter mark");
    assert!(tropes[0]
        .fired_beats()
        .contains(&OrderedFloat(0.25)));
}

#[test]
fn advance_fires_multiple_beats() {
    let defs = vec![def_with_rate_per_day("threat", "Threat", 0.1)];
    let mut tropes = vec![TropeState::new("threat")];

    // 0.1 * 7 = 0.7 — crosses both 0.25 and 0.5 thresholds
    let fired = TropeEngine::advance_between_sessions(&mut tropes, &defs, 7.0);

    assert!((tropes[0].progression() - 0.7).abs() < 1e-9);
    assert_eq!(fired.len(), 2);
    assert!(tropes[0]
        .fired_beats()
        .contains(&OrderedFloat(0.25)));
    assert!(tropes[0]
        .fired_beats()
        .contains(&OrderedFloat(0.5)));
}

#[test]
fn advance_skips_resolved_tropes() {
    let defs = vec![def_with_rate_per_day("done", "Done", 0.1)];
    let mut tropes = vec![TropeState::new("done")];
    tropes[0].set_status(TropeStatus::Resolved);
    tropes[0].set_progression(1.0);

    let fired = TropeEngine::advance_between_sessions(&mut tropes, &defs, 10.0);

    assert!(fired.is_empty());
    assert!((tropes[0].progression() - 1.0).abs() < f64::EPSILON);
}

#[test]
fn advance_skips_dormant_tropes() {
    let defs = vec![def_with_rate_per_day("dormant", "Dormant", 0.1)];
    let mut tropes = vec![TropeState::new("dormant")];
    tropes[0].set_status(TropeStatus::Dormant);

    let fired = TropeEngine::advance_between_sessions(&mut tropes, &defs, 10.0);

    assert!(fired.is_empty());
}

#[test]
fn advance_clamps_at_one() {
    let defs = vec![def_with_rate_per_day("threat", "Threat", 0.5)];
    let mut tropes = vec![TropeState::new("threat")];
    tropes[0].set_progression(0.8);

    // 0.8 + 0.5 * 10 = 5.8 → clamped to 1.0
    let fired = TropeEngine::advance_between_sessions(&mut tropes, &defs, 10.0);

    assert!((tropes[0].progression() - 1.0).abs() < f64::EPSILON);
    // Should fire the 1.0 beat
    assert!(fired.iter().any(|b| b.beat.event == "Climax"));
}

#[test]
fn advance_skips_zero_rate_per_day() {
    let defs = vec![def_with_rate_per_day("static", "Static", 0.0)];
    let mut tropes = vec![TropeState::new("static")];

    let fired = TropeEngine::advance_between_sessions(&mut tropes, &defs, 100.0);

    assert!(fired.is_empty());
    assert!((tropes[0].progression() - 0.0).abs() < f64::EPSILON);
}

#[test]
fn advance_does_not_refire_already_fired_beats() {
    let defs = vec![def_with_rate_per_day("threat", "Threat", 0.05)];
    let mut tropes = vec![TropeState::new("threat")];
    tropes[0].set_progression(0.3);
    // Simulate that 0.25 beat already fired during gameplay
    // We need to access fired_beats — use tick to fire it naturally first
    TropeEngine::tick(&mut tropes, &defs); // fires 0.25 beat at progression 0.4

    let pre_fired = tropes[0].fired_beats().len();

    // Now advance by 3 days: 0.4 + 0.05*3 = 0.55 — crosses 0.5
    let fired = TropeEngine::advance_between_sessions(&mut tropes, &defs, 3.0);

    // Should only fire the 0.5 beat, not re-fire 0.25
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0].beat.event, "Halfway");
    assert!(tropes[0].fired_beats().len() > pre_fired);
}

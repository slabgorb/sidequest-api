//! Story 5-2: Combat event classification
//!
//! Categorize combat outcomes as boring/dramatic/normal, track boring_streak.
//! The classifier examines a RoundResult and produces a CombatEvent for the
//! TensionTracker's gambler's ramp.

use sidequest_game::tension_tracker::{
    classify_round, CombatEvent, DamageEvent, RoundResult, TensionTracker,
};

// =========================================================================
// Classification — dramatic outcomes
// =========================================================================

#[test]
fn kill_shot_is_dramatic() {
    let round = RoundResult {
        round: 1,
        damage_events: vec![DamageEvent {
            attacker: "hero".into(),
            target: "goblin".into(),
            damage: 30,
            round: 1,
        }],
        effects_applied: vec![],
        effects_expired: vec![],
    };
    // A kill is dramatic — any single hit dealing >= lethal threshold
    let event = classify_round(&round, Some("goblin"));
    assert_eq!(event, CombatEvent::Dramatic);
}

#[test]
fn high_damage_round_is_dramatic() {
    let round = RoundResult {
        round: 2,
        damage_events: vec![
            DamageEvent {
                attacker: "hero".into(),
                target: "dragon".into(),
                damage: 25,
                round: 2,
            },
            DamageEvent {
                attacker: "mage".into(),
                target: "dragon".into(),
                damage: 20,
                round: 2,
            },
        ],
        effects_applied: vec![],
        effects_expired: vec![],
    };
    // Total damage >= dramatic threshold → dramatic
    let event = classify_round(&round, None);
    assert_eq!(event, CombatEvent::Dramatic);
}

#[test]
fn status_effect_applied_is_dramatic() {
    let round = RoundResult {
        round: 1,
        damage_events: vec![],
        effects_applied: vec!["Stun".into()],
        effects_expired: vec![],
    };
    let event = classify_round(&round, None);
    assert_eq!(event, CombatEvent::Dramatic);
}

// =========================================================================
// Classification — boring outcomes
// =========================================================================

#[test]
fn zero_damage_round_is_boring() {
    let round = RoundResult {
        round: 3,
        damage_events: vec![],
        effects_applied: vec![],
        effects_expired: vec![],
    };
    let event = classify_round(&round, None);
    assert_eq!(event, CombatEvent::Boring);
}

#[test]
fn all_misses_is_boring() {
    let round = RoundResult {
        round: 4,
        damage_events: vec![
            DamageEvent {
                attacker: "hero".into(),
                target: "goblin".into(),
                damage: 0,
                round: 4,
            },
            DamageEvent {
                attacker: "goblin".into(),
                target: "hero".into(),
                damage: 0,
                round: 4,
            },
        ],
        effects_applied: vec![],
        effects_expired: vec![],
    };
    let event = classify_round(&round, None);
    assert_eq!(event, CombatEvent::Boring);
}

// =========================================================================
// Classification — normal outcomes
// =========================================================================

#[test]
fn moderate_damage_is_normal() {
    let round = RoundResult {
        round: 5,
        damage_events: vec![DamageEvent {
            attacker: "hero".into(),
            target: "goblin".into(),
            damage: 5,
            round: 5,
        }],
        effects_applied: vec![],
        effects_expired: vec![],
    };
    // Low-but-nonzero damage, no kills, no effects → normal
    let event = classify_round(&round, None);
    assert_eq!(event, CombatEvent::Normal);
}

// =========================================================================
// boring_streak tracking
// =========================================================================

#[test]
fn boring_streak_accessible() {
    let tracker = TensionTracker::new();
    assert_eq!(tracker.boring_streak(), 0);
}

#[test]
fn boring_streak_increments_on_boring_events() {
    let mut tracker = TensionTracker::new();
    tracker.record_event(CombatEvent::Boring);
    assert_eq!(tracker.boring_streak(), 1);
    tracker.record_event(CombatEvent::Boring);
    assert_eq!(tracker.boring_streak(), 2);
}

#[test]
fn boring_streak_resets_on_dramatic() {
    let mut tracker = TensionTracker::new();
    tracker.record_event(CombatEvent::Boring);
    tracker.record_event(CombatEvent::Boring);
    tracker.record_event(CombatEvent::Boring);
    assert_eq!(tracker.boring_streak(), 3);
    tracker.record_event(CombatEvent::Dramatic);
    assert_eq!(tracker.boring_streak(), 0);
}

#[test]
fn boring_streak_unchanged_on_normal() {
    let mut tracker = TensionTracker::new();
    tracker.record_event(CombatEvent::Boring);
    tracker.record_event(CombatEvent::Boring);
    assert_eq!(tracker.boring_streak(), 2);
    tracker.record_event(CombatEvent::Normal);
    assert_eq!(tracker.boring_streak(), 2);
}

// =========================================================================
// Integration — classify + record
// =========================================================================

#[test]
fn classify_and_record_updates_tracker() {
    let mut tracker = TensionTracker::new();

    // Boring round
    let boring_round = RoundResult {
        round: 1,
        damage_events: vec![],
        effects_applied: vec![],
        effects_expired: vec![],
    };
    let event = classify_round(&boring_round, None);
    tracker.record_event(event);
    assert_eq!(tracker.boring_streak(), 1);
    assert!(tracker.action_tension() > 0.0);

    // Dramatic round
    let dramatic_round = RoundResult {
        round: 2,
        damage_events: vec![DamageEvent {
            attacker: "hero".into(),
            target: "goblin".into(),
            damage: 30,
            round: 2,
        }],
        effects_applied: vec![],
        effects_expired: vec![],
    };
    let event = classify_round(&dramatic_round, Some("goblin"));
    tracker.record_event(event);
    assert_eq!(tracker.boring_streak(), 0);
    assert_eq!(tracker.action_tension(), 0.0);
}

// =========================================================================
// Edge cases
// =========================================================================

#[test]
fn negative_damage_treated_as_zero() {
    let round = RoundResult {
        round: 1,
        damage_events: vec![DamageEvent {
            attacker: "hero".into(),
            target: "goblin".into(),
            damage: -5,
            round: 1,
        }],
        effects_applied: vec![],
        effects_expired: vec![],
    };
    let event = classify_round(&round, None);
    assert_eq!(event, CombatEvent::Boring);
}

#[test]
fn expired_effects_dont_count_as_dramatic() {
    let round = RoundResult {
        round: 1,
        damage_events: vec![],
        effects_applied: vec![],
        effects_expired: vec!["Stun".into()],
    };
    // Only new effects are dramatic, not expired ones
    let event = classify_round(&round, None);
    assert_eq!(event, CombatEvent::Boring);
}

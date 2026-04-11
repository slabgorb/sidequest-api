//! Story 5-2: Combat event classification tests
//!
//! RED phase — tests reference types and methods that don't exist yet.
//! They will fail to compile until Dev implements:
//!   - DetailedCombatEvent enum (CriticalHit, KillingBlow, DeathSave, FirstBlood, NearMiss, LastStanding)
//!   - spike_magnitude() and decay_rate() methods on DetailedCombatEvent
//!   - classify_combat_outcome() that identifies specific dramatic events from RoundResult
//!   - TensionTracker::observe() that classifies + updates boring_streak + injects spike
//!   - TurnClassification enum (Boring, Dramatic(DetailedCombatEvent))
//!
//! ACs: event types with spike data, classification function, observe integration,
//! boring_streak tracking, spike injection per event type

use sidequest_game::tension_tracker::{
    classify_combat_outcome, DamageEvent, DetailedCombatEvent, RoundResult, TensionTracker,
    TurnClassification,
};

// ============================================================================
// AC: DetailedCombatEvent enum has all 6 variants from ADR (epic context)
// ============================================================================

#[test]
fn detailed_combat_event_has_critical_hit() {
    let event = DetailedCombatEvent::CriticalHit;
    assert_eq!(format!("{:?}", event), "CriticalHit");
}

#[test]
fn detailed_combat_event_has_killing_blow() {
    let event = DetailedCombatEvent::KillingBlow;
    assert_eq!(format!("{:?}", event), "KillingBlow");
}

#[test]
fn detailed_combat_event_has_death_save() {
    let event = DetailedCombatEvent::DeathSave;
    assert_eq!(format!("{:?}", event), "DeathSave");
}

#[test]
fn detailed_combat_event_has_first_blood() {
    let event = DetailedCombatEvent::FirstBlood;
    assert_eq!(format!("{:?}", event), "FirstBlood");
}

#[test]
fn detailed_combat_event_has_near_miss() {
    let event = DetailedCombatEvent::NearMiss;
    assert_eq!(format!("{:?}", event), "NearMiss");
}

#[test]
fn detailed_combat_event_has_last_standing() {
    let event = DetailedCombatEvent::LastStanding;
    assert_eq!(format!("{:?}", event), "LastStanding");
}

#[test]
fn detailed_combat_event_is_copy_and_eq() {
    let a = DetailedCombatEvent::CriticalHit;
    let b = a; // Copy
    assert_eq!(a, b); // Eq
}

// ============================================================================
// AC: Each event type has spike magnitude and decay rate (from epic context)
// Epic specifies: CriticalHit 0.8/0.15, KillingBlow 1.0/0.20,
//   DeathSave 0.7/0.15, FirstBlood 0.6/0.10, NearMiss 0.5/0.10,
//   LastStanding 0.9/0.20
// ============================================================================

#[test]
fn critical_hit_spike_magnitude() {
    assert!((DetailedCombatEvent::CriticalHit.spike_magnitude() - 0.8).abs() < f64::EPSILON);
}

#[test]
fn killing_blow_spike_magnitude() {
    assert!((DetailedCombatEvent::KillingBlow.spike_magnitude() - 1.0).abs() < f64::EPSILON);
}

#[test]
fn death_save_spike_magnitude() {
    assert!((DetailedCombatEvent::DeathSave.spike_magnitude() - 0.7).abs() < f64::EPSILON);
}

#[test]
fn first_blood_spike_magnitude() {
    assert!((DetailedCombatEvent::FirstBlood.spike_magnitude() - 0.6).abs() < f64::EPSILON);
}

#[test]
fn near_miss_spike_magnitude() {
    assert!((DetailedCombatEvent::NearMiss.spike_magnitude() - 0.5).abs() < f64::EPSILON);
}

#[test]
fn last_standing_spike_magnitude() {
    assert!((DetailedCombatEvent::LastStanding.spike_magnitude() - 0.9).abs() < f64::EPSILON);
}

#[test]
fn critical_hit_decay_rate() {
    assert!((DetailedCombatEvent::CriticalHit.decay_rate() - 0.15).abs() < f64::EPSILON);
}

#[test]
fn killing_blow_decay_rate() {
    assert!((DetailedCombatEvent::KillingBlow.decay_rate() - 0.20).abs() < f64::EPSILON);
}

#[test]
fn last_standing_decay_rate() {
    assert!((DetailedCombatEvent::LastStanding.decay_rate() - 0.20).abs() < f64::EPSILON);
}

// ============================================================================
// AC: classify_combat_outcome identifies specific events from RoundResult
// ============================================================================

fn make_round(damage_events: Vec<DamageEvent>, effects: Vec<String>) -> RoundResult {
    RoundResult {
        round: 1,
        damage_events,
        effects_applied: effects,
        effects_expired: vec![],
    }
}

fn damage(attacker: &str, target: &str, amount: i32) -> DamageEvent {
    DamageEvent {
        attacker: attacker.to_string(),
        target: target.to_string(),
        damage: amount,
        round: 1,
    }
}

#[test]
fn classify_kill_as_killing_blow() {
    let round = make_round(vec![damage("hero", "goblin", 20)], vec![]);
    let result = classify_combat_outcome(&round, Some("goblin"), None);
    assert_eq!(
        result,
        TurnClassification::Dramatic(DetailedCombatEvent::KillingBlow)
    );
}

#[test]
fn classify_zero_damage_as_boring() {
    let round = make_round(vec![damage("hero", "goblin", 0)], vec![]);
    let result = classify_combat_outcome(&round, None, None);
    assert_eq!(result, TurnClassification::Boring);
}

#[test]
fn classify_miss_as_boring() {
    let round = make_round(vec![], vec![]);
    let result = classify_combat_outcome(&round, None, None);
    assert_eq!(result, TurnClassification::Boring);
}

#[test]
fn classify_high_damage_as_critical_hit() {
    // High damage without a kill — critical hit
    let round = make_round(vec![damage("hero", "dragon", 25)], vec![]);
    let result = classify_combat_outcome(&round, None, None);
    assert_eq!(
        result,
        TurnClassification::Dramatic(DetailedCombatEvent::CriticalHit)
    );
}

#[test]
fn classify_new_effects_as_dramatic() {
    // Status effects applied = dramatic (at minimum)
    let round = make_round(
        vec![damage("wizard", "orc", 5)],
        vec!["stunned".to_string()],
    );
    let result = classify_combat_outcome(&round, None, None);
    assert!(
        matches!(result, TurnClassification::Dramatic(_)),
        "New status effects should be dramatic"
    );
}

#[test]
fn classify_low_hp_target_as_near_miss() {
    // Target at low HP but not killed — near miss
    // The last parameter is the target's HP ratio (current/max)
    let round = make_round(vec![damage("goblin", "hero", 8)], vec![]);
    let result = classify_combat_outcome(&round, None, Some(0.1)); // hero at 10% HP
    assert_eq!(
        result,
        TurnClassification::Dramatic(DetailedCombatEvent::NearMiss)
    );
}

#[test]
fn classify_moderate_damage_as_normal() {
    // Some damage but not dramatic threshold
    let round = make_round(vec![damage("hero", "goblin", 5)], vec![]);
    let result = classify_combat_outcome(&round, None, None);
    assert_eq!(result, TurnClassification::Normal);
}

// ============================================================================
// AC: TurnClassification enum
// ============================================================================

#[test]
fn turn_classification_boring_exists() {
    let tc = TurnClassification::Boring;
    assert_eq!(format!("{:?}", tc), "Boring");
}

#[test]
fn turn_classification_dramatic_carries_event() {
    let tc = TurnClassification::Dramatic(DetailedCombatEvent::KillingBlow);
    if let TurnClassification::Dramatic(event) = tc {
        assert_eq!(event, DetailedCombatEvent::KillingBlow);
    } else {
        panic!("Expected Dramatic variant");
    }
}

#[test]
fn turn_classification_normal_exists() {
    let tc = TurnClassification::Normal;
    assert_eq!(format!("{:?}", tc), "Normal");
}

// ============================================================================
// AC: TensionTracker::observe() integrates classification + boring_streak + spike
// ============================================================================

#[test]
fn observe_boring_increments_boring_streak() {
    let mut tracker = TensionTracker::new();
    let round = make_round(vec![], vec![]);
    tracker.observe(&round, None, None);
    assert_eq!(tracker.boring_streak(), 1);
    tracker.observe(&round, None, None);
    assert_eq!(tracker.boring_streak(), 2);
}

#[test]
fn observe_dramatic_resets_boring_streak() {
    let mut tracker = TensionTracker::new();
    // Build up boring streak
    let boring_round = make_round(vec![], vec![]);
    tracker.observe(&boring_round, None, None);
    tracker.observe(&boring_round, None, None);
    assert_eq!(tracker.boring_streak(), 2);

    // Dramatic event resets it
    let kill_round = make_round(vec![damage("hero", "goblin", 20)], vec![]);
    tracker.observe(&kill_round, Some("goblin"), None);
    assert_eq!(tracker.boring_streak(), 0);
}

#[test]
fn observe_dramatic_injects_spike() {
    let mut tracker = TensionTracker::new();
    let kill_round = make_round(vec![damage("hero", "goblin", 20)], vec![]);
    tracker.observe(&kill_round, Some("goblin"), None);
    // KillingBlow spike = 1.0
    assert!(
        tracker.active_spike() > 0.0,
        "Dramatic event should inject a spike"
    );
}

#[test]
fn observe_killing_blow_injects_full_spike() {
    let mut tracker = TensionTracker::new();
    let kill_round = make_round(vec![damage("hero", "goblin", 20)], vec![]);
    tracker.observe(&kill_round, Some("goblin"), None);
    assert!(
        (tracker.active_spike() - 1.0).abs() < f64::EPSILON,
        "KillingBlow should inject spike of 1.0, got {}",
        tracker.active_spike()
    );
}

#[test]
fn observe_critical_hit_injects_proportional_spike() {
    let mut tracker = TensionTracker::new();
    let crit_round = make_round(vec![damage("hero", "dragon", 25)], vec![]);
    tracker.observe(&crit_round, None, None);
    assert!(
        (tracker.active_spike() - 0.8).abs() < f64::EPSILON,
        "CriticalHit should inject spike of 0.8, got {}",
        tracker.active_spike()
    );
}

#[test]
fn observe_boring_does_not_inject_spike() {
    let mut tracker = TensionTracker::new();
    let boring_round = make_round(vec![], vec![]);
    tracker.observe(&boring_round, None, None);
    assert!(
        (tracker.active_spike()).abs() < f64::EPSILON,
        "Boring turn should not inject spike"
    );
}

#[test]
fn observe_returns_classification() {
    let mut tracker = TensionTracker::new();
    let kill_round = make_round(vec![damage("hero", "goblin", 20)], vec![]);
    let classification = tracker.observe(&kill_round, Some("goblin"), None);
    assert_eq!(
        classification,
        TurnClassification::Dramatic(DetailedCombatEvent::KillingBlow)
    );
}

#[test]
fn observe_boring_returns_boring() {
    let mut tracker = TensionTracker::new();
    let boring_round = make_round(vec![], vec![]);
    let classification = tracker.observe(&boring_round, None, None);
    assert_eq!(classification, TurnClassification::Boring);
}

// ============================================================================
// AC: Priority ordering — kills beat crits beat effects beat damage
// ============================================================================

#[test]
fn kill_takes_priority_over_high_damage() {
    // Even with critical-level damage, if someone died it's a KillingBlow
    let round = make_round(vec![damage("hero", "goblin", 30)], vec![]);
    let result = classify_combat_outcome(&round, Some("goblin"), None);
    assert_eq!(
        result,
        TurnClassification::Dramatic(DetailedCombatEvent::KillingBlow)
    );
}

#[test]
fn kill_takes_priority_over_effects() {
    let round = make_round(
        vec![damage("hero", "goblin", 10)],
        vec!["poisoned".to_string()],
    );
    let result = classify_combat_outcome(&round, Some("goblin"), None);
    assert_eq!(
        result,
        TurnClassification::Dramatic(DetailedCombatEvent::KillingBlow)
    );
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn negative_damage_treated_as_zero() {
    // Healing or damage reduction — not dramatic
    let round = make_round(vec![damage("healer", "hero", -10)], vec![]);
    let result = classify_combat_outcome(&round, None, None);
    assert_eq!(
        result,
        TurnClassification::Boring,
        "Negative damage (healing) should be boring"
    );
}

#[test]
fn multiple_small_damage_events_sum_for_classification() {
    // Multiple small hits that sum to dramatic threshold
    let round = make_round(
        vec![damage("hero", "goblin", 8), damage("hero", "goblin", 8)],
        vec![],
    );
    let result = classify_combat_outcome(&round, None, None);
    // 16 total damage >= 15 threshold = CriticalHit
    assert_eq!(
        result,
        TurnClassification::Dramatic(DetailedCombatEvent::CriticalHit)
    );
}

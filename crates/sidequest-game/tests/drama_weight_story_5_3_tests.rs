//! Story 5-3: Drama weight computation tests
//!
//! RED phase — tests assert new behavioral properties that don't hold yet:
//!   - drama_weight() = max(action, stakes, effective_spike) — currently additive
//!   - Per-event linear decay — currently exponential with flat SPIKE_DECAY
//!   - Spike replacement (new replaces old) — currently additive
//!   - Spike aging per observe() call — currently in tick()
//!   - EventSpike struct with magnitude + decay_rate — doesn't exist yet
//!
//! ACs from story context: spike injection, linear decay, spike cleanup,
//! spike replacement, drama_weight as max, clamped output, full observe flow

use sidequest_game::combat::{DamageEvent, RoundResult};
use sidequest_game::tension_tracker::{
    classify_combat_outcome, DetailedCombatEvent, TensionTracker, TurnClassification,
};

// ============================================================================
// Helpers
// ============================================================================

/// Build a RoundResult with a single high-damage hit (triggers CriticalHit classification).
fn critical_hit_round() -> RoundResult {
    RoundResult {
        round: 1,
        damage_events: vec![DamageEvent {
            attacker: "hero".into(),
            target: "goblin".into(),
            damage: 20, // above DRAMATIC_DAMAGE_THRESHOLD (15)
            round: 1,
        }],
        effects_applied: vec![],
        effects_expired: vec![],
    }
}

/// Build a RoundResult with a kill (triggers KillingBlow classification).
fn killing_blow_round() -> RoundResult {
    RoundResult {
        round: 1,
        damage_events: vec![DamageEvent {
            attacker: "hero".into(),
            target: "goblin".into(),
            damage: 10,
            round: 1,
        }],
        effects_applied: vec![],
        effects_expired: vec![],
    }
}

/// Build a boring RoundResult (zero damage, no effects).
fn boring_round() -> RoundResult {
    RoundResult {
        round: 1,
        damage_events: vec![],
        effects_applied: vec![],
        effects_expired: vec![],
    }
}

/// Build a RoundResult with low damage and a near-miss HP ratio.
fn near_miss_round() -> RoundResult {
    RoundResult {
        round: 1,
        damage_events: vec![DamageEvent {
            attacker: "hero".into(),
            target: "goblin".into(),
            damage: 5,
            round: 1,
        }],
        effects_applied: vec![],
        effects_expired: vec![],
    }
}

// ============================================================================
// AC: drama_weight() = max(action, stakes, effective_spike) — NOT additive
// ============================================================================

#[test]
fn drama_weight_is_max_of_three_tracks_not_additive() {
    // With action=0.5, stakes=0.3, spike from CriticalHit (0.8):
    // Expected: max(0.5, 0.3, 0.8) = 0.8
    // Current (additive): clamp01(max(0.5, 0.3) + 0.8) = 1.0
    let mut tracker = TensionTracker::with_values(0.5, 0.3);
    // Inject a CriticalHit-magnitude spike directly
    tracker.inject_spike(0.8);
    assert!(
        (tracker.drama_weight() - 0.8).abs() < 0.01,
        "drama_weight should be max(action=0.5, stakes=0.3, spike=0.8) = 0.8, got {}",
        tracker.drama_weight(),
    );
}

#[test]
fn drama_weight_returns_action_when_highest() {
    let mut tracker = TensionTracker::with_values(0.9, 0.2);
    // No spike — drama_weight should be action_tension
    assert!(
        (tracker.drama_weight() - 0.9).abs() < 0.01,
        "drama_weight should be max(0.9, 0.2, 0.0) = 0.9, got {}",
        tracker.drama_weight(),
    );
}

#[test]
fn drama_weight_returns_stakes_when_highest() {
    let mut tracker = TensionTracker::with_values(0.2, 0.85);
    assert!(
        (tracker.drama_weight() - 0.85).abs() < 0.01,
        "drama_weight should be max(0.2, 0.85, 0.0) = 0.85, got {}",
        tracker.drama_weight(),
    );
}

#[test]
fn drama_weight_returns_spike_when_highest() {
    let mut tracker = TensionTracker::with_values(0.1, 0.2);
    // Simulate a KillingBlow spike via observe (kill=Some)
    let round = killing_blow_round();
    tracker.observe(&round, Some("goblin"), None);
    // KillingBlow spike = 1.0 — should dominate
    assert!(
        (tracker.drama_weight() - 1.0).abs() < 0.01,
        "fresh KillingBlow spike (1.0) should override both tracks, got {}",
        tracker.drama_weight(),
    );
}

// ============================================================================
// AC: Spike injection — Dramatic event sets spike with correct magnitude
// ============================================================================

#[test]
fn observe_critical_hit_injects_spike_at_correct_magnitude() {
    let mut tracker = TensionTracker::new();
    let round = critical_hit_round();
    let classification = tracker.observe(&round, None, None);
    assert_eq!(
        classification,
        TurnClassification::Dramatic(DetailedCombatEvent::CriticalHit),
    );
    // CriticalHit spike magnitude = 0.8
    assert!(
        (tracker.active_spike() - 0.8).abs() < 0.01,
        "CriticalHit should inject spike of 0.8, got {}",
        tracker.active_spike(),
    );
}

#[test]
fn observe_killing_blow_injects_spike_at_correct_magnitude() {
    let mut tracker = TensionTracker::new();
    let round = killing_blow_round();
    tracker.observe(&round, Some("goblin"), None);
    // KillingBlow spike magnitude = 1.0
    assert!(
        (tracker.active_spike() - 1.0).abs() < 0.01,
        "KillingBlow should inject spike of 1.0, got {}",
        tracker.active_spike(),
    );
}

#[test]
fn observe_near_miss_injects_spike_at_correct_magnitude() {
    let mut tracker = TensionTracker::new();
    let round = near_miss_round();
    // lowest_hp_ratio <= 0.2 triggers NearMiss
    tracker.observe(&round, None, Some(0.15));
    // NearMiss spike magnitude = 0.5
    assert!(
        (tracker.active_spike() - 0.5).abs() < 0.01,
        "NearMiss should inject spike of 0.5, got {}",
        tracker.active_spike(),
    );
}

// ============================================================================
// AC: Spike decay — per-event LINEAR decay, not exponential
// ============================================================================

#[test]
fn critical_hit_spike_decays_linearly_per_observe() {
    let mut tracker = TensionTracker::new();
    // Inject CriticalHit via observe
    let crit_round = critical_hit_round();
    tracker.observe(&crit_round, None, None);

    // CriticalHit: magnitude=0.8, decay_rate=0.15/turn
    // Turn 0 (just injected): effective_spike = 0.8
    assert!(
        (tracker.active_spike() - 0.8).abs() < 0.01,
        "turn 0: spike should be 0.8, got {}",
        tracker.active_spike(),
    );

    // Turn 1: observe boring → spike ages → effective_spike = 0.8 - 0.15*1 = 0.65
    let boring = boring_round();
    tracker.observe(&boring, None, None);
    assert!(
        (tracker.active_spike() - 0.65).abs() < 0.01,
        "turn 1: spike should be 0.65 (0.8 - 0.15), got {}",
        tracker.active_spike(),
    );

    // Turn 2: observe boring → effective_spike = 0.8 - 0.15*2 = 0.50
    tracker.observe(&boring, None, None);
    assert!(
        (tracker.active_spike() - 0.50).abs() < 0.01,
        "turn 2: spike should be 0.50 (0.8 - 0.30), got {}",
        tracker.active_spike(),
    );

    // Turn 3: 0.8 - 0.15*3 = 0.35
    tracker.observe(&boring, None, None);
    assert!(
        (tracker.active_spike() - 0.35).abs() < 0.01,
        "turn 3: spike should be 0.35 (0.8 - 0.45), got {}",
        tracker.active_spike(),
    );
}

#[test]
fn killing_blow_spike_decay_curve() {
    let mut tracker = TensionTracker::new();
    let kill_round = killing_blow_round();
    tracker.observe(&kill_round, Some("goblin"), None);

    // KillingBlow: magnitude=1.0, decay_rate=0.20/turn
    assert!(
        (tracker.active_spike() - 1.0).abs() < 0.01,
        "turn 0: KillingBlow spike should be 1.0, got {}",
        tracker.active_spike(),
    );

    let boring = boring_round();

    // Turn 1: 1.0 - 0.20*1 = 0.80
    tracker.observe(&boring, None, None);
    assert!(
        (tracker.active_spike() - 0.80).abs() < 0.01,
        "turn 1: spike should be 0.80, got {}",
        tracker.active_spike(),
    );

    // Turn 2: 1.0 - 0.20*2 = 0.60
    tracker.observe(&boring, None, None);
    assert!(
        (tracker.active_spike() - 0.60).abs() < 0.01,
        "turn 2: spike should be 0.60, got {}",
        tracker.active_spike(),
    );

    // Turn 3: 0.40
    tracker.observe(&boring, None, None);
    assert!(
        (tracker.active_spike() - 0.40).abs() < 0.01,
        "turn 3: spike should be 0.40, got {}",
        tracker.active_spike(),
    );

    // Turn 4: 0.20
    tracker.observe(&boring, None, None);
    assert!(
        (tracker.active_spike() - 0.20).abs() < 0.01,
        "turn 4: spike should be 0.20, got {}",
        tracker.active_spike(),
    );

    // Turn 5: 1.0 - 0.20*5 = 0.00 (fully decayed)
    tracker.observe(&boring, None, None);
    assert!(
        tracker.active_spike() < 0.01,
        "turn 5: spike should be fully decayed (0.0), got {}",
        tracker.active_spike(),
    );
}

// ============================================================================
// AC: Spike cleanup — fully decayed spike is removed
// ============================================================================

#[test]
fn spike_cleaned_up_after_full_decay() {
    let mut tracker = TensionTracker::new();
    let kill_round = killing_blow_round();
    tracker.observe(&kill_round, Some("goblin"), None);

    let boring = boring_round();
    // KillingBlow decays in 5 turns at 0.20/turn
    for _ in 0..6 {
        tracker.observe(&boring, None, None);
    }

    // After full decay, spike should be exactly 0.0
    assert_eq!(
        tracker.active_spike(),
        0.0,
        "fully decayed spike should be cleaned up to exactly 0.0",
    );
}

#[test]
fn drama_weight_falls_back_after_spike_decays() {
    let mut tracker = TensionTracker::with_values(0.4, 0.3);
    let kill_round = killing_blow_round();
    tracker.observe(&kill_round, Some("goblin"), None);

    // Initially spike dominates: drama_weight = 1.0
    assert!(
        tracker.drama_weight() > 0.9,
        "fresh KillingBlow should dominate, got {}",
        tracker.drama_weight(),
    );

    let boring = boring_round();
    // Decay the spike fully (6+ boring turns)
    for _ in 0..7 {
        tracker.observe(&boring, None, None);
    }

    // After spike decays, drama_weight should fall back to max(action, stakes)
    // Action tension will have changed due to boring ramp, but spike contribution = 0
    assert!(
        tracker.active_spike() < 0.01,
        "spike should be fully decayed, got {}",
        tracker.active_spike(),
    );
    // drama_weight should equal max(action_tension, stakes_tension) — no spike boost
    let expected = tracker.action_tension().max(tracker.stakes_tension());
    assert!(
        (tracker.drama_weight() - expected).abs() < 0.01,
        "drama_weight should equal max(action, stakes) after spike decay, got {} vs expected {}",
        tracker.drama_weight(),
        expected,
    );
}

// ============================================================================
// AC: Spike replacement — new spike replaces existing, resets decay age
// ============================================================================

#[test]
fn new_spike_replaces_existing_not_additive() {
    let mut tracker = TensionTracker::new();

    // First: inject NearMiss spike (0.5)
    let nm_round = near_miss_round();
    tracker.observe(&nm_round, None, Some(0.15));
    assert!(
        (tracker.active_spike() - 0.5).abs() < 0.01,
        "NearMiss spike should be 0.5, got {}",
        tracker.active_spike(),
    );

    // Second: inject CriticalHit spike (0.8) — should REPLACE, not add
    let crit_round = critical_hit_round();
    tracker.observe(&crit_round, None, None);
    assert!(
        (tracker.active_spike() - 0.8).abs() < 0.01,
        "CriticalHit should replace NearMiss (0.8, not 1.3), got {}",
        tracker.active_spike(),
    );
}

#[test]
fn spike_replacement_resets_decay_age() {
    let mut tracker = TensionTracker::new();

    // Inject CriticalHit, age it 3 turns
    let crit_round = critical_hit_round();
    tracker.observe(&crit_round, None, None);
    let boring = boring_round();
    for _ in 0..3 {
        tracker.observe(&boring, None, None);
    }
    // After 3 ages: 0.8 - 0.15*3 = 0.35
    assert!(
        (tracker.active_spike() - 0.35).abs() < 0.01,
        "after 3 boring turns, CriticalHit should be 0.35, got {}",
        tracker.active_spike(),
    );

    // Now inject a new KillingBlow — should start fresh at 1.0
    let kill_round = killing_blow_round();
    tracker.observe(&kill_round, Some("goblin"), None);
    assert!(
        (tracker.active_spike() - 1.0).abs() < 0.01,
        "new KillingBlow should replace aged CriticalHit at full magnitude 1.0, got {}",
        tracker.active_spike(),
    );
}

// ============================================================================
// AC: Full observe flow — ages spike, updates stakes, classifies, updates action
// ============================================================================

#[test]
fn observe_ages_existing_spike_before_classification() {
    let mut tracker = TensionTracker::new();

    // Inject KillingBlow spike
    let kill_round = killing_blow_round();
    tracker.observe(&kill_round, Some("goblin"), None);
    assert!(
        (tracker.active_spike() - 1.0).abs() < 0.01,
        "should have KillingBlow spike of 1.0",
    );

    // Observe a boring round — spike should be aged FIRST (before classification)
    // KillingBlow decay: 1.0 - 0.20*1 = 0.80
    let boring = boring_round();
    tracker.observe(&boring, None, None);
    assert!(
        (tracker.active_spike() - 0.80).abs() < 0.01,
        "observe should age spike before classification: expected 0.80, got {}",
        tracker.active_spike(),
    );
}

#[test]
fn observe_updates_stakes_tension() {
    let mut tracker = TensionTracker::new();
    // HP-based stakes: 40/100 → stakes = 0.6
    tracker.update_stakes(40, 100);
    assert!(
        (tracker.stakes_tension() - 0.6).abs() < 0.01,
        "stakes tension should be 0.6 for 40% HP, got {}",
        tracker.stakes_tension(),
    );
}

// ============================================================================
// AC: Combined scenarios from story context
// ============================================================================

#[test]
fn combined_scenario_boring_streak_and_hp() {
    // Context AC: boring_streak=5 (action=0.85) + HP at 60% (stakes=0.3) → drama_weight=0.85
    let mut tracker = TensionTracker::new();
    // Build boring streak to 5
    let boring = boring_round();
    for _ in 0..5 {
        tracker.observe(&boring, None, None);
    }
    // Set stakes: 60% HP → stakes = 0.4
    tracker.update_stakes(60, 100);

    // drama_weight should be max(action_tension, 0.4, 0.0)
    // Action tension after 5 boring rounds dominates
    let dw = tracker.drama_weight();
    assert!(
        dw >= 0.4,
        "drama_weight after 5 boring + 60% HP should be at least 0.4, got {}",
        dw,
    );
    assert!(
        dw == tracker.action_tension().max(tracker.stakes_tension()),
        "drama_weight should be max(action={}, stakes={}) = {}, got {}",
        tracker.action_tension(),
        tracker.stakes_tension(),
        tracker.action_tension().max(tracker.stakes_tension()),
        dw,
    );
}

#[test]
fn fresh_killing_blow_overrides_both_tracks() {
    // Context AC: fresh KillingBlow spike (1.0) overrides both tracks
    let mut tracker = TensionTracker::with_values(0.5, 0.7);
    let kill_round = killing_blow_round();
    tracker.observe(&kill_round, Some("goblin"), None);
    assert!(
        (tracker.drama_weight() - 1.0).abs() < 0.01,
        "KillingBlow (1.0) should override action=0.5 and stakes=0.7, got {}",
        tracker.drama_weight(),
    );
}

// ============================================================================
// AC: Clamped output — drama_weight always in 0.0–1.0
// ============================================================================

#[test]
fn drama_weight_never_exceeds_one_with_spike_and_tension() {
    // With high tension on both tracks + spike, result must be clamped to 1.0
    let mut tracker = TensionTracker::with_values(0.9, 0.9);
    let kill_round = killing_blow_round();
    tracker.observe(&kill_round, Some("goblin"), None);
    assert!(
        tracker.drama_weight() <= 1.0,
        "drama_weight must be <= 1.0 even with high tension + spike, got {}",
        tracker.drama_weight(),
    );
    assert!(
        tracker.drama_weight() >= 0.0,
        "drama_weight must be >= 0.0",
    );
}

#[test]
fn drama_weight_at_zero_with_no_activity() {
    let tracker = TensionTracker::new();
    assert_eq!(
        tracker.drama_weight(),
        0.0,
        "fresh tracker should have drama_weight = 0.0",
    );
}

// ============================================================================
// AC: Per-event decay rates — each event type decays differently
// ============================================================================

#[test]
fn near_miss_decays_slower_than_killing_blow() {
    // NearMiss: magnitude=0.5, decay=0.10/turn → lasts 5 turns
    // KillingBlow: magnitude=1.0, decay=0.20/turn → lasts 5 turns
    // After 3 turns: NearMiss=0.5-0.3=0.2, KillingBlow=1.0-0.6=0.4
    // Proportion remaining: NearMiss=40%, KillingBlow=40% — same rate proportionally
    // But absolute decay rate is different (0.10 vs 0.20 per turn)

    let mut tracker_nm = TensionTracker::new();
    let nm_round = near_miss_round();
    tracker_nm.observe(&nm_round, None, Some(0.15));

    let mut tracker_kb = TensionTracker::new();
    let kill_round = killing_blow_round();
    tracker_kb.observe(&kill_round, Some("goblin"), None);

    let boring = boring_round();

    // After 1 turn:
    tracker_nm.observe(&boring, None, None);
    tracker_kb.observe(&boring, None, None);

    let nm_after_1 = tracker_nm.active_spike();
    let kb_after_1 = tracker_kb.active_spike();

    // NearMiss after 1 turn: 0.5 - 0.10 = 0.40
    assert!(
        (nm_after_1 - 0.40).abs() < 0.01,
        "NearMiss after 1 turn should be 0.40, got {}",
        nm_after_1,
    );

    // KillingBlow after 1 turn: 1.0 - 0.20 = 0.80
    assert!(
        (kb_after_1 - 0.80).abs() < 0.01,
        "KillingBlow after 1 turn should be 0.80, got {}",
        kb_after_1,
    );

    // Absolute decay differs: KB dropped 0.20, NM dropped 0.10
    let nm_drop = 0.5 - nm_after_1;
    let kb_drop = 1.0 - kb_after_1;
    assert!(
        (kb_drop - nm_drop).abs() > 0.05,
        "different events should have different absolute decay rates: NM dropped {}, KB dropped {}",
        nm_drop,
        kb_drop,
    );
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn boring_observe_with_no_existing_spike() {
    // Observing a boring round with no spike should not panic or inject anything
    let mut tracker = TensionTracker::new();
    let boring = boring_round();
    tracker.observe(&boring, None, None);
    assert_eq!(
        tracker.active_spike(),
        0.0,
        "boring observe with no spike should leave spike at 0.0",
    );
}

#[test]
fn multiple_dramatic_events_in_sequence() {
    // Rapid dramatic events: each replaces the previous spike
    let mut tracker = TensionTracker::new();
    let boring = boring_round();

    // NearMiss (0.5)
    let nm = near_miss_round();
    tracker.observe(&nm, None, Some(0.15));
    assert!(
        (tracker.active_spike() - 0.5).abs() < 0.01,
        "NearMiss spike should be 0.5",
    );

    // One boring turn — ages NearMiss
    tracker.observe(&boring, None, None);

    // CriticalHit (0.8) — replaces aged NearMiss
    let crit = critical_hit_round();
    tracker.observe(&crit, None, None);
    assert!(
        (tracker.active_spike() - 0.8).abs() < 0.01,
        "CriticalHit should replace aged NearMiss at 0.8, got {}",
        tracker.active_spike(),
    );

    // KillingBlow (1.0) — replaces CriticalHit
    let kill = killing_blow_round();
    tracker.observe(&kill, Some("goblin"), None);
    assert!(
        (tracker.active_spike() - 1.0).abs() < 0.01,
        "KillingBlow should replace CriticalHit at 1.0, got {}",
        tracker.active_spike(),
    );
}

#[test]
fn drama_weight_tracks_spike_decay_correctly() {
    // End-to-end: inject spike, verify drama_weight follows the decay curve
    let mut tracker = TensionTracker::with_values(0.1, 0.1);
    let crit = critical_hit_round();
    tracker.observe(&crit, None, None);

    // Turn 0: drama_weight = max(action, 0.1, 0.8) = 0.8
    assert!(
        (tracker.drama_weight() - 0.8).abs() < 0.05,
        "turn 0: drama_weight should be ~0.8 (spike dominates), got {}",
        tracker.drama_weight(),
    );

    let boring = boring_round();

    // Turn 1: spike = 0.65, drama_weight = max(action, 0.1, 0.65) ≈ 0.65
    tracker.observe(&boring, None, None);
    assert!(
        (tracker.drama_weight() - 0.65).abs() < 0.1,
        "turn 1: drama_weight should be ~0.65 (decayed spike), got {}",
        tracker.drama_weight(),
    );

    // Turn 5: spike = 0.8 - 0.15*5 = 0.05
    for _ in 0..4 {
        tracker.observe(&boring, None, None);
    }
    // By turn 5, spike is nearly gone. drama_weight should reflect base tensions
    assert!(
        tracker.active_spike() < 0.1,
        "turn 5: spike should be nearly decayed, got {}",
        tracker.active_spike(),
    );
}

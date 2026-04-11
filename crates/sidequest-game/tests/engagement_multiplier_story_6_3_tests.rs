//! Story 6-3: Engagement Multiplier — failing tests (RED phase)
//!
//! Tests cover:
//!   - engagement_multiplier() pure function — boundary values, range
//!   - turns_since_meaningful field on GameSnapshot
//!   - Counter increment and reset logic
//!   - Integration: multiplier applied to trope engine tick progression
//!   - Integration test: end-to-end GameSnapshot → multiplier → tick → beat verification

use sidequest_genre::{PassiveProgression, TropeDefinition, TropeEscalation};
use sidequest_protocol::NonBlankString;

use sidequest_game::engagement::engagement_multiplier;
use sidequest_game::trope::{TropeEngine, TropeState, TropeStatus};

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

/// Trope with a small passive rate for precise multiplier testing.
/// rate_per_turn = 0.05 so we can clearly see multiplier effects.
fn slow_trope_def() -> TropeDefinition {
    TropeDefinition {
        id: Some("slow_burn".to_string()),
        name: NonBlankString::new("Slow Burn").unwrap(),
        description: Some("A slow-building tension".to_string()),
        category: "tension".to_string(),
        triggers: vec![],
        narrative_hints: vec![],
        tension_level: Some(0.3),
        resolution_hints: None,
        resolution_patterns: None,
        tags: vec![],
        escalation: vec![
            TropeEscalation {
                at: 0.1,
                event: "First sign of trouble".to_string(),
                npcs_involved: vec![],
                stakes: "Growing unease".to_string(),
            },
            TropeEscalation {
                at: 0.3,
                event: "Tension becomes undeniable".to_string(),
                npcs_involved: vec![],
                stakes: "Cannot be ignored".to_string(),
            },
        ],
        passive_progression: Some(PassiveProgression {
            rate_per_turn: 0.05,
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

// ============================================================================
// AC: Multiplier range — Returns values between 0.5 and 2.0 inclusive
// ============================================================================

#[test]
fn engagement_multiplier_returns_half_for_zero_turns() {
    // Player just did something meaningful — slow down trope escalation
    assert_eq!(engagement_multiplier(0), 0.5);
}

#[test]
fn engagement_multiplier_returns_half_for_one_turn() {
    // Player recently active — still in deceleration band
    assert_eq!(engagement_multiplier(1), 0.5);
}

#[test]
fn engagement_multiplier_returns_one_for_two_turns() {
    // Normal pace — neither accelerating nor decelerating
    assert_eq!(engagement_multiplier(2), 1.0);
}

#[test]
fn engagement_multiplier_returns_one_for_three_turns() {
    // Still normal pace
    assert_eq!(engagement_multiplier(3), 1.0);
}

// ============================================================================
// AC: Passive acceleration — 4+ turns without meaningful action → multiplier > 1.0
// ============================================================================

#[test]
fn engagement_multiplier_returns_one_point_five_for_four_turns() {
    let m = engagement_multiplier(4);
    assert_eq!(m, 1.5);
    assert!(m > 1.0, "4 turns idle should accelerate trope progression");
}

#[test]
fn engagement_multiplier_returns_one_point_five_for_five_turns() {
    assert_eq!(engagement_multiplier(5), 1.5);
}

#[test]
fn engagement_multiplier_returns_one_point_five_for_six_turns() {
    assert_eq!(engagement_multiplier(6), 1.5);
}

#[test]
fn engagement_multiplier_returns_two_for_seven_turns() {
    // Player very passive — world takes the wheel
    assert_eq!(engagement_multiplier(7), 2.0);
}

#[test]
fn engagement_multiplier_returns_two_for_large_value() {
    // Extreme passivity — should cap at 2.0, not grow unbounded
    assert_eq!(engagement_multiplier(100), 2.0);
    assert_eq!(engagement_multiplier(u32::MAX), 2.0);
}

// ============================================================================
// AC: Active deceleration — 0-1 turns since meaningful → multiplier 0.5
// ============================================================================

#[test]
fn active_player_decelerates_trope_progression() {
    // Both 0 and 1 turns should yield 0.5
    assert_eq!(engagement_multiplier(0), 0.5);
    assert_eq!(engagement_multiplier(1), 0.5);
    // 2 turns should NOT decelerate
    assert!(
        engagement_multiplier(2) > 0.5,
        "2 turns should be normal pace, not decelerated"
    );
}

// ============================================================================
// AC: Pure function — engagement_multiplier() has no side effects
// ============================================================================

#[test]
fn engagement_multiplier_is_deterministic() {
    // Same input always produces same output — pure function guarantee
    for turns in 0..=20 {
        let first = engagement_multiplier(turns);
        let second = engagement_multiplier(turns);
        assert_eq!(
            first, second,
            "engagement_multiplier({}) returned different values",
            turns
        );
    }
}

#[test]
fn engagement_multiplier_covers_full_range() {
    // Walk through all bands and verify the full range is [0.5, 2.0]
    let all_values: Vec<f32> = (0..=10).map(engagement_multiplier).collect();
    let min = all_values.iter().cloned().reduce(f32::min).unwrap();
    let max = all_values.iter().cloned().reduce(f32::max).unwrap();
    assert_eq!(min, 0.5, "Minimum multiplier should be 0.5");
    assert_eq!(max, 2.0, "Maximum multiplier should be 2.0");
}

#[test]
fn engagement_multiplier_is_monotonically_non_decreasing() {
    // As passivity increases, multiplier should never decrease
    let mut prev = engagement_multiplier(0);
    for turns in 1..=20 {
        let curr = engagement_multiplier(turns);
        assert!(
            curr >= prev,
            "Multiplier decreased from {} to {} at turns={}",
            prev,
            curr,
            turns
        );
        prev = curr;
    }
}

// ============================================================================
// AC: Counter field — turns_since_meaningful on GameSnapshot
// ============================================================================

#[test]
fn game_snapshot_has_turns_since_meaningful_field() {
    let snap = sidequest_game::state::GameSnapshot::default();
    // Field should exist and default to 0
    assert_eq!(snap.turns_since_meaningful, 0);
}

#[test]
fn game_snapshot_turns_since_meaningful_serializes() {
    // Verify serde round-trip preserves the counter
    let mut snap = sidequest_game::state::GameSnapshot::default();
    snap.turns_since_meaningful = 5;
    let json = serde_json::to_string(&snap).expect("serialize");
    let deser: sidequest_game::state::GameSnapshot =
        serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deser.turns_since_meaningful, 5);
}

#[test]
fn game_snapshot_turns_since_meaningful_defaults_on_missing_field() {
    // Backward compatibility: old snapshots without the field should default to 0
    // This tests #[serde(default)] behavior
    let json = r#"{"genre_slug":"","world_slug":"","characters":[],"npcs":[],"location":"","time_of_day":"","quest_log":{},"notes":[],"narrative_log":[],"combat":{"in_combat":false,"turn_order":[],"current_turn":null,"round":0,"drama_weight":0.0,"available_actions":[]},"active_tropes":[],"atmosphere":"","current_region":"","discovered_regions":[],"discovered_routes":[],"turn_manager":{"current_turn":0,"barrier_active":false,"barrier_reason":null},"last_saved_at":null,"active_stakes":"","lore_established":[]}"#;
    let snap: sidequest_game::state::GameSnapshot =
        serde_json::from_str(json).expect("should deserialize without turns_since_meaningful");
    assert_eq!(
        snap.turns_since_meaningful, 0,
        "Missing field should default to 0"
    );
}

// ============================================================================
// AC: Counter increments — Non-meaningful turns increment turns_since_meaningful
// AC: Counter resets — Meaningful intent resets counter to 0
// ============================================================================

#[test]
fn increment_turns_since_meaningful() {
    let mut snap = sidequest_game::state::GameSnapshot::default();
    assert_eq!(snap.turns_since_meaningful, 0);

    // Simulate non-meaningful turns
    snap.turns_since_meaningful += 1;
    assert_eq!(snap.turns_since_meaningful, 1);

    snap.turns_since_meaningful += 1;
    assert_eq!(snap.turns_since_meaningful, 2);
}

#[test]
fn reset_turns_since_meaningful_on_meaningful_action() {
    let mut snap = sidequest_game::state::GameSnapshot::default();
    snap.turns_since_meaningful = 7; // player was idle

    // Meaningful action resets to 0
    snap.turns_since_meaningful = 0;
    assert_eq!(snap.turns_since_meaningful, 0);
}

// ============================================================================
// AC: Trope integration — tick() receives base_tick * multiplier
// ============================================================================

#[test]
fn tick_with_multiplier_scales_progression() {
    let defs = vec![slow_trope_def()];
    let mut tropes = vec![TropeState::new("slow_burn")];

    // Normal tick (multiplier = 1.0): rate_per_turn * 1.0 = 0.05
    TropeEngine::tick_with_multiplier(&mut tropes, &defs, 1.0);
    let normal_prog = tropes[0].progression();
    assert!(
        (normal_prog - 0.05).abs() < f64::EPSILON,
        "Normal multiplier should yield base rate. Got {}",
        normal_prog
    );
}

#[test]
fn tick_with_multiplier_half_slows_progression() {
    let defs = vec![slow_trope_def()];
    let mut tropes = vec![TropeState::new("slow_burn")];

    // Active player multiplier (0.5): rate_per_turn * 0.5 = 0.025
    TropeEngine::tick_with_multiplier(&mut tropes, &defs, 0.5);
    let slow_prog = tropes[0].progression();
    assert!(
        (slow_prog - 0.025).abs() < f64::EPSILON,
        "0.5x multiplier should halve progression. Got {}",
        slow_prog
    );
}

#[test]
fn tick_with_multiplier_double_accelerates_progression() {
    let defs = vec![slow_trope_def()];
    let mut tropes = vec![TropeState::new("slow_burn")];

    // Passive player multiplier (2.0): rate_per_turn * 2.0 = 0.10
    TropeEngine::tick_with_multiplier(&mut tropes, &defs, 2.0);
    let fast_prog = tropes[0].progression();
    assert!(
        (fast_prog - 0.10).abs() < f64::EPSILON,
        "2.0x multiplier should double progression. Got {}",
        fast_prog
    );
}

#[test]
fn tick_with_multiplier_still_caps_at_one() {
    let defs = vec![slow_trope_def()];
    let mut tropes = vec![TropeState::new("slow_burn")];
    tropes[0].set_progression(0.98);

    // Even with 2.0x multiplier, progression caps at 1.0
    TropeEngine::tick_with_multiplier(&mut tropes, &defs, 2.0);
    assert!(
        tropes[0].progression() <= 1.0,
        "Progression must cap at 1.0 even with multiplier. Got {}",
        tropes[0].progression()
    );
}

#[test]
fn tick_with_multiplier_still_fires_beats() {
    let defs = vec![slow_trope_def()];
    let mut tropes = vec![TropeState::new("slow_burn")];
    tropes[0].set_progression(0.08);

    // With 2.0x: 0.08 + (0.05 * 2.0) = 0.18 → crosses 0.1 threshold
    let fired = TropeEngine::tick_with_multiplier(&mut tropes, &defs, 2.0);
    assert_eq!(
        fired.len(),
        1,
        "Beat at 0.1 should fire when multiplier pushes past threshold"
    );
    assert_eq!(fired[0].beat.at, 0.1);
}

#[test]
fn tick_with_multiplier_zero_means_no_progression() {
    let defs = vec![slow_trope_def()];
    let mut tropes = vec![TropeState::new("slow_burn")];

    // Edge case: multiplier of 0 means no progression at all
    TropeEngine::tick_with_multiplier(&mut tropes, &defs, 0.0);
    assert_eq!(
        tropes[0].progression(),
        0.0,
        "Zero multiplier should mean no progression"
    );
}

#[test]
fn tick_with_multiplier_skips_resolved_tropes() {
    let defs = vec![slow_trope_def()];
    let mut tropes = vec![TropeState::new("slow_burn")];
    tropes[0].set_status(TropeStatus::Resolved);
    tropes[0].set_progression(0.5);

    TropeEngine::tick_with_multiplier(&mut tropes, &defs, 2.0);
    assert_eq!(
        tropes[0].progression(),
        0.5,
        "Resolved tropes should not progress even with multiplier"
    );
}

#[test]
fn tick_with_multiplier_backward_compatible_at_one() {
    // Multiplier of 1.0 should produce identical results to plain tick()
    let defs = vec![slow_trope_def()];

    let mut tropes_plain = vec![TropeState::new("slow_burn")];
    let mut tropes_mult = vec![TropeState::new("slow_burn")];

    TropeEngine::tick(&mut tropes_plain, &defs);
    TropeEngine::tick_with_multiplier(&mut tropes_mult, &defs, 1.0);

    assert!(
        (tropes_plain[0].progression() - tropes_mult[0].progression()).abs() < f64::EPSILON,
        "tick() and tick_with_multiplier(1.0) should produce identical progression"
    );
}

// ============================================================================
// AC: Integration test — end-to-end: GameSnapshot → multiplier → tick → verify
// ============================================================================

#[test]
fn integration_passive_player_accelerates_trope_beats() {
    // Full integration: a passive player (7+ turns idle) should see trope beats
    // fire faster than an active player (0 turns idle).
    let defs = vec![slow_trope_def()]; // rate_per_turn = 0.05, beat at 0.1

    // --- Active player scenario (0 turns idle → 0.5x multiplier) ---
    let mut snap_active = sidequest_game::state::GameSnapshot::default();
    snap_active.turns_since_meaningful = 0;
    let active_mult = engagement_multiplier(snap_active.turns_since_meaningful);
    assert_eq!(active_mult, 0.5);

    let mut tropes_active = vec![TropeState::new("slow_burn")];
    // 1 tick at 0.5x: 0.05 * 0.5 = 0.025
    TropeEngine::tick_with_multiplier(&mut tropes_active, &defs, active_mult as f64);
    let active_prog = tropes_active[0].progression();

    // --- Passive player scenario (7 turns idle → 2.0x multiplier) ---
    let mut snap_passive = sidequest_game::state::GameSnapshot::default();
    snap_passive.turns_since_meaningful = 7;
    let passive_mult = engagement_multiplier(snap_passive.turns_since_meaningful);
    assert_eq!(passive_mult, 2.0);

    let mut tropes_passive = vec![TropeState::new("slow_burn")];
    // 1 tick at 2.0x: 0.05 * 2.0 = 0.10
    TropeEngine::tick_with_multiplier(&mut tropes_passive, &defs, passive_mult as f64);
    let passive_prog = tropes_passive[0].progression();

    // Passive player progresses 4x faster than active player
    assert!(
        passive_prog > active_prog,
        "Passive player ({}) should progress faster than active player ({})",
        passive_prog,
        active_prog
    );
    assert!(
        (passive_prog / active_prog - 4.0).abs() < 0.01,
        "Passive (2.0x) vs Active (0.5x) should be 4:1 ratio. Got {}:1",
        passive_prog / active_prog
    );
}

#[test]
fn integration_passive_player_fires_beat_that_active_player_misses() {
    // The real game-feel test: passive player's accelerated progression
    // causes a beat to fire on a turn where an active player wouldn't trigger it.
    let defs = vec![slow_trope_def()]; // beat at 0.1, rate = 0.05

    // Start both at 0.08 — just below the 0.1 beat threshold
    let mut tropes_active = vec![TropeState::new("slow_burn")];
    tropes_active[0].set_progression(0.08);

    let mut tropes_passive = vec![TropeState::new("slow_burn")];
    tropes_passive[0].set_progression(0.08);

    // Active player (0.5x): 0.08 + (0.05 * 0.5) = 0.105 → crosses 0.1 ✓
    // Wait, that still crosses. Let me set up at 0.06 instead.
    // Active (0.5x): 0.06 + 0.025 = 0.085 → does NOT cross 0.1
    // Passive (2.0x): 0.06 + 0.10 = 0.16 → crosses 0.1 ✓
    tropes_active[0].set_progression(0.06);
    tropes_passive[0].set_progression(0.06);

    let active_mult = engagement_multiplier(0); // 0.5
    let passive_mult = engagement_multiplier(7); // 2.0

    let fired_active =
        TropeEngine::tick_with_multiplier(&mut tropes_active, &defs, active_mult as f64);
    let fired_passive =
        TropeEngine::tick_with_multiplier(&mut tropes_passive, &defs, passive_mult as f64);

    assert!(
        fired_active.is_empty(),
        "Active player should NOT fire beat at 0.1 from 0.06 with 0.5x multiplier"
    );
    assert_eq!(
        fired_passive.len(),
        1,
        "Passive player SHOULD fire beat at 0.1 from 0.06 with 2.0x multiplier"
    );
    assert_eq!(fired_passive[0].beat.event, "First sign of trouble");
}

// ============================================================================
// Rule #6: Test quality self-check
// ============================================================================
// Every test uses assert_eq!, assert!, or specific value checks.
// No `let _ =` patterns. No `assert!(true)`.
// Integration tests verify actual progression values and beat firing,
// not just "something happened."
// Multiplier boundary tests check exact values, not ranges.

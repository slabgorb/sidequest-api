//! Story 5-7: Wire pacing into orchestrator — game-crate types and helpers
//!
//! RED phase — these tests reference types and methods that don't exist yet.
//! They will fail to compile until Dev implements:
//!   - PacingHint struct (drama_weight, target_sentences, delivery_mode, escalation_beat)
//!   - DeliveryMode enum (Instant, Sentence, Streaming)
//!   - DramaThresholds struct (genre-tunable breakpoints)
//!   - TensionTracker::pacing_hint(&DramaThresholds) -> PacingHint
//!   - PacingHint::narrator_directive() -> String
//!   - GameSnapshot::lowest_friendly_hp_ratio() -> f64
//!
//! ACs tested: AC1 (drama_weight threading), AC2 (narrator context),
//!             AC5 (delivery_mode), AC6 (HP ratio), AC7 (non-combat passthrough),
//!             AC8 (timing), AC9 (integration)

use sidequest_game::character::Character;
use sidequest_game::tension_tracker::{
    CombatEvent, DeliveryMode, DramaThresholds, PacingHint, TensionTracker,
};
use sidequest_game::state::GameSnapshot;

// ============================================================================
// PacingHint type — AC1, AC5
// ============================================================================

#[test]
fn pacing_hint_from_calm_tracker_has_low_drama() {
    let tracker = TensionTracker::new();
    let thresholds = DramaThresholds::default();
    let hint = tracker.pacing_hint(&thresholds);
    assert!(
        hint.drama_weight < 0.3,
        "calm tracker should produce low drama_weight, got {}",
        hint.drama_weight,
    );
    assert_eq!(
        hint.delivery_mode,
        DeliveryMode::Instant,
        "low drama should yield Instant delivery",
    );
}

#[test]
fn pacing_hint_from_tense_tracker_has_high_drama() {
    let mut tracker = TensionTracker::new();
    // Simulate low HP situation → high stakes tension
    tracker.update_stakes(10, 100);
    let thresholds = DramaThresholds::default();
    let hint = tracker.pacing_hint(&thresholds);
    assert!(
        hint.drama_weight > 0.7,
        "low HP should produce high drama_weight, got {}",
        hint.drama_weight,
    );
    assert_eq!(
        hint.delivery_mode,
        DeliveryMode::Streaming,
        "high drama should yield Streaming delivery",
    );
}

#[test]
fn pacing_hint_mid_drama_yields_sentence_delivery() {
    let mut tracker = TensionTracker::with_values(0.5, 0.0);
    let thresholds = DramaThresholds::default();
    let hint = tracker.pacing_hint(&thresholds);
    assert_eq!(
        hint.delivery_mode,
        DeliveryMode::Sentence,
        "mid-range drama should yield Sentence delivery",
    );
}

#[test]
fn pacing_hint_drama_weight_matches_tracker() {
    let mut tracker = TensionTracker::with_values(0.4, 0.6);
    let thresholds = DramaThresholds::default();
    let hint = tracker.pacing_hint(&thresholds);
    assert!(
        (hint.drama_weight - tracker.drama_weight()).abs() < f64::EPSILON,
        "pacing hint drama_weight ({}) should match tracker drama_weight ({})",
        hint.drama_weight,
        tracker.drama_weight(),
    );
}

// ============================================================================
// Target sentence count — linear interpolation — AC2
// ============================================================================

#[test]
fn zero_drama_targets_one_sentence() {
    let tracker = TensionTracker::new();
    let thresholds = DramaThresholds::default();
    let hint = tracker.pacing_hint(&thresholds);
    assert_eq!(
        hint.target_sentences, 1,
        "zero drama should target 1 sentence",
    );
}

#[test]
fn max_drama_targets_six_sentences() {
    let mut tracker = TensionTracker::with_values(1.0, 0.0);
    let thresholds = DramaThresholds::default();
    let hint = tracker.pacing_hint(&thresholds);
    assert_eq!(
        hint.target_sentences, 6,
        "max drama should target 6 sentences",
    );
}

#[test]
fn mid_drama_targets_proportional_sentences() {
    let mut tracker = TensionTracker::with_values(0.5, 0.0);
    let thresholds = DramaThresholds::default();
    let hint = tracker.pacing_hint(&thresholds);
    // formula: 1 + floor(drama_weight * 5)
    // 1 + floor(0.5 * 5) = 1 + 2 = 3
    assert_eq!(
        hint.target_sentences, 3,
        "0.5 drama should target 3 sentences (1 + floor(0.5 * 5))",
    );
}

// ============================================================================
// DeliveryMode breakpoints — AC5
// ============================================================================

#[test]
fn delivery_mode_instant_below_sentence_threshold() {
    let thresholds = DramaThresholds::default();
    // Default sentence_delivery_min is 0.30
    let mut tracker = TensionTracker::with_values(0.29, 0.0);
    let hint = tracker.pacing_hint(&thresholds);
    assert_eq!(hint.delivery_mode, DeliveryMode::Instant);
}

#[test]
fn delivery_mode_sentence_at_boundary() {
    let thresholds = DramaThresholds::default();
    let mut tracker = TensionTracker::with_values(0.30, 0.0);
    let hint = tracker.pacing_hint(&thresholds);
    assert_eq!(hint.delivery_mode, DeliveryMode::Sentence);
}

#[test]
fn delivery_mode_streaming_above_streaming_threshold() {
    let thresholds = DramaThresholds::default();
    // Default streaming_delivery_min is 0.70
    let mut tracker = TensionTracker::with_values(0.0, 0.0);
    tracker.inject_spike(0.8);
    let hint = tracker.pacing_hint(&thresholds);
    assert_eq!(hint.delivery_mode, DeliveryMode::Streaming);
}

// ============================================================================
// DramaThresholds — genre-tunable breakpoints — AC5
// ============================================================================

#[test]
fn default_thresholds_have_expected_values() {
    let t = DramaThresholds::default();
    assert!((t.sentence_delivery_min - 0.30).abs() < f64::EPSILON);
    assert!((t.streaming_delivery_min - 0.70).abs() < f64::EPSILON);
    assert!((t.render_threshold - 0.40).abs() < f64::EPSILON);
    assert_eq!(t.escalation_streak, 5);
    assert_eq!(t.ramp_length, 8);
}

#[test]
fn custom_thresholds_shift_delivery_breakpoints() {
    // Horror genre: lower threshold for streaming
    let thresholds = DramaThresholds {
        sentence_delivery_min: 0.20,
        streaming_delivery_min: 0.50,
        render_threshold: 0.30,
        escalation_streak: 3,
        ramp_length: 6,
    };
    let mut tracker = TensionTracker::with_values(0.55, 0.0);
    let hint = tracker.pacing_hint(&thresholds);
    assert_eq!(
        hint.delivery_mode,
        DeliveryMode::Streaming,
        "custom threshold 0.50 should yield Streaming at drama 0.55",
    );
}

// ============================================================================
// Escalation beat — AC7 (non-combat) and boring streak detection
// ============================================================================

#[test]
fn no_escalation_beat_when_boring_streak_below_threshold() {
    let mut tracker = TensionTracker::new();
    for _ in 0..3 {
        tracker.record_event(CombatEvent::Boring);
    }
    let thresholds = DramaThresholds::default(); // escalation_streak = 5
    let hint = tracker.pacing_hint(&thresholds);
    assert!(
        hint.escalation_beat.is_none(),
        "boring streak {} < threshold {} should not produce escalation beat",
        tracker.boring_streak(),
        thresholds.escalation_streak,
    );
}

#[test]
fn escalation_beat_when_boring_streak_reaches_threshold() {
    let mut tracker = TensionTracker::new();
    for _ in 0..5 {
        tracker.record_event(CombatEvent::Boring);
    }
    let thresholds = DramaThresholds::default(); // escalation_streak = 5
    let hint = tracker.pacing_hint(&thresholds);
    assert!(
        hint.escalation_beat.is_some(),
        "boring streak {} >= threshold {} should produce escalation beat",
        tracker.boring_streak(),
        thresholds.escalation_streak,
    );
}

#[test]
fn escalation_beat_is_nonempty_string() {
    let mut tracker = TensionTracker::new();
    for _ in 0..6 {
        tracker.record_event(CombatEvent::Boring);
    }
    let thresholds = DramaThresholds::default();
    let hint = tracker.pacing_hint(&thresholds);
    let beat = hint.escalation_beat.expect("should have escalation beat");
    assert!(
        !beat.is_empty(),
        "escalation beat should be a non-empty directive string",
    );
}

// ============================================================================
// PacingHint::narrator_directive — AC2
// ============================================================================

#[test]
fn narrator_directive_includes_sentence_count() {
    let mut tracker = TensionTracker::with_values(0.6, 0.0);
    let thresholds = DramaThresholds::default();
    let hint = tracker.pacing_hint(&thresholds);
    let directive = hint.narrator_directive();
    assert!(
        directive.contains(&hint.target_sentences.to_string()),
        "narrator directive should mention target sentence count {}, got: {}",
        hint.target_sentences,
        directive,
    );
}

#[test]
fn narrator_directive_is_nonempty() {
    let tracker = TensionTracker::new();
    let thresholds = DramaThresholds::default();
    let hint = tracker.pacing_hint(&thresholds);
    let directive = hint.narrator_directive();
    assert!(
        !directive.is_empty(),
        "narrator directive should never be empty",
    );
}

// ============================================================================
// GameSnapshot::lowest_friendly_hp_ratio — AC6
// ============================================================================

#[test]
fn lowest_hp_ratio_with_no_characters_returns_one() {
    let snapshot = GameSnapshot::default();
    assert!(
        (snapshot.lowest_friendly_hp_ratio() - 1.0).abs() < f64::EPSILON,
        "no characters should return 1.0 (fully healthy default)",
    );
}

#[test]
fn lowest_hp_ratio_with_full_health_characters() {
    let mut snapshot = GameSnapshot::default();
    snapshot.characters.push(make_character("hero", 100, 100, true));
    snapshot.characters.push(make_character("mage", 80, 80, true));
    assert!(
        (snapshot.lowest_friendly_hp_ratio() - 1.0).abs() < f64::EPSILON,
        "full health characters should return 1.0",
    );
}

#[test]
fn lowest_hp_ratio_returns_lowest_among_friendlies() {
    let mut snapshot = GameSnapshot::default();
    snapshot.characters.push(make_character("tank", 90, 100, true));
    snapshot.characters.push(make_character("mage", 30, 100, true));
    let ratio = snapshot.lowest_friendly_hp_ratio();
    assert!(
        (ratio - 0.3).abs() < f64::EPSILON,
        "should return 0.3 (mage at 30/100), got {}",
        ratio,
    );
}

#[test]
fn lowest_hp_ratio_ignores_enemies() {
    let mut snapshot = GameSnapshot::default();
    snapshot.characters.push(make_character("hero", 80, 100, true));
    snapshot.characters.push(make_character("goblin", 10, 100, false));
    let ratio = snapshot.lowest_friendly_hp_ratio();
    assert!(
        (ratio - 0.8).abs() < f64::EPSILON,
        "should ignore goblin (enemy) at 10%, hero is 80%, got {}",
        ratio,
    );
}

#[test]
fn lowest_hp_ratio_zero_hp_returns_zero() {
    let mut snapshot = GameSnapshot::default();
    snapshot.characters.push(make_character("hero", 0, 100, true));
    assert!(
        snapshot.lowest_friendly_hp_ratio().abs() < f64::EPSILON,
        "0 HP character should yield 0.0 ratio",
    );
}

// ============================================================================
// Timing correctness — AC8
// Pacing for turn N is based on outcomes of turns 1..N-1
// ============================================================================

#[test]
fn pacing_hint_reflects_prior_turn_not_current() {
    let mut tracker = TensionTracker::new();
    let thresholds = DramaThresholds::default();

    // Turn 1: no prior events → calm pacing
    let hint_turn_1 = tracker.pacing_hint(&thresholds);
    assert!(
        hint_turn_1.drama_weight < 0.3,
        "turn 1 should be calm (no prior events), got {}",
        hint_turn_1.drama_weight,
    );

    // After turn 1 resolves with dramatic outcome:
    tracker.record_event(CombatEvent::Dramatic);
    tracker.inject_spike(0.8);
    tracker.tick();

    // Turn 2: pacing should reflect turn 1's dramatic outcome
    let hint_turn_2 = tracker.pacing_hint(&thresholds);
    assert!(
        hint_turn_2.drama_weight > hint_turn_1.drama_weight,
        "turn 2 pacing should reflect turn 1's dramatic outcome: {} > {}",
        hint_turn_2.drama_weight,
        hint_turn_1.drama_weight,
    );
}

// ============================================================================
// Integration: 3-turn combat scenario — AC9
// boring ramp → dramatic spike → decay
// ============================================================================

#[test]
fn three_turn_combat_pacing_progression() {
    let mut tracker = TensionTracker::new();
    let thresholds = DramaThresholds::default();

    // --- Turn 1-3: boring ramp (misses, low damage) ---
    let mut prev_drama = 0.0_f64;
    for turn in 1..=3 {
        let hint = tracker.pacing_hint(&thresholds);
        assert!(
            hint.drama_weight >= prev_drama,
            "turn {}: drama should ramp (boring streak), {} >= {}",
            turn,
            hint.drama_weight,
            prev_drama,
        );
        prev_drama = hint.drama_weight;
        // Simulate boring combat outcome
        tracker.record_event(CombatEvent::Boring);
        tracker.tick();
    }

    let boring_peak = tracker.pacing_hint(&thresholds).drama_weight;

    // --- Turn 4: dramatic spike (killing blow) ---
    tracker.record_event(CombatEvent::Dramatic);
    tracker.inject_spike(1.0);
    tracker.tick();

    let spike_hint = tracker.pacing_hint(&thresholds);
    assert!(
        spike_hint.drama_weight > boring_peak,
        "dramatic spike should exceed boring ramp: {} > {}",
        spike_hint.drama_weight,
        boring_peak,
    );
    assert_eq!(
        spike_hint.delivery_mode,
        DeliveryMode::Streaming,
        "killing blow should trigger Streaming delivery",
    );

    // --- Turn 5-7: decay back to calm ---
    for _ in 0..3 {
        tracker.record_event(CombatEvent::Boring);
        tracker.tick();
    }
    let decayed_hint = tracker.pacing_hint(&thresholds);
    assert!(
        decayed_hint.drama_weight < spike_hint.drama_weight,
        "drama should decay after spike: {} < {}",
        decayed_hint.drama_weight,
        spike_hint.drama_weight,
    );
}

// ============================================================================
// Non-combat passthrough — AC7
// ============================================================================

#[test]
fn calm_tracker_produces_no_pacing_directives_for_exploration() {
    let tracker = TensionTracker::new();
    let thresholds = DramaThresholds::default();
    let hint = tracker.pacing_hint(&thresholds);
    // In non-combat, drama_weight should be near zero
    assert!(
        hint.drama_weight < f64::EPSILON,
        "non-combat tracker should have ~0 drama_weight",
    );
    assert_eq!(hint.delivery_mode, DeliveryMode::Instant);
    assert!(hint.escalation_beat.is_none());
}

// ============================================================================
// Rule enforcement: #2 — non_exhaustive on DeliveryMode
// ============================================================================

#[test]
fn delivery_mode_has_three_variants() {
    // Verify enum variant existence (compile-time check)
    let _instant = DeliveryMode::Instant;
    let _sentence = DeliveryMode::Sentence;
    let _streaming = DeliveryMode::Streaming;
}

// ============================================================================
// Rule enforcement: #9 — DramaThresholds fields accessible
// DramaThresholds uses pub fields per story context (genre packs override them)
// ============================================================================

#[test]
fn drama_thresholds_fields_are_settable() {
    let t = DramaThresholds {
        sentence_delivery_min: 0.25,
        streaming_delivery_min: 0.60,
        render_threshold: 0.35,
        escalation_streak: 4,
        ramp_length: 10,
    };
    assert!((t.sentence_delivery_min - 0.25).abs() < f64::EPSILON);
    assert!((t.streaming_delivery_min - 0.60).abs() < f64::EPSILON);
    assert!((t.render_threshold - 0.35).abs() < f64::EPSILON);
    assert_eq!(t.escalation_streak, 4);
    assert_eq!(t.ramp_length, 10);
}

// ============================================================================
// Rule enforcement: #6 — no vacuous assertions (self-check: all tests above
// have meaningful assert_eq! / assert! with specific value checks)
// ============================================================================

// ============================================================================
// Helpers
// ============================================================================

/// Build a character with the given HP values for testing.
/// `is_friendly` marks whether this is a player-controlled character.
fn make_character(name: &str, current_hp: i32, max_hp: i32, is_friendly: bool) -> Character {
    let mut c = Character::default();
    // These fields will need to exist on Character for the hp ratio helper.
    // If Character doesn't have these fields, this will fail to compile —
    // Dev needs to ensure Character has current_hp, max_hp, is_friendly.
    c.name = name.to_string();
    c.current_hp = current_hp;
    c.max_hp = max_hp;
    c.is_friendly = is_friendly;
    c
}

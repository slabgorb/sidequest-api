//! Integration tests for Story 1-7: Game subsystems — CombatState, ChaseState,
//! NarrativeEntry, progression, TurnManager, status effects.

use sidequest_game::{
    // Progression
    level_to_damage,
    level_to_defense,
    level_to_hp,
    xp_for_level,
    // Chase subsystem
    ChaseState,
    ChaseType,
    // Combat subsystem
    CombatState,
    DamageEvent,
    // Narrative
    NarrativeEntry,
    RoundResult,
    StatusEffect,
    StatusEffectKind,
    // Turn management
    TurnManager,
    TurnPhase,
};

// ═══════════════════════════════════════════════════════════
// AC: CombatState — create, track rounds, combatants, damage log
// ═══════════════════════════════════════════════════════════

#[test]
fn combat_state_starts_at_round_one() {
    let state = CombatState::new();
    assert_eq!(state.round(), 1, "combat starts at round 1");
}

#[test]
fn combat_state_advance_round_increments() {
    let mut state = CombatState::new();
    state.advance_round();
    assert_eq!(state.round(), 2);
    state.advance_round();
    assert_eq!(state.round(), 3);
}

#[test]
fn combat_state_starts_with_empty_damage_log() {
    let state = CombatState::new();
    assert!(state.damage_log().is_empty());
}

#[test]
fn combat_state_log_damage_event() {
    let mut state = CombatState::new();
    let event = DamageEvent {
        attacker: "Grog".to_string(),
        target: "Goblin".to_string(),
        damage: 8,
        round: 1,
    };
    state.log_damage(event);
    assert_eq!(state.damage_log().len(), 1);
    assert_eq!(state.damage_log()[0].damage, 8);
    assert_eq!(state.damage_log()[0].attacker, "Grog");
}

#[test]
fn combat_state_multiple_damage_events_ordered() {
    let mut state = CombatState::new();
    state.log_damage(DamageEvent {
        attacker: "Grog".to_string(),
        target: "Goblin".to_string(),
        damage: 5,
        round: 1,
    });
    state.log_damage(DamageEvent {
        attacker: "Goblin".to_string(),
        target: "Grog".to_string(),
        damage: 3,
        round: 1,
    });
    assert_eq!(state.damage_log().len(), 2);
    assert_eq!(state.damage_log()[0].damage, 5);
    assert_eq!(state.damage_log()[1].damage, 3);
}

// ═══════════════════════════════════════════════════════════
// AC: Status effects — duration tracked per round, decrement
// ═══════════════════════════════════════════════════════════

#[test]
fn status_effect_created_with_duration() {
    let effect = StatusEffect::new(StatusEffectKind::Poison, 3);
    assert_eq!(effect.kind(), StatusEffectKind::Poison);
    assert_eq!(effect.remaining_rounds(), 3);
}

#[test]
fn status_effect_decrement_reduces_duration() {
    let mut effect = StatusEffect::new(StatusEffectKind::Stun, 2);
    effect.tick();
    assert_eq!(effect.remaining_rounds(), 1);
}

#[test]
fn status_effect_expired_at_zero() {
    let mut effect = StatusEffect::new(StatusEffectKind::Bless, 1);
    assert!(!effect.is_expired());
    effect.tick();
    assert!(effect.is_expired());
    assert_eq!(effect.remaining_rounds(), 0);
}

#[test]
fn status_effect_tick_does_not_go_negative() {
    let mut effect = StatusEffect::new(StatusEffectKind::Curse, 1);
    effect.tick();
    effect.tick(); // already at 0
    assert_eq!(effect.remaining_rounds(), 0);
}

#[test]
fn combat_state_add_and_tick_status_effects() {
    let mut state = CombatState::new();
    state.add_effect("Grog", StatusEffect::new(StatusEffectKind::Poison, 2));
    let effects = state.effects_on("Grog");
    assert_eq!(effects.len(), 1);
    assert_eq!(effects[0].remaining_rounds(), 2);

    state.tick_effects();
    let effects = state.effects_on("Grog");
    assert_eq!(effects[0].remaining_rounds(), 1);
}

#[test]
fn combat_state_expired_effects_removed_after_tick() {
    let mut state = CombatState::new();
    state.add_effect("Grog", StatusEffect::new(StatusEffectKind::Stun, 1));
    state.tick_effects();
    let effects = state.effects_on("Grog");
    assert!(effects.is_empty(), "expired effects should be removed");
}

// ═══════════════════════════════════════════════════════════
// AC: RoundResult — damage calculated, effects applied
// ═══════════════════════════════════════════════════════════

#[test]
fn round_result_contains_damage_events() {
    let result = RoundResult {
        round: 1,
        damage_events: vec![DamageEvent {
            attacker: "Grog".to_string(),
            target: "Goblin".to_string(),
            damage: 12,
            round: 1,
        }],
        effects_applied: vec![],
        effects_expired: vec![],
    };
    assert_eq!(result.round, 1);
    assert_eq!(result.damage_events.len(), 1);
    assert_eq!(result.damage_events[0].damage, 12);
}

// ═══════════════════════════════════════════════════════════
// AC: ChaseState — create, track rounds, escape vs threshold
// ═══════════════════════════════════════════════════════════

#[test]
fn chase_state_created_with_type_and_threshold() {
    let state = ChaseState::new(ChaseType::Footrace, 0.5);
    assert_eq!(state.chase_type(), ChaseType::Footrace);
    assert!((state.escape_threshold() - 0.5).abs() < f64::EPSILON);
}

#[test]
fn chase_state_starts_at_round_one() {
    let state = ChaseState::new(ChaseType::Stealth, 0.5);
    assert_eq!(state.round(), 1);
}

#[test]
fn chase_state_record_escape_roll() {
    let mut state = ChaseState::new(ChaseType::Footrace, 0.5);
    state.record_roll(0.3); // below threshold, no escape
    assert_eq!(state.rounds().len(), 1);
    assert!(!state.rounds()[0].escaped);
}

#[test]
fn chase_escape_succeeds_above_threshold() {
    let mut state = ChaseState::new(ChaseType::Footrace, 0.5);
    state.record_roll(0.7); // above 0.5 threshold
    assert!(state.rounds()[0].escaped, "roll above threshold = escape");
}

#[test]
fn chase_escape_fails_below_threshold() {
    let mut state = ChaseState::new(ChaseType::Footrace, 0.5);
    state.record_roll(0.3); // below 0.5 threshold
    assert!(
        !state.rounds()[0].escaped,
        "roll below threshold = no escape"
    );
}

#[test]
fn chase_escape_at_exact_threshold_fails() {
    // Edge case: equal to threshold should NOT escape (must exceed)
    let mut state = ChaseState::new(ChaseType::Footrace, 0.5);
    state.record_roll(0.5);
    assert!(!state.rounds()[0].escaped, "exact threshold = no escape");
}

#[test]
fn chase_round_counter_increments_with_rolls() {
    let mut state = ChaseState::new(ChaseType::Negotiation, 0.5);
    state.record_roll(0.3);
    state.record_roll(0.4);
    state.record_roll(0.6);
    assert_eq!(state.round(), 4, "round = number of rolls + 1 initial");
    assert_eq!(state.rounds().len(), 3);
}

#[test]
fn chase_is_resolved_when_escaped() {
    let mut state = ChaseState::new(ChaseType::Footrace, 0.5);
    assert!(!state.is_resolved());
    state.record_roll(0.7);
    assert!(state.is_resolved(), "escape = resolved");
}

// ═══════════════════════════════════════════════════════════
// AC: Progression — level → stats, soft cap at L10
// ═══════════════════════════════════════════════════════════

#[test]
fn level_to_hp_scales_base() {
    let hp = level_to_hp(10, 1);
    assert!(hp >= 10, "level 1 should at least return base HP");
}

#[test]
fn level_to_hp_increases_with_level() {
    let hp_l1 = level_to_hp(10, 1);
    let hp_l5 = level_to_hp(10, 5);
    let hp_l10 = level_to_hp(10, 10);
    assert!(hp_l5 > hp_l1, "HP should increase from L1 to L5");
    assert!(hp_l10 > hp_l5, "HP should increase from L5 to L10");
}

#[test]
fn level_to_hp_soft_cap_at_level_10() {
    // Growth from L10 to L15 should be LESS than L5 to L10
    let growth_5_to_10 = level_to_hp(10, 10) - level_to_hp(10, 5);
    let growth_10_to_15 = level_to_hp(10, 15) - level_to_hp(10, 10);
    assert!(
        growth_10_to_15 < growth_5_to_10,
        "diminishing returns after L10: {growth_10_to_15} should be < {growth_5_to_10}"
    );
}

#[test]
fn level_to_hp_always_at_least_one() {
    // Even at base 1, level 1, HP should be >= 1
    assert!(level_to_hp(1, 1) >= 1);
}

#[test]
fn level_to_damage_linear_scaling() {
    let d1 = level_to_damage(10, 1);
    let d5 = level_to_damage(10, 5);
    let d10 = level_to_damage(10, 10);
    // Linear: each level adds ~0.1 * base
    assert!(d5 > d1, "damage should increase with level");
    assert!(d10 > d5, "damage should continue increasing");
}

#[test]
fn level_to_damage_at_level_one_returns_base() {
    let d = level_to_damage(10, 1);
    assert_eq!(d, 10, "level 1 should return base damage");
}

#[test]
fn level_to_defense_soft_cap_at_level_10() {
    let growth_5_to_10 = level_to_defense(10, 10) - level_to_defense(10, 5);
    let growth_10_to_15 = level_to_defense(10, 15) - level_to_defense(10, 10);
    assert!(
        growth_10_to_15 < growth_5_to_10,
        "defense should have diminishing returns after L10"
    );
}

#[test]
fn level_to_defense_increases_with_level() {
    let def_l1 = level_to_defense(10, 1);
    let def_l10 = level_to_defense(10, 10);
    assert!(def_l10 > def_l1);
}

// ═══════════════════════════════════════════════════════════
// AC: XP and leveling — threshold 100*level
// ═══════════════════════════════════════════════════════════

#[test]
fn xp_threshold_is_100_times_level() {
    assert_eq!(xp_for_level(1), 100);
    assert_eq!(xp_for_level(2), 200);
    assert_eq!(xp_for_level(5), 500);
    assert_eq!(xp_for_level(10), 1000);
}

#[test]
fn xp_threshold_level_zero_is_zero() {
    assert_eq!(xp_for_level(0), 0);
}

// ═══════════════════════════════════════════════════════════
// AC: NarrativeEntry — append-only, immutable, queryable
// ═══════════════════════════════════════════════════════════

#[test]
fn narrative_entry_created_with_fields() {
    let entry = NarrativeEntry {
        timestamp: 1000,
        round: 1,
        author: "narrator".to_string(),
        content: "The cave echoes with distant rumbling.".to_string(),
        tags: vec!["atmosphere".to_string()],
        encounter_tags: vec![],
        speaker: None,
        entry_type: None,
    };
    assert_eq!(entry.timestamp, 1000);
    assert_eq!(entry.round, 1);
    assert_eq!(entry.author, "narrator");
    assert_eq!(entry.content, "The cave echoes with distant rumbling.");
    assert_eq!(entry.tags.len(), 1);
}

#[test]
fn narrative_log_is_append_only() {
    let mut log: Vec<NarrativeEntry> = Vec::new();
    log.push(NarrativeEntry {
        timestamp: 1000,
        round: 1,
        author: "narrator".to_string(),
        content: "First entry.".to_string(),
        tags: vec![],
        encounter_tags: vec![],
        speaker: None,
        entry_type: None,
    });
    log.push(NarrativeEntry {
        timestamp: 2000,
        round: 2,
        author: "combat".to_string(),
        content: "Second entry.".to_string(),
        tags: vec!["combat".to_string()],
        encounter_tags: vec![],
        speaker: None,
        entry_type: None,
    });
    assert_eq!(log.len(), 2);
    assert_eq!(log[0].content, "First entry.");
    assert_eq!(log[1].content, "Second entry.");
}

#[test]
fn narrative_log_query_by_reverse_iteration() {
    let log = vec![
        NarrativeEntry {
            timestamp: 1000,
            round: 1,
            author: "narrator".to_string(),
            content: "First.".to_string(),
            tags: vec![],
            encounter_tags: vec![],
            speaker: None,
            entry_type: None,
        },
        NarrativeEntry {
            timestamp: 2000,
            round: 2,
            author: "narrator".to_string(),
            content: "Second.".to_string(),
            tags: vec![],
            encounter_tags: vec![],
            speaker: None,
            entry_type: None,
        },
        NarrativeEntry {
            timestamp: 3000,
            round: 3,
            author: "narrator".to_string(),
            content: "Third.".to_string(),
            tags: vec![],
            encounter_tags: vec![],
            speaker: None,
            entry_type: None,
        },
    ];
    // Newest first via reverse iteration
    let newest: Vec<&str> = log.iter().rev().map(|e| e.content.as_str()).collect();
    assert_eq!(newest[0], "Third.");
    assert_eq!(newest[1], "Second.");
    assert_eq!(newest[2], "First.");
}

// ═══════════════════════════════════════════════════════════
// AC: TurnManager — tracks phase, round counter, never resets
// ═══════════════════════════════════════════════════════════

#[test]
fn turn_manager_starts_at_round_one() {
    let tm = TurnManager::new();
    assert_eq!(tm.round(), 1);
}

#[test]
fn turn_manager_advance_increments_round() {
    let mut tm = TurnManager::new();
    tm.advance();
    assert_eq!(tm.round(), 2);
    tm.advance();
    assert_eq!(tm.round(), 3);
}

#[test]
fn turn_manager_tracks_phase() {
    let tm = TurnManager::new();
    assert_eq!(tm.phase(), TurnPhase::InputCollection);
}

#[test]
fn turn_manager_phase_advances() {
    let mut tm = TurnManager::new();
    tm.advance_phase();
    assert_eq!(tm.phase(), TurnPhase::IntentRouting);
    tm.advance_phase();
    assert_eq!(tm.phase(), TurnPhase::AgentExecution);
    tm.advance_phase();
    assert_eq!(tm.phase(), TurnPhase::StatePatch);
    tm.advance_phase();
    assert_eq!(tm.phase(), TurnPhase::Broadcast);
}

#[test]
fn turn_manager_round_never_decreases() {
    let mut tm = TurnManager::new();
    for _ in 0..10 {
        let prev = tm.round();
        tm.advance();
        assert!(tm.round() > prev, "round must always increase");
    }
}

// ═══════════════════════════════════════════════════════════
// REWORK: Reviewer-found bugs (round-trip 1)
// ═══════════════════════════════════════════════════════════

/// BUG #1: effects_on() should only return non-expired effects.
/// The doc says "active (non-expired)" but the implementation returns all.
#[test]
fn effects_on_excludes_expired_effects() {
    let mut state = CombatState::new();
    // Add a 1-round effect and a 3-round effect
    state.add_effect("Grog", StatusEffect::new(StatusEffectKind::Stun, 1));
    state.add_effect("Grog", StatusEffect::new(StatusEffectKind::Poison, 3));

    // Before tick: both should be visible
    assert_eq!(state.effects_on("Grog").len(), 2);

    // After tick: Stun (1 round) should be expired and filtered out
    state.tick_effects();
    let active = state.effects_on("Grog");
    assert_eq!(
        active.len(),
        1,
        "effects_on should only return non-expired effects; Stun should be gone"
    );
    assert_eq!(active[0].kind(), StatusEffectKind::Poison);
}

/// BUG #2: record_roll() on a resolved chase should not corrupt state.
#[test]
fn record_roll_after_resolved_is_noop() {
    let mut state = ChaseState::new(ChaseType::Footrace, 0.5);
    state.record_roll(0.7); // escape succeeds
    assert!(state.is_resolved());

    let round_before = state.round();
    let rounds_len_before = state.rounds().len();

    // Rolling after resolution should be a no-op
    state.record_roll(0.3);
    assert_eq!(
        state.round(),
        round_before,
        "round should not increment after resolution"
    );
    assert_eq!(
        state.rounds().len(),
        rounds_len_before,
        "no new rounds should be recorded after resolution"
    );
}

/// BUG #3: StatusEffect with duration 0 should be immediately expired.
/// This documents the edge case explicitly.
#[test]
fn status_effect_zero_duration_is_immediately_expired() {
    let effect = StatusEffect::new(StatusEffectKind::Curse, 0);
    assert!(
        effect.is_expired(),
        "a zero-duration effect should be immediately expired"
    );
    assert_eq!(effect.remaining_rounds(), 0);
}

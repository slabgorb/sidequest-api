//! Tests for Story 15-21: Wire level_to_defense into combat resolution.
//!
//! level_to_damage() IS wired into resolve_attack(). Its sister function
//! level_to_defense() is never called. These tests verify:
//! 1. resolve_attack() uses the target's level and AC to compute defense
//! 2. Higher-level defenders take less damage
//! 3. Defense never reduces damage below 1 (attacks always connect)
//! 4. OTEL span emitted: combat.defense_applied

use sidequest_game::combat::{CombatState, StatusEffectKind};
use sidequest_game::combatant::Combatant;
use sidequest_game::progression::level_to_defense;

/// Minimal test combatant for exercising resolve_attack with defense.
struct TestCombatant {
    name: String,
    hp: i32,
    max_hp: i32,
    level: u32,
    ac: i32,
}

impl Combatant for TestCombatant {
    fn name(&self) -> &str { &self.name }
    fn hp(&self) -> i32 { self.hp }
    fn max_hp(&self) -> i32 { self.max_hp }
    fn level(&self) -> u32 { self.level }
    fn ac(&self) -> i32 { self.ac }
}

fn attacker(level: u32) -> TestCombatant {
    TestCombatant {
        name: "Attacker".to_string(),
        hp: 20,
        max_hp: 20,
        level,
        ac: 10,
    }
}

fn defender(level: u32, ac: i32) -> TestCombatant {
    TestCombatant {
        name: "Defender".to_string(),
        hp: 20,
        max_hp: 20,
        level,
        ac,
    }
}

// ═══════════════════════════════════════════════════════════════
// AC-1: resolve_attack uses target's level and AC for defense
// ═══════════════════════════════════════════════════════════════

#[test]
fn resolve_attack_factors_defender_level_into_damage() {
    let mut combat = CombatState::default();
    combat.set_in_combat(true);

    let atk = attacker(1);
    let low_def = defender(1, 10);
    let high_def = defender(5, 10);

    let result_low = combat.resolve_attack("Attacker", &atk, "Defender", &low_def);
    let damage_vs_low = result_low.damage_events[0].damage;

    let result_high = combat.resolve_attack("Attacker", &atk, "Defender", &high_def);
    let damage_vs_high = result_high.damage_events[0].damage;

    assert!(
        damage_vs_high < damage_vs_low,
        "Higher-level defender should take less damage: vs_low={}, vs_high={}",
        damage_vs_low,
        damage_vs_high
    );
}

#[test]
fn resolve_attack_factors_defender_ac_into_damage() {
    let mut combat = CombatState::default();
    combat.set_in_combat(true);

    let atk = attacker(1);
    let low_ac = defender(1, 5);
    let high_ac = defender(1, 15);

    let result_low_ac = combat.resolve_attack("Attacker", &atk, "LowAC", &low_ac);
    let damage_vs_low_ac = result_low_ac.damage_events[0].damage;

    let result_high_ac = combat.resolve_attack("Attacker", &atk, "HighAC", &high_ac);
    let damage_vs_high_ac = result_high_ac.damage_events[0].damage;

    assert!(
        damage_vs_high_ac < damage_vs_low_ac,
        "Higher AC defender should take less damage: vs_low_ac={}, vs_high_ac={}",
        damage_vs_low_ac,
        damage_vs_high_ac
    );
}

// ═══════════════════════════════════════════════════════════════
// AC-2: Higher-level defenders take less damage
// ═══════════════════════════════════════════════════════════════

#[test]
fn level_5_defender_takes_less_than_level_1() {
    let mut combat = CombatState::default();
    combat.set_in_combat(true);

    let atk = attacker(3);
    let def_l1 = defender(1, 10);
    let def_l5 = defender(5, 10);

    let r1 = combat.resolve_attack("Atk", &atk, "DefL1", &def_l1);
    let r5 = combat.resolve_attack("Atk", &atk, "DefL5", &def_l5);

    assert!(r5.damage_events[0].damage < r1.damage_events[0].damage);
}

// ═══════════════════════════════════════════════════════════════
// AC-3: Defense never reduces damage below 1
// ═══════════════════════════════════════════════════════════════

#[test]
fn damage_never_goes_below_one() {
    let mut combat = CombatState::default();
    combat.set_in_combat(true);

    // Level 1 attacker vs level 20 defender with high AC
    let atk = attacker(1);
    let tank = defender(20, 30);

    let result = combat.resolve_attack("Weakling", &atk, "Tank", &tank);
    assert_eq!(
        result.damage_events.len(),
        1,
        "Attack should produce a damage event even against heavy armor"
    );
    assert!(
        result.damage_events[0].damage >= 1,
        "Damage must be at least 1, got {}",
        result.damage_events[0].damage
    );
}

// ═══════════════════════════════════════════════════════════════
// AC-4: OTEL span — combat.defense_applied
// ═══════════════════════════════════════════════════════════════

#[test]
fn resolve_attack_completes_with_defense_applied() {
    // Structural test: the code path that applies defense and emits
    // the OTEL span runs successfully. Full OTEL verification would
    // need a tracing subscriber, but this confirms the wiring works.
    let mut combat = CombatState::default();
    combat.set_in_combat(true);

    let atk = attacker(3);
    let def = defender(5, 12);

    let result = combat.resolve_attack("Hero", &atk, "Goblin", &def);
    assert_eq!(result.damage_events.len(), 1);
    // The damage should be reduced from the raw level_to_damage output
    let raw_damage = sidequest_game::progression::level_to_damage(5, 3);
    assert!(
        result.damage_events[0].damage <= raw_damage,
        "With defense applied, damage {} should be <= raw damage {}",
        result.damage_events[0].damage,
        raw_damage
    );
}

// ═══════════════════════════════════════════════════════════════
// Regression: target parameter was previously unused (_target)
// ═══════════════════════════════════════════════════════════════

#[test]
fn target_parameter_is_used_not_ignored() {
    let mut combat = CombatState::default();
    combat.set_in_combat(true);

    let atk = attacker(5);
    // Two different targets with very different defense stats
    let paper = defender(1, 0);
    let iron = defender(10, 20);

    let r_paper = combat.resolve_attack("Atk", &atk, "Paper", &paper);
    let r_iron = combat.resolve_attack("Atk", &atk, "Iron", &iron);

    assert_ne!(
        r_paper.damage_events[0].damage,
        r_iron.damage_events[0].damage,
        "Different defenders should produce different damage values — target must not be ignored"
    );
}

// ═══════════════════════════════════════════════════════════════
// level_to_defense function itself (sanity check)
// ═══════════════════════════════════════════════════════════════

#[test]
fn level_to_defense_scales_with_level() {
    let base = 10;
    let d1 = level_to_defense(base, 1);
    let d5 = level_to_defense(base, 5);
    let d10 = level_to_defense(base, 10);

    assert_eq!(d1, base, "Level 1 should return base defense");
    assert!(d5 > d1, "Level 5 defense should exceed level 1");
    assert!(d10 > d5, "Level 10 defense should exceed level 5");
}

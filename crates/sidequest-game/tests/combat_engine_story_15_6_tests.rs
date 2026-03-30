//! Story 15-6: Combat engine tests — engagement, resolution, and cleanup
//!
//! RED phase — these tests define the API contract for CombatState methods
//! that don't exist yet. The combat system is currently a passive state holder
//! with setters (set_in_combat, set_turn_order, etc.) but no ENGINE that
//! drives structured combat:
//!   - engage() — start combat with combatants, initialize turn order
//!   - resolve_action() — resolve a combat action mechanically (attack rolls, damage)
//!   - check_victory() — determine if combat should end
//!   - disengage() — cleanup after combat ends
//!
//! ACs covered:
//!   AC-2: Combat system spawns enemies from creature definitions
//!   AC-3: Turn order initialized with player + spawned creatures
//!   AC-5: Combat rounds execute structured mechanics
//!   AC-6: HP tracking through combat resolution, not narration
//!   AC-7: End-of-combat cleanup
//!   AC-8: Full combat flow from engagement through victory

use sidequest_game::combat::{CombatState, DamageEvent, RoundResult, StatusEffect, StatusEffectKind};
use sidequest_game::combatant::Combatant;
use sidequest_game::creature_core::CreatureCore;
use sidequest_game::inventory::Inventory;
use sidequest_protocol::NonBlankString;

// ============================================================================
// Test helpers
// ============================================================================

fn make_creature(name: &str, hp: i32, max_hp: i32, ac: i32, level: u32) -> CreatureCore {
    CreatureCore {
        name: NonBlankString::new(name).unwrap(),
        description: NonBlankString::new("A test creature").unwrap(),
        personality: NonBlankString::new("Testy").unwrap(),
        level,
        hp,
        max_hp,
        ac,
        xp: 0,
        inventory: Inventory::default(),
        statuses: vec![],
    }
}

// ============================================================================
// AC-2 + AC-3: Combat engagement initializes turn order
// ============================================================================

/// When combat engages with a list of combatants, turn_order must be
/// populated and in_combat set to true.
#[test]
fn engage_sets_in_combat_and_turn_order() {
    let mut combat = CombatState::new();
    let combatant_names = vec!["Grog".to_string(), "Radboar".to_string(), "Spore Crawler".to_string()];

    combat.engage(combatant_names);

    assert!(combat.in_combat(), "engage() must set in_combat to true");
    assert_eq!(
        combat.turn_order().len(),
        3,
        "turn_order must contain all combatants"
    );
    assert!(
        combat.current_turn().is_some(),
        "current_turn must be set after engagement"
    );
    assert_eq!(combat.round(), 1, "combat should start at round 1");
}

/// Engaging with an empty combatant list is a no-op (no combat without combatants).
#[test]
fn engage_with_empty_combatants_does_not_start_combat() {
    let mut combat = CombatState::new();
    combat.engage(vec![]);

    assert!(!combat.in_combat(), "empty engagement should not start combat");
    assert!(combat.turn_order().is_empty());
}

/// Engaging while already in combat should not reset the state.
#[test]
fn engage_while_in_combat_is_idempotent() {
    let mut combat = CombatState::new();
    combat.engage(vec!["Grog".into(), "Goblin".into()]);
    combat.advance_round(); // Now round 2

    // Re-engaging should not reset
    combat.engage(vec!["Grog".into(), "Goblin".into(), "Troll".into()]);
    assert_eq!(combat.round(), 2, "re-engage should not reset round counter");
}

// ============================================================================
// AC-5: Structured combat round resolution
// ============================================================================

/// resolve_round() should process all combatants in turn order and produce
/// a RoundResult with damage events and effects.
#[test]
fn resolve_round_produces_damage_events() {
    let mut combat = CombatState::new();
    combat.engage(vec!["Player".into(), "Goblin".into()]);

    let player = make_creature("Player", 30, 30, 15, 3);
    let goblin = make_creature("Goblin", 10, 10, 12, 1);

    // Create a combat action — player attacks goblin
    let result = combat.resolve_attack("Player", &player, "Goblin", &goblin);

    assert!(result.damage_events.len() > 0, "attack should produce at least one damage event");
    assert_eq!(result.damage_events[0].attacker, "Player");
    assert_eq!(result.damage_events[0].target, "Goblin");
    assert_eq!(result.round, 1, "damage event should be tagged with current round");
}

/// After resolving all turns in a round, the round should advance.
#[test]
fn resolve_round_advances_round_counter() {
    let mut combat = CombatState::new();
    combat.engage(vec!["Player".into(), "Goblin".into()]);

    let player = make_creature("Player", 30, 30, 15, 3);
    let goblin = make_creature("Goblin", 10, 10, 12, 1);

    combat.resolve_attack("Player", &player, "Goblin", &goblin);
    combat.advance_turn(); // Move to Goblin's turn
    combat.resolve_attack("Goblin", &goblin, "Player", &player);
    combat.advance_turn(); // End of round

    assert_eq!(combat.round(), 2, "round should advance after all turns complete");
}

// ============================================================================
// AC-6: HP tracking through combat system, not narration deltas
// ============================================================================

/// resolve_attack should calculate damage based on attacker level and
/// defender AC, not just pass through a narrated number.
#[test]
fn resolve_attack_uses_combat_mechanics() {
    let mut combat = CombatState::new();
    combat.engage(vec!["Player".into(), "Goblin".into()]);

    let player = make_creature("Player", 30, 30, 15, 5);
    let goblin = make_creature("Goblin", 10, 10, 8, 1); // Low AC, should be easy to hit

    let result = combat.resolve_attack("Player", &player, "Goblin", &goblin);

    // Damage should be calculated mechanically, not narrated
    if !result.damage_events.is_empty() {
        let damage = result.damage_events[0].damage;
        assert!(damage > 0, "successful hit should deal positive damage");
        // Damage should be bounded by level-based scaling
        let max_expected = sidequest_game::level_to_damage(5, player.level) * 2;
        assert!(
            damage <= max_expected,
            "damage {} should not exceed level-scaled maximum {}",
            damage,
            max_expected
        );
    }
}

// ============================================================================
// AC-5: Status effects applied during combat resolution
// ============================================================================

#[test]
fn status_effects_modify_combat_resolution() {
    let mut combat = CombatState::new();
    combat.engage(vec!["Player".into(), "Goblin".into()]);

    // Stun the player — should skip their turn
    combat.add_effect("Player", StatusEffect::new(StatusEffectKind::Stun, 1));

    let player = make_creature("Player", 30, 30, 15, 3);
    let goblin = make_creature("Goblin", 10, 10, 12, 1);

    // Stunned player's attack should fail or be skipped
    let result = combat.resolve_attack("Player", &player, "Goblin", &goblin);
    assert!(
        result.damage_events.is_empty(),
        "stunned combatant should not deal damage"
    );
}

// ============================================================================
// AC-7: End-of-combat cleanup
// ============================================================================

/// check_victory should detect when all enemies are dead.
#[test]
fn check_victory_detects_all_enemies_dead() {
    let mut combat = CombatState::new();
    combat.engage(vec!["Player".into(), "Goblin".into(), "Orc".into()]);

    let dead_goblin = make_creature("Goblin", 0, 10, 12, 1);
    let dead_orc = make_creature("Orc", 0, 20, 14, 2);
    let alive_player = make_creature("Player", 25, 30, 15, 3);

    let enemies: Vec<&dyn Combatant> = vec![&dead_goblin, &dead_orc];
    let players: Vec<&dyn Combatant> = vec![&alive_player];

    let outcome = combat.check_victory(&players, &enemies);
    assert!(outcome.is_some(), "should detect victory when all enemies are dead");
}

/// check_victory returns None when enemies are still alive.
#[test]
fn check_victory_none_when_enemies_alive() {
    let mut combat = CombatState::new();
    combat.engage(vec!["Player".into(), "Goblin".into()]);

    let alive_goblin = make_creature("Goblin", 5, 10, 12, 1);
    let alive_player = make_creature("Player", 25, 30, 15, 3);

    let enemies: Vec<&dyn Combatant> = vec![&alive_goblin];
    let players: Vec<&dyn Combatant> = vec![&alive_player];

    let outcome = combat.check_victory(&players, &enemies);
    assert!(outcome.is_none(), "combat continues when enemies alive");
}

/// check_victory detects player defeat (all players dead).
#[test]
fn check_victory_detects_player_defeat() {
    let mut combat = CombatState::new();
    combat.engage(vec!["Player".into(), "Goblin".into()]);

    let alive_goblin = make_creature("Goblin", 10, 10, 12, 1);
    let dead_player = make_creature("Player", 0, 30, 15, 3);

    let enemies: Vec<&dyn Combatant> = vec![&alive_goblin];
    let players: Vec<&dyn Combatant> = vec![&dead_player];

    let outcome = combat.check_victory(&players, &enemies);
    assert!(outcome.is_some(), "should detect defeat when all players are dead");
}

/// disengage() clears all combat state.
#[test]
fn disengage_clears_combat_state() {
    let mut combat = CombatState::new();
    combat.engage(vec!["Player".into(), "Goblin".into()]);
    combat.log_damage(DamageEvent {
        attacker: "Player".into(),
        target: "Goblin".into(),
        damage: 5,
        round: 1,
    });

    combat.disengage();

    assert!(!combat.in_combat(), "in_combat should be false after disengage");
    assert!(combat.turn_order().is_empty(), "turn_order should be empty after disengage");
    assert!(combat.current_turn().is_none(), "current_turn should be None after disengage");
    assert!(combat.damage_log().is_empty(), "damage_log should be cleared after disengage");
}

// ============================================================================
// AC-5: Turn advancement within a round
// ============================================================================

/// advance_turn() should cycle through the turn order.
#[test]
fn advance_turn_cycles_through_order() {
    let mut combat = CombatState::new();
    combat.engage(vec!["Player".into(), "Goblin".into(), "Orc".into()]);

    assert_eq!(combat.current_turn(), Some("Player"), "first turn should be first in order");

    combat.advance_turn();
    assert_eq!(combat.current_turn(), Some("Goblin"), "should advance to second combatant");

    combat.advance_turn();
    assert_eq!(combat.current_turn(), Some("Orc"), "should advance to third combatant");

    combat.advance_turn();
    // After last combatant, should wrap to first and advance round
    assert_eq!(combat.current_turn(), Some("Player"), "should wrap around to first combatant");
    assert_eq!(combat.round(), 2, "round should advance after full cycle");
}

// ============================================================================
// AC-8: Full combat flow from engagement through victory
// ============================================================================

/// Integration test: engage → resolve rounds → check victory → disengage
#[test]
fn full_combat_flow_engagement_to_victory() {
    let mut combat = CombatState::new();

    // Phase 1: Engage
    combat.engage(vec!["Hero".into(), "Goblin".into()]);
    assert!(combat.in_combat());
    assert_eq!(combat.turn_order().len(), 2);

    // Phase 2: Resolve a round (hero attacks goblin)
    let hero = make_creature("Hero", 30, 30, 15, 5);
    let weak_goblin = make_creature("Goblin", 1, 10, 8, 1); // 1 HP — will die

    let result = combat.resolve_attack("Hero", &hero, "Goblin", &weak_goblin);
    // The attack should kill the goblin (1 HP, low AC)

    // Phase 3: Check victory
    let dead_goblin = make_creature("Goblin", 0, 10, 8, 1);
    let players: Vec<&dyn Combatant> = vec![&hero];
    let enemies: Vec<&dyn Combatant> = vec![&dead_goblin];

    let outcome = combat.check_victory(&players, &enemies);
    assert!(outcome.is_some(), "should detect victory");

    // Phase 4: Disengage
    combat.disengage();
    assert!(!combat.in_combat());
    assert!(combat.turn_order().is_empty());
    assert_eq!(combat.round(), 1, "round counter resets after disengage");
}

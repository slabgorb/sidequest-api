//! Story 15-6: Combat engine wiring tests
//!
//! RED phase — tests that verify the combat pipeline produces populated
//! COMBAT_EVENT payloads when combat is active and hostile NPCs exist.
//!
//! The gap: intent router classifies Combat correctly, COMBAT_EVENT fires,
//! but enemies/turn_order are always empty. These tests assert that:
//!   1. broadcast_state_changes populates enemies from hostile NPCs
//!   2. CombatState turn_order and current_turn are populated when combat starts
//!   3. Combat initiation populates the full CombatEventPayload
//!   4. Combat ending clears enemies and turn_order
//!
//! ACs covered:
//!   AC-3: Turn order is initialized with player + spawned creatures
//!   AC-4: COMBAT_EVENT message includes populated enemies array and turn_order
//!   AC-7: End-of-combat cleanup triggers when enemies defeated

use sidequest_game::combat::CombatState;
use sidequest_game::creature_core::CreatureCore;
use sidequest_game::delta::{compute_delta, snapshot};
use sidequest_game::disposition::Disposition;
use sidequest_game::inventory::Inventory;
use sidequest_game::npc::Npc;
use sidequest_game::state::{broadcast_state_changes, GameSnapshot};
use sidequest_protocol::NonBlankString;

// ============================================================================
// Test helpers
// ============================================================================

fn make_hostile_npc(name: &str, hp: i32, max_hp: i32) -> Npc {
    Npc {
        core: CreatureCore {
            name: NonBlankString::new(name).unwrap(),
            description: NonBlankString::new("A hostile creature").unwrap(),
            personality: NonBlankString::new("Aggressive").unwrap(),
            level: 3,
            hp,
            max_hp,
            ac: 14,
            xp: 0,
            inventory: Inventory::default(),
            statuses: vec![],
        },
        voice_id: None,
        disposition: Disposition::new(-20), // Hostile: < -10
        location: None,
        pronouns: None,
        appearance: None,
        age: None,
        build: None,
        height: None,
        distinguishing_features: vec![],
        ocean: None,
        belief_state: Default::default(),
    }
}

fn make_friendly_npc(name: &str) -> Npc {
    Npc {
        core: CreatureCore {
            name: NonBlankString::new(name).unwrap(),
            description: NonBlankString::new("A friendly NPC").unwrap(),
            personality: NonBlankString::new("Helpful").unwrap(),
            level: 1,
            hp: 10,
            max_hp: 10,
            ac: 10,
            xp: 0,
            inventory: Inventory::default(),
            statuses: vec![],
        },
        voice_id: None,
        disposition: Disposition::new(20), // Friendly: > 10
        location: None,
        pronouns: None,
        appearance: None,
        age: None,
        build: None,
        height: None,
        distinguishing_features: vec![],
        ocean: None,
        belief_state: Default::default(),
    }
}


// ============================================================================
// AC-4: COMBAT_EVENT includes populated enemies array when hostile NPCs exist
// ============================================================================

#[test]
fn combat_event_includes_hostile_npcs_as_enemies() {
    // Set up state with combat active and hostile NPCs
    let mut state = GameSnapshot::default();
    state.combat.set_in_combat(true);
    state.combat.set_turn_order(vec!["Player".into(), "Radboar".into()]);
    state.combat.set_current_turn("Player".into());
    state.npcs.push(make_hostile_npc("Radboar", 30, 30));

    // Create a delta where combat changed (before: no combat, after: combat active)
    let before = snapshot(&GameSnapshot::default());
    let after = snapshot(&state);
    let delta = compute_delta(&before, &after);

    assert!(delta.combat_changed(), "delta should detect combat state change");

    let messages = broadcast_state_changes(&delta, &state);

    // Find the CombatEvent message
    let combat_msg = messages
        .iter()
        .find(|m| matches!(m, sidequest_protocol::GameMessage::CombatEvent { .. }))
        .expect("COMBAT_EVENT should be in broadcast messages when combat changes");

    if let sidequest_protocol::GameMessage::CombatEvent { payload, .. } = combat_msg {
        assert!(!payload.enemies.is_empty(), "enemies should NOT be empty when hostile NPCs exist");
        assert_eq!(payload.enemies.len(), 1, "should have exactly 1 enemy (the Radboar)");
        assert_eq!(payload.enemies[0].name, "Radboar");
        assert_eq!(payload.enemies[0].hp, 30);
        assert_eq!(payload.enemies[0].max_hp, 30);
        assert_eq!(payload.enemies[0].ac, Some(14));
    } else {
        panic!("expected CombatEvent variant");
    }
}

#[test]
fn combat_event_excludes_friendly_npcs_from_enemies() {
    let mut state = GameSnapshot::default();
    state.combat.set_in_combat(true);
    state.npcs.push(make_hostile_npc("Radboar", 30, 30));
    state.npcs.push(make_friendly_npc("Fernwalk"));

    let before = snapshot(&GameSnapshot::default());
    let after = snapshot(&state);
    let delta = compute_delta(&before, &after);

    let messages = broadcast_state_changes(&delta, &state);
    let combat_msg = messages
        .iter()
        .find(|m| matches!(m, sidequest_protocol::GameMessage::CombatEvent { .. }))
        .expect("COMBAT_EVENT should be present");

    if let sidequest_protocol::GameMessage::CombatEvent { payload, .. } = combat_msg {
        assert_eq!(payload.enemies.len(), 1, "only hostile NPCs should appear as enemies");
        assert_eq!(payload.enemies[0].name, "Radboar");
    }
}

// ============================================================================
// AC-3: Turn order is initialized with player + creatures
// ============================================================================

#[test]
fn combat_event_includes_turn_order_when_set() {
    let mut state = GameSnapshot::default();
    state.combat.set_in_combat(true);
    state.combat.set_turn_order(vec!["Player".into(), "Radboar".into(), "Spore Crawler".into()]);
    state.combat.set_current_turn("Player".into());
    state.npcs.push(make_hostile_npc("Radboar", 30, 30));
    state.npcs.push(make_hostile_npc("Spore Crawler", 15, 15));

    let before = snapshot(&GameSnapshot::default());
    let after = snapshot(&state);
    let delta = compute_delta(&before, &after);

    let messages = broadcast_state_changes(&delta, &state);
    let combat_msg = messages
        .iter()
        .find(|m| matches!(m, sidequest_protocol::GameMessage::CombatEvent { .. }))
        .expect("COMBAT_EVENT should be present");

    if let sidequest_protocol::GameMessage::CombatEvent { payload, .. } = combat_msg {
        assert_eq!(
            payload.turn_order,
            vec!["Player", "Radboar", "Spore Crawler"],
            "turn_order should contain player and all hostile creatures"
        );
        assert_eq!(payload.current_turn, "Player", "current_turn should be set");
    }
}

// ============================================================================
// AC-7: End-of-combat cleanup — enemies cleared when combat ends
// ============================================================================

#[test]
fn combat_event_clears_enemies_when_combat_ends() {
    // Before: in combat with enemies
    let mut before_state = GameSnapshot::default();
    before_state.combat.set_in_combat(true);
    before_state.combat.set_turn_order(vec!["Player".into(), "Radboar".into()]);
    before_state.npcs.push(make_hostile_npc("Radboar", 30, 30));

    // After: combat ended (enemies defeated)
    let mut after_state = before_state.clone();
    after_state.combat.set_in_combat(false);
    after_state.combat.set_turn_order(vec![]);
    after_state.combat.set_current_turn(String::new());
    // NPC is dead (hp = 0)
    after_state.npcs[0].core.hp = 0;

    let before = snapshot(&before_state);
    let after = snapshot(&after_state);
    let delta = compute_delta(&before, &after);

    assert!(delta.combat_changed());

    let messages = broadcast_state_changes(&delta, &after_state);
    let combat_msg = messages
        .iter()
        .find(|m| matches!(m, sidequest_protocol::GameMessage::CombatEvent { .. }))
        .expect("COMBAT_EVENT should fire when combat ends");

    if let sidequest_protocol::GameMessage::CombatEvent { payload, .. } = combat_msg {
        assert!(!payload.in_combat, "in_combat should be false after combat ends");
        // Dead hostile NPCs with 0 HP are still in the list but combat is over
        assert!(payload.turn_order.is_empty(), "turn_order should be empty after combat");
        assert!(payload.current_turn.is_empty(), "current_turn should be empty after combat");
    }
}

// ============================================================================
// AC-4: Multiple hostile NPCs all appear in enemies list
// ============================================================================

#[test]
fn combat_event_includes_multiple_enemies() {
    let mut state = GameSnapshot::default();
    state.combat.set_in_combat(true);
    state.npcs.push(make_hostile_npc("Radboar", 30, 30));
    state.npcs.push(make_hostile_npc("Spore Crawler", 15, 15));
    state.npcs.push(make_hostile_npc("Dust Wraith", 25, 40));

    let before = snapshot(&GameSnapshot::default());
    let after = snapshot(&state);
    let delta = compute_delta(&before, &after);

    let messages = broadcast_state_changes(&delta, &state);
    let combat_msg = messages
        .iter()
        .find(|m| matches!(m, sidequest_protocol::GameMessage::CombatEvent { .. }))
        .expect("COMBAT_EVENT should be present");

    if let sidequest_protocol::GameMessage::CombatEvent { payload, .. } = combat_msg {
        assert_eq!(payload.enemies.len(), 3, "all hostile NPCs should be enemies");
        let names: Vec<&str> = payload.enemies.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"Radboar"));
        assert!(names.contains(&"Spore Crawler"));
        assert!(names.contains(&"Dust Wraith"));
        // Verify HP values are correct
        let wraith = payload.enemies.iter().find(|e| e.name == "Dust Wraith").unwrap();
        assert_eq!(wraith.hp, 25, "enemy HP should reflect current HP");
        assert_eq!(wraith.max_hp, 40, "enemy max_hp should be set");
    }
}

// ============================================================================
// AC-4: Enemy HP updates reflect damage taken during combat
// ============================================================================

#[test]
fn combat_event_reflects_damaged_enemy_hp() {
    let mut state = GameSnapshot::default();
    state.combat.set_in_combat(true);
    // Radboar took 10 damage
    state.npcs.push(make_hostile_npc("Radboar", 20, 30));

    let before = snapshot(&GameSnapshot::default());
    let after = snapshot(&state);
    let delta = compute_delta(&before, &after);

    let messages = broadcast_state_changes(&delta, &state);
    let combat_msg = messages
        .iter()
        .find(|m| matches!(m, sidequest_protocol::GameMessage::CombatEvent { .. }))
        .expect("COMBAT_EVENT should be present");

    if let sidequest_protocol::GameMessage::CombatEvent { payload, .. } = combat_msg {
        assert_eq!(payload.enemies[0].hp, 20, "enemy HP should show damage taken");
        assert_eq!(payload.enemies[0].max_hp, 30, "max_hp unchanged");
    }
}

// ============================================================================
// CombatPatch: turn_order and current_turn should be applied to CombatState
// ============================================================================

#[test]
fn combat_patch_applies_turn_order() {
    let mut combat = CombatState::new();
    let patch = sidequest_game::state::CombatPatch {
        advance_round: false,
        in_combat: Some(true),
        hp_changes: None,
        turn_order: Some(vec!["Player".into(), "Goblin".into()]),
        current_turn: Some("Player".into()),
        available_actions: None,
        drama_weight: None,
    };

    // Apply the patch to combat state — this is what the server SHOULD do
    if let Some(in_combat) = patch.in_combat {
        combat.set_in_combat(in_combat);
    }
    if let Some(ref order) = patch.turn_order {
        combat.set_turn_order(order.clone());
    }
    if let Some(ref turn) = patch.current_turn {
        combat.set_current_turn(turn.clone());
    }

    assert!(combat.in_combat());
    assert_eq!(combat.turn_order(), &["Player", "Goblin"]);
    assert_eq!(combat.current_turn(), Some("Player"));
}

// ============================================================================
// No combat event when combat state hasn't changed
// ============================================================================

#[test]
fn no_combat_event_when_combat_unchanged() {
    let state = GameSnapshot::default();
    let before = snapshot(&state);
    let after = snapshot(&state);
    let delta = compute_delta(&before, &after);

    assert!(!delta.combat_changed());

    let messages = broadcast_state_changes(&delta, &state);
    let combat_msg = messages
        .iter()
        .find(|m| matches!(m, sidequest_protocol::GameMessage::CombatEvent { .. }));

    assert!(combat_msg.is_none(), "no COMBAT_EVENT when combat state is unchanged");
}

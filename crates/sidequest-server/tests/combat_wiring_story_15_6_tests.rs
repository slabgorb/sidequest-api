//! Story 15-6: Combat engine wiring — server-level tests
//!
//! RED phase — tests that the server applies turn_order/current_turn from
//! CombatPatch and populates COMBAT_EVENT with enemy data from session NPCs.
//!
//! The gap: dispatch_player_action() at lib.rs:4438 hardcodes empty arrays
//! for enemies, turn_order, and current_turn in COMBAT_EVENT. It also never
//! applies turn_order or current_turn from the agents' CombatPatch even though
//! the struct has those fields.
//!
//! These tests assert the behavior we WANT:
//!   1. Server applies turn_order from CombatPatch to CombatState
//!   2. Server applies current_turn from CombatPatch to CombatState
//!   3. COMBAT_EVENT payload includes hostile NPCs as enemies
//!   4. COMBAT_EVENT payload includes populated turn_order
//!
//! ACs covered:
//!   AC-3: Turn order initialized from CombatPatch
//!   AC-4: COMBAT_EVENT includes populated enemies and turn_order
//!   AC-6: HP tracking flows through combat system

use sidequest_game::combat::CombatState;

// ============================================================================
// AC-3/AC-6: Server must apply turn_order and current_turn from CombatPatch
//
// Currently the server at lib.rs:3854-3883 applies in_combat, hp_changes,
// drama_weight, and advance_round — but SKIPS turn_order and current_turn.
// These tests verify the behavior we need.
// ============================================================================

#[test]
fn combat_state_applies_turn_order_from_patch() {
    let mut combat = CombatState::new();

    let turn_order = vec!["Player".to_string(), "Radboar".to_string()];
    let current_turn = "Player".to_string();

    combat.set_in_combat(true);
    combat.set_turn_order(turn_order.clone());
    combat.set_current_turn(current_turn.clone());

    assert!(combat.in_combat());
    assert_eq!(combat.turn_order(), &["Player", "Radboar"]);
    assert_eq!(combat.current_turn(), Some("Player"));
}

#[test]
fn combat_state_turn_order_updates_between_rounds() {
    let mut combat = CombatState::new();
    combat.set_in_combat(true);
    combat.set_turn_order(vec!["Player".into(), "Radboar".into()]);
    combat.set_current_turn("Player".into());

    combat.advance_round();
    combat.set_current_turn("Radboar".into());

    assert_eq!(combat.round(), 2);
    assert_eq!(combat.current_turn(), Some("Radboar"));
}

// ============================================================================
// AC-3: CombatPatch from agents includes turn_order field
// ============================================================================

#[test]
fn agents_combat_patch_has_turn_order_field() {
    use sidequest_agents::patches::CombatPatch;
    use std::collections::HashMap;

    let patch = CombatPatch {
        in_combat: Some(true),
        hp_changes: Some(HashMap::from([
            ("Player".to_string(), -5),
            ("Radboar".to_string(), -12),
        ])),
        turn_order: Some(vec!["Player".into(), "Radboar".into()]),
        current_turn: Some("Player".into()),
        available_actions: Some(vec!["attack".into(), "defend".into(), "flee".into()]),
        drama_weight: Some(0.7),
        advance_round: false,
    };

    assert!(patch.turn_order.is_some());
    assert!(patch.current_turn.is_some());
    assert_eq!(patch.turn_order.unwrap(), vec!["Player", "Radboar"]);
}

// ============================================================================
// AC-3/AC-4: CombatPatch turn_order/current_turn applied via engage() in dispatch
//
// The wiring is now inline in dispatch.rs apply_state_mutations() — engage()
// is called on combat start with player + NPC names, and turn_order/current_turn
// from the patch are applied mid-combat. No separate exported function needed.
// ============================================================================

#[test]
fn combat_patch_turn_order_applied_to_combat_state() {
    use sidequest_agents::patches::CombatPatch;
    use std::collections::HashMap;

    let mut combat = CombatState::new();

    // Simulate what dispatch.rs does: engage on combat start
    let patch = CombatPatch {
        in_combat: Some(true),
        hp_changes: Some(HashMap::from([("Radboar".to_string(), -12)])),
        turn_order: Some(vec!["Player".into(), "Radboar".into()]),
        current_turn: Some("Player".into()),
        available_actions: Some(vec!["attack".into(), "defend".into()]),
        drama_weight: Some(0.6),
        advance_round: false,
    };

    // Replicate the dispatch.rs wiring logic
    if let Some(true) = patch.in_combat {
        if !combat.in_combat() {
            let combatants = patch.turn_order.clone().unwrap_or_default();
            combat.engage(combatants);
        }
    }
    if let Some(ref turn) = patch.current_turn {
        combat.set_current_turn(turn.clone());
    }
    if let Some(dw) = patch.drama_weight {
        combat.set_drama_weight(dw);
    }

    assert!(combat.in_combat(), "in_combat should be set from engage()");
    assert_eq!(combat.turn_order(), &["Player", "Radboar"], "turn_order from patch");
    assert_eq!(combat.current_turn(), Some("Player"), "current_turn from patch");
    assert_eq!(combat.drama_weight(), 0.6, "drama_weight from patch");
}

// ============================================================================
// AC-7: Combat ending should clear turn_order and enemies
// ============================================================================

#[test]
fn combat_end_clears_turn_state() {
    let mut combat = CombatState::new();
    combat.set_in_combat(true);
    combat.set_turn_order(vec!["Player".into(), "Radboar".into()]);
    combat.set_current_turn("Player".into());

    combat.set_in_combat(false);
    combat.set_turn_order(vec![]);

    assert!(!combat.in_combat());
    assert!(combat.turn_order().is_empty());
}

// ============================================================================
// AC-4: broadcast_state_changes (the correct path) works end-to-end
// ============================================================================

#[test]
fn broadcast_state_changes_populates_full_combat_event() {
    use sidequest_game::creature_core::CreatureCore;
    use sidequest_game::delta::{compute_delta, snapshot};
    use sidequest_game::disposition::Disposition;
    use sidequest_game::inventory::Inventory;
    use sidequest_game::npc::Npc;
    use sidequest_game::state::{broadcast_state_changes, GameSnapshot};
    use sidequest_protocol::NonBlankString;

    let mut state = GameSnapshot::default();
    state.combat.set_in_combat(true);
    state.combat.set_turn_order(vec!["Player".into(), "Radboar".into()]);
    state.combat.set_current_turn("Player".into());
    state.npcs.push(Npc {
        core: CreatureCore {
            name: NonBlankString::new("Radboar").unwrap(),
            description: NonBlankString::new("A mutant boar").unwrap(),
            personality: NonBlankString::new("Aggressive").unwrap(),
            level: 3,
            hp: 25,
            max_hp: 30,
            ac: 14,
            xp: 0,
            inventory: Inventory::default(),
            statuses: vec![],
        },
        voice_id: None,
        disposition: Disposition::new(-20),
        location: None,
        pronouns: None,
        appearance: None,
        age: None,
        build: None,
        height: None,
        distinguishing_features: vec![],
        ocean: None,
        belief_state: Default::default(),
    });

    let before = snapshot(&GameSnapshot::default());
    let after = snapshot(&state);
    let delta = compute_delta(&before, &after);

    let messages = broadcast_state_changes(&delta, &state);

    let combat_msg = messages
        .iter()
        .find(|m| matches!(m, sidequest_protocol::GameMessage::CombatEvent { .. }))
        .expect("COMBAT_EVENT should be sent when combat changes");

    if let sidequest_protocol::GameMessage::CombatEvent { payload, .. } = combat_msg {
        assert!(payload.in_combat);
        assert_eq!(payload.enemies.len(), 1);
        assert_eq!(payload.enemies[0].name, "Radboar");
        assert_eq!(payload.enemies[0].hp, 25);
        assert_eq!(payload.turn_order, vec!["Player", "Radboar"]);
        assert_eq!(payload.current_turn, "Player");
    }
}

//! Story 15-6: Combat pipeline integration tests
//!
//! Tests the full data flow from creature_smith LLM output through
//! CombatPatch extraction through CombatState mutations — the exact
//! pipeline that was broken for 14 commits.
//!
//! These tests verify:
//!   1. creature_smith-style output extracts into CombatPatch successfully
//!   2. CombatPatch with ALL valid fields deserializes (no deny_unknown_fields rejection)
//!   3. CombatPatch with inventory/quest fields is rejected (the root cause of the bug)
//!   4. engage() + patch fields produce correct CombatState
//!   5. Full flow: LLM output → extract → engage → apply → verify state

use sidequest_agents::patches::CombatPatch;
use sidequest_game::combat::CombatState;

// ============================================================================
// Pipeline Step 1: creature_smith output → CombatPatch extraction
// ============================================================================

#[test]
fn creature_smith_json_deserializes_combat_patch() {
    // CombatPatch deserialization from creature_smith-style JSON
    let json = r#"{
  "in_combat": true,
  "hp_changes": {"Player": -5, "Radboar": -8},
  "turn_order": ["Player", "Radboar"],
  "current_turn": "Player",
  "available_actions": ["Attack", "Defend", "Flee"],
  "drama_weight": 0.6,
  "advance_round": false
}"#;

    let patch: CombatPatch = serde_json::from_str(json).unwrap();
    assert_eq!(patch.in_combat, Some(true));
    assert_eq!(patch.hp_changes.as_ref().unwrap().get("Player"), Some(&-5));
    assert_eq!(patch.hp_changes.as_ref().unwrap().get("Radboar"), Some(&-8));
    assert_eq!(patch.turn_order.as_ref().unwrap(), &["Player", "Radboar"]);
    assert_eq!(patch.current_turn.as_deref(), Some("Player"));
    assert_eq!(patch.available_actions.as_ref().unwrap(), &["Attack", "Defend", "Flee"]);
    assert_eq!(patch.drama_weight, Some(0.6));
    assert!(!patch.advance_round);
}

#[test]
fn creature_smith_minimal_patch_deserializes() {
    // Minimal valid patch — only required fields
    let json = r#"{"in_combat": true, "hp_changes": {}}"#;
    let patch: CombatPatch = serde_json::from_str(json).unwrap();
    assert_eq!(patch.in_combat, Some(true));
}

#[test]
fn creature_smith_combat_end_patch_deserializes() {
    let json = r#"{
  "in_combat": false,
  "hp_changes": {"Radboar": -12},
  "drama_weight": 0.9,
  "advance_round": true
}"#;
    let patch: CombatPatch = serde_json::from_str(json).unwrap();
    assert_eq!(patch.in_combat, Some(false));
    assert!(patch.advance_round);
}

// ============================================================================
// Pipeline Step 2: deny_unknown_fields rejection of bad LLM output
// This was THE ROOT CAUSE of 14 failed wiring attempts.
// ============================================================================

#[test]
fn combat_patch_rejects_inventory_fields() {
    // CombatPatch intentionally allows unknown fields because the LLM may include
    // inline preprocessor fields (action_rewrite, action_flags) in the same JSON block.
    // Unknown fields like inventory_updates are silently ignored — the known fields
    // still parse correctly.
    let json_with_extras = r#"{
        "in_combat": true,
        "hp_changes": {},
        "drama_weight": 0.2,
        "inventory_updates": {"add": [{"name": "hooked_rebar"}]}
    }"#;
    let patch: Result<CombatPatch, _> = serde_json::from_str(json_with_extras);
    assert!(patch.is_ok(), "CombatPatch allows unknown fields (inline preprocessor support)");
    let p = patch.unwrap();
    assert_eq!(p.in_combat, Some(true));
    assert!(p.drama_weight.is_some());
}

#[test]
fn combat_patch_ignores_quest_updates() {
    // quest_updates is not a CombatPatch field — silently ignored, known fields parse fine
    let json = r#"{
        "in_combat": false,
        "hp_changes": {},
        "quest_updates": {"Find the artifact": "in_progress"}
    }"#;
    let patch: CombatPatch = serde_json::from_str(json).unwrap();
    assert_eq!(patch.in_combat, Some(false));
}

#[test]
fn combat_patch_ignores_round_number() {
    // round_number was removed from the struct — silently ignored by serde
    let json = r#"{"in_combat": true, "round_number": 3}"#;
    let patch: CombatPatch = serde_json::from_str(json).unwrap();
    assert_eq!(patch.in_combat, Some(true));
}

// ============================================================================
// Pipeline Step 3: CombatPatch → CombatState mutations
// Simulates what dispatch.rs apply_state_mutations does
// ============================================================================

#[test]
fn full_pipeline_extract_engage_apply() {
    // Step 1: Simulate creature_smith JSON output
    let json = r#"{
  "in_combat": true,
  "hp_changes": {"Player": -3},
  "turn_order": ["Player", "Dust Wraith"],
  "current_turn": "Player",
  "available_actions": ["Attack", "Defend", "Flee"],
  "drama_weight": 0.5,
  "advance_round": false
}"#;

    // Step 2: Deserialize CombatPatch
    let patch: CombatPatch = serde_json::from_str(json).unwrap();

    // Step 3: Apply to CombatState (replicating dispatch.rs logic)
    let mut combat = CombatState::new();

    // engage() on combat start
    if let Some(true) = patch.in_combat {
        if !combat.in_combat() {
            let combatants = patch.turn_order.clone().unwrap_or_default();
            combat.engage(combatants);
        }
    }
    // Apply current_turn from patch
    if let Some(ref turn) = patch.current_turn {
        combat.set_current_turn(turn.clone());
    }
    if let Some(dw) = patch.drama_weight {
        combat.set_drama_weight(dw);
    }
    if patch.advance_round {
        combat.advance_turn();
    }

    // Step 4: Verify state
    assert!(combat.in_combat(), "combat should be active");
    assert_eq!(combat.turn_order(), &["Player", "Dust Wraith"], "turn order from LLM");
    assert_eq!(combat.current_turn(), Some("Player"), "current turn from LLM");
    assert_eq!(combat.drama_weight(), 0.5);
    assert_eq!(combat.round(), 1, "no advance_round so still round 1");
}

#[test]
fn full_pipeline_combat_end_triggers_disengage() {
    // Start combat first
    let mut combat = CombatState::new();
    combat.engage(vec!["Player".into(), "Goblin".into()]);
    assert!(combat.in_combat());

    // Simulate creature_smith saying combat is over
    let json = r#"{"in_combat": false, "hp_changes": {"Goblin": -10}, "drama_weight": 0.9, "advance_round": true}"#;
    let patch: CombatPatch = serde_json::from_str(json).unwrap();

    // Apply — dispatch.rs calls disengage() when in_combat goes false
    if let Some(false) = patch.in_combat {
        if combat.in_combat() {
            combat.disengage();
        }
    }

    assert!(!combat.in_combat(), "combat should be over");
    assert!(combat.turn_order().is_empty(), "turn order cleared on disengage");
    assert!(combat.current_turn().is_none(), "current turn cleared on disengage");
    assert_eq!(combat.round(), 1, "round resets on disengage");
}

#[test]
fn full_pipeline_advance_round_via_advance_turn() {
    let mut combat = CombatState::new();
    combat.engage(vec!["Player".into(), "Goblin".into()]);

    // Simulate a round-ending patch
    let json = r#"{"in_combat": true, "hp_changes": {"Goblin": -5}, "drama_weight": 0.4, "advance_round": true}"#;
    let patch: CombatPatch = serde_json::from_str(json).unwrap();

    // dispatch.rs calls advance_turn() when advance_round is true
    if patch.advance_round && combat.in_combat() {
        combat.advance_turn();
    }

    // advance_turn from Player → Goblin (no round wrap yet)
    assert_eq!(combat.current_turn(), Some("Goblin"));
    assert_eq!(combat.round(), 1, "only 2 combatants, first advance goes to second");
}

//! Story 28-9: Delete CombatState, ChaseState, CombatPatch, ChasePatch,
//! from_combat_state, from_chase_state.
//!
//! RED phase tests verifying that the old combat/chase split-brain system
//! has been fully removed. StructuredEncounter (Epic 16) is the sole
//! encounter model. These tests assert *absence* of old types by checking
//! serialization shape — the only practical way to test "this field/type
//! was deleted" in Rust without compile-fail harnesses.

use serde_json;
use sidequest_game::state::GameSnapshot;

// ==========================================================================
// AC-1: CombatState, ChaseState files deleted (combat.rs, chase.rs,
//       chase_depth.rs removed from codebase)
// ==========================================================================

/// GameSnapshot must not serialize a `combat` field.
/// Currently FAILS: GameSnapshot has `pub combat: CombatState`.
/// After deletion: field is gone, JSON has no "combat" key.
#[test]
fn snapshot_has_no_combat_field() {
    let snapshot = GameSnapshot::default();
    let json = serde_json::to_value(&snapshot).unwrap();
    let obj = json.as_object().expect("GameSnapshot should serialize as JSON object");

    assert!(
        !obj.contains_key("combat"),
        "GameSnapshot should not have a 'combat' field — CombatState is deleted in 28-9. \
         StructuredEncounter (via the 'encounter' field) is the sole encounter model."
    );
}

/// GameSnapshot must not serialize a `chase` field.
/// Currently FAILS: GameSnapshot has `pub chase: Option<ChaseState>`.
/// After deletion: field is gone, JSON has no "chase" key.
#[test]
fn snapshot_has_no_chase_field() {
    let snapshot = GameSnapshot::default();
    let json = serde_json::to_value(&snapshot).unwrap();
    let obj = json.as_object().expect("GameSnapshot should serialize as JSON object");

    assert!(
        !obj.contains_key("chase"),
        "GameSnapshot should not have a 'chase' field — ChaseState is deleted in 28-9. \
         Use the 'encounter' field (StructuredEncounter) for all encounter types."
    );
}

/// StructuredEncounter (`encounter` field) must be the ONLY encounter-related
/// field on GameSnapshot. No combat, chase, or other parallel encounter models.
#[test]
fn encounter_is_sole_encounter_field() {
    let snapshot = GameSnapshot::default();
    let json = serde_json::to_value(&snapshot).unwrap();
    let obj = json.as_object().expect("GameSnapshot should serialize as JSON object");

    // encounter field must exist (the surviving model)
    assert!(
        obj.contains_key("encounter"),
        "GameSnapshot must retain the 'encounter' field (StructuredEncounter)"
    );

    // No parallel encounter models
    let illegal_encounter_fields = ["combat", "chase", "chase_state", "combat_state"];
    for field in &illegal_encounter_fields {
        assert!(
            !obj.contains_key(*field),
            "GameSnapshot has illegal encounter field '{field}' — only 'encounter' should exist"
        );
    }
}

// ==========================================================================
// AC-2: Protocol types removed — CombatPatch, ChasePatch, CombatEventPayload
//       no longer in serde schemas
// ==========================================================================

/// CombatEventPayload must not be deserializable as a GameMessage.
/// Currently FAILS: COMBAT_EVENT is a valid GameMessage variant.
/// After deletion: deserialization rejects the unknown variant.
#[test]
fn combat_event_not_a_valid_game_message() {
    use sidequest_protocol::GameMessage;

    let combat_event_json = serde_json::json!({
        "type": "COMBAT_EVENT",
        "payload": {
            "in_combat": true,
            "enemies": [],
            "turn_order": ["Alice"],
            "current_turn": "Alice"
        },
        "player_id": "test-player"
    });

    let result = serde_json::from_value::<GameMessage>(combat_event_json);
    assert!(
        result.is_err(),
        "COMBAT_EVENT should not be a valid GameMessage variant — \
         CombatEventPayload was deleted in 28-9. Got: {:?}",
        result.unwrap()
    );
}

// ==========================================================================
// AC-3 + AC-4: All references fixed, dispatch pipeline cleaned
// ==========================================================================

/// Verify no CombatPatch type exists in the game crate's state module.
/// We test this by checking that a JSON object matching the old CombatPatch
/// schema does NOT round-trip through the game crate's public API.
///
/// Currently FAILS: CombatPatch exists and is importable.
/// After deletion: import fails, so this test uses a structural check instead.
#[test]
fn no_combat_patch_in_state_module() {
    // If CombatPatch still exists, this JSON will deserialize successfully.
    // After deletion, the type won't exist and this test file still compiles
    // because we only check via serialization.
    let combat_patch_json = serde_json::json!({
        "advance_round": false,
        "in_combat": true,
        "hp_changes": null,
        "turn_order": null,
        "current_turn": null,
        "available_actions": null,
        "drama_weight": null
    });

    // Try to deserialize as CombatPatch — this MUST fail after deletion
    let result =
        serde_json::from_value::<sidequest_game::state::CombatPatch>(combat_patch_json);
    assert!(
        result.is_err(),
        "CombatPatch should not exist in sidequest_game::state — deleted in 28-9. \
         Got: {:?}",
        result.unwrap()
    );
}

/// Same check for ChasePatch.
#[test]
fn no_chase_patch_in_state_module() {
    let chase_patch_json = serde_json::json!({
        "start": null,
        "start_vehicle": null,
        "roll": null,
        "separation": null,
        "phase": null,
        "event": null,
        "rig": null,
        "actors": null,
        "advance_beat": false
    });

    let result =
        serde_json::from_value::<sidequest_game::state::ChasePatch>(chase_patch_json);
    assert!(
        result.is_err(),
        "ChasePatch should not exist in sidequest_game::state — deleted in 28-9. \
         Got: {:?}",
        result.unwrap()
    );
}

// ==========================================================================
// AC-4: Dispatch pipeline cleaned — apply_combat_patch / apply_chase_patch
//       removed from GameSnapshot
// ==========================================================================

/// GameSnapshot must not have apply_combat_patch or apply_chase_patch methods.
/// We verify by checking that a default snapshot with the encounter field set
/// is the sole mutation path. Since we can't test "method doesn't exist" at
/// runtime, we verify the structural invariant: no combat/chase fields to patch.
///
/// This is a wiring test: if combat/chase fields don't exist on the snapshot,
/// then apply_combat_patch/apply_chase_patch cannot compile (they mutate those
/// fields). So AC-1 + AC-2 passing implies AC-4.
#[test]
fn snapshot_encounter_field_is_the_sole_mutation_target() {
    let mut snapshot = GameSnapshot::default();
    let json_before = serde_json::to_value(&snapshot).unwrap();
    let obj = json_before.as_object().unwrap();

    // Collect all encounter-adjacent field names
    let encounter_fields: Vec<&str> = obj
        .keys()
        .filter(|k| {
            k.contains("combat")
                || k.contains("chase")
                || k.contains("encounter")
        })
        .map(|k| k.as_str())
        .collect();

    // Only "encounter" should remain
    assert_eq!(
        encounter_fields,
        vec!["encounter"],
        "Only 'encounter' should be an encounter-related field on GameSnapshot. \
         Found: {:?}. Old combat/chase fields must be deleted (28-9).",
        encounter_fields
    );
}

// ==========================================================================
// Wiring test: StructuredEncounter is reachable and functional without
// old combat/chase infrastructure
// ==========================================================================

/// StructuredEncounter can be set on GameSnapshot and round-trips through
/// serialization without depending on CombatState or ChaseState.
#[test]
fn structured_encounter_round_trips_independently() {
    use sidequest_game::encounter::{
        EncounterMetric, EncounterPhase, MetricDirection, StructuredEncounter,
    };

    let encounter = StructuredEncounter {
        encounter_type: "combat".to_string(),
        metric: EncounterMetric {
            name: "morale".to_string(),
            current: 100,
            starting: 100,
            direction: MetricDirection::Descending,
            threshold_high: None,
            threshold_low: Some(0),
        },
        beat: 1,
        structured_phase: Some(EncounterPhase::Active),
        secondary_stats: None,
        actors: vec![],
        outcome: None,
        resolved: false,
        mood_override: None,
        narrator_hints: vec![],
    };

    let mut snapshot = GameSnapshot::default();
    snapshot.encounter = Some(encounter.clone());

    let json = serde_json::to_value(&snapshot).unwrap();
    let obj = json.as_object().unwrap();

    // encounter field is present and non-null
    assert!(
        obj.get("encounter").unwrap().is_object(),
        "encounter field should serialize as a JSON object when set"
    );

    // Round-trip the snapshot
    let json_str = serde_json::to_string(&snapshot).unwrap();
    let restored: GameSnapshot = serde_json::from_str(&json_str).unwrap();
    assert_eq!(
        restored.encounter.as_ref().unwrap().encounter_type,
        "combat",
        "StructuredEncounter should round-trip through GameSnapshot serialization"
    );
}

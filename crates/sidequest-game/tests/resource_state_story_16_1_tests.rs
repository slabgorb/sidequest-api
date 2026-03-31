//! Story 16-1: Resource state tracking and persistence tests (RED phase).
//!
//! Tests that GameSnapshot tracks resource values, supports delta application,
//! and survives serde roundtrip (save/load).
//!
//! ACs tested:
//!   AC3 (Track): Resource values update when narrator mentions spend/gain
//!   AC4 (Persist): Resource values survive save/load cycle
//!   AC5 (All genres): Works for packs with and without resource declarations

use sidequest_game::state::GameSnapshot;
use std::collections::HashMap;

// =========================================================================
// AC4: GameSnapshot.resource_state persists via serde roundtrip
// =========================================================================

#[test]
fn game_snapshot_resource_state_json_roundtrip() {
    let mut snapshot = GameSnapshot::default();
    snapshot
        .resource_state
        .insert("luck".to_string(), 4.0);
    snapshot
        .resource_state
        .insert("heat".to_string(), 2.5);

    let json = serde_json::to_string(&snapshot).unwrap();
    let restored: GameSnapshot = serde_json::from_str(&json).unwrap();

    assert_eq!(restored.resource_state.len(), 2);
    assert!(
        (restored.resource_state["luck"] - 4.0).abs() < f64::EPSILON,
        "luck should survive roundtrip"
    );
    assert!(
        (restored.resource_state["heat"] - 2.5).abs() < f64::EPSILON,
        "heat should survive roundtrip"
    );
}

// =========================================================================
// AC5: Old saves without resource_state deserialize with empty map
// =========================================================================

#[test]
fn old_save_without_resource_state_deserializes() {
    // Simulate an old save that doesn't have the resource_state field
    let json = r#"{
        "genre_slug": "spaghetti_western",
        "world_slug": "dusty_gulch",
        "characters": [],
        "npcs": [],
        "location": "Saloon",
        "time_of_day": "high_noon",
        "quest_log": {},
        "notes": [],
        "narrative_log": [],
        "combat": { "round": 0, "participants": [], "damage_log": [], "status_effects": [], "available_actions": [] },
        "atmosphere": "tense",
        "current_region": "town",
        "discovered_regions": [],
        "discovered_routes": [],
        "turn_manager": { "round": 0, "phase": "input_collection", "barrier": null }
    }"#;

    let snapshot: GameSnapshot = serde_json::from_str(json).unwrap();
    assert!(
        snapshot.resource_state.is_empty(),
        "old saves without resource_state should default to empty HashMap"
    );
}

// =========================================================================
// AC5: GameSnapshot.resource_state defaults to empty
// =========================================================================

#[test]
fn game_snapshot_default_has_empty_resource_state() {
    let snapshot = GameSnapshot::default();
    assert!(
        snapshot.resource_state.is_empty(),
        "default GameSnapshot should have empty resource_state"
    );
}

// =========================================================================
// AC3: Resource delta application
// =========================================================================

#[test]
fn apply_resource_delta_updates_value() {
    let mut snapshot = GameSnapshot::default();
    snapshot
        .resource_state
        .insert("luck".to_string(), 4.0);

    // Apply delta: spend 1 luck
    let deltas: HashMap<String, f64> = [("luck".to_string(), -1.0)].into();
    snapshot.apply_resource_deltas(&deltas);

    assert!(
        (snapshot.resource_state["luck"] - 3.0).abs() < f64::EPSILON,
        "luck should decrease by 1 after delta application"
    );
}

#[test]
fn apply_resource_delta_adds_positive() {
    let mut snapshot = GameSnapshot::default();
    snapshot
        .resource_state
        .insert("heat".to_string(), 2.0);

    let deltas: HashMap<String, f64> = [("heat".to_string(), 1.5)].into();
    snapshot.apply_resource_deltas(&deltas);

    assert!(
        (snapshot.resource_state["heat"] - 3.5).abs() < f64::EPSILON,
        "heat should increase by 1.5 after delta application"
    );
}

#[test]
fn apply_resource_delta_clamps_to_max() {
    // This test depends on the implementation knowing about max values.
    // For 16-1's lightweight approach (HashMap<String, f64> only), clamping
    // may require resource declarations to be passed alongside deltas.
    // The test documents the expected behavior — Dev decides the API shape.
    let mut snapshot = GameSnapshot::default();
    snapshot
        .resource_state
        .insert("luck".to_string(), 5.0);

    // Attempt to exceed max (6.0 for luck)
    let deltas: HashMap<String, f64> = [("luck".to_string(), 3.0)].into();

    // If apply_resource_deltas takes declarations for bounds:
    // snapshot.apply_resource_deltas(&deltas, &declarations);
    // For now, test the basic delta path:
    snapshot.apply_resource_deltas(&deltas);

    // Value should be clamped (if declarations available) or 8.0 (if not)
    // Dev will determine the API — this test documents the expectation
    let luck = snapshot.resource_state["luck"];
    assert!(
        luck <= 6.0,
        "luck should be clamped to max of 6.0, got: {luck}"
    );
}

#[test]
fn apply_resource_delta_clamps_to_min() {
    let mut snapshot = GameSnapshot::default();
    snapshot
        .resource_state
        .insert("luck".to_string(), 1.0);

    let deltas: HashMap<String, f64> = [("luck".to_string(), -5.0)].into();
    snapshot.apply_resource_deltas(&deltas);

    let luck = snapshot.resource_state["luck"];
    assert!(
        luck >= 0.0,
        "luck should be clamped to min of 0.0, got: {luck}"
    );
}

#[test]
fn apply_resource_delta_ignores_unknown_resource() {
    let mut snapshot = GameSnapshot::default();
    snapshot
        .resource_state
        .insert("luck".to_string(), 3.0);

    // Delta for a resource that doesn't exist in state
    let deltas: HashMap<String, f64> = [("nonexistent".to_string(), 1.0)].into();
    snapshot.apply_resource_deltas(&deltas);

    // Original state unchanged
    assert!(
        (snapshot.resource_state["luck"] - 3.0).abs() < f64::EPSILON,
        "existing resources should be unaffected by unknown deltas"
    );
    // Unknown resource should NOT be created
    assert!(
        !snapshot.resource_state.contains_key("nonexistent"),
        "unknown resources should not be created by delta application"
    );
}

// =========================================================================
// AC3: Multiple deltas applied at once
// =========================================================================

#[test]
fn apply_resource_deltas_updates_multiple_resources() {
    let mut snapshot = GameSnapshot::default();
    snapshot
        .resource_state
        .insert("luck".to_string(), 4.0);
    snapshot
        .resource_state
        .insert("heat".to_string(), 1.0);

    let deltas: HashMap<String, f64> = [
        ("luck".to_string(), -1.0),
        ("heat".to_string(), 2.0),
    ]
    .into();
    snapshot.apply_resource_deltas(&deltas);

    assert!(
        (snapshot.resource_state["luck"] - 3.0).abs() < f64::EPSILON,
        "luck should decrease"
    );
    assert!(
        (snapshot.resource_state["heat"] - 3.0).abs() < f64::EPSILON,
        "heat should increase"
    );
}

// =========================================================================
// AC4: Resource state survives full save/load cycle with other fields
// =========================================================================

#[test]
fn resource_state_persists_alongside_other_snapshot_fields() {
    let mut snapshot = GameSnapshot::default();
    snapshot.genre_slug = "spaghetti_western".to_string();
    snapshot.location = "Saloon".to_string();
    snapshot
        .resource_state
        .insert("luck".to_string(), 2.0);

    let json = serde_json::to_string(&snapshot).unwrap();
    let restored: GameSnapshot = serde_json::from_str(&json).unwrap();

    assert_eq!(restored.genre_slug, "spaghetti_western");
    assert_eq!(restored.location, "Saloon");
    assert!(
        (restored.resource_state["luck"] - 2.0).abs() < f64::EPSILON,
        "resource_state should persist alongside other fields"
    );
}

//! Story 16-10: ResourcePool struct + YAML schema — generic named resource with thresholds
//!
//! RED phase tests. These test the full ResourcePool lifecycle:
//!   AC1: ResourcePool struct serializes/deserializes correctly (serde)
//!   AC2: YAML schema parses resource declarations with thresholds from rules.yaml
//!   AC3: ResourcePatch applies changes and validates bounds
//!   AC4: Genre loader loads resource declarations at pack init
//!   AC5: Threshold crossing detection works
//!   AC6: decay_per_turn reduces current value each turn
//!   AC7: voluntary flag controls whether player can spend

use sidequest_game::state::{
    GameSnapshot, ResourcePatch, ResourcePatchOp, ResourcePool, ResourceThreshold,
};
use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════
// Test helpers
// ═══════════════════════════════════════════════════════════

fn make_pool(name: &str, current: f64, min: f64, max: f64) -> ResourcePool {
    ResourcePool {
        name: name.to_string(),
        label: name.to_string(),
        current,
        min,
        max,
        voluntary: true,
        decay_per_turn: 0.0,
        thresholds: vec![],

    }
}

fn make_pool_with_thresholds(
    name: &str,
    current: f64,
    min: f64,
    max: f64,
    thresholds: Vec<ResourceThreshold>,
) -> ResourcePool {
    ResourcePool {
        name: name.to_string(),
        label: name.to_string(),
        current,
        min,
        max,
        voluntary: true,
        decay_per_turn: 0.0,
        thresholds,

    }
}

fn snapshot_with_pools(pools: Vec<ResourcePool>) -> GameSnapshot {
    let mut snap = GameSnapshot::default();
    for pool in pools {
        snap.resources.insert(pool.name.clone(), pool);
    }
    snap
}

// ═══════════════════════════════════════════════════════════
// AC1: ResourcePool struct serializes/deserializes (serde)
// ═══════════════════════════════════════════════════════════

#[test]
fn resource_pool_json_roundtrip() {
    let pool = ResourcePool {
        name: "luck".to_string(),
        label: "Luck".to_string(),
        current: 3.0,
        min: 0.0,
        max: 6.0,
        voluntary: true,
        decay_per_turn: 0.0,
        thresholds: vec![
            ResourceThreshold {
                at: 1.0,
                event_id: "luck_critical".to_string(),
                narrator_hint: "Luck is nearly exhausted.".to_string(),
            },
            ResourceThreshold {
                at: 0.0,
                event_id: "luck_depleted".to_string(),
                narrator_hint: "Out of luck entirely.".to_string(),
            },
        ],

    };

    let json = serde_json::to_string(&pool).unwrap();
    let restored: ResourcePool = serde_json::from_str(&json).unwrap();

    assert_eq!(restored.name, "luck");
    assert!((restored.current - 3.0).abs() < f64::EPSILON);
    assert!((restored.min - 0.0).abs() < f64::EPSILON);
    assert!((restored.max - 6.0).abs() < f64::EPSILON);
    assert!(restored.voluntary);
    assert!((restored.decay_per_turn - 0.0).abs() < f64::EPSILON);
    assert_eq!(restored.thresholds.len(), 2);
    assert_eq!(restored.thresholds[0].event_id, "luck_critical");
    assert!((restored.thresholds[0].at - 1.0).abs() < f64::EPSILON);
    assert_eq!(restored.thresholds[1].event_id, "luck_depleted");
}

#[test]
fn resource_pool_yaml_roundtrip() {
    let pool = make_pool("heat", 5.0, 0.0, 10.0);
    let yaml = serde_yaml::to_string(&pool).unwrap();
    let restored: ResourcePool = serde_yaml::from_str(&yaml).unwrap();

    assert_eq!(restored.name, "heat");
    assert!((restored.current - 5.0).abs() < f64::EPSILON);
    assert!((restored.max - 10.0).abs() < f64::EPSILON);
}

#[test]
fn resource_pool_derives_clone_debug() {
    let pool = make_pool("luck", 3.0, 0.0, 6.0);
    let cloned = pool.clone();
    assert_eq!(cloned.name, pool.name);
    assert!((cloned.current - pool.current).abs() < f64::EPSILON);
    // Debug derive — just verify it doesn't panic
    let _debug = format!("{:?}", pool);
}

#[test]
fn resource_threshold_json_roundtrip() {
    let threshold = ResourceThreshold {
        at: 1.0,
        event_id: "luck_critical".to_string(),
        narrator_hint: "Running low on luck.".to_string(),
    };

    let json = serde_json::to_string(&threshold).unwrap();
    let restored: ResourceThreshold = serde_json::from_str(&json).unwrap();

    assert!((restored.at - 1.0).abs() < f64::EPSILON);
    assert_eq!(restored.event_id, "luck_critical");
    assert_eq!(restored.narrator_hint, "Running low on luck.");
}

// ═══════════════════════════════════════════════════════════
// AC1: GameSnapshot with resources HashMap
// ═══════════════════════════════════════════════════════════

#[test]
fn game_snapshot_resources_default_empty() {
    let snap = GameSnapshot::default();
    assert!(snap.resources.is_empty(), "default should have no resource pools");
}

#[test]
fn game_snapshot_resources_json_roundtrip() {
    let mut snap = GameSnapshot::default();
    snap.resources.insert(
        "luck".to_string(),
        make_pool("luck", 3.0, 0.0, 6.0),
    );
    snap.resources.insert(
        "heat".to_string(),
        make_pool("heat", 0.0, 0.0, 10.0),
    );

    let json = serde_json::to_string(&snap).unwrap();
    let restored: GameSnapshot = serde_json::from_str(&json).unwrap();

    assert_eq!(restored.resources.len(), 2);
    assert!(restored.resources.contains_key("luck"));
    assert!(restored.resources.contains_key("heat"));
    assert!((restored.resources["luck"].current - 3.0).abs() < f64::EPSILON);
}

#[test]
fn old_save_without_resources_field_deserializes() {
    // Simulate an old save that doesn't have the resources field
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
        "turn_manager": { "round": 0, "phase": "InputCollection", "barrier": null }
    }"#;

    let snapshot: GameSnapshot = serde_json::from_str(json).unwrap();
    assert!(
        snapshot.resources.is_empty(),
        "old saves without resources should default to empty HashMap"
    );
}

// ═══════════════════════════════════════════════════════════
// AC2: YAML schema parses resource declarations with thresholds
// ═══════════════════════════════════════════════════════════

#[test]
fn resource_pool_from_yaml_with_thresholds() {
    let yaml = r#"
name: luck
current: 3
min: 0
max: 6
voluntary: true
decay_per_turn: 0.0
thresholds:
  - at: 1
    event_id: luck_critical
    narrator_hint: "The character's luck is nearly exhausted."
  - at: 0
    event_id: luck_depleted
    narrator_hint: "Out of luck. Everything depends on skill."
"#;

    let pool: ResourcePool = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(pool.name, "luck");
    assert!((pool.current - 3.0).abs() < f64::EPSILON);
    assert!((pool.min - 0.0).abs() < f64::EPSILON);
    assert!((pool.max - 6.0).abs() < f64::EPSILON);
    assert!(pool.voluntary);
    assert_eq!(pool.thresholds.len(), 2);
    assert!((pool.thresholds[0].at - 1.0).abs() < f64::EPSILON);
    assert_eq!(pool.thresholds[0].event_id, "luck_critical");
    assert_eq!(
        pool.thresholds[1].narrator_hint,
        "Out of luck. Everything depends on skill."
    );
}

#[test]
fn resource_pool_from_yaml_without_thresholds_defaults_empty() {
    let yaml = r#"
name: heat
current: 0
min: 0
max: 10
voluntary: false
decay_per_turn: -0.1
"#;

    let pool: ResourcePool = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(pool.name, "heat");
    assert!(!pool.voluntary);
    assert!((pool.decay_per_turn - (-0.1)).abs() < f64::EPSILON);
    assert!(pool.thresholds.is_empty(), "missing thresholds should default to empty vec");
}

// ═══════════════════════════════════════════════════════════
// AC3: ResourcePatch applies changes and validates bounds
// ═══════════════════════════════════════════════════════════

#[test]
fn resource_patch_add_increases_value() {
    let pool = make_pool("luck", 3.0, 0.0, 6.0);
    let mut snap = snapshot_with_pools(vec![pool]);

    let patch = ResourcePatch {
        resource_name: "luck".to_string(),
        operation: ResourcePatchOp::Add,
        value: 2.0,
    };
    let result = snap.apply_resource_patch(&patch);

    assert!(result.is_ok(), "valid add should succeed");
    assert!(
        (snap.resources["luck"].current - 5.0).abs() < f64::EPSILON,
        "luck should be 5.0 after adding 2.0"
    );
}

#[test]
fn resource_patch_subtract_decreases_value() {
    let pool = make_pool("luck", 3.0, 0.0, 6.0);
    let mut snap = snapshot_with_pools(vec![pool]);

    let patch = ResourcePatch {
        resource_name: "luck".to_string(),
        operation: ResourcePatchOp::Subtract,
        value: 1.0,
    };
    let result = snap.apply_resource_patch(&patch);

    assert!(result.is_ok(), "valid subtract should succeed");
    assert!(
        (snap.resources["luck"].current - 2.0).abs() < f64::EPSILON,
        "luck should be 2.0 after subtracting 1.0"
    );
}

#[test]
fn resource_patch_set_replaces_value() {
    let pool = make_pool("luck", 3.0, 0.0, 6.0);
    let mut snap = snapshot_with_pools(vec![pool]);

    let patch = ResourcePatch {
        resource_name: "luck".to_string(),
        operation: ResourcePatchOp::Set,
        value: 5.0,
    };
    let result = snap.apply_resource_patch(&patch);

    assert!(result.is_ok(), "valid set should succeed");
    assert!(
        (snap.resources["luck"].current - 5.0).abs() < f64::EPSILON,
        "luck should be 5.0 after set"
    );
}

#[test]
fn resource_patch_clamps_to_max() {
    let pool = make_pool("luck", 5.0, 0.0, 6.0);
    let mut snap = snapshot_with_pools(vec![pool]);

    let patch = ResourcePatch {
        resource_name: "luck".to_string(),
        operation: ResourcePatchOp::Add,
        value: 10.0,
    };
    let result = snap.apply_resource_patch(&patch);

    assert!(result.is_ok(), "add exceeding max should clamp, not error");
    assert!(
        (snap.resources["luck"].current - 6.0).abs() < f64::EPSILON,
        "luck should be clamped to max 6.0"
    );
}

#[test]
fn resource_patch_clamps_to_min() {
    let pool = make_pool("luck", 2.0, 0.0, 6.0);
    let mut snap = snapshot_with_pools(vec![pool]);

    let patch = ResourcePatch {
        resource_name: "luck".to_string(),
        operation: ResourcePatchOp::Subtract,
        value: 10.0,
    };
    let result = snap.apply_resource_patch(&patch);

    assert!(result.is_ok(), "subtract exceeding current should clamp, not error");
    assert!(
        (snap.resources["luck"].current - 0.0).abs() < f64::EPSILON,
        "luck should be clamped to min 0.0"
    );
}

#[test]
fn resource_patch_set_rejects_below_min() {
    let pool = make_pool("luck", 3.0, 0.0, 6.0);
    let mut snap = snapshot_with_pools(vec![pool]);

    let patch = ResourcePatch {
        resource_name: "luck".to_string(),
        operation: ResourcePatchOp::Set,
        value: -5.0,
    };
    let result = snap.apply_resource_patch(&patch);

    // Set below min should clamp to min
    assert!(result.is_ok());
    assert!(
        (snap.resources["luck"].current - 0.0).abs() < f64::EPSILON,
        "set below min should clamp to 0.0"
    );
}

#[test]
fn resource_patch_set_rejects_above_max() {
    let pool = make_pool("luck", 3.0, 0.0, 6.0);
    let mut snap = snapshot_with_pools(vec![pool]);

    let patch = ResourcePatch {
        resource_name: "luck".to_string(),
        operation: ResourcePatchOp::Set,
        value: 100.0,
    };
    let result = snap.apply_resource_patch(&patch);

    assert!(result.is_ok());
    assert!(
        (snap.resources["luck"].current - 6.0).abs() < f64::EPSILON,
        "set above max should clamp to 6.0"
    );
}

#[test]
fn resource_patch_unknown_resource_returns_error() {
    let mut snap = GameSnapshot::default();

    let patch = ResourcePatch {
        resource_name: "nonexistent".to_string(),
        operation: ResourcePatchOp::Add,
        value: 1.0,
    };
    let result = snap.apply_resource_patch(&patch);

    assert!(result.is_err(), "patching unknown resource should error");
}

#[test]
fn resource_patch_does_not_modify_state_on_error() {
    let pool = make_pool("luck", 3.0, 0.0, 6.0);
    let mut snap = snapshot_with_pools(vec![pool]);

    // Patch an unknown resource — luck should be untouched
    let patch = ResourcePatch {
        resource_name: "nonexistent".to_string(),
        operation: ResourcePatchOp::Add,
        value: 1.0,
    };
    let _ = snap.apply_resource_patch(&patch);

    assert!(
        (snap.resources["luck"].current - 3.0).abs() < f64::EPSILON,
        "failed patch should not modify any resource state"
    );
}

// ═══════════════════════════════════════════════════════════
// AC3: ResourcePatch serde
// ═══════════════════════════════════════════════════════════

#[test]
fn resource_patch_json_roundtrip() {
    let patch = ResourcePatch {
        resource_name: "luck".to_string(),
        operation: ResourcePatchOp::Subtract,
        value: 2.0,
    };

    let json = serde_json::to_string(&patch).unwrap();
    let restored: ResourcePatch = serde_json::from_str(&json).unwrap();

    assert_eq!(restored.resource_name, "luck");
    assert!(matches!(restored.operation, ResourcePatchOp::Subtract));
    assert!((restored.value - 2.0).abs() < f64::EPSILON);
}

#[test]
fn resource_patch_op_all_variants_serialize() {
    for (op, expected) in [
        (ResourcePatchOp::Add, "add"),
        (ResourcePatchOp::Subtract, "subtract"),
        (ResourcePatchOp::Set, "set"),
    ] {
        let json = serde_json::to_string(&op).unwrap();
        assert!(
            json.to_lowercase().contains(expected),
            "ResourcePatchOp::{expected} should serialize to contain '{expected}', got: {json}"
        );
    }
}

// ═══════════════════════════════════════════════════════════
// AC5: Threshold crossing detection
// ═══════════════════════════════════════════════════════════

#[test]
fn threshold_crossing_detected_on_subtract() {
    let pool = make_pool_with_thresholds(
        "luck",
        3.0,
        0.0,
        6.0,
        vec![
            ResourceThreshold {
                at: 1.0,
                event_id: "luck_critical".to_string(),
                narrator_hint: "Nearly out of luck.".to_string(),
            },
        ],
    );
    let mut snap = snapshot_with_pools(vec![pool]);

    let patch = ResourcePatch {
        resource_name: "luck".to_string(),
        operation: ResourcePatchOp::Subtract,
        value: 2.5,
    };
    let result = snap.apply_resource_patch(&patch).unwrap();

    // Value went from 3.0 to 0.5 — crossed the threshold at 1.0
    assert!(
        !result.crossed_thresholds.is_empty(),
        "should detect crossing the threshold at 1.0"
    );
    assert_eq!(result.crossed_thresholds[0].event_id, "luck_critical");
}

#[test]
fn threshold_not_crossed_when_still_above() {
    let pool = make_pool_with_thresholds(
        "luck",
        3.0,
        0.0,
        6.0,
        vec![
            ResourceThreshold {
                at: 1.0,
                event_id: "luck_critical".to_string(),
                narrator_hint: "Nearly out of luck.".to_string(),
            },
        ],
    );
    let mut snap = snapshot_with_pools(vec![pool]);

    let patch = ResourcePatch {
        resource_name: "luck".to_string(),
        operation: ResourcePatchOp::Subtract,
        value: 1.0,
    };
    let result = snap.apply_resource_patch(&patch).unwrap();

    // Value went from 3.0 to 2.0 — still above threshold at 1.0
    assert!(
        result.crossed_thresholds.is_empty(),
        "should not trigger threshold when value stays above it"
    );
}

#[test]
fn multiple_thresholds_crossed_in_single_patch() {
    let pool = make_pool_with_thresholds(
        "luck",
        5.0,
        0.0,
        6.0,
        vec![
            ResourceThreshold {
                at: 3.0,
                event_id: "luck_low".to_string(),
                narrator_hint: "Luck is running thin.".to_string(),
            },
            ResourceThreshold {
                at: 1.0,
                event_id: "luck_critical".to_string(),
                narrator_hint: "Nearly out of luck.".to_string(),
            },
        ],
    );
    let mut snap = snapshot_with_pools(vec![pool]);

    let patch = ResourcePatch {
        resource_name: "luck".to_string(),
        operation: ResourcePatchOp::Subtract,
        value: 4.5,
    };
    let result = snap.apply_resource_patch(&patch).unwrap();

    // Value went from 5.0 to 0.5 — crossed both 3.0 and 1.0 thresholds
    assert_eq!(
        result.crossed_thresholds.len(),
        2,
        "should cross both thresholds"
    );
    let event_ids: Vec<&str> = result
        .crossed_thresholds
        .iter()
        .map(|t| t.event_id.as_str())
        .collect();
    assert!(event_ids.contains(&"luck_low"));
    assert!(event_ids.contains(&"luck_critical"));
}

#[test]
fn threshold_not_re_triggered_when_already_below() {
    let pool = make_pool_with_thresholds(
        "luck",
        0.5,
        0.0,
        6.0,
        vec![
            ResourceThreshold {
                at: 1.0,
                event_id: "luck_critical".to_string(),
                narrator_hint: "Nearly out of luck.".to_string(),
            },
        ],
    );
    let mut snap = snapshot_with_pools(vec![pool]);

    // Value already at 0.5, below threshold at 1.0 — subtract should not re-trigger
    let patch = ResourcePatch {
        resource_name: "luck".to_string(),
        operation: ResourcePatchOp::Subtract,
        value: 0.2,
    };
    let result = snap.apply_resource_patch(&patch).unwrap();

    assert!(
        result.crossed_thresholds.is_empty(),
        "threshold should not re-trigger when value was already below it"
    );
}

#[test]
fn threshold_crossing_on_set_operation() {
    let pool = make_pool_with_thresholds(
        "luck",
        5.0,
        0.0,
        6.0,
        vec![
            ResourceThreshold {
                at: 2.0,
                event_id: "luck_low".to_string(),
                narrator_hint: "Running low.".to_string(),
            },
        ],
    );
    let mut snap = snapshot_with_pools(vec![pool]);

    let patch = ResourcePatch {
        resource_name: "luck".to_string(),
        operation: ResourcePatchOp::Set,
        value: 1.0,
    };
    let result = snap.apply_resource_patch(&patch).unwrap();

    assert_eq!(result.crossed_thresholds.len(), 1);
    assert_eq!(result.crossed_thresholds[0].event_id, "luck_low");
}

// ═══════════════════════════════════════════════════════════
// AC6: decay_per_turn reduces current value each turn
// ═══════════════════════════════════════════════════════════

#[test]
fn resource_pool_decay_reduces_current() {
    let mut pool = ResourcePool {
        name: "heat".to_string(),
        label: "Heat".to_string(),
        current: 5.0,
        min: 0.0,
        max: 10.0,
        voluntary: false,
        decay_per_turn: -0.5,
        thresholds: vec![],

    };
    let mut snap = snapshot_with_pools(vec![pool]);

    snap.apply_pool_decay();

    assert!(
        (snap.resources["heat"].current - 4.5).abs() < f64::EPSILON,
        "heat should decay by 0.5 per turn"
    );
}

#[test]
fn resource_pool_decay_clamps_to_min() {
    let pool = ResourcePool {
        name: "heat".to_string(),
        label: "Heat".to_string(),
        current: 0.3,
        min: 0.0,
        max: 10.0,
        voluntary: false,
        decay_per_turn: -0.5,
        thresholds: vec![],

    };
    let mut snap = snapshot_with_pools(vec![pool]);

    snap.apply_pool_decay();

    assert!(
        (snap.resources["heat"].current - 0.0).abs() < f64::EPSILON,
        "decay should clamp to min, not go negative"
    );
}

#[test]
fn resource_pool_positive_decay_increases() {
    let pool = ResourcePool {
        name: "mana".to_string(),
        label: "Mana".to_string(),
        current: 5.0,
        min: 0.0,
        max: 10.0,
        voluntary: true,
        decay_per_turn: 1.0,
        thresholds: vec![],

    };
    let mut snap = snapshot_with_pools(vec![pool]);

    snap.apply_pool_decay();

    assert!(
        (snap.resources["mana"].current - 6.0).abs() < f64::EPSILON,
        "positive decay should increase value"
    );
}

#[test]
fn resource_pool_positive_decay_clamps_to_max() {
    let pool = ResourcePool {
        name: "mana".to_string(),
        label: "Mana".to_string(),
        current: 9.5,
        min: 0.0,
        max: 10.0,
        voluntary: true,
        decay_per_turn: 1.0,
        thresholds: vec![],

    };
    let mut snap = snapshot_with_pools(vec![pool]);

    snap.apply_pool_decay();

    assert!(
        (snap.resources["mana"].current - 10.0).abs() < f64::EPSILON,
        "positive decay should clamp to max"
    );
}

#[test]
fn resource_pool_zero_decay_no_change() {
    let pool = make_pool("luck", 3.0, 0.0, 6.0);
    let mut snap = snapshot_with_pools(vec![pool]);

    snap.apply_pool_decay();

    assert!(
        (snap.resources["luck"].current - 3.0).abs() < f64::EPSILON,
        "zero decay should leave value unchanged"
    );
}

// ═══════════════════════════════════════════════════════════
// AC7: voluntary flag controls whether player can spend
// ═══════════════════════════════════════════════════════════

#[test]
fn voluntary_resource_allows_player_spend() {
    let mut pool = make_pool("luck", 3.0, 0.0, 6.0);
    pool.voluntary = true;
    let mut snap = snapshot_with_pools(vec![pool]);

    let patch = ResourcePatch {
        resource_name: "luck".to_string(),
        operation: ResourcePatchOp::Subtract,
        value: 1.0,
    };
    let result = snap.apply_resource_patch(&patch);

    assert!(result.is_ok(), "voluntary resource should allow spend");
    assert!(
        (snap.resources["luck"].current - 2.0).abs() < f64::EPSILON,
        "voluntary spend should reduce value"
    );
}

#[test]
fn involuntary_resource_rejects_player_spend() {
    let mut pool = make_pool("heat", 5.0, 0.0, 10.0);
    pool.voluntary = false;
    let mut snap = snapshot_with_pools(vec![pool]);

    let patch = ResourcePatch {
        resource_name: "heat".to_string(),
        operation: ResourcePatchOp::Subtract,
        value: 1.0,
    };
    // When voluntary=false, player-initiated subtract should be rejected
    let result = snap.apply_resource_patch_player(&patch);

    assert!(
        result.is_err(),
        "involuntary resource should reject player spend"
    );
    // State should be unchanged
    assert!(
        (snap.resources["heat"].current - 5.0).abs() < f64::EPSILON,
        "rejected spend should not modify state"
    );
}

#[test]
fn involuntary_resource_allows_engine_modification() {
    let mut pool = make_pool("heat", 5.0, 0.0, 10.0);
    pool.voluntary = false;
    let mut snap = snapshot_with_pools(vec![pool]);

    // Engine (narrator/LLM) can always modify resources regardless of voluntary flag
    let patch = ResourcePatch {
        resource_name: "heat".to_string(),
        operation: ResourcePatchOp::Subtract,
        value: 1.0,
    };
    let result = snap.apply_resource_patch(&patch);

    assert!(result.is_ok(), "engine should always be able to modify resources");
    assert!(
        (snap.resources["heat"].current - 4.0).abs() < f64::EPSILON,
        "engine modification should apply"
    );
}

#[test]
fn involuntary_resource_allows_add_from_player() {
    let mut pool = make_pool("heat", 5.0, 0.0, 10.0);
    pool.voluntary = false;
    let mut snap = snapshot_with_pools(vec![pool]);

    // Player can add to involuntary resources (gaining heat is fine, spending is not)
    let patch = ResourcePatch {
        resource_name: "heat".to_string(),
        operation: ResourcePatchOp::Add,
        value: 1.0,
    };
    let result = snap.apply_resource_patch_player(&patch);

    // Adding to involuntary is fine — only subtract is restricted
    assert!(result.is_ok(), "player should be able to add to involuntary resource");
}

// ═══════════════════════════════════════════════════════════
// AC4: Genre pack initialization loads resources into pools
// ═══════════════════════════════════════════════════════════

#[test]
fn init_pools_from_declarations() {
    let mut snap = GameSnapshot::default();

    // Simulate declarations loaded from genre pack
    let decl = serde_yaml::from_str::<sidequest_genre::ResourceDeclaration>(
        "name: luck\nlabel: Luck\nmin: 0\nmax: 6\nstarting: 3\nvoluntary: true\ndecay_per_turn: 0.0",
    )
    .unwrap();

    snap.init_resource_pools(&[decl]);

    assert!(snap.resources.contains_key("luck"), "pool should be created from declaration");
    let pool = &snap.resources["luck"];
    assert_eq!(pool.name, "luck");
    assert!((pool.current - 3.0).abs() < f64::EPSILON, "current should equal starting");
    assert!((pool.min - 0.0).abs() < f64::EPSILON);
    assert!((pool.max - 6.0).abs() < f64::EPSILON);
    assert!(pool.voluntary);
}

#[test]
fn init_pools_multiple_declarations() {
    let mut snap = GameSnapshot::default();

    let luck = serde_yaml::from_str::<sidequest_genre::ResourceDeclaration>(
        "name: luck\nlabel: Luck\nmin: 0\nmax: 6\nstarting: 3\nvoluntary: true\ndecay_per_turn: 0.0",
    )
    .unwrap();
    let heat = serde_yaml::from_str::<sidequest_genre::ResourceDeclaration>(
        "name: heat\nlabel: Heat\nmin: 0\nmax: 10\nstarting: 0\nvoluntary: false\ndecay_per_turn: -0.1",
    )
    .unwrap();

    snap.init_resource_pools(&[luck, heat]);

    assert_eq!(snap.resources.len(), 2);
    assert!(snap.resources.contains_key("luck"));
    assert!(snap.resources.contains_key("heat"));
    assert!(!snap.resources["heat"].voluntary);
    assert!((snap.resources["heat"].decay_per_turn - (-0.1)).abs() < f64::EPSILON);
}

#[test]
fn init_pools_empty_declarations_no_crash() {
    let mut snap = GameSnapshot::default();
    snap.init_resource_pools(&[]);
    assert!(snap.resources.is_empty());
}

// ═══════════════════════════════════════════════════════════
// Edge cases and integration
// ═══════════════════════════════════════════════════════════

#[test]
fn resource_patch_result_contains_new_value() {
    let pool = make_pool("luck", 3.0, 0.0, 6.0);
    let mut snap = snapshot_with_pools(vec![pool]);

    let patch = ResourcePatch {
        resource_name: "luck".to_string(),
        operation: ResourcePatchOp::Subtract,
        value: 1.0,
    };
    let result = snap.apply_resource_patch(&patch).unwrap();

    assert!(
        (result.new_value - 2.0).abs() < f64::EPSILON,
        "result should contain the new value after patch"
    );
    assert!(
        (result.old_value - 3.0).abs() < f64::EPSILON,
        "result should contain the old value before patch"
    );
}

#[test]
fn decay_triggers_threshold_crossings() {
    let pool = make_pool_with_thresholds(
        "heat",
        1.0,
        0.0,
        10.0,
        vec![
            ResourceThreshold {
                at: 0.5,
                event_id: "heat_low".to_string(),
                narrator_hint: "Cooling down.".to_string(),
            },
        ],
    );
    let mut pool = pool;
    pool.decay_per_turn = -0.6;
    let mut snap = snapshot_with_pools(vec![pool]);

    let crossings = snap.apply_pool_decay();

    // Value went from 1.0 to 0.4 — crossed threshold at 0.5
    assert!(
        !crossings.is_empty(),
        "decay that crosses a threshold should report it"
    );
    assert_eq!(crossings[0].event_id, "heat_low");
}

#[test]
fn multiple_pools_independent_patches() {
    let luck = make_pool("luck", 3.0, 0.0, 6.0);
    let heat = make_pool("heat", 5.0, 0.0, 10.0);
    let mut snap = snapshot_with_pools(vec![luck, heat]);

    let patch_luck = ResourcePatch {
        resource_name: "luck".to_string(),
        operation: ResourcePatchOp::Subtract,
        value: 1.0,
    };
    let patch_heat = ResourcePatch {
        resource_name: "heat".to_string(),
        operation: ResourcePatchOp::Add,
        value: 2.0,
    };

    snap.apply_resource_patch(&patch_luck).unwrap();
    snap.apply_resource_patch(&patch_heat).unwrap();

    assert!(
        (snap.resources["luck"].current - 2.0).abs() < f64::EPSILON,
        "luck should be 2.0"
    );
    assert!(
        (snap.resources["heat"].current - 7.0).abs() < f64::EPSILON,
        "heat should be 7.0"
    );
}

#[test]
fn resource_pool_with_thresholds_survives_snapshot_roundtrip() {
    let pool = make_pool_with_thresholds(
        "luck",
        3.0,
        0.0,
        6.0,
        vec![
            ResourceThreshold {
                at: 1.0,
                event_id: "luck_critical".to_string(),
                narrator_hint: "Nearly out.".to_string(),
            },
        ],
    );
    let snap = snapshot_with_pools(vec![pool]);

    let json = serde_json::to_string(&snap).unwrap();
    let restored: GameSnapshot = serde_json::from_str(&json).unwrap();

    let pool = &restored.resources["luck"];
    assert_eq!(pool.thresholds.len(), 1);
    assert_eq!(pool.thresholds[0].event_id, "luck_critical");
    assert!((pool.thresholds[0].at - 1.0).abs() < f64::EPSILON);
}

//! Story 38-2: per_actor_state on EncounterActor.
//!
//! Adds HashMap<String, serde_json::Value> for per-pilot scene descriptors.
//! ADR-077 Extension 3 — used by SealedLetterLookup (38-5) to store each
//! pilot's cockpit descriptor between turns.
//!
//! ACs tested:
//!   AC-Field:      EncounterActor has per_actor_state field
//!   AC-Default:    per_actor_state defaults to empty HashMap
//!   AC-Serde:      per_actor_state survives YAML serde round-trip
//!   AC-JSON:       serde_json::Value handles typed descriptors (string, number, bool)
//!   AC-Implicit:   Existing YAML without per_actor_state deserializes (backward compat)
//!   AC-SaveLoad:   StructuredEncounter with per_actor_state survives full round-trip
//!   AC-Wiring:     EncounterActor is publicly accessible from sidequest_game

use sidequest_game::encounter::{
    EncounterActor, EncounterMetric, EncounterPhase, MetricDirection, StructuredEncounter,
};
use std::collections::HashMap;

// =========================================================================
// AC-Field: EncounterActor has per_actor_state field
// =========================================================================

#[test]
fn encounter_actor_has_per_actor_state_field() {
    let actor = EncounterActor {
        name: "Maverick".to_string(),
        role: "pilot".to_string(),
        per_actor_state: HashMap::new(),
    };
    assert_eq!(actor.name, "Maverick");
    assert!(actor.per_actor_state.is_empty());
}

// =========================================================================
// AC-Default: per_actor_state defaults to empty HashMap when absent
// =========================================================================

#[test]
fn encounter_actor_per_actor_state_defaults_to_empty() {
    // Deserialize YAML without per_actor_state — should get empty HashMap
    let yaml = r#"
name: Iceman
role: wingman
"#;
    let actor: EncounterActor = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(actor.name, "Iceman");
    assert_eq!(actor.role, "wingman");
    assert!(
        actor.per_actor_state.is_empty(),
        "per_actor_state should default to empty HashMap when absent from YAML"
    );
}

// =========================================================================
// AC-Serde: per_actor_state survives YAML round-trip
// =========================================================================

#[test]
fn encounter_actor_per_actor_state_yaml_roundtrip_empty() {
    let actor = EncounterActor {
        name: "Goose".to_string(),
        role: "rio".to_string(),
        per_actor_state: HashMap::new(),
    };
    let yaml = serde_yaml::to_string(&actor).unwrap();
    let restored: EncounterActor = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(restored.name, "Goose");
    assert!(restored.per_actor_state.is_empty());
}

#[test]
fn encounter_actor_per_actor_state_yaml_roundtrip_with_data() {
    let mut state = HashMap::new();
    state.insert(
        "bearing".to_string(),
        serde_json::Value::String("merge".to_string()),
    );
    state.insert(
        "range".to_string(),
        serde_json::Value::Number(serde_json::Number::from(500)),
    );
    state.insert("gun_solution".to_string(), serde_json::Value::Bool(false));

    let actor = EncounterActor {
        name: "Viper".to_string(),
        role: "pilot".to_string(),
        per_actor_state: state,
    };

    let yaml = serde_yaml::to_string(&actor).unwrap();
    let restored: EncounterActor = serde_yaml::from_str(&yaml).unwrap();

    assert_eq!(restored.name, "Viper");
    assert_eq!(
        restored.per_actor_state.get("bearing"),
        Some(&serde_json::Value::String("merge".to_string())),
        "bearing should survive round-trip"
    );
    assert_eq!(
        restored.per_actor_state.get("range"),
        Some(&serde_json::Value::Number(serde_json::Number::from(500))),
        "range should survive round-trip"
    );
    assert_eq!(
        restored.per_actor_state.get("gun_solution"),
        Some(&serde_json::Value::Bool(false)),
        "gun_solution should survive round-trip"
    );
}

// =========================================================================
// AC-JSON: serde_json::Value handles typed descriptors
// =========================================================================

#[test]
fn per_actor_state_handles_string_values() {
    let mut state = HashMap::new();
    state.insert(
        "aspect".to_string(),
        serde_json::Value::String("beam".to_string()),
    );
    let actor = EncounterActor {
        name: "Red".to_string(),
        role: "pilot".to_string(),
        per_actor_state: state,
    };
    assert_eq!(
        actor.per_actor_state["aspect"],
        serde_json::Value::String("beam".to_string())
    );
}

#[test]
fn per_actor_state_handles_numeric_values() {
    let mut state = HashMap::new();
    state.insert(
        "energy".to_string(),
        serde_json::Value::Number(serde_json::Number::from(60)),
    );
    let actor = EncounterActor {
        name: "Blue".to_string(),
        role: "pilot".to_string(),
        per_actor_state: state,
    };
    assert_eq!(
        actor.per_actor_state["energy"],
        serde_json::Value::Number(serde_json::Number::from(60))
    );
}

#[test]
fn per_actor_state_handles_boolean_values() {
    let mut state = HashMap::new();
    state.insert("gun_solution".to_string(), serde_json::Value::Bool(true));
    let actor = EncounterActor {
        name: "Gold".to_string(),
        role: "pilot".to_string(),
        per_actor_state: state,
    };
    assert_eq!(
        actor.per_actor_state["gun_solution"],
        serde_json::Value::Bool(true)
    );
}

#[test]
fn per_actor_state_handles_null_values() {
    let mut state = HashMap::new();
    state.insert("damage_log".to_string(), serde_json::Value::Null);
    let actor = EncounterActor {
        name: "Leader".to_string(),
        role: "pilot".to_string(),
        per_actor_state: state,
    };
    assert_eq!(actor.per_actor_state["damage_log"], serde_json::Value::Null);
}

#[test]
fn per_actor_state_handles_float_values() {
    let mut state = HashMap::new();
    state.insert("closure_speed".to_string(), serde_json::json!(3.5));
    let actor = EncounterActor {
        name: "Jester".to_string(),
        role: "pilot".to_string(),
        per_actor_state: state,
    };
    let val = &actor.per_actor_state["closure_speed"];
    assert_eq!(val.as_f64(), Some(3.5), "float values should be preserved");
}

// =========================================================================
// AC-Implicit: Existing YAML without per_actor_state still deserializes
// =========================================================================

#[test]
fn existing_encounter_actor_yaml_backward_compatible() {
    // This is exactly how EncounterActor YAML looks today — no per_actor_state field
    let yaml = r#"
name: "The Good"
role: duelist
"#;
    let actor: EncounterActor = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(actor.name, "The Good");
    assert_eq!(actor.role, "duelist");
    assert!(
        actor.per_actor_state.is_empty(),
        "old YAML without per_actor_state must load with empty HashMap"
    );
}

#[test]
fn encounter_actor_with_explicit_empty_per_actor_state() {
    let yaml = r#"
name: "The Bad"
role: duelist
per_actor_state: {}
"#;
    let actor: EncounterActor = serde_yaml::from_str(yaml).unwrap();
    assert!(actor.per_actor_state.is_empty());
}

#[test]
fn encounter_actor_with_populated_per_actor_state_from_yaml() {
    let yaml = r#"
name: "Ace"
role: pilot
per_actor_state:
  bearing: merge
  range: 500
  gun_solution: false
  energy: 60
"#;
    let actor: EncounterActor = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(actor.per_actor_state.len(), 4);
    assert_eq!(
        actor.per_actor_state["bearing"],
        serde_json::Value::String("merge".to_string())
    );
    // YAML numeric values deserialize as serde_json::Value::Number
    assert!(
        actor.per_actor_state["range"].is_number(),
        "range should be a number"
    );
    assert_eq!(
        actor.per_actor_state["gun_solution"],
        serde_json::Value::Bool(false)
    );
}

// =========================================================================
// AC-SaveLoad: StructuredEncounter with per_actor_state survives full round-trip
// =========================================================================

#[test]
fn structured_encounter_with_per_actor_state_roundtrip() {
    let mut red_state = HashMap::new();
    red_state.insert(
        "bearing".to_string(),
        serde_json::Value::String("merge".to_string()),
    );
    red_state.insert(
        "energy".to_string(),
        serde_json::Value::Number(serde_json::Number::from(60)),
    );

    let mut blue_state = HashMap::new();
    blue_state.insert(
        "bearing".to_string(),
        serde_json::Value::String("tail_chase".to_string()),
    );
    blue_state.insert(
        "energy".to_string(),
        serde_json::Value::Number(serde_json::Number::from(45)),
    );
    blue_state.insert("gun_solution".to_string(), serde_json::Value::Bool(true));

    let encounter = StructuredEncounter {
        encounter_type: "dogfight".to_string(),
        metric: EncounterMetric {
            name: "engagement_control".to_string(),
            current: 15,
            starting: 0,
            direction: MetricDirection::Bidirectional,
            threshold_high: Some(100),
            threshold_low: Some(-100),
        },
        beat: 3,
        structured_phase: Some(EncounterPhase::Escalation),
        secondary_stats: None,
        actors: vec![
            EncounterActor {
                name: "Red Leader".to_string(),
                role: "pilot".to_string(),
                per_actor_state: red_state,
            },
            EncounterActor {
                name: "Blue Leader".to_string(),
                role: "pilot".to_string(),
                per_actor_state: blue_state,
            },
        ],
        outcome: None,
        resolved: false,
        mood_override: Some("dogfight".to_string()),
        narrator_hints: vec!["Two pilots, two cockpits".to_string()],
    };

    // Serialize to YAML
    let yaml = serde_yaml::to_string(&encounter).unwrap();

    // Deserialize back
    let restored: StructuredEncounter = serde_yaml::from_str(&yaml).unwrap();

    // Verify encounter metadata
    assert_eq!(restored.encounter_type, "dogfight");
    assert_eq!(restored.actors.len(), 2);

    // Verify Red Leader's per_actor_state
    let red = &restored.actors[0];
    assert_eq!(red.name, "Red Leader");
    assert_eq!(
        red.per_actor_state.get("bearing"),
        Some(&serde_json::Value::String("merge".to_string())),
        "Red Leader's bearing must survive round-trip"
    );
    assert_eq!(
        red.per_actor_state.get("energy"),
        Some(&serde_json::Value::Number(serde_json::Number::from(60))),
        "Red Leader's energy must survive round-trip"
    );

    // Verify Blue Leader's per_actor_state
    let blue = &restored.actors[1];
    assert_eq!(blue.name, "Blue Leader");
    assert_eq!(
        blue.per_actor_state.get("bearing"),
        Some(&serde_json::Value::String("tail_chase".to_string())),
        "Blue Leader's bearing must survive round-trip"
    );
    assert_eq!(
        blue.per_actor_state.get("gun_solution"),
        Some(&serde_json::Value::Bool(true)),
        "Blue Leader's gun_solution must survive round-trip"
    );
}

#[test]
fn structured_encounter_without_per_actor_state_backward_compat() {
    // Simulate old saved encounter YAML — actors have no per_actor_state
    let yaml = r#"
encounter_type: combat
metric:
  name: hp
  current: 25
  starting: 30
  direction: Descending
  threshold_low: 0
beat: 2
structured_phase: Escalation
actors:
  - name: "Hero"
    role: "fighter"
  - name: "Villain"
    role: "boss"
resolved: false
narrator_hints: []
"#;
    let encounter: StructuredEncounter = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(encounter.actors.len(), 2);
    assert!(
        encounter.actors[0].per_actor_state.is_empty(),
        "old save without per_actor_state should get empty HashMap"
    );
    assert!(
        encounter.actors[1].per_actor_state.is_empty(),
        "old save without per_actor_state should get empty HashMap"
    );
}

// =========================================================================
// AC-Wiring: EncounterActor is publicly accessible from sidequest_game
// =========================================================================

#[test]
fn encounter_actor_is_publicly_accessible() {
    // Verifies the type and its per_actor_state field are accessible
    // through the public API of sidequest_game
    let actor = sidequest_game::encounter::EncounterActor {
        name: "Test".to_string(),
        role: "test".to_string(),
        per_actor_state: std::collections::HashMap::new(),
    };
    let _state: &std::collections::HashMap<String, serde_json::Value> = &actor.per_actor_state;
}

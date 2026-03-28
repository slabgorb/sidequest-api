//! Story 10-1: OCEAN profile on NPC model — five float fields (0.0–10.0).
//!
//! RED phase: these tests compile against the existing API surface but FAIL
//! because OceanProfile and the NPC `ocean` field do not exist yet.
//!
//! Acceptance criteria:
//!   AC-1: OceanProfile struct with five f64 fields
//!   AC-2: Each field constrained to 0.0–10.0
//!   AC-3: Serde round-trip (YAML + JSON)
//!   AC-4: Default impl — all five at 5.0
//!   AC-5: NPC has optional ocean field
//!   AC-6: Backward compat — NPC without ocean still works

use sidequest_game::Npc;

// ─── Helpers ──────────────────────────────────────────────

/// Minimal NPC YAML without ocean — used for backward-compat tests.
const NPC_YAML_NO_OCEAN: &str = r#"
name: "Marta the Innkeeper"
description: "A stout woman with flour-dusted hands"
personality: "Warm and gossipy"
level: 2
hp: 12
max_hp: 12
ac: 10
voice_id: 3
disposition: 15
location: "The Rusty Nail Inn"
pronouns: "she/her"
appearance: "Flour-dusted apron"
statuses: []
inventory:
  items: []
  gold: 0
"#;

/// NPC YAML with an ocean profile block.
const NPC_YAML_WITH_OCEAN: &str = r#"
name: "Marta the Innkeeper"
description: "A stout woman with flour-dusted hands"
personality: "Warm and gossipy"
level: 2
hp: 12
max_hp: 12
ac: 10
voice_id: 3
disposition: 15
location: "The Rusty Nail Inn"
pronouns: "she/her"
appearance: "Flour-dusted apron"
statuses: []
inventory:
  items: []
  gold: 0
ocean:
  openness: 7.5
  conscientiousness: 8.0
  extraversion: 6.0
  agreeableness: 9.0
  neuroticism: 3.0
"#;

/// NPC YAML with out-of-range ocean values (should be clamped or rejected).
const NPC_YAML_OCEAN_OUT_OF_RANGE: &str = r#"
name: "Razortooth"
description: "A scarred raider"
personality: "Cruel and cunning"
level: 4
hp: 18
max_hp: 22
ac: 14
voice_id: null
disposition: -20
location: null
statuses: []
inventory:
  items: []
  gold: 0
ocean:
  openness: -5.0
  conscientiousness: 15.0
  extraversion: 0.0
  agreeableness: 10.0
  neuroticism: 10.1
"#;

// ─── AC-1: OceanProfile struct with five f64 fields ──────

#[test]
fn ocean_profile_deserializes_from_yaml() {
    let npc: Npc = serde_yaml::from_str(NPC_YAML_WITH_OCEAN).unwrap();
    let json = serde_json::to_string(&npc).unwrap();
    // The ocean block must survive serialization — field must exist on Npc.
    assert!(
        json.contains("\"ocean\""),
        "NPC JSON should contain ocean field but got: {json}"
    );
}

#[test]
fn ocean_profile_has_five_dimensions_in_json() {
    let npc: Npc = serde_yaml::from_str(NPC_YAML_WITH_OCEAN).unwrap();
    let json = serde_json::to_string(&npc).unwrap();
    let val: serde_json::Value = serde_json::from_str(&json).unwrap();
    let ocean = val.get("ocean").expect("ocean field missing from NPC JSON");
    assert!(ocean.get("openness").is_some(), "missing openness");
    assert!(ocean.get("conscientiousness").is_some(), "missing conscientiousness");
    assert!(ocean.get("extraversion").is_some(), "missing extraversion");
    assert!(ocean.get("agreeableness").is_some(), "missing agreeableness");
    assert!(ocean.get("neuroticism").is_some(), "missing neuroticism");
}

// ─── AC-2: Range constraint (0.0–10.0) ──────────────────

#[test]
fn ocean_values_clamped_to_valid_range() {
    // Out-of-range values must be clamped: negatives → 0.0, >10.0 → 10.0
    let npc: Npc = serde_yaml::from_str(NPC_YAML_OCEAN_OUT_OF_RANGE).unwrap();
    let json = serde_json::to_string(&npc).unwrap();
    let val: serde_json::Value = serde_json::from_str(&json).unwrap();
    let ocean = val.get("ocean").expect("ocean field missing");

    let o = ocean["openness"].as_f64().expect("openness not f64");
    let c = ocean["conscientiousness"].as_f64().expect("conscientiousness not f64");
    let n = ocean["neuroticism"].as_f64().expect("neuroticism not f64");

    assert!(o >= 0.0, "openness should be clamped to >= 0.0, got {o}");
    assert!(c <= 10.0, "conscientiousness should be clamped to <= 10.0, got {c}");
    assert!(n <= 10.0, "neuroticism should be clamped to <= 10.0, got {n}");
}

// ─── AC-3: Serde round-trip (YAML and JSON) ─────────────

#[test]
fn ocean_yaml_roundtrip() {
    let npc: Npc = serde_yaml::from_str(NPC_YAML_WITH_OCEAN).unwrap();
    let yaml_out = serde_yaml::to_string(&npc).unwrap();
    let npc2: Npc = serde_yaml::from_str(&yaml_out).unwrap();
    let json1 = serde_json::to_string(&npc).unwrap();
    let json2 = serde_json::to_string(&npc2).unwrap();
    let v1: serde_json::Value = serde_json::from_str(&json1).unwrap();
    let v2: serde_json::Value = serde_json::from_str(&json2).unwrap();
    let ocean1 = v1.get("ocean").expect("ocean missing after first deser");
    let ocean2 = v2.get("ocean").expect("ocean missing after YAML round-trip");
    assert_eq!(ocean1, ocean2, "OCEAN profile should survive YAML round-trip");
}

#[test]
fn ocean_json_roundtrip() {
    let npc: Npc = serde_yaml::from_str(NPC_YAML_WITH_OCEAN).unwrap();
    let json = serde_json::to_string(&npc).unwrap();
    let npc2: Npc = serde_json::from_str(&json).unwrap();
    let json2 = serde_json::to_string(&npc2).unwrap();
    let v1: serde_json::Value = serde_json::from_str(&json).unwrap();
    let v2: serde_json::Value = serde_json::from_str(&json2).unwrap();
    let ocean1 = v1.get("ocean").expect("ocean missing after first deser");
    let ocean2 = v2.get("ocean").expect("ocean missing after JSON round-trip");
    assert_eq!(ocean1, ocean2, "OCEAN profile should survive JSON round-trip");
}

// ─── AC-4: Default impl — all five at 5.0 ───────────────

#[test]
fn ocean_default_yaml_has_neutral_values() {
    // An NPC with `ocean: {}` (empty map) should get default 5.0 for all dims.
    let yaml = r#"
name: "Test NPC"
description: "A test"
personality: "Bland"
level: 1
hp: 10
max_hp: 10
ac: 10
voice_id: null
disposition: 0
location: null
statuses: []
inventory:
  items: []
  gold: 0
ocean: {}
"#;
    let npc: Npc = serde_yaml::from_str(yaml).unwrap();
    let json = serde_json::to_string(&npc).unwrap();
    let val: serde_json::Value = serde_json::from_str(&json).unwrap();
    let ocean = val.get("ocean").expect("ocean field missing");

    let expected = 5.0_f64;
    assert_eq!(ocean["openness"].as_f64().unwrap(), expected, "default openness");
    assert_eq!(ocean["conscientiousness"].as_f64().unwrap(), expected, "default conscientiousness");
    assert_eq!(ocean["extraversion"].as_f64().unwrap(), expected, "default extraversion");
    assert_eq!(ocean["agreeableness"].as_f64().unwrap(), expected, "default agreeableness");
    assert_eq!(ocean["neuroticism"].as_f64().unwrap(), expected, "default neuroticism");
}

// ─── AC-5: NPC has optional ocean field ──────────────────

#[test]
fn npc_without_ocean_has_none() {
    let npc: Npc = serde_yaml::from_str(NPC_YAML_NO_OCEAN).unwrap();
    let json = serde_json::to_string(&npc).unwrap();
    let val: serde_json::Value = serde_json::from_str(&json).unwrap();
    // When no ocean field is provided, it should be null or absent in JSON.
    let ocean = val.get("ocean");
    assert!(
        ocean.is_none() || ocean.unwrap().is_null(),
        "NPC without ocean should serialize as null/absent, got: {ocean:?}"
    );
}

#[test]
fn npc_with_ocean_has_some() {
    let npc: Npc = serde_yaml::from_str(NPC_YAML_WITH_OCEAN).unwrap();
    let json = serde_json::to_string(&npc).unwrap();
    let val: serde_json::Value = serde_json::from_str(&json).unwrap();
    let ocean = val.get("ocean");
    assert!(
        ocean.is_some() && !ocean.unwrap().is_null(),
        "NPC with ocean should have non-null ocean field"
    );
}

// ─── AC-6: Backward compatibility ───────────────────────

#[test]
fn existing_npc_yaml_without_ocean_still_deserializes() {
    // NPCs from before story 10-1 have no ocean field — they must still load.
    let npc: Npc = serde_yaml::from_str(NPC_YAML_NO_OCEAN).unwrap();
    assert_eq!(npc.core.name.as_str(), "Marta the Innkeeper");
}

#[test]
fn existing_npc_json_without_ocean_still_deserializes() {
    let json = r#"{"name":"Marta the Innkeeper","description":"A stout woman","personality":"Warm","voice_id":null,"disposition":0,"level":1,"hp":10,"max_hp":10,"ac":10,"location":null,"statuses":[],"inventory":{"items":[],"gold":0}}"#;
    let npc: Npc = serde_json::from_str(json).unwrap();
    assert_eq!(npc.core.name.as_str(), "Marta the Innkeeper");
}

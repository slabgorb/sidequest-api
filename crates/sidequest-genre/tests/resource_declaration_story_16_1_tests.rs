//! Story 16-1: Resource declaration parsing tests (RED phase).
//!
//! Tests that genre packs can declare resources in rules.yaml and that
//! the ResourceDeclaration type deserializes correctly with all fields.
//!
//! ACs tested:
//!   AC1 (Parse): Resource declarations load from any genre pack's rules.yaml
//!   AC5 (All genres): Works for packs with and without resource declarations

use sidequest_genre::{ResourceDeclaration, RulesConfig};
use std::collections::HashMap;

// =========================================================================
// AC1: ResourceDeclaration deserializes from YAML
// =========================================================================

#[test]
fn resource_declaration_deserializes_minimal() {
    let yaml = r#"
name: luck
label: "Luck"
min: 0.0
max: 6.0
starting: 3.0
voluntary: true
decay_per_turn: 0.0
"#;
    let decl: ResourceDeclaration = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(decl.name, "luck");
    assert_eq!(decl.label, "Luck");
    assert!((decl.min - 0.0).abs() < f64::EPSILON);
    assert!((decl.max - 6.0).abs() < f64::EPSILON);
    assert!((decl.starting - 3.0).abs() < f64::EPSILON);
    assert!(decl.voluntary);
    assert!((decl.decay_per_turn - 0.0).abs() < f64::EPSILON);
}

#[test]
fn resource_declaration_deserializes_involuntary_with_decay() {
    let yaml = r#"
name: heat
label: "Heat"
min: 0.0
max: 5.0
starting: 0.0
voluntary: false
decay_per_turn: -0.1
"#;
    let decl: ResourceDeclaration = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(decl.name, "heat");
    assert!(!decl.voluntary);
    assert!((decl.decay_per_turn - (-0.1)).abs() < f64::EPSILON);
}

#[test]
fn resource_declaration_deserializes_high_max() {
    let yaml = r#"
name: humanity
label: "Humanity"
min: 0.0
max: 100.0
starting: 100.0
voluntary: false
decay_per_turn: 0.0
"#;
    let decl: ResourceDeclaration = serde_yaml::from_str(yaml).unwrap();
    assert!((decl.max - 100.0).abs() < f64::EPSILON);
    assert!((decl.starting - 100.0).abs() < f64::EPSILON);
}

// =========================================================================
// AC1: RulesConfig parses resources field
// =========================================================================

#[test]
fn rules_config_with_resources_deserializes() {
    let yaml = r#"
tone: gritty
lethality: moderate
magic_level: none
stat_generation: point_buy
point_buy_budget: 27
ability_score_names: [Brawn, Reflexes, Cunning, Nerve, Grit, Wits]
allowed_classes: [Gunslinger, Drifter]
allowed_races: [Human, Stranger]
class_hp_bases:
  Gunslinger: 8
  Drifter: 10
resources:
  - name: luck
    label: "Luck"
    min: 0.0
    max: 6.0
    starting: 3.0
    voluntary: true
    decay_per_turn: 0.0
  - name: heat
    label: "Heat"
    min: 0.0
    max: 5.0
    starting: 0.0
    voluntary: false
    decay_per_turn: -0.1
"#;
    let rules: RulesConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(rules.resources.len(), 2);
    assert_eq!(rules.resources[0].name, "luck");
    assert_eq!(rules.resources[1].name, "heat");
}

#[test]
fn rules_config_resources_have_correct_metadata() {
    let yaml = r#"
tone: gonzo
lethality: moderate
magic_level: none
stat_generation: point_buy
point_buy_budget: 27
ability_score_names: [Brawn, Reflexes, Cunning, Nerve, Grit, Wits]
allowed_classes: [Gunslinger]
allowed_races: [Human]
class_hp_bases:
  Gunslinger: 8
resources:
  - name: luck
    label: "Luck"
    min: 0.0
    max: 6.0
    starting: 3.0
    voluntary: true
    decay_per_turn: 0.0
"#;
    let rules: RulesConfig = serde_yaml::from_str(yaml).unwrap();
    let luck = &rules.resources[0];
    assert_eq!(luck.label, "Luck");
    assert!(luck.voluntary, "luck should be voluntary (player-spendable)");
    assert!(
        luck.starting >= luck.min && luck.starting <= luck.max,
        "starting value must be within [min, max]"
    );
}

// =========================================================================
// AC5: RulesConfig without resources defaults to empty vec
// =========================================================================

#[test]
fn rules_config_without_resources_defaults_to_empty() {
    let yaml = r#"
tone: gritty
lethality: moderate
magic_level: low
stat_generation: point_buy
point_buy_budget: 27
ability_score_names: [Strength, Dexterity, Constitution, Intelligence, Wisdom, Charisma]
allowed_classes: [Fighter, Rogue, Wizard]
allowed_races: [Human, Elf, Dwarf]
class_hp_bases:
  Fighter: 10
  Rogue: 8
  Wizard: 6
"#;
    let rules: RulesConfig = serde_yaml::from_str(yaml).unwrap();
    assert!(
        rules.resources.is_empty(),
        "genres without resources should get empty vec via #[serde(default)]"
    );
}

// =========================================================================
// AC1: ResourceDeclaration serde roundtrip
// =========================================================================

#[test]
fn resource_declaration_yaml_roundtrip() {
    let decl = ResourceDeclaration {
        name: "luck".to_string(),
        label: "Luck".to_string(),
        min: 0.0,
        max: 6.0,
        starting: 3.0,
        voluntary: true,
        decay_per_turn: 0.0,
        thresholds: vec![],
    };
    let yaml = serde_yaml::to_string(&decl).unwrap();
    let restored: ResourceDeclaration = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(decl.name, restored.name);
    assert_eq!(decl.label, restored.label);
    assert!((decl.max - restored.max).abs() < f64::EPSILON);
    assert!((decl.starting - restored.starting).abs() < f64::EPSILON);
    assert_eq!(decl.voluntary, restored.voluntary);
    assert!((decl.decay_per_turn - restored.decay_per_turn).abs() < f64::EPSILON);
}

// =========================================================================
// Rule #8 / #5: Validation — max >= min, starting in range
// =========================================================================

#[test]
fn resource_declaration_rejects_max_less_than_min() {
    let yaml = r#"
name: broken
label: "Broken"
min: 10.0
max: 5.0
starting: 7.0
voluntary: true
decay_per_turn: 0.0
"#;
    // If ResourceDeclaration uses serde(try_from) or a validating constructor,
    // deserialization of invalid data should fail.
    // If it doesn't validate: this test documents the expectation for 16-10.
    let result: Result<ResourceDeclaration, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_err(),
        "ResourceDeclaration with max < min should be rejected"
    );
}

#[test]
fn resource_declaration_rejects_starting_out_of_range() {
    let yaml = r#"
name: broken
label: "Broken"
min: 0.0
max: 6.0
starting: 10.0
voluntary: true
decay_per_turn: 0.0
"#;
    let result: Result<ResourceDeclaration, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_err(),
        "ResourceDeclaration with starting > max should be rejected"
    );
}

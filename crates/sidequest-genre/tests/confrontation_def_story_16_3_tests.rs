//! Story 16-3: Confrontation YAML schema — genre loader parses encounter declarations.
//!
//! Tests that genre packs can declare confrontation types in rules.yaml and that
//! ConfrontationDef, MetricDef, BeatDef, SecondaryStatDef deserialize correctly.
//!
//! ACs tested:
//!   AC-Parse:     Confrontation declarations load from rules.yaml
//!   AC-Validate:  Invalid schema (missing fields, bad references) produces clear error
//!   AC-EmptyOK:   Packs without confrontations load normally (empty vec)
//!   AC-StatCheck: stat_check validated against genre ability_score_names
//!   AC-Roundtrip: Parsed ConfrontationDef serializes back to valid YAML

use sidequest_genre::{
    BeatDef, ConfrontationDef, MetricDef, RulesConfig, SecondaryStatDef,
};

// =========================================================================
// AC-Parse: ConfrontationDef deserializes from YAML
// =========================================================================

#[test]
fn confrontation_def_deserializes_minimal() {
    let yaml = r#"
type: combat
label: "Combat"
category: combat
metric:
  name: hp
  direction: descending
  starting: 30
  threshold_low: 0
beats:
  - id: attack
    label: "Attack"
    metric_delta: -5
    stat_check: MIGHT
"#;
    let def: ConfrontationDef = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(def.confrontation_type, "combat");
    assert_eq!(def.label, "Combat");
    assert_eq!(def.category, "combat");
    assert_eq!(def.metric.name, "hp");
    assert_eq!(def.beats.len(), 1);
    assert_eq!(def.beats[0].id, "attack");
    assert!(def.secondary_stats.is_empty());
    assert!(def.escalates_to.is_none());
    assert!(def.mood.is_none());
}

#[test]
fn confrontation_def_deserializes_full() {
    let yaml = r#"
type: standoff
label: "Standoff"
category: pre_combat
metric:
  name: tension
  direction: ascending
  starting: 0
  threshold_high: 10
beats:
  - id: size_up
    label: "Size Up"
    metric_delta: 2
    stat_check: CUNNING
    reveals: opponent_detail
  - id: bluff
    label: "Bluff"
    metric_delta: 3
    stat_check: NERVE
    risk: "opponent may call it — immediate draw"
  - id: draw
    label: "Draw"
    metric_delta: 0
    resolution: true
    stat_check: DRAW
secondary_stats:
  - name: focus
    source_stat: NERVE
    spendable: true
escalates_to: combat
mood: standoff
"#;
    let def: ConfrontationDef = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(def.confrontation_type, "standoff");
    assert_eq!(def.category, "pre_combat");
    assert_eq!(def.metric.name, "tension");
    assert_eq!(def.beats.len(), 3);
    assert_eq!(def.secondary_stats.len(), 1);
    assert_eq!(def.secondary_stats[0].name, "focus");
    assert_eq!(def.escalates_to.as_deref(), Some("combat"));
    assert_eq!(def.mood.as_deref(), Some("standoff"));
}

// =========================================================================
// AC-Parse: MetricDef deserializes with all direction variants
// =========================================================================

#[test]
fn metric_def_ascending() {
    let yaml = r#"
name: tension
direction: ascending
starting: 0
threshold_high: 10
"#;
    let metric: MetricDef = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(metric.name, "tension");
    assert_eq!(metric.direction, "ascending");
    assert_eq!(metric.starting, 0);
    assert_eq!(metric.threshold_high, Some(10));
    assert_eq!(metric.threshold_low, None);
}

#[test]
fn metric_def_descending() {
    let yaml = r#"
name: hp
direction: descending
starting: 30
threshold_low: 0
"#;
    let metric: MetricDef = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(metric.direction, "descending");
    assert_eq!(metric.threshold_low, Some(0));
    assert_eq!(metric.threshold_high, None);
}

#[test]
fn metric_def_bidirectional() {
    let yaml = r#"
name: leverage
direction: bidirectional
starting: 5
threshold_high: 10
threshold_low: 0
"#;
    let metric: MetricDef = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(metric.direction, "bidirectional");
    assert_eq!(metric.threshold_high, Some(10));
    assert_eq!(metric.threshold_low, Some(0));
}

// =========================================================================
// AC-Parse: BeatDef deserializes with all field combinations
// =========================================================================

#[test]
fn beat_def_minimal() {
    let yaml = r#"
id: attack
label: "Attack"
metric_delta: -5
stat_check: MIGHT
"#;
    let beat: BeatDef = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(beat.id, "attack");
    assert_eq!(beat.label, "Attack");
    assert_eq!(beat.metric_delta, -5);
    assert_eq!(beat.stat_check, "MIGHT");
    assert!(beat.risk.is_none());
    assert!(beat.reveals.is_none());
    assert!(!beat.resolution.unwrap_or(false));
}

#[test]
fn beat_def_with_all_optional_fields() {
    let yaml = r#"
id: draw
label: "Draw"
metric_delta: 0
stat_check: DRAW
risk: "lethal if you lose"
reveals: opponent_weakness
resolution: true
"#;
    let beat: BeatDef = serde_yaml::from_str(yaml).unwrap();
    assert!(beat.resolution.unwrap_or(false), "draw should be a resolution beat");
    assert_eq!(beat.risk.as_deref(), Some("lethal if you lose"));
    assert_eq!(beat.reveals.as_deref(), Some("opponent_weakness"));
}

// =========================================================================
// AC-Parse: SecondaryStatDef deserializes
// =========================================================================

#[test]
fn secondary_stat_def_deserializes() {
    let yaml = r#"
name: focus
source_stat: NERVE
spendable: true
"#;
    let stat: SecondaryStatDef = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(stat.name, "focus");
    assert_eq!(stat.source_stat, "NERVE");
    assert!(stat.spendable);
}

#[test]
fn secondary_stat_def_non_spendable() {
    let yaml = r#"
name: shields
source_stat: ENGINEERING
spendable: false
"#;
    let stat: SecondaryStatDef = serde_yaml::from_str(yaml).unwrap();
    assert!(!stat.spendable);
}

// =========================================================================
// AC-Parse: RulesConfig with confrontations
// =========================================================================

#[test]
fn rules_config_with_confrontations_deserializes() {
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
confrontations:
  - type: standoff
    label: "Standoff"
    category: pre_combat
    metric:
      name: tension
      direction: ascending
      starting: 0
      threshold_high: 10
    beats:
      - id: size_up
        label: "Size Up"
        metric_delta: 2
        stat_check: CUNNING
  - type: combat
    label: "Combat"
    category: combat
    metric:
      name: hp
      direction: descending
      starting: 30
      threshold_low: 0
    beats:
      - id: attack
        label: "Attack"
        metric_delta: -5
        stat_check: BRAWN
"#;
    let rules: RulesConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(rules.confrontations.len(), 2);
    assert_eq!(rules.confrontations[0].confrontation_type, "standoff");
    assert_eq!(rules.confrontations[1].confrontation_type, "combat");
}

// =========================================================================
// AC-EmptyOK: RulesConfig without confrontations defaults to empty
// =========================================================================

#[test]
fn rules_config_without_confrontations_defaults_to_empty() {
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
        rules.confrontations.is_empty(),
        "genres without confrontations should get empty vec via #[serde(default)]"
    );
}

// =========================================================================
// AC-Roundtrip: ConfrontationDef serializes/deserializes losslessly
// =========================================================================

#[test]
fn confrontation_def_yaml_roundtrip() {
    let yaml = r#"
type: standoff
label: "Standoff"
category: pre_combat
metric:
  name: tension
  direction: ascending
  starting: 0
  threshold_high: 10
beats:
  - id: size_up
    label: "Size Up"
    metric_delta: 2
    stat_check: CUNNING
secondary_stats:
  - name: focus
    source_stat: NERVE
    spendable: true
escalates_to: combat
mood: standoff
"#;
    let original: ConfrontationDef = serde_yaml::from_str(yaml).unwrap();
    let serialized = serde_yaml::to_string(&original).unwrap();
    let restored: ConfrontationDef = serde_yaml::from_str(&serialized).unwrap();

    assert_eq!(original.confrontation_type, restored.confrontation_type);
    assert_eq!(original.label, restored.label);
    assert_eq!(original.category, restored.category);
    assert_eq!(original.metric.name, restored.metric.name);
    assert_eq!(original.metric.direction, restored.metric.direction);
    assert_eq!(original.metric.starting, restored.metric.starting);
    assert_eq!(original.beats.len(), restored.beats.len());
    assert_eq!(original.beats[0].id, restored.beats[0].id);
    assert_eq!(original.secondary_stats.len(), restored.secondary_stats.len());
    assert_eq!(original.escalates_to, restored.escalates_to);
    assert_eq!(original.mood, restored.mood);
}

// =========================================================================
// AC-Validate: Invalid metric direction rejected
// =========================================================================

#[test]
fn confrontation_def_rejects_invalid_metric_direction() {
    let yaml = r#"
type: broken
label: "Broken"
category: combat
metric:
  name: hp
  direction: sideways
  starting: 30
  threshold_low: 0
beats:
  - id: attack
    label: "Attack"
    metric_delta: -5
    stat_check: MIGHT
"#;
    let result: Result<ConfrontationDef, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_err(),
        "invalid metric direction 'sideways' should be rejected"
    );
}

// =========================================================================
// AC-Validate: Invalid category rejected
// =========================================================================

#[test]
fn confrontation_def_rejects_invalid_category() {
    let yaml = r#"
type: broken
label: "Broken"
category: underwater
metric:
  name: hp
  direction: descending
  starting: 30
  threshold_low: 0
beats:
  - id: attack
    label: "Attack"
    metric_delta: -5
    stat_check: MIGHT
"#;
    let result: Result<ConfrontationDef, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_err(),
        "invalid category 'underwater' should be rejected"
    );
}

// =========================================================================
// AC-Validate: Duplicate beat IDs rejected
// =========================================================================

#[test]
fn confrontation_def_rejects_duplicate_beat_ids() {
    let yaml = r#"
type: broken
label: "Broken"
category: combat
metric:
  name: hp
  direction: descending
  starting: 30
  threshold_low: 0
beats:
  - id: attack
    label: "Attack"
    metric_delta: -5
    stat_check: MIGHT
  - id: attack
    label: "Heavy Attack"
    metric_delta: -10
    stat_check: MIGHT
"#;
    // Duplicate beat IDs within a single confrontation type should fail validation.
    // This might be caught by serde(try_from) or by validate().
    let result: Result<ConfrontationDef, _> = serde_yaml::from_str(yaml);
    if let Ok(def) = result {
        // If serde doesn't catch it, the validate step should
        // For now, assert the two beats have different IDs
        let ids: Vec<&str> = def.beats.iter().map(|b| b.id.as_str()).collect();
        let unique: std::collections::HashSet<&str> = ids.iter().copied().collect();
        assert_ne!(
            ids.len(),
            unique.len(),
            "test setup: YAML has duplicate beat IDs — validation should catch this"
        );
        // If we reach here, serde allowed it — Dev must add validation
        panic!(
            "ConfrontationDef should reject duplicate beat IDs, \
             but deserialization succeeded without validation"
        );
    }
    // If serde rejected it, that's also acceptable
}

// =========================================================================
// AC-Validate: Missing required fields rejected
// =========================================================================

#[test]
fn confrontation_def_rejects_missing_type() {
    let yaml = r#"
label: "No Type"
category: combat
metric:
  name: hp
  direction: descending
  starting: 30
beats:
  - id: attack
    label: "Attack"
    metric_delta: -5
    stat_check: MIGHT
"#;
    let result: Result<ConfrontationDef, _> = serde_yaml::from_str(yaml);
    assert!(result.is_err(), "confrontation without 'type' field should be rejected");
}

#[test]
fn confrontation_def_rejects_missing_beats() {
    let yaml = r#"
type: empty
label: "Empty"
category: combat
metric:
  name: hp
  direction: descending
  starting: 30
  threshold_low: 0
"#;
    let result: Result<ConfrontationDef, _> = serde_yaml::from_str(yaml);
    // Either serde rejects (missing field) or validation catches (empty beats)
    match result {
        Err(_) => {} // serde caught it — good
        Ok(def) => {
            assert!(
                !def.beats.is_empty(),
                "confrontation with no beats should be rejected — \
                 a confrontation must have at least one action"
            );
        }
    }
}

#[test]
fn metric_def_rejects_missing_name() {
    let yaml = r#"
direction: ascending
starting: 0
threshold_high: 10
"#;
    let result: Result<MetricDef, _> = serde_yaml::from_str(yaml);
    assert!(result.is_err(), "MetricDef without name should be rejected");
}

#[test]
fn beat_def_rejects_missing_id() {
    let yaml = r#"
label: "No ID"
metric_delta: -5
stat_check: MIGHT
"#;
    let result: Result<BeatDef, _> = serde_yaml::from_str(yaml);
    assert!(result.is_err(), "BeatDef without id should be rejected");
}

// =========================================================================
// AC-Validate: Error messages are descriptive
// =========================================================================

#[test]
fn invalid_direction_error_message_is_descriptive() {
    let yaml = r#"
type: broken
label: "Broken"
category: combat
metric:
  name: hp
  direction: sideways
  starting: 30
beats:
  - id: attack
    label: "Attack"
    metric_delta: -5
    stat_check: MIGHT
"#;
    let err = serde_yaml::from_str::<ConfrontationDef>(yaml)
        .expect_err("should reject invalid direction");
    let msg = err.to_string();
    assert!(
        msg.contains("sideways") || msg.contains("direction"),
        "error message should mention the invalid value or field name, got: {msg}"
    );
}

#[test]
fn invalid_category_error_message_is_descriptive() {
    let yaml = r#"
type: broken
label: "Broken"
category: underwater
metric:
  name: hp
  direction: descending
  starting: 30
beats:
  - id: attack
    label: "Attack"
    metric_delta: -5
    stat_check: MIGHT
"#;
    let err = serde_yaml::from_str::<ConfrontationDef>(yaml)
        .expect_err("should reject invalid category");
    let msg = err.to_string();
    assert!(
        msg.contains("underwater") || msg.contains("category"),
        "error message should mention the invalid value or field name, got: {msg}"
    );
}

// =========================================================================
// AC-StatCheck: stat_check validated against ability_score_names
// =========================================================================

#[test]
fn validate_rejects_invalid_stat_check() {
    // This tests cross-reference validation: a beat's stat_check must reference
    // an ability score name declared in the same rules.yaml.
    // The validation happens at GenrePack::validate() time, not serde time.
    let yaml = r#"
tone: gritty
lethality: moderate
magic_level: none
stat_generation: point_buy
point_buy_budget: 27
ability_score_names: [Brawn, Reflexes, Cunning, Nerve, Grit, Wits]
allowed_classes: [Gunslinger]
allowed_races: [Human]
class_hp_bases:
  Gunslinger: 8
confrontations:
  - type: standoff
    label: "Standoff"
    category: pre_combat
    metric:
      name: tension
      direction: ascending
      starting: 0
      threshold_high: 10
    beats:
      - id: punch
        label: "Punch"
        metric_delta: 1
        stat_check: NONEXISTENT_STAT
"#;
    // Serde should succeed (stat_check is a String)
    let rules: RulesConfig = serde_yaml::from_str(yaml)
        .expect("serde should parse — stat_check validation is semantic, not structural");

    // Validation must catch the invalid stat_check reference
    assert_eq!(rules.confrontations.len(), 1);
    assert_eq!(rules.confrontations[0].beats[0].stat_check, "NONEXISTENT_STAT");

    // Now load a real pack and inject the bad confrontation to test validate()
    let packs_dir = genre_packs_path();
    let mut pack = sidequest_genre::load_genre_pack(&packs_dir.join("mutant_wasteland"))
        .expect("mutant_wasteland should load");

    // Inject a confrontation with an invalid stat_check
    pack.rules.confrontations = rules.confrontations;

    let result = pack.validate();
    assert!(
        result.is_err(),
        "validate() should reject confrontation with invalid stat_check 'NONEXISTENT_STAT'"
    );
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("NONEXISTENT_STAT"),
        "validation error should mention the invalid stat_check, got: {err_msg}"
    );
}

// =========================================================================
// AC-Validate: escalates_to references known confrontation type
// =========================================================================

#[test]
fn validate_escalates_to_references_known_type() {
    let yaml = r#"
tone: gritty
lethality: moderate
magic_level: none
stat_generation: point_buy
point_buy_budget: 27
ability_score_names: [Brawn, Reflexes, Cunning]
allowed_classes: [Gunslinger]
allowed_races: [Human]
class_hp_bases:
  Gunslinger: 8
confrontations:
  - type: standoff
    label: "Standoff"
    category: pre_combat
    metric:
      name: tension
      direction: ascending
      starting: 0
      threshold_high: 10
    beats:
      - id: draw
        label: "Draw"
        metric_delta: 0
        stat_check: BRAWN
    escalates_to: nonexistent_type
"#;
    let rules: RulesConfig = serde_yaml::from_str(yaml)
        .expect("serde should parse — escalates_to validation is semantic");
    assert_eq!(
        rules.confrontations[0].escalates_to.as_deref(),
        Some("nonexistent_type")
    );
    // Now load a real pack and inject to test validate()
    let packs_dir = genre_packs_path();
    let mut pack = sidequest_genre::load_genre_pack(&packs_dir.join("mutant_wasteland"))
        .expect("mutant_wasteland should load");

    pack.rules.confrontations = rules.confrontations;

    let result = pack.validate();
    assert!(
        result.is_err(),
        "validate() should reject escalates_to referencing nonexistent confrontation type"
    );
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("nonexistent_type"),
        "validation error should mention the invalid escalates_to target, got: {err_msg}"
    );
}

// =========================================================================
// Integration: All genre packs load without error
// =========================================================================

/// Helper to locate genre packs directory.
fn genre_packs_path() -> std::path::PathBuf {
    if let Ok(path) = std::env::var("GENRE_PACKS_PATH") {
        return std::path::PathBuf::from(path);
    }
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.join("../../../sidequest-content/genre_packs")
}

#[test]
fn all_genre_packs_load_with_confrontations_field() {
    let packs_dir = genre_packs_path();
    let entries: Vec<_> = std::fs::read_dir(&packs_dir)
        .expect("should read genre_packs directory")
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .collect();

    assert!(
        !entries.is_empty(),
        "should find at least one genre pack in {}",
        packs_dir.display()
    );

    for entry in &entries {
        let pack_path = entry.path();
        let pack_name = pack_path.file_name().unwrap().to_string_lossy();

        let result = sidequest_genre::load_genre_pack(&pack_path);
        assert!(
            result.is_ok(),
            "genre pack '{}' should load without error, got: {:?}",
            pack_name,
            result.err()
        );

        // Verify the confrontations field exists and is accessible
        // (even if empty — that's fine for packs without confrontations)
        let pack = result.unwrap();
        // This line will fail to compile until GenrePack has a confrontations field
        let _confrontations = &pack.rules.confrontations;
    }
}

// =========================================================================
// Integration: Packs without confrontations have empty vec
// =========================================================================

#[test]
fn packs_without_confrontations_yaml_have_empty_vec() {
    let packs_dir = genre_packs_path();

    // low_fantasy is unlikely to have confrontations in v1
    let lf_path = packs_dir.join("low_fantasy");
    if lf_path.exists() {
        let pack = sidequest_genre::load_genre_pack(&lf_path)
            .expect("low_fantasy should load");
        assert!(
            pack.rules.confrontations.is_empty(),
            "low_fantasy should have empty confrontations (no YAML declarations yet)"
        );
    }
}

// =========================================================================
// Rust Rule #2: Public enums should have #[non_exhaustive] if they'll grow
// =========================================================================

// Note: MetricDirection and ConfrontationCategory should be enums, not raw strings.
// If Dev implements them as enums, they should have #[non_exhaustive].
// This is a design recommendation captured in the TEA assessment, not a
// compile-time test (Rust's #[non_exhaustive] is not testable at runtime).

// =========================================================================
// Rust Rule #8: Deserialize bypass — if ConfrontationDef validates on
// construction, deserialization must also validate
// =========================================================================

#[test]
fn confrontation_def_deserialize_validates_same_as_constructor() {
    // If Dev adds a validating constructor (e.g., ConfrontationDef::new() -> Result),
    // then serde deserialization must enforce the same rules.
    // Test: deserialize invalid data and confirm rejection.
    let yaml_empty_type = r#"
type: ""
label: "Empty Type"
category: combat
metric:
  name: hp
  direction: descending
  starting: 30
beats:
  - id: attack
    label: "Attack"
    metric_delta: -5
    stat_check: MIGHT
"#;
    let result: Result<ConfrontationDef, _> = serde_yaml::from_str(yaml_empty_type);
    // Empty type string should be rejected — a confrontation must have a type identifier
    assert!(
        result.is_err(),
        "ConfrontationDef with empty type should be rejected by serde validation"
    );
}

#[test]
fn beat_def_deserialize_rejects_empty_id() {
    let yaml = r#"
id: ""
label: "Empty ID"
metric_delta: -5
stat_check: MIGHT
"#;
    let result: Result<BeatDef, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_err(),
        "BeatDef with empty id should be rejected"
    );
}

// =========================================================================
// Edge cases: metric thresholds
// =========================================================================

#[test]
fn metric_def_both_thresholds_optional() {
    let yaml = r#"
name: morale
direction: bidirectional
starting: 5
"#;
    let metric: MetricDef = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(metric.threshold_high, None);
    assert_eq!(metric.threshold_low, None);
}

#[test]
fn metric_def_negative_starting() {
    let yaml = r#"
name: debt
direction: descending
starting: -10
threshold_low: -100
"#;
    let metric: MetricDef = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(metric.starting, -10);
    assert_eq!(metric.threshold_low, Some(-100));
}

// =========================================================================
// Edge cases: multiple confrontation types in one rules.yaml
// =========================================================================

#[test]
fn rules_config_multiple_confrontations_all_accessible() {
    let yaml = r#"
tone: gonzo
lethality: high
magic_level: none
stat_generation: point_buy
point_buy_budget: 27
ability_score_names: [Brawn, Reflexes, Cunning, Nerve, Grit, Wits]
allowed_classes: [Gunslinger]
allowed_races: [Human]
class_hp_bases:
  Gunslinger: 8
confrontations:
  - type: standoff
    label: "Standoff"
    category: pre_combat
    metric:
      name: tension
      direction: ascending
      starting: 0
      threshold_high: 10
    beats:
      - id: draw
        label: "Draw"
        metric_delta: 0
        stat_check: NERVE
  - type: duel
    label: "Duel"
    category: combat
    metric:
      name: hp
      direction: descending
      starting: 20
      threshold_low: 0
    beats:
      - id: slash
        label: "Slash"
        metric_delta: -3
        stat_check: REFLEXES
  - type: negotiation
    label: "Negotiation"
    category: social
    metric:
      name: leverage
      direction: ascending
      starting: 0
      threshold_high: 5
    beats:
      - id: persuade
        label: "Persuade"
        metric_delta: 1
        stat_check: CUNNING
"#;
    let rules: RulesConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(rules.confrontations.len(), 3);

    let types: Vec<&str> = rules
        .confrontations
        .iter()
        .map(|c| c.confrontation_type.as_str())
        .collect();
    assert!(types.contains(&"standoff"));
    assert!(types.contains(&"duel"));
    assert!(types.contains(&"negotiation"));
}

// =========================================================================
// Edge: beat with zero metric_delta is valid
// =========================================================================

#[test]
fn beat_with_zero_delta_is_valid() {
    let yaml = r#"
id: observe
label: "Observe"
metric_delta: 0
stat_check: WITS
reveals: enemy_position
"#;
    let beat: BeatDef = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(beat.metric_delta, 0);
    assert_eq!(beat.reveals.as_deref(), Some("enemy_position"));
}

// =========================================================================
// Edge: category values
// =========================================================================

#[test]
fn all_valid_categories_accepted() {
    let categories = ["combat", "social", "pre_combat", "movement"];
    for cat in &categories {
        let yaml = format!(
            r#"
type: test_{cat}
label: "Test"
category: {cat}
metric:
  name: x
  direction: ascending
  starting: 0
beats:
  - id: act
    label: "Act"
    metric_delta: 1
    stat_check: MIGHT
"#
        );
        let result: Result<ConfrontationDef, _> = serde_yaml::from_str(&yaml);
        assert!(
            result.is_ok(),
            "category '{cat}' should be accepted, got: {:?}",
            result.err()
        );
    }
}

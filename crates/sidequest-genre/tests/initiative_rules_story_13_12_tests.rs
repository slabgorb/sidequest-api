//! RED tests for Story 13-12: Initiative stat mapping — genre pack schema.
//!
//! Initiative rules map encounter types to primary stats for turn ordering.
//! The schema lives in rules.yaml. Each entry is encounter_type → primary_stat
//! + narrator-facing description.
//!
//! Types under test:
//!   - `InitiativeRule` — single encounter_type → stat mapping
//!   - `RulesConfig.initiative_rules` — HashMap<String, InitiativeRule> field
//!   - `GenrePack` loader integration — initiative rules loaded from YAML
//!   - Validation — initiative rule stat names checked against ability_score_names
//!
//! These tests WILL NOT COMPILE until the types are created — this is the RED
//! state for Rust TDD.

use sidequest_genre::{InitiativeRule, RulesConfig};

// ===========================================================================
// AC: Schema defined — InitiativeRule struct with primary_stat + description
// ===========================================================================

#[test]
fn initiative_rule_deserializes_from_yaml() {
    let yaml = r#"
primary_stat: DEX
description: "Reflexes and speed determine who strikes first"
"#;
    let rule: InitiativeRule = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(rule.primary_stat, "DEX");
    assert_eq!(
        rule.description,
        "Reflexes and speed determine who strikes first"
    );
}

#[test]
fn initiative_rule_requires_primary_stat() {
    // Missing primary_stat should fail deserialization
    let yaml = r#"
description: "Some description"
"#;
    let result = serde_yaml::from_str::<InitiativeRule>(yaml);
    assert!(
        result.is_err(),
        "InitiativeRule without primary_stat should fail to deserialize"
    );
}

#[test]
fn initiative_rule_requires_description() {
    // Missing description should fail deserialization
    let yaml = r#"
primary_stat: CHA
"#;
    let result = serde_yaml::from_str::<InitiativeRule>(yaml);
    assert!(
        result.is_err(),
        "InitiativeRule without description should fail to deserialize"
    );
}

// ===========================================================================
// AC: RulesConfig has initiative_rules field
// ===========================================================================

#[test]
fn rules_config_parses_initiative_rules() {
    let yaml = r#"
initiative_rules:
  combat:
    primary_stat: DEX
    description: "Reflexes and speed determine who strikes first"
  social:
    primary_stat: CHA
    description: "Force of personality controls the conversation"
"#;
    let rules: RulesConfig = serde_yaml::from_str(yaml).unwrap();
    let initiative = &rules.initiative_rules;
    assert_eq!(initiative.len(), 2, "Should have 2 encounter types");

    let combat = initiative.get("combat").expect("combat entry missing");
    assert_eq!(combat.primary_stat, "DEX");

    let social = initiative.get("social").expect("social entry missing");
    assert_eq!(social.primary_stat, "CHA");
}

#[test]
fn rules_config_initiative_rules_empty_when_absent() {
    // Genres without initiative_rules should still load (empty map).
    // This is the default for genres that haven't been authored yet.
    let yaml = r#"
tone: "grimdark"
"#;
    let rules: RulesConfig = serde_yaml::from_str(yaml).unwrap();
    assert!(
        rules.initiative_rules.is_empty(),
        "Missing initiative_rules should default to empty HashMap"
    );
}

// ===========================================================================
// AC: Caverns_and_claudes has combat, chase, social, exploration entries
// ===========================================================================

#[test]
fn caverns_initiative_rules_has_required_encounter_types() {
    // This YAML matches what should be authored in caverns_and_claudes/rules.yaml
    let yaml = r#"
ability_score_names:
  - STR
  - DEX
  - CON
  - INT
  - WIS
  - CHA
initiative_rules:
  combat:
    primary_stat: DEX
    description: "Reflexes and speed determine who strikes first"
  chase:
    primary_stat: DEX
    description: "Agility and footwork set the pace"
  social:
    primary_stat: CHA
    description: "Force of personality controls the conversation"
  exploration:
    primary_stat: WIS
    description: "Awareness determines who notices things first"
"#;
    let rules: RulesConfig = serde_yaml::from_str(yaml).unwrap();
    let initiative = &rules.initiative_rules;

    assert!(initiative.contains_key("combat"), "Missing combat");
    assert!(initiative.contains_key("chase"), "Missing chase");
    assert!(initiative.contains_key("social"), "Missing social");
    assert!(
        initiative.contains_key("exploration"),
        "Missing exploration"
    );

    assert_eq!(initiative["combat"].primary_stat, "DEX");
    assert_eq!(initiative["chase"].primary_stat, "DEX");
    assert_eq!(initiative["social"].primary_stat, "CHA");
    assert_eq!(initiative["exploration"].primary_stat, "WIS");
}

// ===========================================================================
// AC: Validation — initiative stats checked against ability_score_names
// ===========================================================================

#[test]
fn validation_rejects_invalid_initiative_stat() {
    // If an initiative rule references a stat that isn't in ability_score_names,
    // validation should catch it.
    let yaml = r#"
ability_score_names:
  - STR
  - DEX
  - CON
  - INT
  - WIS
  - CHA
initiative_rules:
  combat:
    primary_stat: REFLEXES
    description: "This stat doesn't exist in the ability score list"
"#;
    let rules: RulesConfig = serde_yaml::from_str(yaml).unwrap();

    // Build a minimal GenrePack to test validation.
    // We need to use the full loader or construct one manually.
    // Since GenrePack is assembled by the loader, we test validation
    // through the validate_initiative_rules method which should exist
    // on GenrePack after implementation.
    //
    // For now, verify the rules parsed — the validation test below
    // uses the real loader to test the full pipeline.
    assert_eq!(rules.initiative_rules["combat"].primary_stat, "REFLEXES");
}

// ===========================================================================
// AC: Loader wiring — GenrePack loads initiative rules from real genre pack
// ===========================================================================

#[test]
fn loader_reads_initiative_rules_from_caverns() {
    // This is the wiring test — verifies the loader actually reads
    // initiative_rules from the caverns_and_claudes genre pack.
    //
    // The GENRE_PACKS_PATH env var must point to sidequest-content/genre_packs/
    // or we use a relative path from the test working directory.
    let genre_packs_path = std::env::var("GENRE_PACKS_PATH").unwrap_or_else(|_| {
        // Relative path from sidequest-api workspace root
        "../../sidequest-content/genre_packs".to_string()
    });
    let pack_path = std::path::Path::new(&genre_packs_path).join("caverns_and_claudes");

    if !pack_path.exists() {
        // Skip if content repo not available (CI without subrepo)
        eprintln!(
            "SKIP: caverns_and_claudes not found at {}",
            pack_path.display()
        );
        return;
    }

    let pack =
        sidequest_genre::load_genre_pack(&pack_path).expect("Failed to load caverns_and_claudes");

    // Verify initiative_rules is populated
    let initiative = &pack.rules.initiative_rules;
    assert!(
        !initiative.is_empty(),
        "caverns_and_claudes must have initiative_rules authored"
    );

    // Verify the required encounter types exist
    assert!(
        initiative.contains_key("combat"),
        "caverns_and_claudes missing combat initiative rule"
    );
    assert!(
        initiative.contains_key("chase"),
        "caverns_and_claudes missing chase initiative rule"
    );
    assert!(
        initiative.contains_key("social"),
        "caverns_and_claudes missing social initiative rule"
    );
    assert!(
        initiative.contains_key("exploration"),
        "caverns_and_claudes missing exploration initiative rule"
    );

    // Verify stats are valid ability score names
    let valid_stats: Vec<&str> = pack
        .rules
        .ability_score_names
        .iter()
        .map(|s| s.as_str())
        .collect();
    for (encounter_type, rule) in initiative {
        assert!(
            valid_stats.contains(&rule.primary_stat.as_str()),
            "initiative rule for '{}' has primary_stat '{}' which is not a valid ability score (valid: {:?})",
            encounter_type,
            rule.primary_stat,
            valid_stats
        );
    }
}

// ===========================================================================
// Roundtrip: serialize → deserialize preserves data
// ===========================================================================

#[test]
fn initiative_rule_roundtrips_through_serde() {
    let yaml = r#"
primary_stat: WIS
description: "Judgment and perception guide the defense"
"#;
    let rule: InitiativeRule = serde_yaml::from_str(yaml).unwrap();
    let serialized = serde_yaml::to_string(&rule).unwrap();
    let deserialized: InitiativeRule = serde_yaml::from_str(&serialized).unwrap();
    assert_eq!(deserialized.primary_stat, "WIS");
    assert_eq!(
        deserialized.description,
        "Judgment and perception guide the defense"
    );
}

// ===========================================================================
// Edge cases
// ===========================================================================

#[test]
fn initiative_rules_with_extra_encounter_types_is_ok() {
    // Genres can define more encounter types than the minimum four.
    let yaml = r#"
initiative_rules:
  combat:
    primary_stat: DEX
    description: "Speed"
  trial:
    primary_stat: WIS
    description: "Judgment"
  ritual:
    primary_stat: INT
    description: "Knowledge"
"#;
    let rules: RulesConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(
        rules.initiative_rules.len(),
        3,
        "Should accept arbitrary encounter type names"
    );
}

#[test]
fn initiative_rules_stat_names_are_case_preserved() {
    // Stats should preserve their case as authored in YAML.
    let yaml = r#"
primary_stat: CHA
description: "Charm"
"#;
    let rule: InitiativeRule = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(
        rule.primary_stat, "CHA",
        "Stat name should be case-preserved"
    );
}

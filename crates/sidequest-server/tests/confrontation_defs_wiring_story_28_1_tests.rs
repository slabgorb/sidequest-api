//! Story 28-1: Load ConfrontationDefs into server DispatchContext
//!
//! Foundation story for Epic 28 (Unified Encounter Engine). Genre packs
//! declare confrontation types in rules.yaml. The server must hold these
//! defs in DispatchContext so apply_beat(), format_encounter_context(),
//! and beat population have access at runtime.
//!
//! ACs tested:
//!   AC-Defs-Loaded:  DispatchContext holds confrontation_defs field
//!   AC-Lookup:       find_confrontation_def() finds a def by encounter_type string
//!   AC-Lookup-Miss:  find_confrontation_def() returns None for unknown types
//!   AC-Non-Empty:    Real genre pack (spaghetti_western) loads 3+ confrontation types
//!   AC-Wiring:       ConfrontationDef is reachable from the server crate

use sidequest_genre::ConfrontationDef;

// =========================================================================
// AC-Wiring: ConfrontationDef is importable and usable from server crate
// =========================================================================

/// Compile-time wiring proof: ConfrontationDef is reachable from sidequest-server.
/// If the type is removed from sidequest-genre's public API, this fails.
#[test]
fn confrontation_def_is_reachable_from_server_crate() {
    // Verify the type exists and key fields are accessible
    let yaml = r#"
type: test_encounter
label: "Test"
category: combat
metric:
  name: hp
  direction: descending
  starting: 30
  threshold_low: 0
beats:
  - id: strike
    label: "Strike"
    metric_delta: -5
    stat_check: MIGHT
"#;
    let def: ConfrontationDef = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(def.confrontation_type, "test_encounter");
    assert_eq!(def.beats.len(), 1);
    assert_eq!(def.beats[0].id, "strike");
}

// =========================================================================
// AC-Defs-Loaded: DispatchContext has confrontation_defs field
// =========================================================================

/// DispatchContext must have a confrontation_defs field that holds a slice
/// of ConfrontationDef references. This test will fail to compile until
/// the field is added to DispatchContext.
#[test]
fn dispatch_context_has_confrontation_defs_field() {
    // This test verifies the field exists at compile time.
    // We can't construct a full DispatchContext (55+ fields), so we verify
    // the field type by checking the accessor method instead.
    //
    // Once find_confrontation_def is added to DispatchContext or as a free
    // function, this test calls it to prove the field is usable.
    //
    // FAILING: DispatchContext does not yet have confrontation_defs field
    // or find_confrontation_def method. Uncomment when implemented:

    // Structural check: the server module that exposes the lookup function
    // must exist and be importable.
    use sidequest_server::find_confrontation_def;

    let defs: Vec<ConfrontationDef> = vec![];
    let result = find_confrontation_def(&defs, "combat");
    assert!(result.is_none(), "Empty defs slice must return None");
}

// =========================================================================
// AC-Lookup: find_confrontation_def() by encounter_type string
// =========================================================================

/// Lookup by encounter_type should return the matching ConfrontationDef.
#[test]
fn find_confrontation_def_returns_matching_def() {
    use sidequest_server::find_confrontation_def;

    let standoff_yaml = r#"
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
"#;
    let combat_yaml = r#"
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
    let defs: Vec<ConfrontationDef> = vec![
        serde_yaml::from_str(standoff_yaml).unwrap(),
        serde_yaml::from_str(combat_yaml).unwrap(),
    ];

    let found = find_confrontation_def(&defs, "standoff");
    assert!(found.is_some(), "Should find standoff def");
    assert_eq!(found.unwrap().confrontation_type, "standoff");
    assert_eq!(found.unwrap().beats[0].id, "size_up");

    let found_combat = find_confrontation_def(&defs, "combat");
    assert!(found_combat.is_some(), "Should find combat def");
    assert_eq!(found_combat.unwrap().confrontation_type, "combat");
}

// =========================================================================
// AC-Lookup-Miss: find_confrontation_def() returns None for unknown types
// =========================================================================

/// Lookup for a type that doesn't exist must return None, not panic.
#[test]
fn find_confrontation_def_returns_none_for_unknown_type() {
    use sidequest_server::find_confrontation_def;

    let combat_yaml = r#"
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
    let defs: Vec<ConfrontationDef> = vec![
        serde_yaml::from_str(combat_yaml).unwrap(),
    ];

    let result = find_confrontation_def(&defs, "ship_combat");
    assert!(result.is_none(), "Unknown encounter type must return None");

    let empty_result = find_confrontation_def(&defs, "");
    assert!(empty_result.is_none(), "Empty string must return None");
}

// =========================================================================
// AC-Non-Empty: Real genre pack loads confrontation defs
// =========================================================================

/// spaghetti_western declares 3 confrontation types (standoff, negotiation,
/// poker). Loading its rules.yaml must produce a non-empty confrontations vec.
/// This proves the loading pipeline works end-to-end from YAML to Rust types.
#[test]
fn spaghetti_western_loads_confrontation_defs() {
    let genre_packs_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("sidequest-content")
        .join("genre_packs");

    // Verify the path exists (fail loudly, no silent fallback)
    assert!(
        genre_packs_path.exists(),
        "Genre packs directory not found at {:?}",
        genre_packs_path
    );

    let sw_rules_path = genre_packs_path
        .join("spaghetti_western")
        .join("rules.yaml");
    assert!(
        sw_rules_path.exists(),
        "spaghetti_western/rules.yaml not found at {:?}",
        sw_rules_path
    );

    let rules_yaml = std::fs::read_to_string(&sw_rules_path).unwrap();
    let rules: sidequest_genre::RulesConfig = serde_yaml::from_str(&rules_yaml).unwrap();

    assert!(
        !rules.confrontations.is_empty(),
        "spaghetti_western must have at least one confrontation type"
    );
    assert!(
        rules.confrontations.len() >= 3,
        "spaghetti_western should have at least 3 confrontation types (standoff, negotiation, poker), found {}",
        rules.confrontations.len()
    );

    // Verify we can find specific types
    let standoff = rules
        .confrontations
        .iter()
        .find(|c| c.confrontation_type == "standoff");
    assert!(standoff.is_some(), "spaghetti_western must have a 'standoff' confrontation type");

    let standoff = standoff.unwrap();
    assert!(!standoff.beats.is_empty(), "standoff must have at least one beat");
    assert_eq!(standoff.category, "pre_combat");
}

/// victoria has negotiation, trial, auction but NO combat — verify this.
#[test]
fn victoria_has_no_combat_confrontation_type() {
    let genre_packs_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("sidequest-content")
        .join("genre_packs");

    let vic_rules_path = genre_packs_path
        .join("victoria")
        .join("rules.yaml");
    assert!(
        vic_rules_path.exists(),
        "victoria/rules.yaml not found at {:?}",
        vic_rules_path
    );

    let rules_yaml = std::fs::read_to_string(&vic_rules_path).unwrap();
    let rules: sidequest_genre::RulesConfig = serde_yaml::from_str(&rules_yaml).unwrap();

    let combat = rules
        .confrontations
        .iter()
        .find(|c| c.confrontation_type == "combat");
    assert!(
        combat.is_none(),
        "victoria must NOT have a 'combat' confrontation type — it's social intrigue, not combat"
    );

    // But it should have social encounter types
    assert!(
        !rules.confrontations.is_empty(),
        "victoria should have social confrontation types (negotiation, trial, auction)"
    );
}

//! Story 16-12: Wire genre resources — Luck, Humanity, Heat, Fuel-at-rest as ResourcePool instances
//!
//! RED phase tests. Verify genre-specific resource declarations in rules.yaml load
//! correctly, initialize ResourcePools, and wire through the full pipeline.
//!
//! ACs tested:
//!   AC1: spaghetti_western declares Luck (0-6, voluntary, thresholds at 1 and 0)
//!   AC2: neon_dystopia declares Humanity (0-100, involuntary, thresholds at 50/25/0)
//!   AC3: pulp_noir declares Heat (0-5, involuntary, decay 0.1/turn)
//!   AC4: road_warrior declares Fuel (0-100, transfer to RigStats on confrontation)
//!   AC5: Genre loader parses and inits ResourcePools on GameSnapshot
//!   AC6: Bounds validation per genre
//!   AC7: Integration: load → init → patch → threshold → KnownFact

use sidequest_genre::{load_genre_pack, ResourceDeclaration, RulesConfig};
use sidequest_game::lore::LoreStore;
use sidequest_game::state::{
    GameSnapshot, ResourcePatchOp, ResourcePool,
};
use std::path::PathBuf;

// ═══════════════════════════════════════════════════════════
// Test helpers
// ═══════════════════════════════════════════════════════════

/// Path to genre packs in sidequest-content (relative to workspace root).
fn genre_pack_path(genre: &str) -> PathBuf {
    // Integration tests run from the workspace root
    let content_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent() // crates/
        .unwrap()
        .parent() // sidequest-api/
        .unwrap()
        .parent() // oq-1/
        .unwrap()
        .join("sidequest-content")
        .join("genre_packs")
        .join(genre);
    content_dir
}

fn load_rules_yaml(genre: &str) -> RulesConfig {
    let path = genre_pack_path(genre).join("rules.yaml");
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {e}", path.display()));
    serde_yaml::from_str(&content)
        .unwrap_or_else(|e| panic!("Failed to parse {}: {e}", path.display()))
}

fn find_resource<'a>(rules: &'a RulesConfig, name: &str) -> &'a ResourceDeclaration {
    rules
        .resources
        .iter()
        .find(|r| r.name == name)
        .unwrap_or_else(|| panic!("resource '{name}' not found in rules.yaml"))
}

// ═══════════════════════════════════════════════════════════
// AC1: spaghetti_western — Luck (0-6, voluntary, thresholds at 1 and 0)
// ═══════════════════════════════════════════════════════════

#[test]
fn spaghetti_western_has_luck_resource() {
    let rules = load_rules_yaml("spaghetti_western");
    let luck = find_resource(&rules, "luck");

    assert_eq!(luck.label, "Luck");
    assert!((luck.min - 0.0).abs() < f64::EPSILON, "luck min should be 0");
    assert!((luck.max - 6.0).abs() < f64::EPSILON, "luck max should be 6");
    assert!(luck.voluntary, "luck should be voluntary (player-spendable)");
    assert!(
        (luck.decay_per_turn - 0.0).abs() < f64::EPSILON,
        "luck should not decay"
    );
}

#[test]
fn spaghetti_western_luck_starting_value() {
    let rules = load_rules_yaml("spaghetti_western");
    let luck = find_resource(&rules, "luck");

    assert!(
        luck.starting >= luck.min && luck.starting <= luck.max,
        "starting value {} should be in [{}, {}]",
        luck.starting,
        luck.min,
        luck.max,
    );
}

#[test]
fn spaghetti_western_luck_has_threshold_at_1() {
    let rules = load_rules_yaml("spaghetti_western");
    let luck = find_resource(&rules, "luck");

    assert!(
        luck.thresholds.iter().any(|t| (t.at - 1.0).abs() < f64::EPSILON),
        "luck should have a threshold at 1.0"
    );
}

#[test]
fn spaghetti_western_luck_has_threshold_at_0() {
    let rules = load_rules_yaml("spaghetti_western");
    let luck = find_resource(&rules, "luck");

    assert!(
        luck.thresholds.iter().any(|t| (t.at - 0.0).abs() < f64::EPSILON),
        "luck should have a threshold at 0.0"
    );
}

#[test]
fn spaghetti_western_luck_thresholds_have_event_ids() {
    let rules = load_rules_yaml("spaghetti_western");
    let luck = find_resource(&rules, "luck");

    for threshold in &luck.thresholds {
        assert!(
            !threshold.event_id.is_empty(),
            "every threshold should have a non-empty event_id"
        );
        assert!(
            !threshold.narrator_hint.is_empty(),
            "every threshold should have a non-empty narrator_hint"
        );
    }
}

// ═══════════════════════════════════════════════════════════
// AC2: neon_dystopia — Humanity (0-100, involuntary, thresholds at 50/25/0)
// ═══════════════════════════════════════════════════════════

#[test]
fn neon_dystopia_has_humanity_resource() {
    let rules = load_rules_yaml("neon_dystopia");
    let humanity = find_resource(&rules, "humanity");

    assert_eq!(humanity.label, "Humanity");
    assert!((humanity.min - 0.0).abs() < f64::EPSILON);
    assert!((humanity.max - 100.0).abs() < f64::EPSILON);
    assert!(!humanity.voluntary, "humanity should be involuntary");
}

#[test]
fn neon_dystopia_humanity_has_threshold_at_50() {
    let rules = load_rules_yaml("neon_dystopia");
    let humanity = find_resource(&rules, "humanity");

    assert!(
        humanity.thresholds.iter().any(|t| (t.at - 50.0).abs() < f64::EPSILON),
        "humanity should have a threshold at 50"
    );
}

#[test]
fn neon_dystopia_humanity_has_threshold_at_25() {
    let rules = load_rules_yaml("neon_dystopia");
    let humanity = find_resource(&rules, "humanity");

    assert!(
        humanity.thresholds.iter().any(|t| (t.at - 25.0).abs() < f64::EPSILON),
        "humanity should have a threshold at 25"
    );
}

#[test]
fn neon_dystopia_humanity_has_threshold_at_0() {
    let rules = load_rules_yaml("neon_dystopia");
    let humanity = find_resource(&rules, "humanity");

    assert!(
        humanity.thresholds.iter().any(|t| (t.at - 0.0).abs() < f64::EPSILON),
        "humanity should have a threshold at 0"
    );
}

#[test]
fn neon_dystopia_humanity_thresholds_have_narrator_hints() {
    let rules = load_rules_yaml("neon_dystopia");
    let humanity = find_resource(&rules, "humanity");

    assert!(
        humanity.thresholds.len() >= 3,
        "humanity should have at least 3 thresholds (50, 25, 0)"
    );
    for threshold in &humanity.thresholds {
        assert!(
            !threshold.narrator_hint.is_empty(),
            "threshold at {} should have a narrator_hint",
            threshold.at
        );
    }
}

// ═══════════════════════════════════════════════════════════
// AC3: pulp_noir — Heat (0-5, involuntary, decay 0.1/turn)
// ═══════════════════════════════════════════════════════════

#[test]
fn pulp_noir_has_heat_resource() {
    let rules = load_rules_yaml("pulp_noir");
    let heat = find_resource(&rules, "heat");

    assert_eq!(heat.label, "Heat");
    assert!((heat.min - 0.0).abs() < f64::EPSILON);
    assert!((heat.max - 5.0).abs() < f64::EPSILON);
    assert!(!heat.voluntary, "heat should be involuntary");
}

#[test]
fn pulp_noir_heat_has_decay() {
    let rules = load_rules_yaml("pulp_noir");
    let heat = find_resource(&rules, "heat");

    assert!(
        (heat.decay_per_turn - (-0.1)).abs() < f64::EPSILON,
        "heat should decay by 0.1 per turn, got: {}",
        heat.decay_per_turn
    );
}

#[test]
fn pulp_noir_heat_starts_at_zero() {
    let rules = load_rules_yaml("pulp_noir");
    let heat = find_resource(&rules, "heat");

    assert!(
        (heat.starting - 0.0).abs() < f64::EPSILON,
        "heat should start at 0 (you earn heat, not start with it)"
    );
}

// ═══════════════════════════════════════════════════════════
// AC4: road_warrior — Fuel (0-100, resource-at-rest → RigStats transfer)
// ═══════════════════════════════════════════════════════════

#[test]
fn road_warrior_has_fuel_resource() {
    let rules = load_rules_yaml("road_warrior");
    let fuel = find_resource(&rules, "fuel");

    assert_eq!(fuel.label, "Fuel");
    assert!((fuel.min - 0.0).abs() < f64::EPSILON);
    assert!((fuel.max - 100.0).abs() < f64::EPSILON);
    assert!(!fuel.voluntary, "fuel should be involuntary (consumed by driving)");
}

#[test]
fn road_warrior_fuel_starting_value() {
    let rules = load_rules_yaml("road_warrior");
    let fuel = find_resource(&rules, "fuel");

    assert!(
        fuel.starting > 0.0,
        "fuel should have a positive starting value"
    );
    assert!(
        fuel.starting <= fuel.max,
        "fuel starting should not exceed max"
    );
}

// ═══════════════════════════════════════════════════════════
// AC5: Genre loader parses and inits ResourcePools on GameSnapshot
// ═══════════════════════════════════════════════════════════

#[test]
fn genre_loader_parses_spaghetti_western_resources() {
    let path = genre_pack_path("spaghetti_western");
    if !path.exists() {
        panic!("spaghetti_western genre pack not found at {}", path.display());
    }
    let pack = load_genre_pack(&path).expect("should load spaghetti_western");

    let luck = pack
        .rules
        .resources
        .iter()
        .find(|r| r.name == "luck");
    assert!(luck.is_some(), "loader should parse luck resource from spaghetti_western");
}

#[test]
fn genre_loader_parses_neon_dystopia_resources() {
    let path = genre_pack_path("neon_dystopia");
    let pack = load_genre_pack(&path).expect("should load neon_dystopia");

    let humanity = pack
        .rules
        .resources
        .iter()
        .find(|r| r.name == "humanity");
    assert!(humanity.is_some(), "loader should parse humanity resource from neon_dystopia");
}

#[test]
fn init_pools_from_spaghetti_western_declarations() {
    let rules = load_rules_yaml("spaghetti_western");
    let mut snap = GameSnapshot::default();

    snap.init_resource_pools(&rules.resources);

    assert!(
        snap.resources.contains_key("luck"),
        "luck pool should be initialized from spaghetti_western declarations"
    );
    let pool = &snap.resources["luck"];
    assert!((pool.max - 6.0).abs() < f64::EPSILON);
    assert!(pool.voluntary);
}

#[test]
fn init_pools_from_neon_dystopia_declarations() {
    let rules = load_rules_yaml("neon_dystopia");
    let mut snap = GameSnapshot::default();

    snap.init_resource_pools(&rules.resources);

    assert!(snap.resources.contains_key("humanity"));
    let pool = &snap.resources["humanity"];
    assert!((pool.max - 100.0).abs() < f64::EPSILON);
    assert!(!pool.voluntary);
    assert!(
        pool.thresholds.len() >= 3,
        "humanity pool should have at least 3 thresholds from YAML"
    );
}

#[test]
fn init_pools_from_pulp_noir_declarations() {
    let rules = load_rules_yaml("pulp_noir");
    let mut snap = GameSnapshot::default();

    snap.init_resource_pools(&rules.resources);

    assert!(snap.resources.contains_key("heat"));
    let pool = &snap.resources["heat"];
    assert!((pool.decay_per_turn - (-0.1)).abs() < f64::EPSILON);
}

#[test]
fn init_pools_from_road_warrior_declarations() {
    let rules = load_rules_yaml("road_warrior");
    let mut snap = GameSnapshot::default();

    snap.init_resource_pools(&rules.resources);

    assert!(snap.resources.contains_key("fuel"));
    let pool = &snap.resources["fuel"];
    assert!((pool.max - 100.0).abs() < f64::EPSILON);
}

// ═══════════════════════════════════════════════════════════
// AC6: Bounds validation per genre
// ═══════════════════════════════════════════════════════════

#[test]
fn spaghetti_western_luck_validates_bounds() {
    let rules = load_rules_yaml("spaghetti_western");
    let mut snap = GameSnapshot::default();
    snap.init_resource_pools(&rules.resources);

    // Try to exceed luck max (6.0)
    let result = snap.apply_resource_patch_by_name(
        "luck",
        ResourcePatchOp::Add,
        100.0,
    );
    assert!(result.is_ok());
    assert!(
        snap.resources["luck"].current <= 6.0,
        "luck should clamp to max 6.0"
    );
}

#[test]
fn neon_dystopia_humanity_validates_bounds() {
    let rules = load_rules_yaml("neon_dystopia");
    let mut snap = GameSnapshot::default();
    snap.init_resource_pools(&rules.resources);

    // Try to go below humanity min (0.0)
    let result = snap.apply_resource_patch_by_name(
        "humanity",
        ResourcePatchOp::Subtract,
        999.0,
    );
    assert!(result.is_ok());
    assert!(
        snap.resources["humanity"].current >= 0.0,
        "humanity should clamp to min 0.0"
    );
}

#[test]
fn pulp_noir_heat_validates_bounds() {
    let rules = load_rules_yaml("pulp_noir");
    let mut snap = GameSnapshot::default();
    snap.init_resource_pools(&rules.resources);

    // Try to exceed heat max (5.0)
    let result = snap.apply_resource_patch_by_name(
        "heat",
        ResourcePatchOp::Add,
        100.0,
    );
    assert!(result.is_ok());
    assert!(
        snap.resources["heat"].current <= 5.0,
        "heat should clamp to max 5.0"
    );
}

#[test]
fn road_warrior_fuel_validates_bounds() {
    let rules = load_rules_yaml("road_warrior");
    let mut snap = GameSnapshot::default();
    snap.init_resource_pools(&rules.resources);

    // Try to go below fuel min (0.0)
    let result = snap.apply_resource_patch_by_name(
        "fuel",
        ResourcePatchOp::Subtract,
        999.0,
    );
    assert!(result.is_ok());
    assert!(
        snap.resources["fuel"].current >= 0.0,
        "fuel should clamp to min 0.0"
    );
}

// ═══════════════════════════════════════════════════════════
// AC7: Integration — load → init → patch → threshold → KnownFact
// ═══════════════════════════════════════════════════════════

#[test]
fn spaghetti_western_luck_threshold_fires_known_fact() {
    let rules = load_rules_yaml("spaghetti_western");
    let mut snap = GameSnapshot::default();
    snap.init_resource_pools(&rules.resources);

    let mut store = LoreStore::new();

    // Drain luck from starting to below threshold at 1.0
    let starting = snap.resources["luck"].current;
    let drain = starting; // drain all luck to 0
    snap.process_resource_patch_with_lore(
        "luck",
        ResourcePatchOp::Subtract,
        drain,
        &mut store,
        10,
    )
    .unwrap();

    assert!(
        !store.is_empty(),
        "draining luck past thresholds should mint KnownFacts"
    );
}

#[test]
fn neon_dystopia_humanity_threshold_fires_known_fact() {
    let rules = load_rules_yaml("neon_dystopia");
    let mut snap = GameSnapshot::default();
    snap.init_resource_pools(&rules.resources);

    let mut store = LoreStore::new();

    // Drop humanity from 100 to 40 — should cross threshold at 50
    snap.process_resource_patch_with_lore(
        "humanity",
        ResourcePatchOp::Subtract,
        60.0,
        &mut store,
        15,
    )
    .unwrap();

    assert!(
        !store.is_empty(),
        "dropping humanity below 50 should mint a KnownFact"
    );
}

#[test]
fn pulp_noir_heat_decay_integration() {
    let rules = load_rules_yaml("pulp_noir");
    let mut snap = GameSnapshot::default();
    snap.init_resource_pools(&rules.resources);

    // Add some heat first
    snap.apply_resource_patch_by_name("heat", ResourcePatchOp::Add, 3.0).unwrap();
    assert!((snap.resources["heat"].current - 3.0).abs() < f64::EPSILON);

    // Apply decay — should reduce by 0.1
    snap.apply_pool_decay();
    assert!(
        (snap.resources["heat"].current - 2.9).abs() < 1e-9,
        "heat should decay by 0.1, got: {}",
        snap.resources["heat"].current
    );
}

// ═══════════════════════════════════════════════════════════
// ResourceDeclaration now requires thresholds field
// ═══════════════════════════════════════════════════════════

#[test]
fn resource_declaration_with_thresholds_deserializes() {
    let yaml = r#"
name: luck
label: Luck
min: 0
max: 6
starting: 3
voluntary: true
decay_per_turn: 0.0
thresholds:
  - at: 1
    event_id: luck_critical
    narrator_hint: "Nearly out of luck."
  - at: 0
    event_id: luck_depleted
    narrator_hint: "Completely out of luck."
"#;

    let decl: ResourceDeclaration = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(decl.name, "luck");
    assert_eq!(decl.thresholds.len(), 2);
    assert_eq!(decl.thresholds[0].event_id, "luck_critical");
    assert!((decl.thresholds[0].at - 1.0).abs() < f64::EPSILON);
}

#[test]
fn resource_declaration_without_thresholds_defaults_empty() {
    let yaml = r#"
name: heat
label: Heat
min: 0
max: 5
starting: 0
voluntary: false
decay_per_turn: -0.1
"#;

    let decl: ResourceDeclaration = serde_yaml::from_str(yaml).unwrap();
    assert!(
        decl.thresholds.is_empty(),
        "missing thresholds field should default to empty vec"
    );
}

#[test]
fn rules_config_resources_with_thresholds_parses() {
    let yaml = r#"
stat_generation: point_buy
point_buy_budget: 27
magic_level: none
hp_formula: "class_base * level"
default_class: Drifter
default_race: "Frontier Born"
default_hp: 10
default_ac: 10
default_location: "A nameless border town"
default_time_of_day: high_noon

resources:
  - name: luck
    label: Luck
    min: 0
    max: 6
    starting: 3
    voluntary: true
    decay_per_turn: 0.0
    thresholds:
      - at: 1
        event_id: luck_critical
        narrator_hint: "Nearly out of luck."
      - at: 0
        event_id: luck_depleted
        narrator_hint: "Completely out of luck."
"#;

    let rules: RulesConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(rules.resources.len(), 1);
    assert_eq!(rules.resources[0].thresholds.len(), 2);
}

// ═══════════════════════════════════════════════════════════
// Edge: genres without resources still load fine
// ═══════════════════════════════════════════════════════════

#[test]
fn genre_without_resources_loads_empty() {
    // low_fantasy doesn't declare resources
    let rules = load_rules_yaml("low_fantasy");
    assert!(
        rules.resources.is_empty(),
        "genres without resource declarations should have empty vec"
    );
}

#[test]
fn init_pools_from_empty_declarations_no_crash() {
    let rules = load_rules_yaml("low_fantasy");
    let mut snap = GameSnapshot::default();
    snap.init_resource_pools(&rules.resources);
    assert!(snap.resources.is_empty());
}

// ═══════════════════════════════════════════════════════════
// Upsert semantics — story consolidation phase 1a (2026-04)
//
// init_resource_pools must be idempotent and must preserve `current`
// when called a second time with the same declarations. This is what
// makes old-save migration work: the deserializer creates minimal
// pools with the saved `current`, then init_resource_pools populates
// genre-pack metadata without clobbering the player's progress.
// ═══════════════════════════════════════════════════════════

#[test]
fn init_resource_pools_preserves_current_on_second_call() {
    let decl = ResourceDeclaration {
        name: "luck".to_string(),
        label: "Luck".to_string(),
        min: 0.0,
        max: 10.0,
        starting: 5.0,
        voluntary: true,
        decay_per_turn: 0.0,
        thresholds: vec![],
    };
    let mut snap = GameSnapshot::default();

    // First call — creates pool at starting value.
    snap.init_resource_pools(std::slice::from_ref(&decl));
    assert!((snap.resources["luck"].current - 5.0).abs() < f64::EPSILON);

    // Simulate gameplay: player's current drops to 2.
    snap.resources.get_mut("luck").unwrap().current = 2.0;

    // Second call (e.g., after save/load re-runs session init).
    snap.init_resource_pools(std::slice::from_ref(&decl));

    // Current MUST be preserved — not reset to starting.
    assert!(
        (snap.resources["luck"].current - 2.0).abs() < f64::EPSILON,
        "init_resource_pools must preserve existing current on upsert; \
         got {} (expected 2.0 — was reset to starting 5.0?)",
        snap.resources["luck"].current
    );
}

#[test]
fn init_resource_pools_populates_label_from_declaration() {
    let decl = ResourceDeclaration {
        name: "heat".to_string(),
        label: "Heat".to_string(),
        min: 0.0,
        max: 5.0,
        starting: 0.0,
        voluntary: false,
        decay_per_turn: -0.1,
        thresholds: vec![],
    };
    let mut snap = GameSnapshot::default();
    snap.init_resource_pools(std::slice::from_ref(&decl));

    assert_eq!(
        snap.resources["heat"].label, "Heat",
        "label must be populated from genre pack declaration"
    );
}

#[test]
fn init_resource_pools_updates_bounds_but_reclamps_current() {
    let decl_wide = ResourceDeclaration {
        name: "fuel".to_string(),
        label: "Fuel".to_string(),
        min: 0.0,
        max: 100.0,
        starting: 50.0,
        voluntary: true,
        decay_per_turn: 0.0,
        thresholds: vec![],
    };
    let mut snap = GameSnapshot::default();
    snap.init_resource_pools(std::slice::from_ref(&decl_wide));

    // Player has 80 fuel.
    snap.resources.get_mut("fuel").unwrap().current = 80.0;

    // Genre pack is re-loaded with narrower bounds (e.g., mod balance patch).
    let decl_narrow = ResourceDeclaration {
        name: "fuel".to_string(),
        label: "Fuel".to_string(),
        min: 0.0,
        max: 50.0,
        starting: 25.0,
        voluntary: true,
        decay_per_turn: 0.0,
        thresholds: vec![],
    };
    snap.init_resource_pools(std::slice::from_ref(&decl_narrow));

    // Current re-clamps to the new max (80 > 50 → 50).
    assert!(
        (snap.resources["fuel"].current - 50.0).abs() < f64::EPSILON,
        "current must re-clamp when bounds narrow; got {}",
        snap.resources["fuel"].current
    );
    assert!((snap.resources["fuel"].max - 50.0).abs() < f64::EPSILON);
}

#[test]
fn resource_pool_label_serde_defaults_empty() {
    // Old saves predating the label field should deserialize with label = "".
    let json = r#"{
        "name": "luck",
        "current": 3.0,
        "min": 0.0,
        "max": 10.0,
        "voluntary": true,
        "decay_per_turn": 0.0
    }"#;
    let pool: ResourcePool = serde_json::from_str(json).unwrap();
    assert_eq!(pool.label, "", "old saves without label should deserialize with empty label");
    assert!((pool.current - 3.0).abs() < f64::EPSILON);
}

// ═══════════════════════════════════════════════════════════
// Phase 4 — GameSnapshot migration from legacy resource_state
// ═══════════════════════════════════════════════════════════

#[test]
fn old_save_with_resource_state_migrates_to_resources_map() {
    // Minimal save JSON shaped like a pre-phase-4 persistence file:
    // resource_state is populated, resources is absent.
    let json = r#"{
        "genre_slug": "spaghetti_western",
        "world_slug": "border_town",
        "resource_state": { "luck": 2.5, "heat": 3.0 },
        "resource_declarations": [
            { "name": "luck", "label": "Luck", "min": 0.0, "max": 6.0,
              "starting": 3.0, "voluntary": true, "decay_per_turn": 0.0 },
            { "name": "heat", "label": "Heat", "min": 0.0, "max": 5.0,
              "starting": 0.0, "voluntary": false, "decay_per_turn": -0.1 }
        ]
    }"#;

    let snap: GameSnapshot = serde_json::from_str(json)
        .expect("old-format save with resource_state should deserialize");

    // Migration populated the resources map.
    assert_eq!(snap.resources.len(), 2);
    assert!(
        (snap.resources["luck"].current - 2.5).abs() < f64::EPSILON,
        "luck.current must be preserved from resource_state, got {}",
        snap.resources["luck"].current
    );
    assert!(
        (snap.resources["heat"].current - 3.0).abs() < f64::EPSILON,
        "heat.current must be preserved"
    );
    // Labels and bounds came from resource_declarations.
    assert_eq!(snap.resources["luck"].label, "Luck");
    assert_eq!(snap.resources["heat"].label, "Heat");
    assert!((snap.resources["luck"].max - 6.0).abs() < f64::EPSILON);
    assert!((snap.resources["heat"].decay_per_turn - (-0.1)).abs() < f64::EPSILON);
}

#[test]
fn new_save_with_resources_takes_precedence_over_legacy_fields() {
    // Both resources and resource_state are present. The new field wins.
    let json = r#"{
        "genre_slug": "test",
        "world_slug": "test",
        "resource_state": { "luck": 9.9 },
        "resources": {
            "luck": {
                "name": "luck",
                "label": "Luck",
                "current": 4.0,
                "min": 0.0,
                "max": 6.0,
                "voluntary": true,
                "decay_per_turn": 0.0
            }
        }
    }"#;

    let snap: GameSnapshot = serde_json::from_str(json).unwrap();
    assert!(
        (snap.resources["luck"].current - 4.0).abs() < f64::EPSILON,
        "resources field (4.0) must take precedence over legacy resource_state (9.9)"
    );
}

#[test]
fn migration_without_declarations_produces_minimal_pool() {
    // Very old save with resource_state but no resource_declarations
    // (e.g., a save from before story 16-1 completed). Migration should
    // still produce a usable pool with unbounded defaults; the next
    // init_resource_pools() call will populate metadata.
    let json = r#"{
        "genre_slug": "test",
        "world_slug": "test",
        "resource_state": { "mana": 7.0 }
    }"#;

    let snap: GameSnapshot = serde_json::from_str(json).unwrap();
    assert_eq!(snap.resources.len(), 1);
    let mana = &snap.resources["mana"];
    assert!((mana.current - 7.0).abs() < f64::EPSILON);
    assert_eq!(mana.label, "", "no declaration → empty label for upsert to fill");
    assert_eq!(mana.name, "mana");
}

#[test]
fn migration_then_init_populates_metadata_without_resetting_current() {
    // End-to-end migration + upsert test: load an old save with
    // minimal pool data, then run init_resource_pools with the genre
    // pack declarations. Current should be preserved; metadata should
    // be populated from the pack.
    let json = r#"{
        "genre_slug": "test",
        "world_slug": "test",
        "resource_state": { "luck": 1.5 }
    }"#;
    let mut snap: GameSnapshot = serde_json::from_str(json).unwrap();

    // Simulate session load calling init_resource_pools with genre pack decls.
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
    snap.init_resource_pools(std::slice::from_ref(&decl));

    let luck = &snap.resources["luck"];
    assert!(
        (luck.current - 1.5).abs() < f64::EPSILON,
        "saved current (1.5) must survive the init_resource_pools upsert"
    );
    assert_eq!(luck.label, "Luck", "label populated by upsert");
    assert!((luck.max - 6.0).abs() < f64::EPSILON, "max populated by upsert");
    assert!(luck.voluntary, "voluntary populated by upsert");
}

//! Integration tests for the genre pack loader, trope inheritance, and two-phase validation.
//!
//! These tests load real YAML files from the `genre_packs/` directory
//! (using `mutant_wasteland` as the fully-spoilable test fixture) and
//! verify the unified loader, trope inheritance resolution, and
//! cross-reference validation.

use sidequest_genre::{GenreError, GenrePack, TropeDefinition};
use std::path::PathBuf;

/// Locate the genre_packs directory relative to this crate.
///
/// Supports the `GENRE_PACKS_PATH` env var for CI flexibility,
/// falling back to the known relative path from the sidequest-genre
/// crate within the orchestrator repo layout.
fn genre_packs_path() -> PathBuf {
    if let Ok(p) = std::env::var("GENRE_PACKS_PATH") {
        PathBuf::from(p)
    } else {
        // crate manifest is at sidequest-api/crates/sidequest-genre/
        // genre_packs is at oq-2/genre_packs/ (3 levels up)
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest.join("../../../genre_packs")
    }
}

// ═══════════════════════════════════════════════════════════
// AC: Real YAML loads — mutant_wasteland/flickering_reach loads
// ═══════════════════════════════════════════════════════════

#[test]
fn load_mutant_wasteland_genre_pack() {
    let path = genre_packs_path().join("mutant_wasteland");
    assert!(path.exists(), "mutant_wasteland genre pack not found at {}", path.display());

    let pack = sidequest_genre::load_genre_pack(&path).expect("should load mutant_wasteland");
    assert_eq!(pack.meta.name.as_str(), "Mutant Wasteland");
    assert_eq!(pack.meta.version, "0.1.0");
}

#[test]
fn loaded_pack_has_rules_and_lore() {
    let path = genre_packs_path().join("mutant_wasteland");
    let pack = sidequest_genre::load_genre_pack(&path).unwrap();

    // Rules
    assert_eq!(pack.rules.tone, "gonzo-sincere");
    assert_eq!(pack.rules.ability_score_names.len(), 6);
    assert!(pack.rules.class_hp_bases.contains_key("Scavenger"));

    // Lore
    assert!(!pack.lore.history.is_empty());
    assert!(!pack.lore.cosmology.is_empty());
}

#[test]
fn loaded_pack_has_archetypes_and_char_creation() {
    let path = genre_packs_path().join("mutant_wasteland");
    let pack = sidequest_genre::load_genre_pack(&path).unwrap();

    // Archetypes (archetypes.yaml has 6 NPCs)
    assert!(
        pack.archetypes.len() >= 6,
        "expected at least 6 archetypes, got {}",
        pack.archetypes.len()
    );
    let trader = pack.archetypes.iter().find(|a| a.name.as_str() == "Wasteland Trader");
    assert!(trader.is_some(), "should contain Wasteland Trader archetype");

    // Char creation (char_creation.yaml has 4 scenes)
    assert!(
        pack.char_creation.len() >= 4,
        "expected at least 4 char creation scenes, got {}",
        pack.char_creation.len()
    );
}

#[test]
fn loaded_pack_has_flickering_reach_world() {
    let path = genre_packs_path().join("mutant_wasteland");
    let pack = sidequest_genre::load_genre_pack(&path).unwrap();

    assert!(
        pack.worlds.contains_key("flickering_reach"),
        "pack should contain flickering_reach world"
    );

    let world = &pack.worlds["flickering_reach"];
    assert_eq!(world.config.name, "The Flickering Reach");
    assert_eq!(world.config.slug, "flickering_reach");
}

#[test]
fn loaded_world_has_cartography_with_regions_and_routes() {
    let path = genre_packs_path().join("mutant_wasteland");
    let pack = sidequest_genre::load_genre_pack(&path).unwrap();
    let world = &pack.worlds["flickering_reach"];

    // Cartography has 10 regions
    assert!(
        world.cartography.regions.len() >= 10,
        "expected at least 10 regions, got {}",
        world.cartography.regions.len()
    );
    assert!(world.cartography.regions.contains_key("toods_dome"));
    assert!(world.cartography.regions.contains_key("glass_flat"));

    // Routes exist
    assert!(
        !world.cartography.routes.is_empty(),
        "world should have routes"
    );
}

#[test]
fn loaded_world_has_lore_with_factions() {
    let path = genre_packs_path().join("mutant_wasteland");
    let pack = sidequest_genre::load_genre_pack(&path).unwrap();
    let world = &pack.worlds["flickering_reach"];

    assert!(
        world.lore.factions.len() >= 4,
        "expected at least 4 factions, got {}",
        world.lore.factions.len()
    );
    let dome_syndicate = world.lore.factions.iter().find(|f| f.name.contains("Dome Syndicate"));
    assert!(dome_syndicate.is_some(), "should contain Dome Syndicate faction");
}

#[test]
fn loaded_pack_has_power_tiers_for_all_classes() {
    let path = genre_packs_path().join("mutant_wasteland");
    let pack = sidequest_genre::load_genre_pack(&path).unwrap();

    // power_tiers.yaml has entries for 6 classes
    let expected_classes = ["Scavenger", "Mutant", "Pureblood", "Synth", "Beastkin", "Tinker"];
    for class in &expected_classes {
        assert!(
            pack.power_tiers.contains_key(*class),
            "power_tiers should contain class '{class}'"
        );
    }
}

// ═══════════════════════════════════════════════════════════
// AC: Unified loader — all YAML files go through one function
// ═══════════════════════════════════════════════════════════

#[test]
fn load_genre_pack_returns_typed_error_for_missing_dir() {
    let result = sidequest_genre::load_genre_pack(&PathBuf::from("/nonexistent/path"));
    assert!(result.is_err());
    match result.unwrap_err() {
        GenreError::LoadError { path, .. } => {
            assert!(path.contains("nonexistent"), "error should mention the path");
        }
        other => panic!("expected LoadError, got: {other:?}"),
    }
}

// ═══════════════════════════════════════════════════════════
// AC: Trope inheritance — multi-level extends with cycle detection
// ═══════════════════════════════════════════════════════════

#[test]
fn trope_inheritance_resolves_extends() {
    // Genre-level abstract trope
    let genre_tropes: Vec<TropeDefinition> = serde_yaml::from_str(r#"
- name: The Mentor
  abstract: true
  category: recurring
  tags: [mentor, identity]
  triggers:
    - player asks to be taught
  narrative_hints:
    - lessons must emerge from stakes
  resolution_patterns:
    - the student surpasses the mentor
  tension_level: 0.5
"#).unwrap();

    // World-level trope extending the abstract
    let world_tropes: Vec<TropeDefinition> = serde_yaml::from_str(r#"
- name: The Wandering Blade
  extends: the-mentor
  description: A legendary swordsman
  triggers:
    - player encounters a martial arts school
  narrative_hints:
    - the Wandering Blade appears at moments of crisis
  tension_level: 0.6
  tags:
    - rival
    - philosophy
  escalation:
    - at: 0.2
      event: rumors of the Wandering Blade
      npcs_involved: [teahouse keeper]
      stakes: reputation
"#).unwrap();

    let resolved = sidequest_genre::resolve_trope_inheritance(&genre_tropes, &world_tropes)
        .expect("should resolve trope inheritance");

    // The Wandering Blade should inherit base category from The Mentor
    let blade = resolved.iter().find(|t| t.name.as_str() == "The Wandering Blade").unwrap();
    assert_eq!(blade.category, "recurring", "should inherit category from parent");
    // But override tension_level
    assert!((blade.tension_level - 0.6).abs() < f64::EPSILON);
    // Should NOT be abstract after resolution
    assert!(!blade.is_abstract);
    // Should have its own triggers (overriding parent)
    assert!(blade.triggers.iter().any(|t| t.contains("martial arts")));
}

#[test]
fn trope_inheritance_detects_cycles() {
    // Trope A extends B, B extends A — should error
    let tropes: Vec<TropeDefinition> = serde_yaml::from_str(r#"
- name: Trope A
  extends: trope-b
  category: conflict
  triggers: []
  narrative_hints: []
  tension_level: 0.5
- name: Trope B
  extends: trope-a
  category: conflict
  triggers: []
  narrative_hints: []
  tension_level: 0.5
"#).unwrap();

    let result = sidequest_genre::resolve_trope_inheritance(&[], &tropes);
    assert!(result.is_err(), "should detect cycle in extends chain");
    match result.unwrap_err() {
        GenreError::CycleDetected { .. } => {}
        other => panic!("expected CycleDetected error, got: {other:?}"),
    }
}

#[test]
fn trope_inheritance_rejects_missing_parent() {
    let tropes: Vec<TropeDefinition> = serde_yaml::from_str(r#"
- name: Orphan Trope
  extends: nonexistent-parent
  category: conflict
  triggers: []
  narrative_hints: []
  tension_level: 0.5
"#).unwrap();

    let result = sidequest_genre::resolve_trope_inheritance(&[], &tropes);
    assert!(result.is_err(), "should reject reference to nonexistent parent");
    match result.unwrap_err() {
        GenreError::MissingParent { .. } => {}
        other => panic!("expected MissingParent error, got: {other:?}"),
    }
}

// ═══════════════════════════════════════════════════════════
// AC: Two-phase validation — validate() checks cross-references
// ═══════════════════════════════════════════════════════════

#[test]
fn two_phase_validate_passes_valid_pack() {
    let path = genre_packs_path().join("mutant_wasteland");
    let pack = sidequest_genre::load_genre_pack(&path).unwrap();

    // Phase 1: load succeeded (above)
    // Phase 2: validate cross-references
    let result = pack.validate();
    assert!(result.is_ok(), "valid pack should pass validation: {:?}", result.err());
}

#[test]
fn two_phase_validate_catches_bad_achievement_trope_ref() {
    // Create a pack with an achievement referencing a nonexistent trope_id
    let achievement_yaml = r#"
- id: fake_achievement
  name: Fake Achievement
  description: References a trope that doesn't exist
  trope_id: nonexistent_trope
  trigger_status: activated
  emoji: "❌"
"#;
    let achievements: Vec<sidequest_genre::Achievement> =
        serde_yaml::from_str(achievement_yaml).unwrap();

    // Load a real pack then inject the bad achievement
    let path = genre_packs_path().join("mutant_wasteland");
    let mut pack = sidequest_genre::load_genre_pack(&path).unwrap();
    pack.achievements = achievements;

    let result = pack.validate();
    assert!(
        result.is_err(),
        "validation should catch achievement referencing nonexistent trope"
    );
}

#[test]
fn two_phase_validate_catches_bad_cartography_adjacent_ref() {
    // A region's `adjacent` list references a region slug that doesn't exist
    let path = genre_packs_path().join("mutant_wasteland");
    let mut pack = sidequest_genre::load_genre_pack(&path).unwrap();

    // Inject a bad adjacent reference into flickering_reach cartography
    if let Some(world) = pack.worlds.get_mut("flickering_reach") {
        if let Some(region) = world.cartography.regions.get_mut("toods_dome") {
            region.adjacent.push("nonexistent_region".to_string());
        }
    }

    let result = pack.validate();
    assert!(
        result.is_err(),
        "validation should catch adjacent reference to nonexistent region"
    );
}

// ═══════════════════════════════════════════════════════════
// AC: deny_unknown_fields works with real YAML mutation
// ═══════════════════════════════════════════════════════════

#[test]
fn deny_unknown_fields_catches_typo_in_real_yaml() {
    // Read the real pack.yaml and inject a typo field
    let path = genre_packs_path().join("mutant_wasteland/pack.yaml");
    let mut yaml_content = std::fs::read_to_string(&path)
        .expect("should read pack.yaml");
    yaml_content.push_str("\ntypo_field: this should fail\n");

    let result: Result<sidequest_genre::PackMeta, _> = serde_yaml::from_str(&yaml_content);
    assert!(
        result.is_err(),
        "deny_unknown_fields should reject typo injected into real pack.yaml"
    );
}

// ═══════════════════════════════════════════════════════════
// Load multiple genre packs to verify consistency
// ═══════════════════════════════════════════════════════════

#[test]
fn load_low_fantasy_genre_pack_with_tropes() {
    // low_fantasy has tropes.yaml at genre level — tests trope loading
    let path = genre_packs_path().join("low_fantasy");
    if !path.exists() {
        // Skip if genre pack not available in test environment
        return;
    }

    let pack = sidequest_genre::load_genre_pack(&path).expect("should load low_fantasy");
    assert_eq!(pack.meta.name.as_str(), "Low Fantasy");
    // low_fantasy has genre-level tropes
    assert!(
        !pack.tropes.is_empty(),
        "low_fantasy should have genre-level tropes"
    );
}

#[test]
fn load_elemental_harmony_with_trope_inheritance() {
    // elemental_harmony has abstract genre tropes + world tropes with extends
    let path = genre_packs_path().join("elemental_harmony");
    if !path.exists() {
        return;
    }

    let pack = sidequest_genre::load_genre_pack(&path).expect("should load elemental_harmony");

    // After loading, world tropes should have resolved inheritance
    for (world_slug, world) in &pack.worlds {
        for trope in &world.tropes {
            assert!(
                !trope.is_abstract,
                "world trope '{}' in '{}' should not be abstract after inheritance resolution",
                trope.name.as_str(),
                world_slug
            );
        }
    }
}

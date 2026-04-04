//! RED-phase tests for Story 23-2: Tiered lore summaries.
//!
//! These tests verify that faction, culture, and region (location) types
//! support a required `summary` field — a one-line (~10 token) descriptor
//! used as a safety net for the narrator prompt RAG pipeline.
//!
//! Test strategy:
//! - AC1: YAML deserialization with summary field
//! - AC2: Accessor method returns correct value
//! - AC3: Missing summary causes deserialization error (required, not optional)
//! - Integration: low_fantasy genre pack loads with summaries present

use sidequest_genre::{CartographyConfig, Culture, Faction, WorldLore};
use std::path::PathBuf;

/// Locate the genre_packs directory relative to this crate.
fn genre_packs_path() -> PathBuf {
    if let Ok(p) = std::env::var("GENRE_PACKS_PATH") {
        PathBuf::from(p)
    } else {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest.join("../../../sidequest-content/genre_packs")
    }
}

// ═══════════════════════════════════════════════════════════
// Faction summary tests
// ═══════════════════════════════════════════════════════════

#[test]
fn faction_deserializes_with_summary() {
    let yaml = r#"
name: The Crown Remnant
summary: "Descendants of the fallen kingdom seeking to restore order"
description: "Loyalists who still claim descent from the Aldric bloodline."
disposition: neutral
"#;
    let faction: Faction = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(faction.summary, "Descendants of the fallen kingdom seeking to restore order");
}

#[test]
fn faction_summary_accessor() {
    let yaml = r#"
name: The Merchant Consortium
summary: "Pragmatic traders united by profit"
description: "A coalition of wealthy trading families."
disposition: wary
"#;
    let faction: Faction = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(faction.summary(), "Pragmatic traders united by profit");
}

#[test]
fn faction_summary_not_in_extras() {
    // Summary must be a first-class field, not captured by serde(flatten) extras
    let yaml = r#"
name: Test Faction
summary: "Test summary"
description: "Test description"
"#;
    let faction: Faction = serde_yaml::from_str(yaml).unwrap();
    assert!(
        !faction.extras.contains_key("summary"),
        "summary should be a struct field, not in extras"
    );
}

#[test]
fn faction_missing_summary_errors() {
    // Summary is required — omitting it must cause a deserialization error
    let yaml = r#"
name: No Summary Faction
description: "This faction has no summary."
disposition: neutral
"#;
    let result = serde_yaml::from_str::<Faction>(yaml);
    assert!(
        result.is_err(),
        "Faction without summary should fail deserialization"
    );
}

// ═══════════════════════════════════════════════════════════
// Culture summary tests
// ═══════════════════════════════════════════════════════════

#[test]
fn culture_deserializes_with_summary() {
    let yaml = r#"
name: Brevonne
summary: "Franco-Celtic river kingdom culture"
description: "Smooth, elegant names inspired by French and Celtic traditions"
slots:
  given_name:
    corpora:
      - corpus: french.txt
        weight: 1.0
    lookback: 3
person_patterns:
  - "{given_name}"
place_patterns:
  - "{given_name}'s Keep"
"#;
    let culture: Culture = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(culture.summary, "Franco-Celtic river kingdom culture");
}

#[test]
fn culture_summary_accessor() {
    let yaml = r#"
name: Nordmark
summary: "Nordic highland clan culture"
description: "Harder consonants and patronymics"
slots:
  given_name:
    corpora:
      - corpus: norse.txt
        weight: 1.0
    lookback: 3
person_patterns:
  - "{given_name}"
place_patterns:
  - "{given_name}heim"
"#;
    let culture: Culture = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(culture.summary(), "Nordic highland clan culture");
}

#[test]
fn culture_missing_summary_errors() {
    let yaml = r#"
name: No Summary Culture
description: "This culture has no summary."
slots: {}
person_patterns: []
place_patterns: []
"#;
    let result = serde_yaml::from_str::<Culture>(yaml);
    assert!(
        result.is_err(),
        "Culture without summary should fail deserialization"
    );
}

// ═══════════════════════════════════════════════════════════
// Region (location) summary tests
// ═══════════════════════════════════════════════════════════

#[test]
fn region_deserializes_with_summary() {
    // In the codebase, "locations" are regions in CartographyConfig
    let yaml = r#"
world_name: Test World
starting_region: heartland
regions:
  heartland:
    name: The Heartland
    summary: "Fertile central plains and trade crossroads"
    description: "Fertile central plains dotted with farming villages."
"#;
    let config: CartographyConfig = serde_yaml::from_str(yaml).unwrap();
    let region = config.regions.get("heartland").expect("heartland should exist");
    assert_eq!(region.summary, "Fertile central plains and trade crossroads");
}

#[test]
fn region_summary_accessor() {
    let yaml = r#"
world_name: Test World
starting_region: test
regions:
  test:
    name: Test Region
    summary: "A test region for summary verification"
    description: "Test description."
"#;
    let config: CartographyConfig = serde_yaml::from_str(yaml).unwrap();
    let region = config.regions.get("test").unwrap();
    assert_eq!(region.summary(), "A test region for summary verification");
}

#[test]
fn region_summary_not_in_extras() {
    let yaml = r#"
world_name: Test World
starting_region: test
regions:
  test:
    name: Test Region
    summary: "Test summary"
    description: "Test description."
"#;
    let config: CartographyConfig = serde_yaml::from_str(yaml).unwrap();
    let region = config.regions.get("test").unwrap();
    assert!(
        !region.extras.contains_key("summary"),
        "summary should be a struct field, not in extras"
    );
}

#[test]
fn region_missing_summary_errors() {
    // Region summary is required — omitting it must fail
    let yaml = r#"
world_name: Test World
starting_region: test
regions:
  test:
    name: Test Region
    description: "No summary here."
"#;
    let result = serde_yaml::from_str::<CartographyConfig>(yaml);
    assert!(
        result.is_err(),
        "Region without summary should fail deserialization"
    );
}

// ═══════════════════════════════════════════════════════════
// WorldLore faction summary tests
// ═══════════════════════════════════════════════════════════

#[test]
fn world_lore_faction_has_summary() {
    // WorldLore also contains Vec<Faction> — summaries must work there too
    let yaml = r#"
world_name: Test World
factions:
  - name: Test Faction
    summary: "A test faction for world lore"
    description: "Test description."
    disposition: neutral
"#;
    let lore: WorldLore = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(lore.factions.len(), 1);
    assert_eq!(lore.factions[0].summary(), "A test faction for world lore");
}

// ═══════════════════════════════════════════════════════════
// Integration: low_fantasy genre pack loads with summaries
// ═══════════════════════════════════════════════════════════

#[test]
fn low_fantasy_loads_with_summaries() {
    let path = genre_packs_path().join("low_fantasy");
    assert!(
        path.exists(),
        "low_fantasy genre pack not found at {}",
        path.display()
    );

    let pack = sidequest_genre::load_genre_pack(&path).expect("should load low_fantasy with summaries");

    // Verify genre-level factions have summaries
    assert!(
        !pack.lore.factions.is_empty(),
        "low_fantasy should have genre-level factions"
    );
    for faction in &pack.lore.factions {
        assert!(
            !faction.summary().is_empty(),
            "Faction '{}' should have a non-empty summary",
            faction.name
        );
    }

    // Verify cultures have summaries
    assert!(
        !pack.cultures.is_empty(),
        "low_fantasy should have cultures"
    );
    for culture in &pack.cultures {
        assert!(
            !culture.summary().is_empty(),
            "Culture '{}' should have a non-empty summary",
            culture.name
        );
    }
}

#[test]
fn low_fantasy_worlds_have_region_summaries() {
    let path = genre_packs_path().join("low_fantasy");
    let pack = sidequest_genre::load_genre_pack(&path).expect("should load low_fantasy");

    // Cartography lives on World, not GenrePack
    // Some worlds use hierarchical navigation (world_graph) instead of regions
    let mut checked_any = false;
    for (world_slug, world) in &pack.worlds {
        if world.cartography.regions.is_empty() {
            continue; // hierarchical/room_graph worlds have no regions
        }
        checked_any = true;
        for (slug, region) in &world.cartography.regions {
            assert!(
                !region.summary().is_empty(),
                "Region '{}' in world '{}' should have a non-empty summary",
                slug,
                world_slug
            );
        }
    }
    assert!(checked_any, "At least one low_fantasy world should have regions");
}

#[test]
fn low_fantasy_world_lore_factions_have_summaries() {
    let path = genre_packs_path().join("low_fantasy");
    let pack = sidequest_genre::load_genre_pack(&path).expect("should load low_fantasy");

    // World-level lore also contains factions
    for (world_slug, world) in &pack.worlds {
        for faction in &world.lore.factions {
            assert!(
                !faction.summary().is_empty(),
                "World faction '{}' in '{}' should have a non-empty summary",
                faction.name,
                world_slug
            );
        }
    }
}

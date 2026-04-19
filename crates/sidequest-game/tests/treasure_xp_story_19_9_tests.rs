//! RED-phase tests for Story 19-9: Treasure-as-XP — gold extraction grants affinity progress.
//!
//! Tests exercise:
//! - Gold increase on surface location triggers affinity progress
//! - xp_affinity field in genre rules configures target affinity
//! - No effect when gold changes inside dungeon (room_graph non-entrance rooms)
//! - 100 GP extracted to surface = 100 progress on configured affinity
//! - OTEL event metadata (treasure.extracted)
//! - Edge cases: gold decrease, missing xp_affinity config, zero delta

use std::collections::HashMap;

use sidequest_game::affinity::AffinityState;
use sidequest_game::character::Character;
use sidequest_game::creature_core::CreatureCore;
use sidequest_game::inventory::Inventory;
use sidequest_game::state::GameSnapshot;
use sidequest_game::treasure_xp::{apply_treasure_xp, TreasureXpConfig, TreasureXpResult};
use sidequest_genre::{RoomDef, RoomExit};
use sidequest_protocol::NonBlankString;

// ═══════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════

/// Build a minimal dungeon: entrance → corridor → treasure_room.
fn sample_dungeon() -> Vec<RoomDef> {
    vec![
        RoomDef {
            id: "entrance".into(),
            name: "Dungeon Entrance".into(),
            room_type: "entrance".into(),
            size: (3, 3),
            keeper_awareness_modifier: 1.0,
            exits: vec![RoomExit::Corridor {
                target: "corridor".into(),
            }],
            description: None,
            grid: None,
            legend: None,
            tactical_scale: None,
        },
        RoomDef {
            id: "corridor".into(),
            name: "Dark Corridor".into(),
            room_type: "normal".into(),
            size: (2, 4),
            keeper_awareness_modifier: 1.0,
            exits: vec![
                RoomExit::Corridor {
                    target: "entrance".into(),
                },
                RoomExit::Door {
                    target: "treasure_room".into(),
                    is_locked: false,
                },
            ],
            description: None,
            grid: None,
            legend: None,
            tactical_scale: None,
        },
        RoomDef {
            id: "treasure_room".into(),
            name: "Treasure Vault".into(),
            room_type: "normal".into(),
            size: (4, 4),
            keeper_awareness_modifier: 1.5,
            exits: vec![RoomExit::Door {
                target: "corridor".into(),
                is_locked: false,
            }],
            description: None,
            grid: None,
            legend: None,
            tactical_scale: None,
        },
    ]
}

fn config_with_affinity(affinity: &str) -> TreasureXpConfig {
    TreasureXpConfig {
        xp_affinity: Some(affinity.to_string()),
    }
}

fn config_without_affinity() -> TreasureXpConfig {
    TreasureXpConfig { xp_affinity: None }
}

fn snapshot_at_location(location: &str) -> GameSnapshot {
    GameSnapshot {
        location: location.to_string(),
        ..Default::default()
    }
}

/// Build a minimal valid Character for testing.
fn test_character_with_affinities(affinities: Vec<AffinityState>) -> Character {
    Character {
        core: CreatureCore {
            name: NonBlankString::new("Thorn Ironhide").unwrap(),
            description: NonBlankString::new("A scarred dwarf warrior").unwrap(),
            personality: NonBlankString::new("Gruff but loyal").unwrap(),
            level: 3,
            edge: sidequest_game::creature_core::placeholder_edge_pool(),
            acquired_advancements: vec![],
            xp: 0,
            inventory: Inventory::default(),
            statuses: vec![],
        },
        backstory: NonBlankString::new("Raised in the iron mines").unwrap(),
        narrative_state: "Exploring the dungeon".to_string(),
        hooks: vec![],
        char_class: NonBlankString::new("Fighter").unwrap(),
        race: NonBlankString::new("Dwarf").unwrap(),
        pronouns: "he/him".to_string(),
        stats: HashMap::new(),
        abilities: vec![],
        known_facts: vec![],
        affinities,
        is_friendly: true,
        resolved_archetype: None,
        archetype_provenance: None,
    }
}

// ═══════════════════════════════════════════════════════════
// AC-1: Gold increase on surface triggers affinity progress
// ═══════════════════════════════════════════════════════════

#[test]
fn gold_increase_at_entrance_room_grants_affinity_progress() {
    let rooms = sample_dungeon();
    let mut snap = snapshot_at_location("entrance");
    snap.characters
        .push(test_character_with_affinities(vec![AffinityState::new(
            "Plunderer",
        )]));
    let config = config_with_affinity("Plunderer");

    let result = apply_treasure_xp(&mut snap, 50, &config, Some(&rooms));

    assert!(
        result.applied,
        "Expected affinity progress to be applied at entrance room"
    );
    assert_eq!(result.gold_amount, 50);
    assert_eq!(result.affinity_name.as_deref(), Some("Plunderer"));
}

#[test]
fn gold_increase_in_region_mode_grants_affinity_progress() {
    // Region mode = no room graph = always surface
    let mut snap = snapshot_at_location("Town of Millhaven");
    snap.characters
        .push(test_character_with_affinities(vec![AffinityState::new(
            "Plunderer",
        )]));
    let config = config_with_affinity("Plunderer");

    let result = apply_treasure_xp(&mut snap, 100, &config, None);

    assert!(result.applied, "Region mode should always count as surface");
    assert_eq!(result.gold_amount, 100);
    assert_eq!(result.affinity_name.as_deref(), Some("Plunderer"));
}

// ═══════════════════════════════════════════════════════════
// AC-2: xp_affinity configures target affinity
// ═══════════════════════════════════════════════════════════

#[test]
fn xp_affinity_name_drives_which_affinity_receives_progress() {
    let mut snap = snapshot_at_location("Town Square");
    snap.characters.push(test_character_with_affinities(vec![]));
    let config = config_with_affinity("Treasure Hunter");

    let result = apply_treasure_xp(&mut snap, 25, &config, None);

    assert!(result.applied);
    assert_eq!(
        result.affinity_name.as_deref(),
        Some("Treasure Hunter"),
        "The affinity receiving progress must match xp_affinity config"
    );
}

#[test]
fn missing_xp_affinity_config_means_no_effect() {
    let mut snap = snapshot_at_location("Town Square");
    snap.characters.push(test_character_with_affinities(vec![]));
    let config = config_without_affinity();

    let result = apply_treasure_xp(&mut snap, 100, &config, None);

    assert!(
        !result.applied,
        "No xp_affinity configured = no treasure XP"
    );
    assert_eq!(result.gold_amount, 0);
    assert!(result.affinity_name.is_none());
}

// ═══════════════════════════════════════════════════════════
// AC-3: No effect inside dungeon
// ═══════════════════════════════════════════════════════════

#[test]
fn gold_increase_in_dungeon_corridor_no_effect() {
    let rooms = sample_dungeon();
    let mut snap = snapshot_at_location("corridor");
    snap.characters
        .push(test_character_with_affinities(vec![AffinityState::new(
            "Plunderer",
        )]));
    let config = config_with_affinity("Plunderer");

    let result = apply_treasure_xp(&mut snap, 200, &config, Some(&rooms));

    assert!(
        !result.applied,
        "Should not grant XP inside dungeon corridor"
    );
    assert_eq!(result.gold_amount, 0);
}

#[test]
fn gold_increase_in_treasure_room_no_effect() {
    let rooms = sample_dungeon();
    let mut snap = snapshot_at_location("treasure_room");
    snap.characters
        .push(test_character_with_affinities(vec![AffinityState::new(
            "Plunderer",
        )]));
    let config = config_with_affinity("Plunderer");

    let result = apply_treasure_xp(&mut snap, 500, &config, Some(&rooms));

    assert!(!result.applied, "Should not grant XP inside treasure room");
}

// ═══════════════════════════════════════════════════════════
// AC-4: 100 GP → 100 progress (1:1 mapping)
// ═══════════════════════════════════════════════════════════

#[test]
fn hundred_gp_grants_hundred_progress() {
    let mut snap = snapshot_at_location("Town of Millhaven");
    snap.characters
        .push(test_character_with_affinities(vec![AffinityState::new(
            "Plunderer",
        )]));
    let config = config_with_affinity("Plunderer");

    let result = apply_treasure_xp(&mut snap, 100, &config, None);

    assert!(result.applied);
    assert_eq!(result.gold_amount, 100);
    assert_eq!(result.new_progress, Some(100));

    // Verify the actual affinity state was mutated
    let aff = snap.characters[0]
        .affinities
        .iter()
        .find(|a| a.name == "Plunderer")
        .expect("Plunderer affinity should exist");
    assert_eq!(aff.progress, 100, "1:1 gold-to-progress mapping");
}

#[test]
fn cumulative_gold_extractions_accumulate_progress() {
    let mut snap = snapshot_at_location("Town");
    snap.characters
        .push(test_character_with_affinities(vec![AffinityState::new(
            "Plunderer",
        )]));
    let config = config_with_affinity("Plunderer");

    // First extraction: 50 GP
    apply_treasure_xp(&mut snap, 50, &config, None);
    // Second extraction: 75 GP
    apply_treasure_xp(&mut snap, 75, &config, None);

    let aff = snap.characters[0]
        .affinities
        .iter()
        .find(|a| a.name == "Plunderer")
        .expect("Plunderer affinity should exist");
    assert_eq!(aff.progress, 125, "50 + 75 = 125 cumulative progress");
}

// ═══════════════════════════════════════════════════════════
// Edge cases
// ═══════════════════════════════════════════════════════════

#[test]
fn zero_gold_delta_no_effect() {
    let mut snap = snapshot_at_location("Town");
    snap.characters.push(test_character_with_affinities(vec![]));
    let config = config_with_affinity("Plunderer");

    let result = apply_treasure_xp(&mut snap, 0, &config, None);

    assert!(!result.applied, "Zero gold delta should not trigger XP");
}

#[test]
fn affinity_created_if_absent_on_character() {
    let mut snap = snapshot_at_location("Town");
    // Character has no affinities at all
    snap.characters.push(test_character_with_affinities(vec![]));
    let config = config_with_affinity("Plunderer");

    let result = apply_treasure_xp(&mut snap, 30, &config, None);

    assert!(result.applied);
    let aff = snap.characters[0]
        .affinities
        .iter()
        .find(|a| a.name == "Plunderer")
        .expect("Plunderer affinity should be auto-created");
    assert_eq!(aff.progress, 30);
    assert_eq!(aff.tier, 0);
}

#[test]
fn no_characters_in_snapshot_no_panic() {
    let mut snap = snapshot_at_location("Town");
    // Empty characters vec
    let config = config_with_affinity("Plunderer");

    let result = apply_treasure_xp(&mut snap, 100, &config, None);

    // Should not panic, but should not apply either (no character to advance)
    assert!(!result.applied, "No characters = nothing to advance");
}

// ═══════════════════════════════════════════════════════════
// OTEL event metadata
// ═══════════════════════════════════════════════════════════

#[test]
fn result_carries_otel_metadata() {
    let mut snap = snapshot_at_location("Town");
    snap.characters
        .push(test_character_with_affinities(vec![AffinityState::new(
            "Plunderer",
        )]));
    let config = config_with_affinity("Plunderer");

    let result = apply_treasure_xp(&mut snap, 42, &config, None);

    // Result should carry enough data for the server to emit an OTEL event
    assert!(result.applied);
    assert_eq!(result.gold_amount, 42);
    assert_eq!(result.affinity_name.as_deref(), Some("Plunderer"));
    assert_eq!(result.new_progress, Some(42));
}

// ═══════════════════════════════════════════════════════════
// Surface detection: entrance room counts as surface
// ═══════════════════════════════════════════════════════════

#[test]
fn entrance_room_type_is_surface() {
    let rooms = sample_dungeon();
    let mut snap = snapshot_at_location("entrance");
    snap.characters
        .push(test_character_with_affinities(vec![AffinityState::new(
            "Plunderer",
        )]));
    let config = config_with_affinity("Plunderer");

    let result = apply_treasure_xp(&mut snap, 100, &config, Some(&rooms));

    assert!(result.applied, "Entrance room (surface) should grant XP");
    assert_eq!(result.new_progress, Some(100));
}

#[test]
fn location_not_in_room_graph_is_surface() {
    // Player location doesn't match any room ID → treated as outside the graph
    let rooms = sample_dungeon();
    let mut snap = snapshot_at_location("Town of Millhaven");
    snap.characters.push(test_character_with_affinities(vec![]));
    let config = config_with_affinity("Plunderer");

    let result = apply_treasure_xp(&mut snap, 50, &config, Some(&rooms));

    assert!(result.applied, "Location outside room graph = surface");
}

// ═══════════════════════════════════════════════════════════
// Wiring test: module is reachable from lib.rs
// ═══════════════════════════════════════════════════════════

#[test]
fn treasure_xp_module_is_exported() {
    // This test verifies the module is wired into the public API of sidequest-game.
    // If this fails to compile, the module isn't registered in lib.rs.
    let _config = TreasureXpConfig { xp_affinity: None };
    let _result = TreasureXpResult {
        applied: false,
        gold_amount: 0,
        affinity_name: None,
        new_progress: None,
    };
}

// ═══════════════════════════════════════════════════════════
// RulesConfig integration: xp_affinity field
// ═══════════════════════════════════════════════════════════

#[test]
fn rules_config_xp_affinity_serde_roundtrip() {
    // xp_affinity must exist on RulesConfig and survive serialization
    use sidequest_genre::RulesConfig;

    let yaml = r#"
tone: "gonzo-sincere"
lethality: "high"
xp_affinity: "Plunderer"
"#;
    let rules: RulesConfig = serde_yaml::from_str(yaml).expect("should parse xp_affinity field");
    assert_eq!(
        rules.xp_affinity.as_deref(),
        Some("Plunderer"),
        "xp_affinity field must be accessible on RulesConfig"
    );

    // Round-trip through JSON
    let json = serde_json::to_string(&rules).unwrap();
    let back: RulesConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(back.xp_affinity.as_deref(), Some("Plunderer"));
}

#[test]
fn rules_config_xp_affinity_defaults_to_none() {
    use sidequest_genre::RulesConfig;

    // YAML with no xp_affinity field — should default to None, not panic
    let yaml = r#"
tone: "dark"
lethality: "low"
"#;
    let rules: RulesConfig = serde_yaml::from_str(yaml).expect("should parse without xp_affinity");
    assert!(
        rules.xp_affinity.is_none(),
        "Missing xp_affinity should default to None"
    );
}

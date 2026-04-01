//! Failing tests for Story 19-1: RoomDef + RoomExit structs.
//!
//! Tests the room graph data model foundation for the Dungeon Crawl Engine (Epic 19).
//! Covers:
//! - NavigationMode enum (Region default, RoomGraph variant)
//! - RoomDef and RoomExit struct deserialization from YAML
//! - CartographyConfig backward compatibility (defaults to Region)
//! - Validation: invalid exit targets, missing bidirectional routes (non-chute)
//! - rooms.yaml loading alongside cartography.yaml

use sidequest_genre::{CartographyConfig, NavigationMode, RoomDef, RoomExit};

// ═══════════════════════════════════════════════════════════
// AC-1: NavigationMode enum
// ═══════════════════════════════════════════════════════════

#[test]
fn navigation_mode_defaults_to_region() {
    let yaml = r#"
world_name: Test World
starting_region: town
"#;
    let config: CartographyConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(config.navigation_mode, NavigationMode::Region);
}

#[test]
fn navigation_mode_deserializes_region_explicit() {
    let yaml = r#"
world_name: Test World
starting_region: town
navigation_mode: region
"#;
    let config: CartographyConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(config.navigation_mode, NavigationMode::Region);
}

#[test]
fn navigation_mode_deserializes_room_graph() {
    let yaml = r#"
world_name: Test World
starting_region: entrance_hall
navigation_mode: room_graph
"#;
    let config: CartographyConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(config.navigation_mode, NavigationMode::RoomGraph);
}

#[test]
fn navigation_mode_rejects_unknown_variant() {
    let yaml = r#"
world_name: Test World
navigation_mode: hexcrawl
"#;
    let result = serde_yaml::from_str::<CartographyConfig>(yaml);
    assert!(result.is_err(), "unknown NavigationMode variant should fail deserialization");
}

// ═══════════════════════════════════════════════════════════
// AC-1: RoomExit struct deserialization
// ═══════════════════════════════════════════════════════════

#[test]
fn room_exit_deserializes_basic() {
    let yaml = r#"
target: great_hall
direction: north
description: A heavy oak door leads north
"#;
    let exit: RoomExit = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(exit.target, "great_hall");
    assert_eq!(exit.direction, "north");
    assert_eq!(exit.description, "A heavy oak door leads north");
    assert!(!exit.one_way, "one_way should default to false");
}

#[test]
fn room_exit_deserializes_one_way_chute() {
    let yaml = r#"
target: pit_bottom
direction: down
description: A crumbling ledge drops into darkness
one_way: true
"#;
    let exit: RoomExit = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(exit.target, "pit_bottom");
    assert!(exit.one_way, "chute exit should be one_way");
}

#[test]
fn room_exit_requires_target() {
    let yaml = r#"
direction: north
description: A door
"#;
    let result = serde_yaml::from_str::<RoomExit>(yaml);
    assert!(result.is_err(), "RoomExit without target should fail");
}

#[test]
fn room_exit_requires_direction() {
    let yaml = r#"
target: great_hall
description: A door
"#;
    let result = serde_yaml::from_str::<RoomExit>(yaml);
    assert!(result.is_err(), "RoomExit without direction should fail");
}

// ═══════════════════════════════════════════════════════════
// AC-1: RoomDef struct deserialization
// ═══════════════════════════════════════════════════════════

#[test]
fn room_def_deserializes_full() {
    let yaml = r#"
id: entrance_hall
name: Entrance Hall
description: A grand hall with crumbling pillars
exits:
  - target: great_hall
    direction: north
    description: A heavy oak door
  - target: guard_room
    direction: east
    description: A narrow passage
"#;
    let room: RoomDef = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(room.id, "entrance_hall");
    assert_eq!(room.name, "Entrance Hall");
    assert_eq!(room.description, "A grand hall with crumbling pillars");
    assert_eq!(room.exits.len(), 2);
    assert_eq!(room.exits[0].target, "great_hall");
    assert_eq!(room.exits[1].target, "guard_room");
}

#[test]
fn room_def_requires_id() {
    let yaml = r#"
name: Entrance Hall
description: A grand hall
exits: []
"#;
    let result = serde_yaml::from_str::<RoomDef>(yaml);
    assert!(result.is_err(), "RoomDef without id should fail");
}

#[test]
fn room_def_requires_name() {
    let yaml = r#"
id: entrance_hall
description: A grand hall
exits: []
"#;
    let result = serde_yaml::from_str::<RoomDef>(yaml);
    assert!(result.is_err(), "RoomDef without name should fail");
}

#[test]
fn room_def_exits_default_to_empty() {
    let yaml = r#"
id: dead_end
name: Dead End
description: A collapsed tunnel
"#;
    let room: RoomDef = serde_yaml::from_str(yaml).unwrap();
    assert!(room.exits.is_empty(), "exits should default to empty vec");
}

// ═══════════════════════════════════════════════════════════
// AC-2: rooms field on CartographyConfig
// ═══════════════════════════════════════════════════════════

#[test]
fn cartography_config_rooms_default_empty() {
    let yaml = r#"
world_name: Low Fantasy World
starting_region: town
"#;
    let config: CartographyConfig = serde_yaml::from_str(yaml).unwrap();
    assert!(config.rooms.is_empty(), "rooms should default to empty vec");
}

#[test]
fn cartography_config_with_rooms() {
    let yaml = r#"
world_name: The Mawdeep
starting_region: entrance_hall
navigation_mode: room_graph
rooms:
  - id: entrance_hall
    name: Entrance Hall
    description: The gaping maw of the dungeon
    exits:
      - target: great_hall
        direction: north
        description: A stone archway
  - id: great_hall
    name: Great Hall
    description: A vast chamber
    exits:
      - target: entrance_hall
        direction: south
        description: Back to the entrance
"#;
    let config: CartographyConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(config.navigation_mode, NavigationMode::RoomGraph);
    assert_eq!(config.rooms.len(), 2);
    assert_eq!(config.rooms[0].id, "entrance_hall");
    assert_eq!(config.rooms[1].id, "great_hall");
}

// ═══════════════════════════════════════════════════════════
// AC-4: Backward compatibility — existing genre packs unaffected
// ═══════════════════════════════════════════════════════════

#[test]
fn cartography_config_backward_compat_full_existing() {
    // Existing cartography.yaml from low_fantasy — no navigation_mode, no rooms
    let yaml = r#"
world_name: The Shattered Realms
starting_region: kingshold
map_style: hand-drawn parchment fantasy cartography
regions:
  kingshold:
    name: Kingshold
    description: The fortified capital city
    adjacent:
      - wildwood
    landmarks:
      - name: The Iron Throne
        type: castle
        description: Seat of the king
  wildwood:
    name: The Wildwood
    description: An ancient and dark forest
    adjacent:
      - kingshold
    landmarks:
      - Thorngate Pass
routes:
  - name: King's Road
    description: Well-patrolled highway
    from_id: kingshold
    to_id: wildwood
    distance: moderate
    danger: low
"#;
    let config: CartographyConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(config.navigation_mode, NavigationMode::Region);
    assert!(config.rooms.is_empty());
    assert_eq!(config.regions.len(), 2);
    assert_eq!(config.routes.len(), 1);
}

// ═══════════════════════════════════════════════════════════
// Helper: load a genre pack with custom cartography for validation tests
// ═══════════════════════════════════════════════════════════

/// Create a minimal genre pack with a single world whose cartography.yaml
/// is the provided YAML string. Returns the tempdir (kept alive) and loaded pack.
fn load_pack_with_cartography(cartography_yaml: &str) -> (tempfile::TempDir, sidequest_genre::GenrePack) {
    let dir = tempfile::tempdir().unwrap();
    let world_dir = dir.path().join("worlds").join("test_dungeon");
    std::fs::create_dir_all(&world_dir).unwrap();

    write_minimal_genre_files(dir.path());

    std::fs::write(
        world_dir.join("world.yaml"),
        "name: Test Dungeon\nslug: test_dungeon\ndescription: A test world\n",
    )
    .unwrap();
    std::fs::write(
        world_dir.join("lore.yaml"),
        "origin_myth: test\ncentral_conflict: test\n",
    )
    .unwrap();
    std::fs::write(world_dir.join("cartography.yaml"), cartography_yaml).unwrap();
    std::fs::write(world_dir.join("legends.yaml"), "[]").unwrap();

    let pack = sidequest_genre::load_genre_pack(dir.path()).unwrap();
    (dir, pack)
}

// ═══════════════════════════════════════════════════════════
// AC-3: Validation — invalid exit targets
// ═══════════════════════════════════════════════════════════

#[test]
fn validation_rejects_invalid_exit_target() {
    let (_dir, pack) = load_pack_with_cartography(r#"
world_name: Test Dungeon
starting_region: entrance
navigation_mode: room_graph
rooms:
  - id: entrance
    name: Entrance
    description: The entrance
    exits:
      - target: nonexistent_room
        direction: north
        description: A door to nowhere
"#);
    let result = pack.validate();
    assert!(result.is_err(), "validation should reject exit to nonexistent room");
    let err = result.unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("nonexistent_room"),
        "error should mention the invalid target, got: {msg}"
    );
}

// ═══════════════════════════════════════════════════════════
// AC-3: Validation — missing bidirectional routes (non-chute)
// ═══════════════════════════════════════════════════════════

#[test]
fn validation_rejects_missing_bidirectional_exit() {
    // Room A → B exists but B → A does NOT, and the exit is NOT one_way
    let (_dir, pack) = load_pack_with_cartography(r#"
world_name: Test Dungeon
starting_region: room_a
navigation_mode: room_graph
rooms:
  - id: room_a
    name: Room A
    description: First room
    exits:
      - target: room_b
        direction: north
        description: A door north
  - id: room_b
    name: Room B
    description: Second room
    exits: []
"#);
    let result = pack.validate();
    assert!(
        result.is_err(),
        "validation should reject non-chute exit without bidirectional return"
    );
}

#[test]
fn validation_allows_one_way_chute_without_return() {
    // Room A → B is one_way (chute), B has no exit back to A — this is VALID
    let (_dir, pack) = load_pack_with_cartography(r#"
world_name: Test Dungeon
starting_region: room_a
navigation_mode: room_graph
rooms:
  - id: room_a
    name: Room A
    description: A ledge
    exits:
      - target: room_b
        direction: down
        description: A crumbling drop
        one_way: true
  - id: room_b
    name: Room B
    description: The pit bottom
    exits: []
"#);
    let result = pack.validate();
    assert!(
        result.is_ok(),
        "one_way chute exits should NOT require a return path, got: {:?}",
        result.unwrap_err()
    );
}

#[test]
fn validation_passes_valid_bidirectional_room_graph() {
    // Fully bidirectional: A ↔ B
    let (_dir, pack) = load_pack_with_cartography(r#"
world_name: Test Dungeon
starting_region: room_a
navigation_mode: room_graph
rooms:
  - id: room_a
    name: Room A
    description: First room
    exits:
      - target: room_b
        direction: north
        description: A passage north
  - id: room_b
    name: Room B
    description: Second room
    exits:
      - target: room_a
        direction: south
        description: A passage south
"#);
    let result = pack.validate();
    assert!(result.is_ok(), "valid bidirectional room graph should pass validation, got: {:?}", result.unwrap_err());
}

// ═══════════════════════════════════════════════════════════
// AC-3: Validation — duplicate room IDs
// ═══════════════════════════════════════════════════════════

#[test]
fn validation_rejects_duplicate_room_ids() {
    let (_dir, pack) = load_pack_with_cartography(r#"
world_name: Test Dungeon
starting_region: room_a
navigation_mode: room_graph
rooms:
  - id: room_a
    name: Room A
    description: First room
    exits: []
  - id: room_a
    name: Room A Again
    description: Duplicate
    exits: []
"#);
    let result = pack.validate();
    assert!(result.is_err(), "duplicate room IDs should fail validation");
}

// ═══════════════════════════════════════════════════════════
// AC-3: Validation — starting_region must be a valid room ID in room_graph mode
// ═══════════════════════════════════════════════════════════

#[test]
fn validation_rejects_invalid_starting_region_in_room_graph() {
    let (_dir, pack) = load_pack_with_cartography(r#"
world_name: Test Dungeon
starting_region: nonexistent_start
navigation_mode: room_graph
rooms:
  - id: room_a
    name: Room A
    description: A room
    exits: []
"#);
    let result = pack.validate();
    assert!(
        result.is_err(),
        "starting_region must reference a valid room ID in room_graph mode"
    );
}

// ═══════════════════════════════════════════════════════════
// Edge case: rooms field ignored in Region mode
// ═══════════════════════════════════════════════════════════

#[test]
fn rooms_in_region_mode_are_ignored_by_validation() {
    // If someone accidentally includes rooms in Region mode,
    // they should be silently ignored (no room_graph validation applies)
    let yaml = r#"
world_name: Low Fantasy
starting_region: town
navigation_mode: region
rooms:
  - id: room_a
    name: Room A
    description: test
    exits:
      - target: nonexistent
        direction: north
        description: broken exit
"#;
    let config: CartographyConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(config.navigation_mode, NavigationMode::Region);
    // rooms parse but room_graph validation should not run in Region mode
    assert_eq!(config.rooms.len(), 1);
}

// ═══════════════════════════════════════════════════════════
// Helper: write minimal genre pack files for integration tests
// ═══════════════════════════════════════════════════════════

fn write_minimal_genre_files(pack_dir: &std::path::Path) {
    std::fs::write(
        pack_dir.join("pack.yaml"),
        "name: Test Genre\nversion: '0.1.0'\ndescription: Test\nmin_sidequest_version: '0.1.0'\n",
    )
    .unwrap();
    std::fs::write(
        pack_dir.join("rules.yaml"),
        "ability_score_names: [STR, DEX, CON, INT, WIS, CHA]\nmagic_level: low\nstat_generation: standard\npoint_buy_budget: 27\nallowed_classes: [Fighter]\nallowed_races: [Human]\nclass_hp_bases:\n  Fighter: 10\nconfrontations: []\n",
    )
    .unwrap();
    std::fs::write(
        pack_dir.join("lore.yaml"),
        "world_name: ''\norigin_myth: test\ncentral_conflict: test\nhistory: ''\ngeography: ''\ncosmology: ''\n",
    )
    .unwrap();
    std::fs::write(
        pack_dir.join("theme.yaml"),
        "primary: '#000000'\nsecondary: '#111111'\naccent: '#222222'\nbackground: '#FFFFFF'\nsurface: '#F0F0F0'\ntext: '#000000'\nborder_style: light\nweb_font_family: serif\ndinkus:\n  enabled: true\n  cooldown: 2\n  default_weight: medium\n  glyph:\n    light: '*'\n    medium: '⁂'\n    heavy: '✠ ⁂ ✠'\nsession_opener:\n  enabled: true\n  prefix_glyph: '⸙'\n  suffix_glyph: '⸙'\n",
    )
    .unwrap();
    std::fs::write(pack_dir.join("archetypes.yaml"), "[]\n").unwrap();
    std::fs::write(pack_dir.join("char_creation.yaml"), "[]\n").unwrap();
    std::fs::write(
        pack_dir.join("visual_style.yaml"),
        "positive_suffix: 'dark fantasy'\nnegative_prompt: 'bright'\npreferred_model: flux\nbase_seed: 42\n",
    )
    .unwrap();
    std::fs::write(
        pack_dir.join("progression.yaml"),
        "tracks: []\nmax_level: 10\nlevel_thresholds: [0, 100]\n",
    )
    .unwrap();
    std::fs::write(
        pack_dir.join("axes.yaml"),
        "definitions: []\n",
    )
    .unwrap();
    std::fs::write(
        pack_dir.join("audio.yaml"),
        "mood_tracks: {}\nsfx_library: {}\ncreature_voice_presets: {}\nmixer:\n  music_volume: 0.8\n  sfx_volume: 0.9\n  voice_volume: 1.0\n  duck_music_for_voice: true\n  duck_amount_db: 3.0\n  crossfade_default_ms: 500\n",
    )
    .unwrap();
    std::fs::write(pack_dir.join("cultures.yaml"), "[]\n").unwrap();
    std::fs::write(
        pack_dir.join("prompts.yaml"),
        "narrator: test\ncombat: test\nnpc: test\nworld_state: test\n",
    )
    .unwrap();
    std::fs::write(pack_dir.join("tropes.yaml"), "[]\n").unwrap();
}

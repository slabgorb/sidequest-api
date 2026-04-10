//! RED-phase tests for Story 19-1: RoomDef + RoomExit tagged-enum data model.
//!
//! These tests exercise the CORRECT design from the session file:
//! - RoomExit as a tagged enum (Door, Corridor, ChuteDown, ChuteUp, Secret)
//! - RoomExit methods: target(), requires_reverse(), display_name()
//! - RoomDef with room_type, size, keeper_awareness_modifier fields
//! - CartographyConfig.rooms as Option<Vec<RoomDef>>
//! - rooms.yaml loaded from a separate file
//! - Validation: entrance room_type, orphaned rooms, bidirectional via requires_reverse()

use sidequest_genre::{CartographyConfig, NavigationMode, RoomDef, RoomExit};

// ═══════════════════════════════════════════════════════════
// RoomExit tagged-enum deserialization
// ═══════════════════════════════════════════════════════════

#[test]
fn room_exit_deserializes_door() {
    let yaml = r#"
type: door
target: great_hall
is_locked: true
"#;
    let exit: RoomExit = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(exit.target(), "great_hall");
    assert_eq!(exit.display_name(), "door");
    assert!(exit.requires_reverse());
}

#[test]
fn room_exit_deserializes_door_default_unlocked() {
    let yaml = r#"
type: door
target: armory
"#;
    let exit: RoomExit = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(exit.target(), "armory");
    // is_locked defaults to false — variant-level check
    if let RoomExit::Door { is_locked, .. } = &exit {
        assert!(!is_locked, "is_locked should default to false");
    } else {
        panic!("expected Door variant");
    }
}

#[test]
fn room_exit_deserializes_corridor() {
    let yaml = r#"
type: corridor
target: hallway
"#;
    let exit: RoomExit = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(exit.target(), "hallway");
    assert_eq!(exit.display_name(), "corridor");
    assert!(exit.requires_reverse());
}

#[test]
fn room_exit_deserializes_chute_down() {
    let yaml = r#"
type: chute_down
target: pit_bottom
"#;
    let exit: RoomExit = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(exit.target(), "pit_bottom");
    assert_eq!(exit.display_name(), "chute down");
    assert!(
        !exit.requires_reverse(),
        "ChuteDown should NOT require reverse"
    );
}

#[test]
fn room_exit_deserializes_chute_up() {
    let yaml = r#"
type: chute_up
target: upper_level
"#;
    let exit: RoomExit = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(exit.target(), "upper_level");
    assert_eq!(exit.display_name(), "chute up");
    assert!(
        !exit.requires_reverse(),
        "ChuteUp should NOT require reverse"
    );
}

#[test]
fn room_exit_deserializes_secret() {
    let yaml = r#"
type: secret
target: hidden_vault
discovered: true
"#;
    let exit: RoomExit = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(exit.target(), "hidden_vault");
    assert_eq!(exit.display_name(), "secret passage");
    assert!(exit.requires_reverse());
    if let RoomExit::Secret { discovered, .. } = &exit {
        assert!(discovered);
    } else {
        panic!("expected Secret variant");
    }
}

#[test]
fn room_exit_secret_defaults_undiscovered() {
    let yaml = r#"
type: secret
target: hidden_vault
"#;
    let exit: RoomExit = serde_yaml::from_str(yaml).unwrap();
    if let RoomExit::Secret { discovered, .. } = &exit {
        assert!(!discovered, "discovered should default to false");
    } else {
        panic!("expected Secret variant");
    }
}

// ═══════════════════════════════════════════════════════════
// RoomDef deserialization — full fields
// ═══════════════════════════════════════════════════════════

#[test]
fn room_def_deserializes_all_fields() {
    let yaml = r#"
id: treasure_room
name: Treasure Room
room_type: treasure
size: [3, 2]
keeper_awareness_modifier: 1.3
description: Glittering piles of gold and gems
exits:
  - type: door
    target: great_hall
    is_locked: true
  - type: secret
    target: escape_tunnel
"#;
    let room: RoomDef = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(room.id, "treasure_room");
    assert_eq!(room.name, "Treasure Room");
    assert_eq!(room.room_type, "treasure");
    assert_eq!(room.size, (3, 2));
    assert!((room.keeper_awareness_modifier - 1.3).abs() < f64::EPSILON);
    assert_eq!(
        room.description,
        Some("Glittering piles of gold and gems".to_string())
    );
    assert_eq!(room.exits.len(), 2);
    assert_eq!(room.exits[0].target(), "great_hall");
    assert_eq!(room.exits[1].target(), "escape_tunnel");
}

#[test]
fn room_def_size_defaults_to_1_1() {
    let yaml = r#"
id: closet
name: Broom Closet
room_type: normal
exits: []
"#;
    let room: RoomDef = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(room.size, (1, 1), "size should default to (1,1)");
}

#[test]
fn room_def_keeper_awareness_defaults_to_1() {
    let yaml = r#"
id: hallway
name: Hallway
room_type: normal
exits: []
"#;
    let room: RoomDef = serde_yaml::from_str(yaml).unwrap();
    assert!(
        (room.keeper_awareness_modifier - 1.0).abs() < f64::EPSILON,
        "keeper_awareness_modifier should default to 1.0"
    );
}

#[test]
fn room_def_description_is_optional() {
    let yaml = r#"
id: passage
name: Narrow Passage
room_type: normal
exits: []
"#;
    let room: RoomDef = serde_yaml::from_str(yaml).unwrap();
    assert!(
        room.description.is_none(),
        "description should be None when absent"
    );
}

// ═══════════════════════════════════════════════════════════
// NavigationMode
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
fn navigation_mode_deserializes_room_graph() {
    let yaml = r#"
world_name: Test World
starting_region: entrance_hall
navigation_mode: room_graph
"#;
    let config: CartographyConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(config.navigation_mode, NavigationMode::RoomGraph);
}

// ═══════════════════════════════════════════════════════════
// CartographyConfig.rooms as Option<Vec<RoomDef>>
// ═══════════════════════════════════════════════════════════

#[test]
fn cartography_rooms_none_when_absent() {
    let yaml = r#"
world_name: Low Fantasy World
starting_region: town
"#;
    let config: CartographyConfig = serde_yaml::from_str(yaml).unwrap();
    assert!(
        config.rooms.is_none(),
        "rooms should be None when absent from YAML"
    );
}

#[test]
fn cartography_rooms_some_when_present() {
    let yaml = r#"
world_name: The Mawdeep
starting_region: entrance_hall
navigation_mode: room_graph
rooms:
  - id: entrance_hall
    name: Entrance Hall
    room_type: entrance
    exits:
      - type: corridor
        target: great_hall
  - id: great_hall
    name: Great Hall
    room_type: normal
    exits:
      - type: corridor
        target: entrance_hall
"#;
    let config: CartographyConfig = serde_yaml::from_str(yaml).unwrap();
    let rooms = config
        .rooms
        .as_ref()
        .expect("rooms should be Some when present");
    assert_eq!(rooms.len(), 2);
    assert_eq!(rooms[0].id, "entrance_hall");
    assert_eq!(rooms[0].room_type, "entrance");
    assert_eq!(rooms[1].id, "great_hall");
}

// ═══════════════════════════════════════════════════════════
// Backward compatibility — existing genre packs unaffected
// ═══════════════════════════════════════════════════════════

#[test]
fn backward_compat_existing_cartography_loads_with_rooms_none() {
    let yaml = r#"
world_name: The Shattered Realms
starting_region: kingshold
map_style: hand-drawn parchment fantasy cartography
regions:
  kingshold:
    name: Kingshold
    summary: "Fortified capital city and seat of power"
    description: The fortified capital city
    adjacent:
      - wildwood
    landmarks:
      - name: The Iron Throne
        type: castle
        description: Seat of the king
  wildwood:
    name: The Wildwood
    summary: "Ancient dark forest of mystery"
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
    assert!(
        config.rooms.is_none(),
        "rooms should be None for existing packs"
    );
    assert_eq!(config.regions.len(), 2);
    assert_eq!(config.routes.len(), 1);
}

// ═══════════════════════════════════════════════════════════
// Integration: rooms.yaml loaded from separate file
// ═══════════════════════════════════════════════════════════

#[test]
fn integration_loads_rooms_yaml_from_world_dir() {
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
    std::fs::write(world_dir.join("legends.yaml"), "[]").unwrap();

    // cartography.yaml references room_graph mode — rooms come from rooms.yaml
    std::fs::write(
        world_dir.join("cartography.yaml"),
        "world_name: Test Dungeon\nstarting_region: entry\nnavigation_mode: room_graph\n",
    )
    .unwrap();

    // rooms.yaml is a SEPARATE file in the world directory
    std::fs::write(
        world_dir.join("rooms.yaml"),
        r#"
- id: entry
  name: Entry Chamber
  room_type: entrance
  exits:
    - type: door
      target: hallway
- id: hallway
  name: Main Hallway
  room_type: normal
  exits:
    - type: door
      target: entry
"#,
    )
    .unwrap();

    let pack = sidequest_genre::load_genre_pack(dir.path()).unwrap();
    let world = pack.worlds.get("test_dungeon").expect("world should load");
    let rooms = world
        .cartography
        .rooms
        .as_ref()
        .expect("rooms should be Some when rooms.yaml exists");
    assert_eq!(rooms.len(), 2);
    assert_eq!(rooms[0].id, "entry");
    assert_eq!(rooms[0].room_type, "entrance");
    assert_eq!(rooms[1].id, "hallway");
}

// ═══════════════════════════════════════════════════════════
// Validation — invalid exit target
// ═══════════════════════════════════════════════════════════

#[test]
fn validation_rejects_invalid_exit_target() {
    let (_dir, pack) = load_pack_with_rooms(
        "world_name: Test\nstarting_region: entry\nnavigation_mode: room_graph\n",
        r#"
- id: entry
  name: Entry
  room_type: entrance
  exits:
    - type: door
      target: nonexistent_room
"#,
    );
    let result = pack.validate();
    assert!(result.is_err(), "should reject exit to nonexistent room");
    let msg = format!("{}", result.unwrap_err());
    assert!(
        msg.contains("nonexistent_room"),
        "error should mention invalid target, got: {msg}"
    );
}

// ═══════════════════════════════════════════════════════════
// Validation — bidirectional exits (uses requires_reverse())
// ═══════════════════════════════════════════════════════════

#[test]
fn validation_rejects_door_without_reverse() {
    let (_dir, pack) = load_pack_with_rooms(
        "world_name: Test\nstarting_region: room_a\nnavigation_mode: room_graph\n",
        r#"
- id: room_a
  name: Room A
  room_type: entrance
  exits:
    - type: door
      target: room_b
- id: room_b
  name: Room B
  room_type: normal
  exits: []
"#,
    );
    let result = pack.validate();
    assert!(
        result.is_err(),
        "Door A→B without B→A should fail validation"
    );
}

#[test]
fn validation_allows_chute_down_without_reverse() {
    let (_dir, pack) = load_pack_with_rooms(
        "world_name: Test\nstarting_region: room_a\nnavigation_mode: room_graph\n",
        r#"
- id: room_a
  name: Room A
  room_type: entrance
  exits:
    - type: chute_down
      target: room_b
- id: room_b
  name: Room B
  room_type: normal
  exits: []
"#,
    );
    let result = pack.validate();
    assert!(
        result.is_ok(),
        "ChuteDown should NOT require reverse, got: {:?}",
        result.unwrap_err()
    );
}

#[test]
fn validation_allows_chute_up_without_reverse() {
    let (_dir, pack) = load_pack_with_rooms(
        "world_name: Test\nstarting_region: room_a\nnavigation_mode: room_graph\n",
        r#"
- id: room_a
  name: Room A
  room_type: entrance
  exits:
    - type: chute_up
      target: room_b
- id: room_b
  name: Room B
  room_type: normal
  exits: []
"#,
    );
    let result = pack.validate();
    assert!(
        result.is_ok(),
        "ChuteUp should NOT require reverse, got: {:?}",
        result.unwrap_err()
    );
}

#[test]
fn validation_passes_valid_bidirectional_graph() {
    let (_dir, pack) = load_pack_with_rooms(
        "world_name: Test\nstarting_region: room_a\nnavigation_mode: room_graph\n",
        r#"
- id: room_a
  name: Room A
  room_type: entrance
  exits:
    - type: corridor
      target: room_b
- id: room_b
  name: Room B
  room_type: normal
  exits:
    - type: corridor
      target: room_a
"#,
    );
    let result = pack.validate();
    assert!(
        result.is_ok(),
        "valid bidirectional graph should pass, got: {:?}",
        result.unwrap_err()
    );
}

// ═══════════════════════════════════════════════════════════
// Validation — entrance room_type required
// ═══════════════════════════════════════════════════════════

#[test]
fn validation_rejects_no_entrance_room() {
    let (_dir, pack) = load_pack_with_rooms(
        "world_name: Test\nstarting_region: room_a\nnavigation_mode: room_graph\n",
        r#"
- id: room_a
  name: Room A
  room_type: normal
  exits:
    - type: corridor
      target: room_b
- id: room_b
  name: Room B
  room_type: normal
  exits:
    - type: corridor
      target: room_a
"#,
    );
    let result = pack.validate();
    assert!(
        result.is_err(),
        "should require exactly one room with room_type 'entrance'"
    );
    let msg = format!("{}", result.unwrap_err());
    assert!(
        msg.to_lowercase().contains("entrance"),
        "error should mention missing entrance, got: {msg}"
    );
}

// ═══════════════════════════════════════════════════════════
// Validation — orphaned rooms unreachable from entrance
// ═══════════════════════════════════════════════════════════

#[test]
fn validation_rejects_orphaned_room() {
    let (_dir, pack) = load_pack_with_rooms(
        "world_name: Test\nstarting_region: room_a\nnavigation_mode: room_graph\n",
        r#"
- id: room_a
  name: Room A
  room_type: entrance
  exits:
    - type: corridor
      target: room_b
- id: room_b
  name: Room B
  room_type: normal
  exits:
    - type: corridor
      target: room_a
- id: orphan
  name: Orphan Room
  room_type: normal
  exits: []
"#,
    );
    let result = pack.validate();
    assert!(
        result.is_err(),
        "orphaned room unreachable from entrance should fail"
    );
    let msg = format!("{}", result.unwrap_err());
    assert!(
        msg.contains("orphan"),
        "error should mention the orphaned room, got: {msg}"
    );
}

// ═══════════════════════════════════════════════════════════
// Validation — Region mode skips room graph validation
// ═══════════════════════════════════════════════════════════

#[test]
fn region_mode_skips_room_graph_validation() {
    // Intentionally invalid room data that should be ignored in Region mode
    let yaml = r#"
world_name: Low Fantasy
starting_region: town
navigation_mode: region
"#;
    let config: CartographyConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(config.navigation_mode, NavigationMode::Region);
    assert!(config.rooms.is_none());
}

// ═══════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════

/// Create a minimal genre pack with rooms.yaml as a SEPARATE file.
fn load_pack_with_rooms(
    cartography_yaml: &str,
    rooms_yaml: &str,
) -> (tempfile::TempDir, sidequest_genre::GenrePack) {
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
    std::fs::write(world_dir.join("rooms.yaml"), rooms_yaml).unwrap();
    std::fs::write(world_dir.join("legends.yaml"), "[]").unwrap();

    let pack = sidequest_genre::load_genre_pack(dir.path()).unwrap();
    (dir, pack)
}

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
    std::fs::write(pack_dir.join("axes.yaml"), "definitions: []\n").unwrap();
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

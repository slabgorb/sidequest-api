//! RED-phase tests for Story 23-3: Universal room graph cartography.
//!
//! Adds a Hierarchical navigation mode with two-level graph support:
//! - WorldGraphNode: major locations with metadata
//! - GraphEdge: connections between nodes with danger/terrain/distance
//! - Sub-graphs: internal topology for expandable world-graph nodes
//!
//! CartographyConfig gains `world_graph` and `sub_graphs` fields.
//! Danger semantics: 0 = fast travel, >0 = story-generating scene.

use sidequest_genre::{
    CartographyConfig, GraphEdge, NavigationMode, SubGraph, Terrain, WorldGraphNode,
};

// ═══════════════════════════════════════════════════════════
// NavigationMode::Hierarchical
// ═══════════════════════════════════════════════════════════

#[test]
fn navigation_mode_deserializes_hierarchical() {
    let yaml = r#"
world_name: Test World
starting_region: solenne
navigation_mode: hierarchical
"#;
    let config: CartographyConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(config.navigation_mode, NavigationMode::Hierarchical);
}

#[test]
fn navigation_mode_default_still_region() {
    let yaml = r#"
world_name: Test World
starting_region: town
"#;
    let config: CartographyConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(config.navigation_mode, NavigationMode::Region);
}

#[test]
fn navigation_mode_room_graph_still_works() {
    let yaml = r#"
world_name: Test World
starting_region: entrance_hall
navigation_mode: room_graph
"#;
    let config: CartographyConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(config.navigation_mode, NavigationMode::RoomGraph);
}

// ═══════════════════════════════════════════════════════════
// Terrain enum
// ═══════════════════════════════════════════════════════════

#[test]
fn terrain_deserializes_all_variants() {
    let variants = vec![
        ("road", Terrain::Road),
        ("wilderness", Terrain::Wilderness),
        ("water", Terrain::Water),
        ("underground", Terrain::Underground),
    ];
    for (yaml_str, expected) in variants {
        let terrain: Terrain = serde_yaml::from_str(&format!("\"{}\"", yaml_str)).unwrap();
        assert_eq!(
            terrain, expected,
            "failed to deserialize terrain variant: {yaml_str}"
        );
    }
}

#[test]
fn terrain_defaults_to_road() {
    assert_eq!(Terrain::default(), Terrain::Road);
}

// ═══════════════════════════════════════════════════════════
// WorldGraphNode deserialization
// ═══════════════════════════════════════════════════════════

#[test]
fn world_graph_node_deserializes_all_fields() {
    let yaml = r#"
id: solenne
name: Solenne
description: A river city split between merchant quarters and the Lantern Quarter.
"#;
    let node: WorldGraphNode = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(node.id, "solenne");
    assert_eq!(node.name, "Solenne");
    assert_eq!(
        node.description,
        "A river city split between merchant quarters and the Lantern Quarter."
    );
}

#[test]
fn world_graph_node_description_is_optional() {
    let yaml = r#"
id: the_stump
name: The Stump
"#;
    let node: WorldGraphNode = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(node.id, "the_stump");
    assert!(node.description.is_empty());
}

// ═══════════════════════════════════════════════════════════
// GraphEdge deserialization
// ═══════════════════════════════════════════════════════════

#[test]
fn graph_edge_deserializes_all_fields() {
    let yaml = r#"
from: solenne
to: the_glass_waste
danger: 3
terrain: wilderness
distance: 2
"#;
    let edge: GraphEdge = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(edge.from, "solenne");
    assert_eq!(edge.to, "the_glass_waste");
    assert_eq!(edge.danger, 3);
    assert_eq!(edge.terrain, Terrain::Wilderness);
    assert_eq!(edge.distance, 2);
}

#[test]
fn graph_edge_danger_zero_means_fast_travel() {
    let yaml = r#"
from: solenne
to: merchant_quarter
danger: 0
"#;
    let edge: GraphEdge = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(edge.danger, 0, "danger=0 should mean fast travel");
    assert!(
        edge.is_fast_travel(),
        "danger=0 edge should report is_fast_travel()"
    );
}

#[test]
fn graph_edge_danger_positive_means_story_generating() {
    let yaml = r#"
from: solenne
to: the_glass_waste
danger: 2
"#;
    let edge: GraphEdge = serde_yaml::from_str(yaml).unwrap();
    assert!(!edge.is_fast_travel(), "danger>0 should NOT be fast travel");
    assert!(
        edge.is_story_generating(),
        "danger>0 should be story-generating"
    );
}

#[test]
fn graph_edge_terrain_defaults_to_road() {
    let yaml = r#"
from: a
to: b
danger: 0
"#;
    let edge: GraphEdge = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(edge.terrain, Terrain::Road);
}

#[test]
fn graph_edge_distance_defaults_to_one() {
    let yaml = r#"
from: a
to: b
danger: 0
"#;
    let edge: GraphEdge = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(edge.distance, 1, "distance should default to 1");
}

#[test]
fn graph_edge_optional_encounter_table_key() {
    let yaml = r#"
from: solenne
to: the_glass_waste
danger: 3
terrain: wilderness
encounter_table_key: glass_waste_ambush
"#;
    let edge: GraphEdge = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(
        edge.encounter_table_key.as_deref(),
        Some("glass_waste_ambush")
    );
}

#[test]
fn graph_edge_encounter_table_key_none_when_absent() {
    let yaml = r#"
from: a
to: b
danger: 1
"#;
    let edge: GraphEdge = serde_yaml::from_str(yaml).unwrap();
    assert!(edge.encounter_table_key.is_none());
}

// ═══════════════════════════════════════════════════════════
// SubGraph deserialization
// ═══════════════════════════════════════════════════════════

#[test]
fn sub_graph_deserializes_nodes_and_edges() {
    let yaml = r#"
nodes:
  - id: river_docks
    name: River Docks
  - id: merchant_quarter
    name: Merchant Quarter
edges:
  - from: river_docks
    to: merchant_quarter
    danger: 0
"#;
    let sg: SubGraph = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(sg.nodes.len(), 2);
    assert_eq!(sg.edges.len(), 1);
    assert_eq!(sg.nodes[0].id, "river_docks");
    assert_eq!(sg.edges[0].from, "river_docks");
    assert_eq!(sg.edges[0].to, "merchant_quarter");
}

// ═══════════════════════════════════════════════════════════
// CartographyConfig with world_graph and sub_graphs
// ═══════════════════════════════════════════════════════════

#[test]
fn cartography_hierarchical_full_config() {
    let yaml = r#"
world_name: The Pinwheel Coast
starting_region: solenne
navigation_mode: hierarchical
world_graph:
  nodes:
    - id: solenne
      name: Solenne
      description: A river city
    - id: the_glass_waste
      name: The Glass Waste
      description: Miles of fused earth
  edges:
    - from: solenne
      to: the_glass_waste
      danger: 3
      terrain: wilderness
      distance: 2
sub_graphs:
  solenne:
    nodes:
      - id: river_docks
        name: River Docks
      - id: merchant_quarter
        name: Merchant Quarter
    edges:
      - from: river_docks
        to: merchant_quarter
        danger: 0
"#;
    let config: CartographyConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(config.navigation_mode, NavigationMode::Hierarchical);

    let wg = config
        .world_graph
        .as_ref()
        .expect("world_graph should be Some");
    assert_eq!(wg.nodes.len(), 2);
    assert_eq!(wg.edges.len(), 1);
    assert_eq!(wg.nodes[0].id, "solenne");
    assert_eq!(wg.edges[0].danger, 3);

    let sgs = config
        .sub_graphs
        .as_ref()
        .expect("sub_graphs should be Some");
    assert!(
        sgs.contains_key("solenne"),
        "sub_graphs should contain 'solenne'"
    );
    let solenne_sg = &sgs["solenne"];
    assert_eq!(solenne_sg.nodes.len(), 2);
    assert_eq!(solenne_sg.edges.len(), 1);
}

#[test]
fn cartography_hierarchical_no_sub_graphs() {
    let yaml = r#"
world_name: Small World
starting_region: bridge
navigation_mode: hierarchical
world_graph:
  nodes:
    - id: bridge
      name: Bridge
    - id: engineering
      name: Engineering
  edges:
    - from: bridge
      to: engineering
      danger: 0
"#;
    let config: CartographyConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(config.navigation_mode, NavigationMode::Hierarchical);
    assert!(config.world_graph.is_some());
    // sub_graphs absent or empty — both valid for single-level graphs
    let sgs = config.sub_graphs.as_ref();
    assert!(
        sgs.is_none() || sgs.unwrap().is_empty(),
        "sub_graphs should be None or empty for single-level graphs"
    );
}

#[test]
fn cartography_world_graph_none_for_region_mode() {
    let yaml = r#"
world_name: Region World
starting_region: town
navigation_mode: region
"#;
    let config: CartographyConfig = serde_yaml::from_str(yaml).unwrap();
    assert!(
        config.world_graph.is_none(),
        "world_graph should be None in Region mode"
    );
    assert!(
        config.sub_graphs.is_none(),
        "sub_graphs should be None in Region mode"
    );
}

#[test]
fn cartography_world_graph_none_for_room_graph_mode() {
    let yaml = r#"
world_name: Dungeon
starting_region: entrance
navigation_mode: room_graph
"#;
    let config: CartographyConfig = serde_yaml::from_str(yaml).unwrap();
    assert!(
        config.world_graph.is_none(),
        "world_graph should be None in RoomGraph mode"
    );
    assert!(
        config.sub_graphs.is_none(),
        "sub_graphs should be None in RoomGraph mode"
    );
}

// ═══════════════════════════════════════════════════════════
// Backward compatibility — existing packs unaffected
// ═══════════════════════════════════════════════════════════

#[test]
fn backward_compat_region_mode_ignores_new_fields() {
    let yaml = r#"
world_name: The Shattered Realms
starting_region: kingshold
map_style: hand-drawn parchment
regions:
  kingshold:
    name: Kingshold
    summary: "The capital city"
    description: The capital
    adjacent:
      - wildwood
  wildwood:
    name: The Wildwood
    summary: "Dark ancient forest"
    description: Dark forest
    adjacent:
      - kingshold
routes:
  - name: King's Road
    description: Main highway
    from_id: kingshold
    to_id: wildwood
    distance: moderate
    danger: low
"#;
    let config: CartographyConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(config.navigation_mode, NavigationMode::Region);
    assert!(config.world_graph.is_none());
    assert!(config.sub_graphs.is_none());
    assert_eq!(config.regions.len(), 2);
    assert_eq!(config.routes.len(), 1);
}

// ═══════════════════════════════════════════════════════════
// Validation — hierarchical mode
// ═══════════════════════════════════════════════════════════

#[test]
fn validation_rejects_edge_referencing_nonexistent_node() {
    let (_dir, pack) = load_pack_with_world_graph(
        r#"
world_name: Test
starting_region: a
navigation_mode: hierarchical
world_graph:
  nodes:
    - id: a
      name: Node A
  edges:
    - from: a
      to: ghost_node
      danger: 1
"#,
    );
    let result = pack.validate();
    assert!(
        result.is_err(),
        "edge to nonexistent node should fail validation"
    );
    let msg = format!("{}", result.unwrap_err());
    assert!(
        msg.contains("ghost_node"),
        "error should mention invalid node reference, got: {msg}"
    );
}

#[test]
fn validation_rejects_sub_graph_for_nonexistent_parent() {
    let (_dir, pack) = load_pack_with_world_graph(
        r#"
world_name: Test
starting_region: a
navigation_mode: hierarchical
world_graph:
  nodes:
    - id: a
      name: Node A
  edges: []
sub_graphs:
  nonexistent_parent:
    nodes:
      - id: inner
        name: Inner
    edges: []
"#,
    );
    let result = pack.validate();
    assert!(
        result.is_err(),
        "sub_graph keyed to nonexistent world node should fail validation"
    );
    let msg = format!("{}", result.unwrap_err());
    assert!(
        msg.contains("nonexistent_parent"),
        "error should mention invalid parent, got: {msg}"
    );
}

#[test]
fn validation_rejects_sub_graph_edge_to_nonexistent_sub_node() {
    let (_dir, pack) = load_pack_with_world_graph(
        r#"
world_name: Test
starting_region: city
navigation_mode: hierarchical
world_graph:
  nodes:
    - id: city
      name: City
  edges: []
sub_graphs:
  city:
    nodes:
      - id: market
        name: Market
    edges:
      - from: market
        to: ghost_district
        danger: 0
"#,
    );
    let result = pack.validate();
    assert!(
        result.is_err(),
        "sub_graph edge to nonexistent sub-node should fail validation"
    );
    let msg = format!("{}", result.unwrap_err());
    assert!(
        msg.contains("ghost_district"),
        "error should mention invalid sub-node reference, got: {msg}"
    );
}

#[test]
fn validation_rejects_starting_region_not_in_world_graph() {
    let (_dir, pack) = load_pack_with_world_graph(
        r#"
world_name: Test
starting_region: nonexistent_start
navigation_mode: hierarchical
world_graph:
  nodes:
    - id: a
      name: Node A
  edges: []
"#,
    );
    let result = pack.validate();
    assert!(
        result.is_err(),
        "starting_region not in world_graph should fail validation"
    );
}

#[test]
fn validation_rejects_duplicate_node_ids() {
    let (_dir, pack) = load_pack_with_world_graph(
        r#"
world_name: Test
starting_region: a
navigation_mode: hierarchical
world_graph:
  nodes:
    - id: a
      name: Node A
    - id: a
      name: Node A Again
  edges: []
"#,
    );
    let result = pack.validate();
    assert!(result.is_err(), "duplicate node IDs should fail validation");
}

#[test]
fn validation_passes_valid_hierarchical_graph() {
    let (_dir, pack) = load_pack_with_world_graph(
        r#"
world_name: Test Coast
starting_region: town
navigation_mode: hierarchical
world_graph:
  nodes:
    - id: town
      name: Town
    - id: forest
      name: Forest
  edges:
    - from: town
      to: forest
      danger: 1
      terrain: wilderness
sub_graphs:
  town:
    nodes:
      - id: market
        name: Market
      - id: tavern
        name: Tavern
    edges:
      - from: market
        to: tavern
        danger: 0
"#,
    );
    let result = pack.validate();
    assert!(
        result.is_ok(),
        "valid hierarchical graph should pass validation, got: {:?}",
        result.unwrap_err()
    );
}

// ═══════════════════════════════════════════════════════════
// Integration: loader parses world_graph from cartography.yaml
// ═══════════════════════════════════════════════════════════

#[test]
fn integration_loads_hierarchical_cartography() {
    let dir = tempfile::tempdir().unwrap();
    let world_dir = dir.path().join("worlds").join("test_coast");
    std::fs::create_dir_all(&world_dir).unwrap();

    write_minimal_genre_files(dir.path());

    std::fs::write(
        world_dir.join("world.yaml"),
        "name: Test Coast\nslug: test_coast\ndescription: A test world\n",
    )
    .unwrap();
    std::fs::write(
        world_dir.join("lore.yaml"),
        "origin_myth: test\ncentral_conflict: test\n",
    )
    .unwrap();
    std::fs::write(world_dir.join("legends.yaml"), "[]").unwrap();

    std::fs::write(
        world_dir.join("cartography.yaml"),
        r#"
world_name: Test Coast
starting_region: town
navigation_mode: hierarchical
world_graph:
  nodes:
    - id: town
      name: Town
      description: A small coastal town
    - id: wilderness
      name: The Wilderness
      description: Dense forest
  edges:
    - from: town
      to: wilderness
      danger: 2
      terrain: wilderness
      distance: 3
sub_graphs:
  town:
    nodes:
      - id: docks
        name: Docks
      - id: market
        name: Market Square
    edges:
      - from: docks
        to: market
        danger: 0
"#,
    )
    .unwrap();

    let pack = sidequest_genre::load_genre_pack(dir.path()).unwrap();
    let world = pack.worlds.get("test_coast").expect("world should load");

    assert_eq!(
        world.cartography.navigation_mode,
        NavigationMode::Hierarchical
    );

    let wg = world
        .cartography
        .world_graph
        .as_ref()
        .expect("world_graph should be loaded");
    assert_eq!(wg.nodes.len(), 2);
    assert_eq!(wg.edges.len(), 1);
    assert_eq!(wg.edges[0].from, "town");
    assert_eq!(wg.edges[0].to, "wilderness");
    assert_eq!(wg.edges[0].danger, 2);
    assert_eq!(wg.edges[0].terrain, Terrain::Wilderness);
    assert_eq!(wg.edges[0].distance, 3);

    let sgs = world
        .cartography
        .sub_graphs
        .as_ref()
        .expect("sub_graphs should be loaded");
    let town_sg = sgs.get("town").expect("town sub_graph should exist");
    assert_eq!(town_sg.nodes.len(), 2);
    assert_eq!(town_sg.edges.len(), 1);
    assert!(town_sg.edges[0].is_fast_travel());
}

// ═══════════════════════════════════════════════════════════
// WorldGraph helper methods
// ═══════════════════════════════════════════════════════════

#[test]
fn world_graph_node_by_id() {
    let yaml = r#"
nodes:
  - id: town
    name: Town
  - id: forest
    name: Forest
edges: []
"#;
    let wg: sidequest_genre::WorldGraph = serde_yaml::from_str(yaml).unwrap();
    let node = wg.node_by_id("town");
    assert!(node.is_some(), "should find node by id");
    assert_eq!(node.unwrap().name, "Town");
    assert!(
        wg.node_by_id("ghost").is_none(),
        "nonexistent id should return None"
    );
}

#[test]
fn world_graph_neighbors() {
    let yaml = r#"
nodes:
  - id: a
    name: A
  - id: b
    name: B
  - id: c
    name: C
edges:
  - from: a
    to: b
    danger: 0
  - from: a
    to: c
    danger: 2
  - from: b
    to: c
    danger: 1
"#;
    let wg: sidequest_genre::WorldGraph = serde_yaml::from_str(yaml).unwrap();
    let mut neighbors: Vec<&str> = wg.neighbors("a").collect();
    neighbors.sort();
    assert_eq!(neighbors, vec!["b", "c"], "a should have neighbors b and c");

    // Edges are directional in YAML but bidirectional for traversal
    let mut b_neighbors: Vec<&str> = wg.neighbors("b").collect();
    b_neighbors.sort();
    assert_eq!(
        b_neighbors,
        vec!["a", "c"],
        "b should have neighbors a (reverse edge) and c (forward edge)"
    );
}

#[test]
fn world_graph_edges_from() {
    let yaml = r#"
nodes:
  - id: a
    name: A
  - id: b
    name: B
edges:
  - from: a
    to: b
    danger: 3
    terrain: wilderness
"#;
    let wg: sidequest_genre::WorldGraph = serde_yaml::from_str(yaml).unwrap();
    let edges: Vec<&GraphEdge> = wg.edges_from("a").collect();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].to, "b");
    assert_eq!(edges[0].danger, 3);
}

// ═══════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════

/// Create a minimal genre pack with hierarchical cartography for validation tests.
fn load_pack_with_world_graph(
    cartography_yaml: &str,
) -> (tempfile::TempDir, sidequest_genre::GenrePack) {
    let dir = tempfile::tempdir().unwrap();
    let world_dir = dir.path().join("worlds").join("test_world");
    std::fs::create_dir_all(&world_dir).unwrap();

    write_minimal_genre_files(dir.path());

    std::fs::write(
        world_dir.join("world.yaml"),
        "name: Test World\nslug: test_world\ndescription: A test\n",
    )
    .unwrap();
    std::fs::write(
        world_dir.join("lore.yaml"),
        "origin_myth: test\ncentral_conflict: test\n",
    )
    .unwrap();
    std::fs::write(world_dir.join("legends.yaml"), "[]").unwrap();
    std::fs::write(world_dir.join("cartography.yaml"), cartography_yaml).unwrap();

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
        "mood_tracks: {}\nsfx_library: {}\ncreature_voice_presets: {}\nmixer:\n  music_volume: 0.8\n  sfx_volume: 0.9\n  voice_volume: 1.0\n  crossfade_default_ms: 500\n",
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

//! World configuration, cartography, and navigation types from `world.yaml` and `cartography.yaml`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════
// world.yaml
// ═══════════════════════════════════════════════════════════

/// World metadata.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorldConfig {
    /// World display name.
    pub name: String,
    /// URL-safe slug (optional — can be inferred from directory name).
    #[serde(default)]
    pub slug: String,
    /// Description text.
    pub description: String,
    /// Starting location description.
    #[serde(default)]
    pub starting_location: String,
    /// Axis values for this world.
    #[serde(default)]
    pub axis_snapshot: HashMap<String, f64>,
    /// Historical era (e.g., "Late 1970s").
    #[serde(default)]
    pub era: Option<String>,
    /// Tonal description for AI narration.
    #[serde(default)]
    pub tone: Option<String>,
    /// Genre-specific extensions (factions, faction_count, etc.).
    /// Captured for AI prompt injection without engine-level typing.
    #[serde(flatten)]
    pub extras: HashMap<String, serde_json::Value>,
}

// ═══════════════════════════════════════════════════════════
// cartography.yaml
// ════════════════════════════════════════════════════��══════

/// Navigation mode for a world's cartography.
///
/// `Region` (default) uses freeform location strings with region metadata.
/// `RoomGraph` uses validated room IDs with checked exits — required for
/// dungeon crawl genre packs where room transitions drive game mechanics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum NavigationMode {
    /// Freeform region-based navigation (default for all existing genre packs).
    #[default]
    Region,
    /// Validated room graph with checked exits (dungeon crawl mode).
    RoomGraph,
    /// Hierarchical world graph with optional sub-graphs per node.
    Hierarchical,
}


/// A single exit from a room to another room.
///
/// Tagged enum discriminated by `type` in YAML/JSON. Each variant carries
/// its own metadata (e.g., `is_locked` for doors, `discovered` for secrets).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RoomExit {
    /// Normal door: bidirectional by default, optionally locked.
    Door {
        /// Target room ID this exit leads to.
        target: String,
        /// Whether the door is locked.
        #[serde(default)]
        is_locked: bool,
    },
    /// Open corridor: bidirectional.
    Corridor {
        /// Target room ID this exit leads to.
        target: String,
    },
    /// One-way drop (no reverse required).
    ChuteDown {
        /// Target room ID this exit leads to.
        target: String,
    },
    /// One-way ascent (no reverse required, rare).
    ChuteUp {
        /// Target room ID this exit leads to.
        target: String,
    },
    /// Secret passage: bidirectional but hidden until discovered.
    Secret {
        /// Target room ID this exit leads to.
        target: String,
        /// Whether the passage has been discovered.
        #[serde(default)]
        discovered: bool,
    },
}

impl RoomExit {
    /// Target room ID this exit leads to.
    pub fn target(&self) -> &str {
        match self {
            RoomExit::Door { target, .. }
            | RoomExit::Corridor { target }
            | RoomExit::ChuteDown { target }
            | RoomExit::ChuteUp { target }
            | RoomExit::Secret { target, .. } => target,
        }
    }

    /// Whether this exit requires a return path from the target room.
    pub fn requires_reverse(&self) -> bool {
        matches!(
            self,
            RoomExit::Door { .. } | RoomExit::Corridor { .. } | RoomExit::Secret { .. }
        )
    }

    /// Display name for UI/narration.
    pub fn display_name(&self) -> &str {
        match self {
            RoomExit::Door { .. } => "door",
            RoomExit::Corridor { .. } => "corridor",
            RoomExit::ChuteDown { .. } => "chute down",
            RoomExit::ChuteUp { .. } => "chute up",
            RoomExit::Secret { .. } => "secret passage",
        }
    }
}

/// A legend entry mapping a glyph character to a feature type and label.
///
/// Used in tactical grid maps (ADR-071) to define what uppercase letter
/// glyphs represent in the ASCII grid.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegendEntry {
    /// Feature type string (e.g., "cover", "hazard", "atmosphere").
    pub r#type: String,
    /// Human-readable label (e.g., "Worn tooth stumps").
    pub label: String,
}

/// A room in the dungeon room graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomDef {
    /// Unique room identifier (slug).
    pub id: String,
    /// Display name.
    pub name: String,
    /// Room type: "entrance", "normal", "boss", "treasure", "dead_end".
    pub room_type: String,
    /// Physical dimensions for layout (width, height in grid units).
    #[serde(default = "default_room_size")]
    pub size: (u32, u32),
    /// How much Keeper awareness escalates per transition (0.8–1.5).
    #[serde(default = "default_keeper_awareness_modifier")]
    pub keeper_awareness_modifier: f64,
    /// Exits leading to other rooms.
    #[serde(default)]
    pub exits: Vec<RoomExit>,
    /// Optional description for UI/lore.
    #[serde(default)]
    pub description: Option<String>,
    /// Raw ASCII grid string for tactical maps (ADR-071).
    #[serde(default)]
    pub grid: Option<String>,
    /// Cells per grid unit for tactical scale.
    #[serde(default)]
    pub tactical_scale: Option<u32>,
    /// Legend mapping uppercase glyphs to feature definitions.
    #[serde(default)]
    pub legend: Option<HashMap<char, LegendEntry>>,
}

fn default_room_size() -> (u32, u32) {
    (1, 1)
}

fn default_keeper_awareness_modifier() -> f64 {
    1.0
}

// ═══════════════════════════════════════════════════════════
// Hierarchical world graph (Story 23-3)
// ═══════════════════════════════════════════════════════════

/// Terrain type for graph edges.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum Terrain {
    /// Established roads and highways between locations.
    #[default]
    Road,
    /// Untamed terrain — forests, plains, wastelands.
    Wilderness,
    /// Rivers, seas, and waterways.
    Water,
    /// Caves, tunnels, and subterranean passages.
    Underground,
}


/// A node in the world graph — a major location (city, region, landmark).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldGraphNode {
    /// Unique node identifier (slug).
    pub id: String,
    /// Display name.
    pub name: String,
    /// Optional description for context/narration.
    #[serde(default)]
    pub description: String,
}

/// An edge between two world graph nodes.
///
/// Danger semantics: 0 = fast travel ("you arrive"), >0 = story-generating scene.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    /// Source node ID.
    pub from: String,
    /// Destination node ID.
    pub to: String,
    /// Danger level: 0 = fast travel, >0 = story-generating encounter.
    pub danger: u32,
    /// Terrain type (defaults to road).
    #[serde(default)]
    pub terrain: Terrain,
    /// Travel distance in abstract units (affects travel time / number of beats).
    #[serde(default = "default_edge_distance")]
    pub distance: u32,
    /// Optional encounter table key for story-generating edges.
    #[serde(default)]
    pub encounter_table_key: Option<String>,
}

fn default_edge_distance() -> u32 {
    1
}

impl GraphEdge {
    /// Whether this edge represents fast travel (danger == 0).
    pub fn is_fast_travel(&self) -> bool {
        self.danger == 0
    }

    /// Whether this edge generates a story scene (danger > 0).
    pub fn is_story_generating(&self) -> bool {
        self.danger > 0
    }
}

/// A sub-graph: internal topology for a world graph node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubGraph {
    /// Internal location nodes.
    #[serde(default)]
    pub nodes: Vec<WorldGraphNode>,
    /// Internal edges between sub-nodes.
    #[serde(default)]
    pub edges: Vec<GraphEdge>,
}

/// The top-level world graph: coarse nodes and edges between major locations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldGraph {
    /// Major location nodes.
    #[serde(default)]
    pub nodes: Vec<WorldGraphNode>,
    /// Edges between nodes with danger/terrain/distance.
    #[serde(default)]
    pub edges: Vec<GraphEdge>,
}

impl WorldGraph {
    /// Find a node by its ID.
    pub fn node_by_id(&self, id: &str) -> Option<&WorldGraphNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// Iterate over neighbor node IDs (bidirectional traversal).
    pub fn neighbors<'a>(&'a self, node_id: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.edges.iter().filter_map(move |e| {
            if e.from == node_id {
                Some(e.to.as_str())
            } else if e.to == node_id {
                Some(e.from.as_str())
            } else {
                None
            }
        })
    }

    /// Iterate over edges originating from a node (forward direction only).
    pub fn edges_from<'a>(&'a self, node_id: &'a str) -> impl Iterator<Item = &'a GraphEdge> + 'a {
        self.edges.iter().filter(move |e| e.from == node_id)
    }
}

/// Map and region configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CartographyConfig {
    /// World name.
    #[serde(default)]
    pub world_name: String,
    /// Starting region slug (or starting room ID in room_graph mode).
    #[serde(default)]
    pub starting_region: String,
    /// Map style prompt for image generation.
    #[serde(default)]
    pub map_style: String,
    /// Map resolution in pixels [width, height] (null if not specified).
    #[serde(default)]
    pub map_resolution: Option<[u32; 2]>,
    /// Navigation mode — Region (default) or RoomGraph.
    #[serde(default)]
    pub navigation_mode: NavigationMode,
    /// Regions keyed by slug (used in Region mode).
    #[serde(default)]
    pub regions: HashMap<String, Region>,
    /// Routes between regions (used in Region mode).
    #[serde(default)]
    pub routes: Vec<Route>,
    /// Room definitions (used in RoomGraph mode). `None` for region-based packs.
    #[serde(default)]
    pub rooms: Option<Vec<RoomDef>>,
    /// Hierarchical world graph (used in Hierarchical mode).
    #[serde(default)]
    pub world_graph: Option<WorldGraph>,
    /// Sub-graphs keyed by parent world-graph node ID (used in Hierarchical mode).
    #[serde(default)]
    pub sub_graphs: Option<HashMap<String, SubGraph>>,
}

/// A map region.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Region {
    /// Display name.
    pub name: String,
    /// One-line summary (~10 tokens) for tiered lore retrieval safety net.
    pub summary: String,
    /// Description.
    pub description: String,
    /// Slugs of adjacent regions.
    #[serde(default)]
    pub adjacent: Vec<String>,
    /// Named landmarks (either simple strings or detailed objects).
    #[serde(default)]
    pub landmarks: Vec<Landmark>,
    /// Origin char-creation scene (if any).
    #[serde(default)]
    pub origin: Option<String>,
    /// Rivers passing through (strings or detailed objects).
    #[serde(default)]
    pub rivers: Vec<Landmark>,
    /// Settlements in this region (strings or detailed objects).
    #[serde(default)]
    pub settlements: Vec<Landmark>,
    /// Terrain type (e.g., "elevated_expressway", "coastal_mountain_pass").
    #[serde(default)]
    pub terrain: Option<String>,
    /// Faction controlling this region.
    #[serde(default)]
    pub controlled_by: Option<String>,
    /// Genre-specific region extensions (chase_profile, etc.).
    #[serde(flatten)]
    pub extras: HashMap<String, serde_json::Value>,
}

impl Region {
    /// One-line summary for narrator prompt RAG pipeline.
    pub fn summary(&self) -> &str {
        &self.summary
    }
}

/// A landmark — either a simple name string or a detailed object.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum Landmark {
    /// Simple landmark name.
    Name(String),
    /// Detailed landmark with type and description.
    Detailed {
        /// Landmark name.
        name: String,
        /// Landmark type (crater, shrine, etc.).
        #[serde(rename = "type")]
        landmark_type: String,
        /// Description.
        description: String,
    },
}

impl<'de> Deserialize<'de> for Landmark {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum LandmarkRepr {
            Name(String),
            Detailed {
                name: String,
                #[serde(rename = "type")]
                landmark_type: String,
                description: String,
            },
        }

        match LandmarkRepr::deserialize(deserializer)? {
            LandmarkRepr::Name(s) => Ok(Landmark::Name(s)),
            LandmarkRepr::Detailed {
                name,
                landmark_type,
                description,
            } => Ok(Landmark::Detailed {
                name,
                landmark_type,
                description,
            }),
        }
    }
}

/// A route between regions.
///
/// Supports two formats:
/// - Point-to-point (low_fantasy): from_id, to_id, distance, danger
/// - Waypoint-based (road_warrior): id, waypoints, difficulty
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Route {
    /// Route name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Route slug (waypoint format).
    #[serde(default)]
    pub id: Option<String>,
    /// Source region slug (point-to-point format).
    #[serde(default)]
    pub from_id: Option<String>,
    /// Destination region slug (point-to-point format).
    #[serde(default)]
    pub to_id: Option<String>,
    /// Travel distance category (point-to-point format).
    #[serde(default)]
    pub distance: Option<String>,
    /// Danger level (point-to-point format).
    #[serde(default)]
    pub danger: Option<String>,
    /// Ordered waypoints (waypoint format).
    #[serde(default)]
    pub waypoints: Vec<String>,
    /// Difficulty level (waypoint format).
    #[serde(default)]
    pub difficulty: Option<String>,
    /// Genre-specific route extensions (faction_crossings, etc.).
    #[serde(flatten)]
    pub extras: HashMap<String, serde_json::Value>,
}

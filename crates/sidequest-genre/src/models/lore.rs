//! Genre-level and world-level lore types from `lore.yaml`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Genre-level lore.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Lore {
    /// World name (may be empty at genre level).
    pub world_name: String,
    /// History text.
    pub history: String,
    /// Geography description (may be empty).
    pub geography: String,
    /// Cosmology / religion / metaphysics.
    pub cosmology: String,
    /// Factions (some genre-level packs include factions at the top level).
    #[serde(default)]
    pub factions: Vec<Faction>,
    /// Genre-specific lore extensions (setting_anchor, etc.).
    #[serde(flatten)]
    pub extras: HashMap<String, serde_json::Value>,
}

/// World-specific lore with factions.
///
/// Accepts both the low_fantasy format (world_name/history/geography/cosmology)
/// and the road_warrior format (setting/faction_relations/daily_life).
/// Genre-specific fields land in `extras` for AI prompt injection.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorldLore {
    /// World name (low_fantasy format).
    #[serde(default)]
    pub world_name: Option<String>,
    /// History text (low_fantasy format).
    #[serde(default)]
    pub history: Option<String>,
    /// Geography description (low_fantasy format).
    #[serde(default)]
    pub geography: Option<String>,
    /// Cosmology text (low_fantasy format).
    #[serde(default)]
    pub cosmology: Option<String>,
    /// Political factions (simple format — name/description/disposition).
    #[serde(default)]
    pub factions: Vec<Faction>,
    /// Genre-specific lore extensions (setting, faction_relations, daily_life, etc.).
    #[serde(flatten)]
    pub extras: HashMap<String, serde_json::Value>,
}

/// A political or social faction.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Faction {
    /// Faction name.
    pub name: String,
    /// Description of the faction.
    pub description: String,
    /// Starting disposition toward the player.
    #[serde(default)]
    pub disposition: String,
    /// Genre-specific faction extensions.
    #[serde(flatten)]
    pub extras: HashMap<String, serde_json::Value>,
}

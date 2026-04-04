//! Historical legend types from `legends.yaml`.

use serde::{Deserialize, Serialize};

/// A historical legend.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Legend {
    /// Legend name.
    pub name: String,
    /// Summary text (also accepts "description" from road_warrior format).
    #[serde(default, alias = "description")]
    pub summary: String,
    /// Historical era.
    #[serde(default)]
    pub era: String,
    /// Cultures affected.
    #[serde(default)]
    pub affected_cultures: Vec<String>,
    /// Impact on those cultures.
    #[serde(default)]
    pub cultural_impact: String,
    /// Grudges between factions.
    #[serde(default)]
    pub faction_grudges: Vec<FactionGrudge>,
    /// Knowledge lost due to this event.
    #[serde(default)]
    pub lost_arts: Vec<String>,
    /// Monuments related to this legend.
    #[serde(default)]
    pub monuments: Vec<String>,
    /// Physical scars on the landscape from this event.
    #[serde(default)]
    pub terrain_scars: Vec<TerrainScar>,
}

/// A physical scar on the landscape from a historical event.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TerrainScar {
    /// Scar name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Region slug (may be empty).
    #[serde(default)]
    pub region: String,
    /// Scar type (crater, dead_zone, etc.).
    #[serde(rename = "type")]
    pub scar_type: String,
}

/// A grudge between two factions from a historical event.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactionGrudge {
    /// The faction holding the grudge.
    pub from: String,
    /// The faction being resented.
    pub to: String,
    /// Why the grudge exists.
    pub reason: String,
}

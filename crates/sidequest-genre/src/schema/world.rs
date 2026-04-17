use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// World-tier content. Named instances: funnels, factions, POIs, leitmotif
/// bindings, world-specific image prompt additions.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldContent {
    /// Funnel entries that collapse Jungian/RPG role pairs into named archetypes.
    #[serde(default)]
    pub funnels: Vec<FunnelEntry>,
    /// Named factions present in this world.
    #[serde(default)]
    pub factions: Vec<FactionEntry>,
    /// Leitmotif bindings: named entity → music track filename.
    #[serde(default)]
    pub leitmotifs: HashMap<String, String>,
    /// Additional image prompt text appended for this world.
    #[serde(default)]
    pub additional_image_prompt: Option<String>,
}

/// A funnel collapses one or more Jungian/RPG role pairs into a named archetype.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FunnelEntry {
    /// Display name for this archetype (e.g. "Thornwall Mender").
    pub name: String,
    /// List of [jungian_id, rpg_role_id] pairs this funnel absorbs.
    pub absorbs: Vec<[String; 2]>,
    /// Faction this archetype belongs to, if any.
    #[serde(default)]
    pub faction: Option<String>,
    /// Lore description for narrator context.
    #[serde(default)]
    pub lore: String,
    /// Cultural standing of this archetype (e.g. "respected", "feared").
    #[serde(default)]
    pub cultural_status: Option<String>,
}

/// A named faction within the world.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactionEntry {
    /// Display name for this faction.
    pub name: String,
    /// Lore description for narrator context.
    #[serde(default)]
    pub description: String,
}

use serde::{Deserialize, Serialize};

/// Global-tier content. Genre-agnostic structural primitives.
/// No proper nouns, no lore, no culture-specific flavor.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlobalContent {
    /// Jungian archetype axis entries (e.g. Hero, Shadow, Trickster).
    #[serde(default)]
    pub jungian_axis: Vec<JungianAxisEntry>,
    /// RPG role axis entries (e.g. tank, healer, striker).
    #[serde(default)]
    pub rpg_role_axis: Vec<RpgRoleAxisEntry>,
    /// NPC role axis entries (e.g. mentor, antagonist, ally).
    #[serde(default)]
    pub npc_role_axis: Vec<NpcRoleAxisEntry>,
}

/// A single entry on the Jungian archetype axis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JungianAxisEntry {
    /// Unique identifier for this archetype (e.g. "hero").
    pub id: String,
    /// Core motivational drive for this archetype.
    #[serde(default)]
    pub drive: String,
    /// OCEAN personality tendencies as free-form YAML.
    #[serde(default)]
    pub ocean_tendencies: serde_yaml::Value,
    /// Stat names that this archetype is naturally drawn toward.
    #[serde(default)]
    pub stat_affinity: Vec<String>,
}

/// A single entry on the RPG role axis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RpgRoleAxisEntry {
    /// Unique identifier for this role (e.g. "healer").
    pub id: String,
    /// Stat names that this role favors.
    #[serde(default)]
    pub stat_affinity: Vec<String>,
    /// Combat function label (e.g. "support", "frontline").
    #[serde(default)]
    pub combat_function: String,
}

/// A single entry on the NPC role axis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcRoleAxisEntry {
    /// Unique identifier for this NPC role (e.g. "mentor").
    pub id: String,
    /// Narrative function label (e.g. "guide", "gatekeeper").
    #[serde(default)]
    pub narrative_function: String,
    /// If true, skip the enrichment pass for this role.
    #[serde(default)]
    pub skip_enrichment: bool,
}

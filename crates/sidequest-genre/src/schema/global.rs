use serde::{Deserialize, Serialize};

/// Global-tier content. Genre-agnostic structural primitives.
/// No proper nouns, no lore, no culture-specific flavor.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlobalContent {
    #[serde(default)]
    pub jungian_axis: Vec<JungianAxisEntry>,
    #[serde(default)]
    pub rpg_role_axis: Vec<RpgRoleAxisEntry>,
    #[serde(default)]
    pub npc_role_axis: Vec<NpcRoleAxisEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JungianAxisEntry {
    pub id: String,
    #[serde(default)]
    pub drive: String,
    #[serde(default)]
    pub ocean_tendencies: serde_yaml::Value,
    #[serde(default)]
    pub stat_affinity: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RpgRoleAxisEntry {
    pub id: String,
    #[serde(default)]
    pub stat_affinity: Vec<String>,
    #[serde(default)]
    pub combat_function: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpcRoleAxisEntry {
    pub id: String,
    #[serde(default)]
    pub narrative_function: String,
    #[serde(default)]
    pub skip_enrichment: bool,
}

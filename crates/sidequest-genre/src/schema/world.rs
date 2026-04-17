use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// World-tier content. Named instances: funnels, factions, POIs, leitmotif
/// bindings, world-specific image prompt additions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldContent {
    #[serde(default)]
    pub funnels: Vec<FunnelEntry>,
    #[serde(default)]
    pub factions: Vec<FactionEntry>,
    #[serde(default)]
    pub leitmotifs: HashMap<String, String>,
    #[serde(default)]
    pub additional_image_prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FunnelEntry {
    pub name: String,
    pub absorbs: Vec<[String; 2]>,
    #[serde(default)]
    pub faction: Option<String>,
    #[serde(default)]
    pub lore: String,
    #[serde(default)]
    pub cultural_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactionEntry {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Genre-tier content. Patterns, constraints, fallback *shapes* — never named
/// instances. No funnels, no POIs, no faction names, no leitmotifs tied to a
/// specific named thing. Enforced by absence of fields for those concerns.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenreContent {
    #[serde(default)]
    pub valid_pairings: HashMap<String, Vec<[String; 2]>>,
    #[serde(default)]
    pub genre_flavor: HashMap<String, GenreFlavorEntry>,
    #[serde(default)]
    pub stat_name_mapping: HashMap<String, String>,
    #[serde(default)]
    pub ambient_music_library: Vec<String>,
    #[serde(default)]
    pub music_library: Vec<String>,
    #[serde(default)]
    pub lora_checkpoint: Option<String>,
    #[serde(default)]
    pub base_style_prompt: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenreFlavorEntry {
    #[serde(default)]
    pub speech_pattern: String,
    #[serde(default)]
    pub equipment_tendency: String,
    #[serde(default)]
    pub visual_cues: String,
    #[serde(default)]
    pub fallback_name: Option<String>,
}

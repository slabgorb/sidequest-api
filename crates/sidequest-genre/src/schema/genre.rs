use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Pairing weight class for genre-level archetype pairings.
///
/// Used as the key in `GenreContent.valid_pairings` so that typos in YAML
/// (e.g., `rrare:`) fail at load time instead of silently dropping entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PairingTier {
    /// Frequently expected pairing in this genre.
    Common,
    /// Occasionally valid pairing in this genre.
    Uncommon,
    /// Atypical but permitted pairing in this genre.
    Rare,
    /// Explicitly disallowed pairing in this genre.
    Forbidden,
}

/// Genre-tier content. Patterns, constraints, fallback *shapes* — never named
/// instances. No funnels, no POIs, no faction names, no leitmotifs tied to a
/// specific named thing. Enforced by absence of fields for those concerns.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenreContent {
    /// Valid Jungian/RPG role pairings: archetype id → list of [jungian, rpg_role] pairs.
    #[serde(default)]
    pub valid_pairings: HashMap<PairingTier, Vec<[String; 2]>>,
    /// Genre flavor overrides keyed by archetype id.
    #[serde(default)]
    pub genre_flavor: HashMap<String, GenreFlavorEntry>,
    /// Mapping from canonical stat names to genre-specific display names.
    #[serde(default)]
    pub stat_name_mapping: HashMap<String, String>,
    /// Ambient music track filenames available in this genre.
    #[serde(default)]
    pub ambient_music_library: Vec<String>,
    /// Scored music track filenames available in this genre.
    #[serde(default)]
    pub music_library: Vec<String>,
    /// LoRA checkpoint filename for image generation.
    #[serde(default)]
    pub lora_checkpoint: Option<String>,
    /// Base style prompt prepended to all image generation requests.
    #[serde(default)]
    pub base_style_prompt: Option<String>,
}

/// Flavor overrides for a single archetype within this genre.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenreFlavorEntry {
    /// Speech pattern description for narrator guidance.
    #[serde(default)]
    pub speech_pattern: String,
    /// Typical equipment style description.
    #[serde(default)]
    pub equipment_tendency: String,
    /// Visual description cues for image generation.
    #[serde(default)]
    pub visual_cues: String,
    /// Optional display name fallback when no culture reskin applies.
    #[serde(default)]
    pub fallback_name: Option<String>,
}

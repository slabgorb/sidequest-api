use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Culture-tier content. Terminal flavor pass: names, speech, visual cues,
/// disposition, scenario variants. Never structural rules (those are Genre
/// or Global).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CultureContent {
    /// Unique identifier for this culture (e.g. "thornwall").
    pub id: String,
    /// Human-readable display name (e.g. "Thornwall").
    pub display_name: String,
    /// What this culture entry represents in the world model.
    pub represents: CultureRepresents,
    /// Optional binding to a Markov corpus for name generation.
    #[serde(default)]
    pub corpus_binding: Option<CorpusBinding>,
    /// Default disposition lean for NPCs of this culture (e.g. "neutral").
    #[serde(default)]
    pub default_disposition_lean: Option<String>,
    /// Archetype reskins keyed by funnel display name.
    #[serde(default)]
    pub reskins: HashMap<String, ArchetypeReskin>,
    /// Speech overrides keyed by context key.
    #[serde(default)]
    pub speech: HashMap<String, String>,
    /// Disposition toward other cultures or factions keyed by their id.
    #[serde(default)]
    pub disposition_toward: HashMap<String, String>,
}

/// What a culture entry represents in the world model.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CultureRepresents {
    /// A racial or species grouping.
    Race,
    /// A named faction.
    #[default]
    Faction,
    /// A class or profession.
    Class,
    /// A composite culture spanning multiple axes.
    Composite,
}

/// Binding to one or two Markov corpora for procedural name generation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusBinding {
    /// Primary corpus name.
    pub primary: String,
    /// Optional secondary corpus name for blended generation.
    #[serde(default)]
    pub secondary: Option<String>,
}

/// Flavor overrides for a single named archetype within this culture.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchetypeReskin {
    /// Override display name for this archetype in this culture.
    #[serde(default)]
    pub display_name: Option<String>,
    /// Override speech pattern description for narrator guidance.
    #[serde(default)]
    pub speech_pattern: Option<String>,
    /// Visual description cues for image generation.
    #[serde(default)]
    pub visual_cues: Vec<String>,
}

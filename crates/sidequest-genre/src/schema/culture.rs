use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Culture-tier content. Terminal flavor pass: names, speech, visual cues,
/// disposition, scenario variants. Never structural rules (those are Genre
/// or Global).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CultureContent {
    pub id: String,
    pub display_name: String,
    pub represents: CultureRepresents,
    #[serde(default)]
    pub corpus_binding: Option<CorpusBinding>,
    #[serde(default)]
    pub default_disposition_lean: Option<String>,
    #[serde(default)]
    pub reskins: HashMap<String, ArchetypeReskin>,
    #[serde(default)]
    pub speech: HashMap<String, String>,
    #[serde(default)]
    pub disposition_toward: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CultureRepresents {
    Race,
    Faction,
    Class,
    Composite,
}

impl Default for CultureRepresents {
    fn default() -> Self {
        Self::Faction
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusBinding {
    pub primary: String,
    #[serde(default)]
    pub secondary: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchetypeReskin {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub speech_pattern: Option<String>,
    #[serde(default)]
    pub visual_cues: Vec<String>,
}

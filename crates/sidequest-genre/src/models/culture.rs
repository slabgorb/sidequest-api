//! Name generation culture types from `cultures.yaml`.

use serde::{Deserialize, Serialize};
use sidequest_protocol::NonBlankString;
use std::collections::HashMap;

/// A name-generation culture.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Culture {
    /// Culture name.
    pub name: NonBlankString,
    /// Description.
    pub description: String,
    /// Named generation slots.
    pub slots: HashMap<String, CultureSlot>,
    /// Person name patterns using slot references.
    pub person_patterns: Vec<String>,
    /// Place name patterns using slot references.
    pub place_patterns: Vec<String>,
}

/// A name-generation slot — corpus-based, word-list-based, or file-based.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CultureSlot {
    /// Markov corpus references (for generated names).
    #[serde(default)]
    pub corpora: Option<Vec<CorpusRef>>,
    /// Markov chain lookback depth.
    #[serde(default)]
    pub lookback: Option<u32>,
    /// Fixed word list (for deterministic slots).
    #[serde(default)]
    pub word_list: Option<Vec<String>>,
    /// Plain text file of names (one per line) in corpus/.
    /// Used by real-world-name genres (pulp_noir, victoria) instead of Markov.
    #[serde(default)]
    pub names_file: Option<String>,
    /// Files containing words to reject from generation.
    #[serde(default)]
    pub reject_files: Vec<String>,
}

/// A reference to a Markov corpus file.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CorpusRef {
    /// Corpus filename.
    pub corpus: String,
    /// Blending weight.
    pub weight: f64,
}

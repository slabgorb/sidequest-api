//! Core lore types: `LoreStore`, `LoreFragment`, `LoreCategory`, `LoreSource`.
//!
//! Story 11-1 / 11-2: Defines the indexed-collection types that represent
//! a single piece of world-building knowledge plus the store that manages them.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::similarity::cosine_similarity;

// ---------------------------------------------------------------------------
// LoreStore — in-memory indexed collection of LoreFragments (story 11-2)
// ---------------------------------------------------------------------------

/// In-memory indexed collection of [`LoreFragment`]s with category/keyword
/// queries and token-budget tracking.
#[derive(Default)]
pub struct LoreStore {
    pub(super) fragments: HashMap<String, LoreFragment>,
}

impl LoreStore {
    /// Create an empty store.
    pub fn new() -> Self {
        Self {
            fragments: HashMap::new(),
        }
    }

    /// Insert a fragment into the store.
    /// Returns `Err` if a fragment with the same id already exists.
    pub fn add(&mut self, fragment: LoreFragment) -> Result<(), String> {
        if self.fragments.contains_key(fragment.id()) {
            return Err(format!("duplicate id: {}", fragment.id()));
        }
        self.fragments.insert(fragment.id().to_string(), fragment);
        Ok(())
    }

    /// Return all fragments matching the given category.
    pub fn query_by_category(&self, category: &LoreCategory) -> Vec<&LoreFragment> {
        self.fragments
            .values()
            .filter(|f| f.category() == category)
            .collect()
    }

    /// Return all fragments whose content contains `keyword` (case-insensitive).
    pub fn query_by_keyword(&self, keyword: &str) -> Vec<&LoreFragment> {
        let keyword_lower = keyword.to_lowercase();
        self.fragments
            .values()
            .filter(|f| f.content().to_lowercase().contains(&keyword_lower))
            .collect()
    }

    /// Sum of token estimates across all stored fragments.
    pub fn total_tokens(&self) -> usize {
        self.fragments.values().map(|f| f.token_estimate()).sum()
    }

    /// Number of stored fragments.
    pub fn len(&self) -> usize {
        self.fragments.len()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.fragments.is_empty()
    }

    /// Attach an embedding vector to an existing fragment by id.
    /// Returns `Err` if the fragment does not exist.
    pub fn set_embedding(&mut self, id: &str, embedding: Vec<f32>) -> Result<(), String> {
        let frag = self
            .fragments
            .get_mut(id)
            .ok_or_else(|| format!("fragment not found: {id}"))?;
        frag.embedding = Some(embedding);
        Ok(())
    }

    /// Count of fragments that have embedding vectors attached.
    pub fn fragments_with_embeddings_count(&self) -> usize {
        self.fragments
            .values()
            .filter(|f| f.embedding().is_some())
            .count()
    }

    /// Return the top-k fragments most similar to `query_embedding`, sorted by
    /// descending cosine similarity. Fragments without embeddings are skipped.
    pub fn query_by_similarity(
        &self,
        query_embedding: &[f32],
        top_k: usize,
    ) -> Vec<(&LoreFragment, f32)> {
        let mut scored: Vec<(&LoreFragment, f32)> = self
            .fragments
            .values()
            .filter_map(|f| {
                f.embedding()
                    .map(|emb| (f, cosine_similarity(query_embedding, emb)))
            })
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(top_k);
        scored
    }
}

/// Category of a lore fragment.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum LoreCategory {
    /// Historical events and timelines.
    History,
    /// Places, terrain, and spatial relationships.
    Geography,
    /// Faction lore and political structures.
    Faction,
    /// Notable characters and their backgrounds.
    Character,
    /// Significant items, artifacts, or equipment.
    Item,
    /// Specific in-game events.
    Event,
    /// Languages, dialects, and naming conventions.
    Language,
    /// User-defined category.
    Custom(String),
}

/// Where a lore fragment originated.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum LoreSource {
    /// Loaded from a genre pack YAML file.
    GenrePack,
    /// Created during character creation.
    CharacterCreation,
    /// Generated from an in-game event.
    GameEvent,
}

/// A single indexed piece of world-building knowledge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoreFragment {
    id: String,
    category: LoreCategory,
    content: String,
    token_estimate: usize,
    source: LoreSource,
    turn_created: Option<u64>,
    metadata: HashMap<String, String>,
    /// Optional embedding vector for semantic similarity search (story 11-6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    embedding: Option<Vec<f32>>,
}

impl LoreFragment {
    /// Create a new lore fragment. Token estimate is auto-computed from content.
    pub fn new(
        id: String,
        category: LoreCategory,
        content: String,
        source: LoreSource,
        turn_created: Option<u64>,
        metadata: HashMap<String, String>,
    ) -> Self {
        let token_estimate = content.len().div_ceil(4);
        Self {
            id,
            category,
            content,
            token_estimate,
            source,
            turn_created,
            metadata,
            embedding: None,
        }
    }

    /// The fragment's unique identifier.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The category of this lore fragment.
    pub fn category(&self) -> &LoreCategory {
        &self.category
    }

    /// The narrative content.
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Estimated token count (~4 chars per token).
    pub fn token_estimate(&self) -> usize {
        self.token_estimate
    }

    /// Where this fragment originated.
    pub fn source(&self) -> &LoreSource {
        &self.source
    }

    /// The turn number when this fragment was created, if any.
    pub fn turn_created(&self) -> Option<u64> {
        self.turn_created
    }

    /// Arbitrary key-value metadata.
    pub fn metadata(&self) -> &HashMap<String, String> {
        &self.metadata
    }

    /// The optional embedding vector for semantic search.
    pub fn embedding(&self) -> Option<&[f32]> {
        self.embedding.as_deref()
    }

    /// Builder-style setter: attach an embedding vector for semantic search.
    pub fn with_embedding(mut self, embedding: Vec<f32>) -> Self {
        self.embedding = Some(embedding);
        self
    }
}

/// Display-friendly label for a [`LoreCategory`].
impl std::fmt::Display for LoreCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoreCategory::History => write!(f, "History"),
            LoreCategory::Geography => write!(f, "Geography"),
            LoreCategory::Faction => write!(f, "Faction"),
            LoreCategory::Character => write!(f, "Character"),
            LoreCategory::Item => write!(f, "Item"),
            LoreCategory::Event => write!(f, "Event"),
            LoreCategory::Language => write!(f, "Language"),
            LoreCategory::Custom(s) => write!(f, "{s}"),
        }
    }
}

/// Convert a protocol [`sidequest_protocol::FactCategory`] into a [`LoreCategory`].
///
/// This bridges the narrator's structured footnote output (which uses `FactCategory`)
/// into the LoreStore's internal category taxonomy. Used when routing footnote
/// discoveries through the RAG pipeline.
impl From<sidequest_protocol::FactCategory> for LoreCategory {
    fn from(cat: sidequest_protocol::FactCategory) -> Self {
        match cat {
            sidequest_protocol::FactCategory::Lore => LoreCategory::History,
            sidequest_protocol::FactCategory::Place => LoreCategory::Geography,
            sidequest_protocol::FactCategory::Person => LoreCategory::Character,
            sidequest_protocol::FactCategory::Quest => LoreCategory::Event,
            sidequest_protocol::FactCategory::Ability => LoreCategory::Item,
            // FactCategory is #[non_exhaustive] — future variants default to Event.
            _ => LoreCategory::Event,
        }
    }
}

//! Lore fragment model — indexed narrative facts with category, token estimate,
//! and metadata.
//!
//! Story 11-1: Defines the `LoreFragment` type that represents a single piece
//! of world-building knowledge (history, geography, factions, etc.) with an
//! estimated token count for context-window budgeting.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn sample_metadata() -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("author".to_string(), "narrator".to_string());
        m.insert("region".to_string(), "flickering_reach".to_string());
        m
    }

    fn sample_fragment() -> LoreFragment {
        LoreFragment::new(
            "lore-001".to_string(),
            LoreCategory::History,
            "The Flickering Reach was once a thriving trade hub.".to_string(),
            LoreSource::GenrePack,
            Some(5),
            sample_metadata(),
        )
    }

    // === Constructor and field storage ===

    #[test]
    fn new_stores_id() {
        let frag = sample_fragment();
        assert_eq!(frag.id(), "lore-001");
    }

    #[test]
    fn new_stores_category() {
        let frag = sample_fragment();
        assert_eq!(frag.category(), &LoreCategory::History);
    }

    #[test]
    fn new_stores_content() {
        let frag = sample_fragment();
        assert_eq!(
            frag.content(),
            "The Flickering Reach was once a thriving trade hub."
        );
    }

    #[test]
    fn new_stores_source() {
        let frag = sample_fragment();
        assert_eq!(frag.source(), &LoreSource::GenrePack);
    }

    #[test]
    fn new_stores_turn_created() {
        let frag = sample_fragment();
        assert_eq!(frag.turn_created(), Some(5));
    }

    #[test]
    fn new_stores_metadata() {
        let frag = sample_fragment();
        assert_eq!(frag.metadata().get("author").unwrap(), "narrator");
        assert_eq!(frag.metadata().get("region").unwrap(), "flickering_reach");
    }

    #[test]
    fn new_with_none_turn_created() {
        let frag = LoreFragment::new(
            "lore-002".to_string(),
            LoreCategory::Geography,
            "Mountains to the north.".to_string(),
            LoreSource::GameEvent,
            None,
            HashMap::new(),
        );
        assert_eq!(frag.turn_created(), None);
    }

    // === Token estimation ===

    #[test]
    fn token_estimate_100_chars() {
        // 100 chars ÷ 4 = 25 tokens
        let content = "a".repeat(100);
        let frag = LoreFragment::new(
            "tok-100".to_string(),
            LoreCategory::Event,
            content,
            LoreSource::GameEvent,
            None,
            HashMap::new(),
        );
        assert_eq!(frag.token_estimate(), 25);
    }

    #[test]
    fn token_estimate_short_string() {
        // 7 chars → ceil(7/4) = 2 tokens
        let frag = LoreFragment::new(
            "tok-short".to_string(),
            LoreCategory::Item,
            "hello!!".to_string(),
            LoreSource::GenrePack,
            None,
            HashMap::new(),
        );
        assert_eq!(frag.token_estimate(), 2);
    }

    #[test]
    fn token_estimate_empty_string() {
        let frag = LoreFragment::new(
            "tok-empty".to_string(),
            LoreCategory::Language,
            String::new(),
            LoreSource::CharacterCreation,
            None,
            HashMap::new(),
        );
        assert_eq!(frag.token_estimate(), 0);
    }

    #[test]
    fn token_estimate_one_char() {
        let frag = LoreFragment::new(
            "tok-1".to_string(),
            LoreCategory::Character,
            "x".to_string(),
            LoreSource::GenrePack,
            None,
            HashMap::new(),
        );
        assert_eq!(frag.token_estimate(), 1);
    }

    #[test]
    fn constructor_auto_computes_token_estimate() {
        let frag = sample_fragment();
        // "The Flickering Reach was once a thriving trade hub." = 52 chars
        // 52 / 4 = 13 tokens
        assert_eq!(frag.token_estimate(), 13);
    }

    // === LoreCategory variants ===

    #[test]
    fn all_fixed_categories_are_distinct() {
        let categories = vec![
            LoreCategory::History,
            LoreCategory::Geography,
            LoreCategory::Faction,
            LoreCategory::Character,
            LoreCategory::Item,
            LoreCategory::Event,
            LoreCategory::Language,
        ];
        for (i, a) in categories.iter().enumerate() {
            for (j, b) in categories.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b);
                }
            }
        }
    }

    #[test]
    fn custom_category_holds_value() {
        let cat = LoreCategory::Custom("Prophecy".to_string());
        if let LoreCategory::Custom(ref s) = cat {
            assert_eq!(s, "Prophecy");
        } else {
            panic!("Expected Custom variant");
        }
    }

    #[test]
    fn custom_categories_with_different_values_are_distinct() {
        let a = LoreCategory::Custom("Prophecy".to_string());
        let b = LoreCategory::Custom("Religion".to_string());
        assert_ne!(a, b);
    }

    // === LoreSource variants ===

    #[test]
    fn all_sources_are_distinct() {
        let sources = vec![
            LoreSource::GenrePack,
            LoreSource::CharacterCreation,
            LoreSource::GameEvent,
        ];
        for (i, a) in sources.iter().enumerate() {
            for (j, b) in sources.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b);
                }
            }
        }
    }

    // === Serde round-trip ===

    #[test]
    fn serde_json_round_trip() {
        let frag = sample_fragment();
        let json = serde_json::to_string(&frag).expect("serialize");
        let restored: LoreFragment = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.id(), frag.id());
        assert_eq!(restored.category(), frag.category());
        assert_eq!(restored.content(), frag.content());
        assert_eq!(restored.token_estimate(), frag.token_estimate());
        assert_eq!(restored.source(), frag.source());
        assert_eq!(restored.turn_created(), frag.turn_created());
        assert_eq!(restored.metadata(), frag.metadata());
    }

    #[test]
    fn serde_round_trip_custom_category() {
        let frag = LoreFragment::new(
            "custom-001".to_string(),
            LoreCategory::Custom("Prophecy".to_string()),
            "The chosen one will rise.".to_string(),
            LoreSource::CharacterCreation,
            Some(10),
            HashMap::new(),
        );
        let json = serde_json::to_string(&frag).expect("serialize");
        let restored: LoreFragment = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.category(), &LoreCategory::Custom("Prophecy".to_string()));
    }

    #[test]
    fn serde_round_trip_with_metadata() {
        let frag = sample_fragment();
        let json = serde_json::to_string(&frag).expect("serialize");
        let restored: LoreFragment = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.metadata().len(), 2);
        assert_eq!(restored.metadata().get("author").unwrap(), "narrator");
    }

    // === Metadata ===

    #[test]
    fn empty_metadata_is_valid() {
        let frag = LoreFragment::new(
            "meta-empty".to_string(),
            LoreCategory::Faction,
            "Some faction lore.".to_string(),
            LoreSource::GenrePack,
            None,
            HashMap::new(),
        );
        assert!(frag.metadata().is_empty());
    }

    #[test]
    fn metadata_supports_arbitrary_keys() {
        let mut meta = HashMap::new();
        meta.insert("custom_key".to_string(), "custom_value".to_string());
        meta.insert("another".to_string(), "entry".to_string());
        meta.insert("number".to_string(), "42".to_string());

        let frag = LoreFragment::new(
            "meta-arb".to_string(),
            LoreCategory::Item,
            "A mysterious artifact.".to_string(),
            LoreSource::GameEvent,
            Some(3),
            meta,
        );
        assert_eq!(frag.metadata().len(), 3);
        assert_eq!(frag.metadata().get("number").unwrap(), "42");
    }
}

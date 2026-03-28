//! Conlang (constructed language) module — morpheme glossary for genre packs.
//!
//! Provides a schema for defining morphemes (word fragments) with meaning,
//! pronunciation hints, and categories, organized into per-language glossaries.

use serde::{Deserialize, Serialize};

/// The grammatical category of a morpheme.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MorphemeCategory {
    /// A prefix morpheme (attached before a root).
    Prefix,
    /// A suffix morpheme (attached after a root).
    Suffix,
    /// A root morpheme (the core meaning-bearing unit).
    Root,
    /// A standalone particle (e.g., conjunctions, articles).
    Particle,
}

/// A single morpheme in a constructed language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Morpheme {
    /// The word fragment, e.g., "zar".
    pub morpheme: String,
    /// The meaning of the morpheme, e.g., "fire".
    pub meaning: String,
    /// Optional pronunciation hint, e.g., "zahr".
    pub pronunciation_hint: Option<String>,
    /// The grammatical category.
    pub category: MorphemeCategory,
    /// Which constructed language this belongs to.
    pub language_id: String,
}

/// A collection of morphemes for a single constructed language.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MorphemeGlossary {
    /// Unique identifier for the language.
    pub language_id: String,
    /// Human-readable name for the language.
    pub language_name: String,
    morphemes: Vec<Morpheme>,
}

impl MorphemeGlossary {
    /// Create an empty glossary for the given language.
    pub fn new(language_id: impl Into<String>, language_name: impl Into<String>) -> Self {
        Self {
            language_id: language_id.into(),
            language_name: language_name.into(),
            morphemes: Vec::new(),
        }
    }

    /// Add a morpheme to the glossary.
    pub fn add(&mut self, morpheme: Morpheme) {
        self.morphemes.push(morpheme);
    }

    /// Look up a morpheme by its string form.
    pub fn lookup(&self, morpheme_str: &str) -> Option<&Morpheme> {
        self.morphemes.iter().find(|m| m.morpheme == morpheme_str)
    }

    /// Return all morphemes matching the given category.
    pub fn by_category(&self, category: MorphemeCategory) -> Vec<&Morpheme> {
        self.morphemes.iter().filter(|m| m.category == category).collect()
    }

    /// Shorthand for `by_category(MorphemeCategory::Root)`.
    pub fn roots(&self) -> Vec<&Morpheme> {
        self.by_category(MorphemeCategory::Root)
    }

    /// Shorthand for `by_category(MorphemeCategory::Prefix)`.
    pub fn prefixes(&self) -> Vec<&Morpheme> {
        self.by_category(MorphemeCategory::Prefix)
    }

    /// Shorthand for `by_category(MorphemeCategory::Suffix)`.
    pub fn suffixes(&self) -> Vec<&Morpheme> {
        self.by_category(MorphemeCategory::Suffix)
    }

    /// Return the number of morphemes in the glossary.
    pub fn len(&self) -> usize {
        self.morphemes.len()
    }

    /// Return true if the glossary contains no morphemes.
    pub fn is_empty(&self) -> bool {
        self.morphemes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_morpheme(morpheme: &str, meaning: &str, category: MorphemeCategory) -> Morpheme {
        Morpheme {
            morpheme: morpheme.to_string(),
            meaning: meaning.to_string(),
            pronunciation_hint: None,
            category,
            language_id: "draconic".to_string(),
        }
    }

    fn sample_morpheme_with_hint(
        morpheme: &str,
        meaning: &str,
        hint: &str,
        category: MorphemeCategory,
    ) -> Morpheme {
        Morpheme {
            morpheme: morpheme.to_string(),
            meaning: meaning.to_string(),
            pronunciation_hint: Some(hint.to_string()),
            category,
            language_id: "draconic".to_string(),
        }
    }

    fn populated_glossary() -> MorphemeGlossary {
        let mut g = MorphemeGlossary::new("draconic", "Draconic");
        g.add(sample_morpheme_with_hint("zar", "fire", "zahr", MorphemeCategory::Root));
        g.add(sample_morpheme("kel", "of the", MorphemeCategory::Particle));
        g.add(sample_morpheme("vor", "great", MorphemeCategory::Prefix));
        g.add(sample_morpheme("thi", "one who", MorphemeCategory::Suffix));
        g.add(sample_morpheme("dra", "dragon", MorphemeCategory::Root));
        g
    }

    // === MorphemeCategory tests ===

    #[test]
    fn category_prefix_exists() {
        let c = MorphemeCategory::Prefix;
        assert_eq!(c, MorphemeCategory::Prefix);
    }

    #[test]
    fn category_suffix_exists() {
        let c = MorphemeCategory::Suffix;
        assert_eq!(c, MorphemeCategory::Suffix);
    }

    #[test]
    fn category_root_exists() {
        let c = MorphemeCategory::Root;
        assert_eq!(c, MorphemeCategory::Root);
    }

    #[test]
    fn category_particle_exists() {
        let c = MorphemeCategory::Particle;
        assert_eq!(c, MorphemeCategory::Particle);
    }

    #[test]
    fn category_serde_round_trip() {
        let categories = vec![
            MorphemeCategory::Prefix,
            MorphemeCategory::Suffix,
            MorphemeCategory::Root,
            MorphemeCategory::Particle,
        ];
        let yaml = serde_yaml::to_string(&categories).unwrap();
        let deserialized: Vec<MorphemeCategory> = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(deserialized, categories);
    }

    #[test]
    fn category_serde_uses_snake_case() {
        let yaml = serde_yaml::to_string(&MorphemeCategory::Prefix).unwrap();
        assert!(yaml.contains("prefix"));
    }

    // === Morpheme tests ===

    #[test]
    fn morpheme_stores_all_fields() {
        let m = sample_morpheme_with_hint("zar", "fire", "zahr", MorphemeCategory::Root);
        assert_eq!(m.morpheme, "zar");
        assert_eq!(m.meaning, "fire");
        assert_eq!(m.pronunciation_hint, Some("zahr".to_string()));
        assert_eq!(m.category, MorphemeCategory::Root);
        assert_eq!(m.language_id, "draconic");
    }

    #[test]
    fn morpheme_without_pronunciation_hint() {
        let m = sample_morpheme("kel", "of the", MorphemeCategory::Particle);
        assert_eq!(m.pronunciation_hint, None);
    }

    #[test]
    fn morpheme_serde_round_trip_with_hint() {
        let m = sample_morpheme_with_hint("zar", "fire", "zahr", MorphemeCategory::Root);
        let yaml = serde_yaml::to_string(&m).unwrap();
        let deserialized: Morpheme = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(deserialized, m);
    }

    #[test]
    fn morpheme_serde_round_trip_without_hint() {
        let m = sample_morpheme("kel", "of the", MorphemeCategory::Particle);
        let yaml = serde_yaml::to_string(&m).unwrap();
        let deserialized: Morpheme = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(deserialized, m);
    }

    // === MorphemeGlossary construction ===

    #[test]
    fn new_glossary_stores_language_id() {
        let g = MorphemeGlossary::new("draconic", "Draconic");
        assert_eq!(g.language_id, "draconic");
    }

    #[test]
    fn new_glossary_stores_language_name() {
        let g = MorphemeGlossary::new("draconic", "Draconic");
        assert_eq!(g.language_name, "Draconic");
    }

    #[test]
    fn new_glossary_is_empty() {
        let g = MorphemeGlossary::new("draconic", "Draconic");
        assert!(g.is_empty());
    }

    #[test]
    fn new_glossary_len_is_zero() {
        let g = MorphemeGlossary::new("draconic", "Draconic");
        assert_eq!(g.len(), 0);
    }

    // === add and len ===

    #[test]
    fn add_increases_len() {
        let mut g = MorphemeGlossary::new("draconic", "Draconic");
        g.add(sample_morpheme("zar", "fire", MorphemeCategory::Root));
        assert_eq!(g.len(), 1);
    }

    #[test]
    fn add_multiple_morphemes() {
        let g = populated_glossary();
        assert_eq!(g.len(), 5);
    }

    #[test]
    fn is_empty_false_after_add() {
        let mut g = MorphemeGlossary::new("draconic", "Draconic");
        g.add(sample_morpheme("zar", "fire", MorphemeCategory::Root));
        assert!(!g.is_empty());
    }

    // === lookup ===

    #[test]
    fn lookup_finds_existing_morpheme() {
        let g = populated_glossary();
        let found = g.lookup("zar");
        assert!(found.is_some());
        assert_eq!(found.unwrap().meaning, "fire");
    }

    #[test]
    fn lookup_returns_none_for_missing() {
        let g = populated_glossary();
        assert!(g.lookup("xyz").is_none());
    }

    // === by_category ===

    #[test]
    fn by_category_roots() {
        let g = populated_glossary();
        let roots = g.by_category(MorphemeCategory::Root);
        assert_eq!(roots.len(), 2);
        assert!(roots.iter().all(|m| m.category == MorphemeCategory::Root));
    }

    #[test]
    fn by_category_prefixes() {
        let g = populated_glossary();
        let prefixes = g.by_category(MorphemeCategory::Prefix);
        assert_eq!(prefixes.len(), 1);
        assert_eq!(prefixes[0].morpheme, "vor");
    }

    #[test]
    fn by_category_empty_result() {
        let mut g = MorphemeGlossary::new("draconic", "Draconic");
        g.add(sample_morpheme("zar", "fire", MorphemeCategory::Root));
        let suffixes = g.by_category(MorphemeCategory::Suffix);
        assert!(suffixes.is_empty());
    }

    // === shorthand methods ===

    #[test]
    fn roots_returns_root_morphemes() {
        let g = populated_glossary();
        let roots = g.roots();
        assert_eq!(roots.len(), 2);
    }

    #[test]
    fn prefixes_returns_prefix_morphemes() {
        let g = populated_glossary();
        let prefixes = g.prefixes();
        assert_eq!(prefixes.len(), 1);
    }

    #[test]
    fn suffixes_returns_suffix_morphemes() {
        let g = populated_glossary();
        let suffixes = g.suffixes();
        assert_eq!(suffixes.len(), 1);
    }

    // === Serde on glossary ===

    #[test]
    fn glossary_serde_round_trip() {
        let g = populated_glossary();
        let yaml = serde_yaml::to_string(&g).unwrap();
        let deserialized: MorphemeGlossary = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(deserialized.language_id, "draconic");
        assert_eq!(deserialized.language_name, "Draconic");
        assert_eq!(deserialized.len(), 5);
    }

    #[test]
    fn glossary_serde_preserves_morpheme_data() {
        let g = populated_glossary();
        let yaml = serde_yaml::to_string(&g).unwrap();
        let deserialized: MorphemeGlossary = serde_yaml::from_str(&yaml).unwrap();
        let zar = deserialized.lookup("zar").unwrap();
        assert_eq!(zar.meaning, "fire");
        assert_eq!(zar.pronunciation_hint, Some("zahr".to_string()));
    }
}

//! Conlang (constructed language) module — morpheme glossary for genre packs.
//!
//! Provides a schema for defining morphemes (word fragments) with meaning,
//! pronunciation hints, and categories, organized into per-language glossaries.

#[allow(unused_imports)]
use rand::{prelude::*, rngs::StdRng, SeedableRng};
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

/// The structural pattern used to assemble a name from morphemes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NamePattern {
    /// A single root morpheme.
    Root,
    /// A prefix followed by a root.
    PrefixRoot,
    /// A root followed by a suffix.
    RootSuffix,
    /// A prefix, root, and suffix.
    PrefixRootSuffix,
}

/// A generated name with morpheme decomposition and gloss.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedName {
    /// The generated name, e.g., "zar'kethi".
    pub name: String,
    /// The meaning gloss, e.g., "fire-walker".
    pub gloss: String,
    /// Pronunciation hint if all component morphemes have one.
    pub pronunciation: Option<String>,
    /// The structural pattern used.
    pub pattern: NamePattern,
    /// Which constructed language this name belongs to.
    pub language_id: String,
}

/// Configuration for name generation.
#[derive(Debug, Clone)]
pub struct NameGenConfig {
    /// How many names to generate.
    pub count: usize,
    /// Seed for deterministic RNG.
    pub seed: u64,
    /// Separator between morphemes, e.g., "'" or "".
    pub separator: String,
    /// Weights for (Root, PrefixRoot, RootSuffix, PrefixRootSuffix).
    pub pattern_weights: (f32, f32, f32, f32),
}

/// A collection of generated names for a language.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NameBank {
    /// Which constructed language these names belong to.
    pub language_id: String,
    /// The generated names.
    pub names: Vec<GeneratedName>,
}

/// Format a name bank as a text block suitable for injection into a narrator prompt.
///
/// Returns a section with a header and one line per name (up to `max_names`),
/// showing name, gloss, and optional pronunciation.
/// Returns an empty string if the bank is empty or `max_names` is zero.
pub fn format_name_bank_for_prompt(bank: &NameBank, max_names: usize) -> String {
    if bank.names.is_empty() || max_names == 0 {
        return String::new();
    }

    let mut lines = Vec::new();
    lines.push(format!("## Names ({})", bank.language_id));

    for name in bank.names.iter().take(max_names) {
        let line = if let Some(ref pron) = name.pronunciation {
            format!("- {} — \"{}\" [{}]", name.name, name.gloss, pron)
        } else {
            format!("- {} — \"{}\"", name.name, name.gloss)
        };
        lines.push(line);
    }

    lines.join("\n")
}

impl NameBank {
    /// Generate a bank of names from a glossary and config.
    pub fn generate(glossary: &MorphemeGlossary, config: &NameGenConfig) -> Self {
        let roots = glossary.roots();
        let prefixes = glossary.prefixes();
        let suffixes = glossary.suffixes();

        if roots.is_empty() {
            return Self {
                language_id: glossary.language_id.clone(),
                names: Vec::new(),
            };
        }

        // Build available patterns with their weights
        let mut available: Vec<(NamePattern, f32)> = Vec::new();
        let (w_r, w_pr, w_rs, w_prs) = config.pattern_weights;
        if w_r > 0.0 {
            available.push((NamePattern::Root, w_r));
        }
        if w_pr > 0.0 && !prefixes.is_empty() {
            available.push((NamePattern::PrefixRoot, w_pr));
        }
        if w_rs > 0.0 && !suffixes.is_empty() {
            available.push((NamePattern::RootSuffix, w_rs));
        }
        if w_prs > 0.0 && !prefixes.is_empty() && !suffixes.is_empty() {
            available.push((NamePattern::PrefixRootSuffix, w_prs));
        }

        if available.is_empty() {
            // Fallback to Root pattern
            available.push((NamePattern::Root, 1.0));
        }

        let mut rng = StdRng::seed_from_u64(config.seed);
        let mut names = Vec::with_capacity(config.count);

        for _ in 0..config.count {
            // Weighted pattern selection
            let total_weight: f32 = available.iter().map(|(_, w)| w).sum();
            let mut roll = rng.random::<f32>() * total_weight;
            let mut pattern = &available[0].0;
            for (p, w) in &available {
                roll -= w;
                if roll <= 0.0 {
                    pattern = p;
                    break;
                }
            }

            let (morphemes, selected_pattern) = match pattern {
                NamePattern::Root => {
                    let root = roots[rng.random_range(0..roots.len())];
                    (vec![root], NamePattern::Root)
                }
                NamePattern::PrefixRoot => {
                    let prefix = prefixes[rng.random_range(0..prefixes.len())];
                    let root = roots[rng.random_range(0..roots.len())];
                    (vec![prefix, root], NamePattern::PrefixRoot)
                }
                NamePattern::RootSuffix => {
                    let root = roots[rng.random_range(0..roots.len())];
                    let suffix = suffixes[rng.random_range(0..suffixes.len())];
                    (vec![root, suffix], NamePattern::RootSuffix)
                }
                NamePattern::PrefixRootSuffix => {
                    let prefix = prefixes[rng.random_range(0..prefixes.len())];
                    let root = roots[rng.random_range(0..roots.len())];
                    let suffix = suffixes[rng.random_range(0..suffixes.len())];
                    (vec![prefix, root, suffix], NamePattern::PrefixRootSuffix)
                }
            };

            let name = morphemes
                .iter()
                .map(|m| m.morpheme.as_str())
                .collect::<Vec<_>>()
                .join(&config.separator);

            let gloss = morphemes
                .iter()
                .map(|m| m.meaning.as_str())
                .collect::<Vec<_>>()
                .join("-");

            let pronunciation = if morphemes.iter().all(|m| m.pronunciation_hint.is_some()) {
                Some(
                    morphemes
                        .iter()
                        .map(|m| m.pronunciation_hint.as_deref().unwrap())
                        .collect::<Vec<_>>()
                        .join(&config.separator),
                )
            } else {
                None
            };

            names.push(GeneratedName {
                name,
                gloss,
                pronunciation,
                pattern: selected_pattern,
                language_id: glossary.language_id.clone(),
            });
        }

        Self {
            language_id: glossary.language_id.clone(),
            names,
        }
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

    // === NamePattern tests ===

    #[test]
    fn name_pattern_all_variants_exist() {
        let _r = NamePattern::Root;
        let _pr = NamePattern::PrefixRoot;
        let _rs = NamePattern::RootSuffix;
        let _prs = NamePattern::PrefixRootSuffix;
    }

    #[test]
    fn name_pattern_serde_round_trip() {
        let patterns = vec![
            NamePattern::Root,
            NamePattern::PrefixRoot,
            NamePattern::RootSuffix,
            NamePattern::PrefixRootSuffix,
        ];
        let yaml = serde_yaml::to_string(&patterns).unwrap();
        let deserialized: Vec<NamePattern> = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(deserialized, patterns);
    }

    #[test]
    fn name_pattern_serde_uses_snake_case() {
        let yaml = serde_yaml::to_string(&NamePattern::PrefixRoot).unwrap();
        assert!(yaml.contains("prefix_root"));
    }

    // === GeneratedName tests ===

    #[test]
    fn generated_name_stores_all_fields() {
        let name = GeneratedName {
            name: "zar'thi".to_string(),
            gloss: "fire-one who".to_string(),
            pronunciation: Some("zahr'thee".to_string()),
            pattern: NamePattern::RootSuffix,
            language_id: "draconic".to_string(),
        };
        assert_eq!(name.name, "zar'thi");
        assert_eq!(name.gloss, "fire-one who");
        assert_eq!(name.pronunciation, Some("zahr'thee".to_string()));
        assert_eq!(name.pattern, NamePattern::RootSuffix);
        assert_eq!(name.language_id, "draconic");
    }

    // === NameGenConfig tests ===

    #[test]
    fn name_gen_config_construction() {
        let config = NameGenConfig {
            count: 10,
            seed: 42,
            separator: "'".to_string(),
            pattern_weights: (1.0, 1.0, 1.0, 1.0),
        };
        assert_eq!(config.count, 10);
        assert_eq!(config.seed, 42);
        assert_eq!(config.separator, "'");
        assert_eq!(config.pattern_weights, (1.0, 1.0, 1.0, 1.0));
    }

    // === Helper for name generation tests ===

    fn glossary_with_hints() -> MorphemeGlossary {
        let mut g = MorphemeGlossary::new("draconic", "Draconic");
        g.add(sample_morpheme_with_hint("zar", "fire", "zahr", MorphemeCategory::Root));
        g.add(sample_morpheme_with_hint("dra", "dragon", "drah", MorphemeCategory::Root));
        g.add(sample_morpheme_with_hint("kel", "stone", "kel", MorphemeCategory::Root));
        g.add(sample_morpheme_with_hint("vor", "great", "vohr", MorphemeCategory::Prefix));
        g.add(sample_morpheme_with_hint("ash", "dark", "ahsh", MorphemeCategory::Prefix));
        g.add(sample_morpheme_with_hint("thi", "one who", "thee", MorphemeCategory::Suffix));
        g.add(sample_morpheme_with_hint("nar", "born of", "nahr", MorphemeCategory::Suffix));
        g
    }

    fn default_config(count: usize, seed: u64) -> NameGenConfig {
        NameGenConfig {
            count,
            seed,
            separator: "'".to_string(),
            pattern_weights: (1.0, 1.0, 1.0, 1.0),
        }
    }

    // === NameBank::generate tests ===

    #[test]
    fn generate_produces_correct_count() {
        let glossary = glossary_with_hints();
        let config = default_config(5, 42);
        let bank = NameBank::generate(&glossary, &config);
        assert_eq!(bank.names.len(), 5);
    }

    #[test]
    fn generate_sets_language_id() {
        let glossary = glossary_with_hints();
        let config = default_config(3, 42);
        let bank = NameBank::generate(&glossary, &config);
        assert_eq!(bank.language_id, "draconic");
        for name in &bank.names {
            assert_eq!(name.language_id, "draconic");
        }
    }

    #[test]
    fn generate_deterministic_same_seed() {
        let glossary = glossary_with_hints();
        let config = default_config(10, 123);
        let bank1 = NameBank::generate(&glossary, &config);
        let bank2 = NameBank::generate(&glossary, &config);
        assert_eq!(bank1.names, bank2.names);
    }

    #[test]
    fn generate_different_seeds_produce_different_names() {
        let glossary = glossary_with_hints();
        let bank1 = NameBank::generate(&glossary, &default_config(10, 42));
        let bank2 = NameBank::generate(&glossary, &default_config(10, 99));
        let names1: Vec<&str> = bank1.names.iter().map(|n| n.name.as_str()).collect();
        let names2: Vec<&str> = bank2.names.iter().map(|n| n.name.as_str()).collect();
        assert_ne!(names1, names2);
    }

    #[test]
    fn generate_gloss_joins_meanings_with_dash() {
        let glossary = glossary_with_hints();
        let config = default_config(20, 42);
        let bank = NameBank::generate(&glossary, &config);
        for name in &bank.names {
            match name.pattern {
                NamePattern::Root => {
                    assert!(!name.gloss.contains('-'), "Root gloss should be a single meaning");
                }
                NamePattern::PrefixRoot | NamePattern::RootSuffix => {
                    assert_eq!(name.gloss.matches('-').count(), 1,
                        "Two-part gloss should have one dash: {}", name.gloss);
                }
                NamePattern::PrefixRootSuffix => {
                    assert_eq!(name.gloss.matches('-').count(), 2,
                        "Three-part gloss should have two dashes: {}", name.gloss);
                }
            }
        }
    }

    #[test]
    fn generate_pronunciation_present_when_all_hints_exist() {
        let glossary = glossary_with_hints();
        let config = default_config(20, 42);
        let bank = NameBank::generate(&glossary, &config);
        // All morphemes in glossary_with_hints have pronunciation hints,
        // so every generated name should have a pronunciation.
        for name in &bank.names {
            assert!(name.pronunciation.is_some(),
                "Expected pronunciation for '{}' but got None", name.name);
        }
    }

    #[test]
    fn generate_pronunciation_none_when_hint_missing() {
        let mut glossary = MorphemeGlossary::new("draconic", "Draconic");
        // Root with hint, prefix without hint
        glossary.add(sample_morpheme_with_hint("zar", "fire", "zahr", MorphemeCategory::Root));
        glossary.add(sample_morpheme("vor", "great", MorphemeCategory::Prefix));
        // Force PrefixRoot only
        let config = NameGenConfig {
            count: 5,
            seed: 42,
            separator: "'".to_string(),
            pattern_weights: (0.0, 1.0, 0.0, 0.0),
        };
        let bank = NameBank::generate(&glossary, &config);
        for name in &bank.names {
            assert!(name.pronunciation.is_none(),
                "Expected no pronunciation for '{}' but got {:?}", name.name, name.pronunciation);
        }
    }

    #[test]
    fn generate_uses_different_patterns() {
        let glossary = glossary_with_hints();
        let config = default_config(50, 42);
        let bank = NameBank::generate(&glossary, &config);
        let has_root = bank.names.iter().any(|n| n.pattern == NamePattern::Root);
        let has_prefix_root = bank.names.iter().any(|n| n.pattern == NamePattern::PrefixRoot);
        let has_root_suffix = bank.names.iter().any(|n| n.pattern == NamePattern::RootSuffix);
        let has_prefix_root_suffix = bank.names.iter().any(|n| n.pattern == NamePattern::PrefixRootSuffix);
        assert!(has_root, "Expected some Root pattern names");
        assert!(has_prefix_root, "Expected some PrefixRoot pattern names");
        assert!(has_root_suffix, "Expected some RootSuffix pattern names");
        assert!(has_prefix_root_suffix, "Expected some PrefixRootSuffix pattern names");
    }

    #[test]
    fn generate_empty_glossary_returns_empty_bank() {
        let glossary = MorphemeGlossary::new("draconic", "Draconic");
        let config = default_config(5, 42);
        let bank = NameBank::generate(&glossary, &config);
        assert!(bank.names.is_empty());
    }

    #[test]
    fn generate_roots_only_glossary_uses_root_pattern() {
        let mut glossary = MorphemeGlossary::new("draconic", "Draconic");
        glossary.add(sample_morpheme_with_hint("zar", "fire", "zahr", MorphemeCategory::Root));
        glossary.add(sample_morpheme_with_hint("dra", "dragon", "drah", MorphemeCategory::Root));
        let config = default_config(10, 42);
        let bank = NameBank::generate(&glossary, &config);
        assert_eq!(bank.names.len(), 10);
        for name in &bank.names {
            assert_eq!(name.pattern, NamePattern::Root,
                "With only roots, pattern should be Root but got {:?}", name.pattern);
        }
    }

    // === format_name_bank_for_prompt tests ===

    fn make_name(name: &str, gloss: &str, pronunciation: Option<&str>) -> GeneratedName {
        GeneratedName {
            name: name.to_string(),
            gloss: gloss.to_string(),
            pronunciation: pronunciation.map(|p| p.to_string()),
            pattern: NamePattern::Root,
            language_id: "draconic".to_string(),
        }
    }

    fn make_bank(names: Vec<GeneratedName>) -> NameBank {
        NameBank {
            language_id: "draconic".to_string(),
            names,
        }
    }

    #[test]
    fn format_prompt_empty_bank_returns_empty_string() {
        let bank = make_bank(vec![]);
        let result = format_name_bank_for_prompt(&bank, 10);
        assert_eq!(result, "");
    }

    #[test]
    fn format_prompt_single_name_includes_name_and_gloss() {
        let bank = make_bank(vec![
            make_name("zar", "fire", None),
        ]);
        let result = format_name_bank_for_prompt(&bank, 10);
        assert!(result.contains("zar"), "Output should contain the name 'zar'");
        assert!(result.contains("fire"), "Output should contain the gloss 'fire'");
    }

    #[test]
    fn format_prompt_multiple_names_each_on_own_line() {
        let bank = make_bank(vec![
            make_name("zar", "fire", None),
            make_name("dra", "dragon", None),
            make_name("kel", "stone", None),
        ]);
        let result = format_name_bank_for_prompt(&bank, 10);
        // Each name should appear on its own line
        let lines: Vec<&str> = result.lines().collect();
        let name_lines: Vec<&&str> = lines.iter().filter(|l| !l.starts_with("##") && (l.contains("zar") || l.contains("dra") || l.contains("kel"))).collect();
        assert_eq!(name_lines.len(), 3, "Each name should be on its own line");
    }

    #[test]
    fn format_prompt_starts_with_section_header() {
        let bank = make_bank(vec![
            make_name("zar", "fire", None),
        ]);
        let result = format_name_bank_for_prompt(&bank, 10);
        let first_line = result.lines().next().unwrap();
        assert!(first_line.starts_with("##"), "Output should start with a markdown header, got: '{}'", first_line);
    }

    #[test]
    fn format_prompt_header_contains_language_id() {
        let bank = make_bank(vec![
            make_name("zar", "fire", None),
        ]);
        let result = format_name_bank_for_prompt(&bank, 10);
        let first_line = result.lines().next().unwrap();
        assert!(first_line.contains("draconic") || first_line.contains("Draconic"),
            "Header should contain language_id, got: '{}'", first_line);
    }

    #[test]
    fn format_prompt_max_names_limits_output() {
        let bank = make_bank(vec![
            make_name("zar", "fire", None),
            make_name("dra", "dragon", None),
            make_name("kel", "stone", None),
            make_name("vor", "great", None),
            make_name("thi", "walker", None),
        ]);
        let result = format_name_bank_for_prompt(&bank, 2);
        // Only first 2 names should appear
        assert!(result.contains("zar"), "First name should be included");
        assert!(result.contains("dra"), "Second name should be included");
        assert!(!result.contains("kel"), "Third name should NOT be included");
        assert!(!result.contains("vor"), "Fourth name should NOT be included");
        assert!(!result.contains("thi"), "Fifth name should NOT be included");
    }

    #[test]
    fn format_prompt_max_names_larger_than_bank_includes_all() {
        let bank = make_bank(vec![
            make_name("zar", "fire", None),
            make_name("dra", "dragon", None),
        ]);
        let result = format_name_bank_for_prompt(&bank, 100);
        assert!(result.contains("zar"));
        assert!(result.contains("dra"));
    }

    #[test]
    fn format_prompt_max_names_zero_returns_empty_string() {
        let bank = make_bank(vec![
            make_name("zar", "fire", None),
        ]);
        let result = format_name_bank_for_prompt(&bank, 0);
        assert_eq!(result, "");
    }

    #[test]
    fn format_prompt_includes_pronunciation_when_present() {
        let bank = make_bank(vec![
            make_name("zar", "fire", Some("zahr")),
        ]);
        let result = format_name_bank_for_prompt(&bank, 10);
        assert!(result.contains("zahr"), "Output should include pronunciation 'zahr'");
    }

    #[test]
    fn format_prompt_omits_pronunciation_gracefully_when_absent() {
        let bank = make_bank(vec![
            make_name("zar", "fire", None),
        ]);
        let result = format_name_bank_for_prompt(&bank, 10);
        assert!(result.contains("zar"), "Name should still appear");
        assert!(result.contains("fire"), "Gloss should still appear");
        // Should not contain empty parens or "None"
        assert!(!result.contains("None"), "Should not contain literal 'None'");
    }

    #[test]
    fn format_prompt_no_trailing_whitespace_on_lines() {
        let bank = make_bank(vec![
            make_name("zar", "fire", Some("zahr")),
            make_name("dra", "dragon", None),
        ]);
        let result = format_name_bank_for_prompt(&bank, 10);
        for line in result.lines() {
            assert_eq!(line, line.trim_end(), "Line has trailing whitespace: '{}'", line);
        }
    }
}

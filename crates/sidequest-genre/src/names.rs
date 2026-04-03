//! Template-based name generator with corpus blending.
//!
//! Combines Markov chain word generation with cultural naming patterns.
//! Each culture defines slots (given_name, family_name, etc.) that draw from
//! Markov-trained corpora or static word lists. Templates like
//! `{given_name} {family_name}` assemble slots into full names.
//!
//! Ported from sq-2 `sidequest/procgen/names.py`.

use std::collections::HashMap;
use std::path::Path;

use rand::Rng;

use crate::markov::MarkovChain;
use crate::models::{Culture, CultureSlot};

/// Generates words for a single naming slot.
///
/// Uses a Markov chain (trained on blended corpora) and/or a static word list.
/// Multiple corpora are trained into the same chain so character transitions
/// blend at the phonemic level — producing words that don't exist in either
/// source language.
pub struct SlotGenerator {
    chain: Option<MarkovChain>,
    word_list: Vec<String>,
}

impl SlotGenerator {
    /// Generate a single word from this slot.
    pub fn generate<R: Rng>(&self, rng: &mut R) -> String {
        let has_chain = self.chain.as_ref().is_some_and(|c| !c.is_empty());
        let has_list = !self.word_list.is_empty();

        if !has_chain && !has_list {
            return String::new();
        }

        // If both sources, chain gets 2x weight over word list
        let use_chain = if has_chain && has_list {
            rng.random_ratio(2, 3)
        } else {
            has_chain
        };

        if use_chain {
            if let Some(chain) = &self.chain {
                // Try to get a word within reasonable bounds
                for _ in 0..20 {
                    let word = chain.make_word(rng);
                    if word.len() >= 2 && word.len() <= 12 {
                        return word;
                    }
                }
                return chain.make_word(rng);
            }
        }

        // Fall back to word list
        let idx = rng.random_range(0..self.word_list.len());
        self.word_list[idx].clone()
    }
}

/// Generates names from template patterns using slot generators.
pub struct NameGenerator {
    slots: HashMap<String, SlotGenerator>,
    /// Person name patterns like `{given_name} {family_name}`.
    pub person_patterns: Vec<String>,
    /// Place name patterns like `{adjective} {place_noun}`.
    pub place_patterns: Vec<String>,
}

impl NameGenerator {
    /// Generate a person name using a random person pattern.
    pub fn generate_person<R: Rng>(&self, rng: &mut R) -> String {
        if self.person_patterns.is_empty() {
            return String::new();
        }
        let idx = rng.random_range(0..self.person_patterns.len());
        self.fill(&self.person_patterns[idx], rng)
    }

    /// Generate a place name using a random place pattern.
    pub fn generate_place<R: Rng>(&self, rng: &mut R) -> String {
        if self.place_patterns.is_empty() {
            return String::new();
        }
        let idx = rng.random_range(0..self.place_patterns.len());
        self.fill(&self.place_patterns[idx], rng)
    }

    /// Fill a pattern template with generated slot values.
    fn fill<R: Rng>(&self, pattern: &str, rng: &mut R) -> String {
        let mut cache: HashMap<String, String> = HashMap::new();
        let mut result = pattern.to_string();

        // Find all {slot_name} references and replace them
        loop {
            let start = result.find('{');
            let end = result.find('}');
            match (start, end) {
                (Some(s), Some(e)) if s < e => {
                    let slot_name = &result[s + 1..e];
                    let value = cache
                        .entry(slot_name.to_string())
                        .or_insert_with(|| {
                            self.slots
                                .get(slot_name)
                                .unwrap_or_else(|| panic!("Missing name slot '{}' — culture config is incomplete", slot_name))
                                .generate(rng)
                        })
                        .clone();
                    result = format!("{}{}{}", &result[..s], value, &result[e + 1..]);
                }
                _ => break,
            }
        }

        titlecase_name(&result)
    }
}

/// Title-case a name, keeping small words lowercase.
///
/// "kael de morvaine" → "Kael de Morvaine"
fn titlecase_name(name: &str) -> String {
    const SMALL_WORDS: &[&str] = &[
        "de", "of", "the", "and", "le", "la", "von", "van", "du", "des",
    ];

    name.split_whitespace()
        .enumerate()
        .map(|(i, word)| {
            if i == 0 || !SMALL_WORDS.contains(&word.to_lowercase().as_str()) {
                let mut chars = word.chars();
                match chars.next() {
                    Some(first) => {
                        let upper: String = first.to_uppercase().collect();
                        format!("{}{}", upper, chars.as_str())
                    }
                    None => String::new(),
                }
            } else {
                word.to_lowercase()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Result of building a name generator, including the translation dictionary.
pub struct NameGeneratorResult {
    /// The name generator.
    pub generator: NameGenerator,
    /// English → fantasy word mappings produced during word list translation.
    /// Can be serialized and groomed.
    pub dictionary: HashMap<String, String>,
}

/// Build a `NameGenerator` from a `Culture` and corpus directory.
///
/// All corpora for a slot are trained into a single `MarkovChain` so that
/// character transitions blend at the phonemic level. When a slot has both
/// a word_list and corpora, the word list is translated through the chain
/// to produce fantasy equivalents (e.g., "Voltkin" → "Kravtik").
pub fn build_from_culture<R: Rng>(
    culture: &Culture,
    corpus_dir: &Path,
    rng: &mut R,
) -> NameGeneratorResult {
    use crate::markov::{generate_dictionary, translate_word_list};

    let mut slots = HashMap::new();
    let mut dictionary = HashMap::new();

    for (slot_name, slot_config) in &culture.slots {
        let chain = build_chain_for_slot(slot_config, corpus_dir);

        let mut word_list = slot_config.word_list.clone().unwrap_or_default();

        // Load names_file if present — check names/ sibling of corpus/, then corpus/ itself
        if let Some(ref names_file) = slot_config.names_file {
            let names_dir = corpus_dir.parent().unwrap_or(corpus_dir).join("names").join(names_file);
            let corpus_fallback = corpus_dir.join(names_file);
            let names_path = if names_dir.exists() { names_dir } else { corpus_fallback };
            if let Ok(text) = std::fs::read_to_string(&names_path) {
                let file_names: Vec<String> = text
                    .lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect();
                word_list.extend(file_names);
            }
        }

        // If slot has both word_list and corpora, translate the word list into
        // fantasy equivalents. The original English words map to generated words
        // so the semantic meaning is preserved but the phonemic flavor matches
        // the culture's language.
        if !word_list.is_empty() {
            if let Some(ref chain) = chain {
                let slot_dict = generate_dictionary(chain, &word_list, rng);
                let translated = translate_word_list(&word_list, &slot_dict);
                dictionary.extend(slot_dict);
                word_list = translated;
            }
        }

        slots.insert(slot_name.clone(), SlotGenerator { chain, word_list });
    }

    NameGeneratorResult {
        generator: NameGenerator {
            slots,
            person_patterns: culture.person_patterns.clone(),
            place_patterns: culture.place_patterns.clone(),
        },
        dictionary,
    }
}

/// Build and train a MarkovChain for a single slot's corpora.
fn build_chain_for_slot(slot: &CultureSlot, corpus_dir: &Path) -> Option<MarkovChain> {
    let corpora = slot.corpora.as_ref()?;
    if corpora.is_empty() {
        return None;
    }

    let lookback = slot.lookback.unwrap_or(2) as usize;
    let mut chain = MarkovChain::new(lookback);

    for corpus_ref in corpora {
        let corpus_path = corpus_dir.join(&corpus_ref.corpus);
        let text = match std::fs::read_to_string(&corpus_path) {
            Ok(t) => t,
            Err(_) => continue,
        };

        // Weight > 1 means train multiple times for more influence
        let rounds = (corpus_ref.weight.round() as usize).max(1);
        for _ in 0..rounds {
            chain.train_file(&text);
        }
    }

    // Load reject files
    for reject_file in &slot.reject_files {
        let reject_path = corpus_dir.join(reject_file);
        if let Ok(text) = std::fs::read_to_string(&reject_path) {
            let words = text
                .lines()
                .map(|l| l.trim().to_lowercase())
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>();
            chain.add_reject_words(words.into_iter());
        }
    }

    Some(chain)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sidequest_protocol::NonBlankString;

    fn test_culture() -> Culture {
        Culture {
            name: NonBlankString::new("Ember Isles").unwrap(),
            description: "Fire nation".to_string(),
            slots: HashMap::from([
                (
                    "family_name".to_string(),
                    CultureSlot {
                        corpora: None,
                        lookback: None,
                        names_file: None,
                        word_list: Some(vec![
                            "Haruki".to_string(),
                            "Sakura".to_string(),
                            "Takeshi".to_string(),
                        ]),
                        reject_files: vec![],
                    },
                ),
                (
                    "place_noun".to_string(),
                    CultureSlot {
                        corpora: None,
                        lookback: None,
                        names_file: None,
                        word_list: Some(vec![
                            "Forge".to_string(),
                            "Caldera".to_string(),
                            "Shrine".to_string(),
                        ]),
                        reject_files: vec![],
                    },
                ),
                (
                    "adjective".to_string(),
                    CultureSlot {
                        corpora: None,
                        lookback: None,
                        names_file: None,
                        word_list: Some(vec![
                            "burning".to_string(),
                            "iron".to_string(),
                            "crimson".to_string(),
                        ]),
                        reject_files: vec![],
                    },
                ),
            ]),
            person_patterns: vec!["{family_name} {place_noun}".to_string()],
            place_patterns: vec![
                "{adjective} {place_noun}".to_string(),
                "{family_name} {place_noun}".to_string(),
            ],
        }
    }

    #[test]
    fn generates_place_name() {
        let culture = test_culture();
        let result = build_from_culture(&culture, Path::new("."), &mut rand::rng());
        let gen = result.generator;
        let name = gen.generate_place(&mut rand::rng());
        assert!(!name.is_empty());
        // Should be title-cased
        assert!(name.chars().next().unwrap().is_uppercase());
    }

    #[test]
    fn generates_person_name() {
        let culture = test_culture();
        let result = build_from_culture(&culture, Path::new("."), &mut rand::rng());
        let gen = result.generator;
        let name = gen.generate_person(&mut rand::rng());
        assert!(!name.is_empty());
    }

    #[test]
    fn titlecase_preserves_small_words() {
        assert_eq!(titlecase_name("kael de morvaine"), "Kael de Morvaine");
        assert_eq!(titlecase_name("the burning forge"), "The Burning Forge");
    }

    #[test]
    fn fill_replaces_slots() {
        let culture = test_culture();
        let result = build_from_culture(&culture, Path::new("."), &mut rand::rng());
        let gen = result.generator;
        // Multiple calls should produce valid names
        for _ in 0..10 {
            let name = gen.generate_place(&mut rand::rng());
            assert!(!name.is_empty());
            // Should contain at least one space (two words)
            assert!(name.contains(' '), "name should have two words: {}", name);
        }
    }
}

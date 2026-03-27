//! Character-level Markov chain for generating fantasy words.
//!
//! Ported from Keith Avery's fantasy-language-maker (Go: slabgorb/lango,
//! Python: slabgorb/fantasy-language-maker). Trains on text corpora and
//! produces words that "sound like" the source language.

use std::collections::HashMap;

use rand::Rng;

/// Sentinel characters for word boundaries.
const START: char = '^';
const END: char = '$';

/// Character-level Markov chain word generator.
///
/// Train on text, then call `make_word()` to generate fantasy words
/// that share the phonemic character of the training data.
///
/// ```
/// use sidequest_genre::markov::MarkovChain;
///
/// let mut chain = MarkovChain::new(2);
/// chain.add_word("sakura");
/// chain.add_word("takeshi");
/// chain.add_word("haruki");
/// let word = chain.make_word(&mut rand::rng());
/// assert!(!word.is_empty());
/// ```
#[derive(Debug, Clone)]
pub struct MarkovChain {
    lookback: usize,
    /// Maps context string → (next_char → count).
    chain: HashMap<String, HashMap<char, u32>>,
    /// Words to reject from generation output.
    reject_words: std::collections::HashSet<String>,
}

impl MarkovChain {
    /// Create a new chain with the given lookback depth.
    ///
    /// `lookback` of 2 produces wilder names, 3 produces smoother ones.
    pub fn new(lookback: usize) -> Self {
        Self {
            lookback,
            chain: HashMap::new(),
            reject_words: std::collections::HashSet::new(),
        }
    }

    /// Generate the start key: `lookback` copies of the start sentinel.
    fn start_key(&self) -> Vec<char> {
        vec![START; self.lookback]
    }

    /// Add a single word to the chain.
    pub fn add_word(&mut self, word: &str) {
        let lower = word.to_lowercase();
        let mut key = self.start_key();
        for ch in lower.chars() {
            if !ch.is_alphabetic() {
                continue;
            }
            let key_str: String = key.iter().collect();
            self.chain
                .entry(key_str)
                .or_default()
                .entry(ch)
                .and_modify(|c| *c += 1)
                .or_insert(1);
            key.push(ch);
            key.remove(0);
        }
        // Mark end of word
        let key_str: String = key.iter().collect();
        self.chain
            .entry(key_str)
            .or_default()
            .entry(END)
            .and_modify(|c| *c += 1)
            .or_insert(1);
    }

    /// Train on raw text — splits into words, strips non-letters.
    pub fn train(&mut self, text: &str) {
        for line in text.lines() {
            for word in line.split_whitespace() {
                let cleaned: String = word.chars().filter(|c| c.is_alphabetic()).collect();
                if !cleaned.is_empty() {
                    self.add_word(&cleaned);
                }
            }
        }
    }

    /// Train on a file's text, stripping Project Gutenberg front/back matter.
    pub fn train_file(&mut self, text: &str) {
        let lines: Vec<&str> = text.lines().collect();
        let mut in_body = false;
        let mut body = Vec::new();

        for line in &lines {
            if !in_body {
                if line.contains("*** START") {
                    in_body = true;
                }
                continue;
            }
            if line.contains("*** END") {
                break;
            }
            body.push(*line);
        }

        if body.is_empty() {
            self.train(text);
        } else {
            self.train(&body.join("\n"));
        }
    }

    /// Add words to the reject set.
    pub fn add_reject_words(&mut self, words: impl IntoIterator<Item = String>) {
        self.reject_words.extend(words);
    }

    /// Generate a single fantasy word.
    pub fn make_word<R: Rng>(&self, rng: &mut R) -> String {
        if self.chain.is_empty() {
            return String::new();
        }

        let mut word = String::new();
        let mut key = self.start_key();

        for _ in 0..50 {
            let key_str: String = key.iter().collect();
            let ch = match self.chain.get(&key_str) {
                Some(counts) => weighted_choice(counts, rng),
                None => END,
            };
            if ch == END {
                break;
            }
            word.push(ch);
            key.push(ch);
            key.remove(0);
        }

        word
    }

    /// Generate multiple unique words within length bounds.
    pub fn make_words<R: Rng>(
        &self,
        count: usize,
        min_length: usize,
        max_length: usize,
        rng: &mut R,
    ) -> Vec<String> {
        let mut words = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let max_attempts = count * 20;

        for _ in 0..max_attempts {
            if words.len() >= count {
                break;
            }
            let word = self.make_word(rng);
            if word.len() >= min_length
                && word.len() <= max_length
                && !seen.contains(&word)
                && !self.reject_words.contains(&word)
            {
                seen.insert(word.clone());
                words.push(word);
            }
        }

        words
    }

    /// Whether the chain has any training data.
    pub fn is_empty(&self) -> bool {
        self.chain.is_empty()
    }
}

/// Pick a random character weighted by counts.
fn weighted_choice<R: Rng>(counts: &HashMap<char, u32>, rng: &mut R) -> char {
    let total: u32 = counts.values().sum();
    if total == 0 {
        return END;
    }
    let mut threshold = rng.random_range(0..total);
    for (&ch, &count) in counts {
        if threshold < count {
            return ch;
        }
        threshold -= count;
    }
    END
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_chain_returns_empty() {
        let chain = MarkovChain::new(2);
        let word = chain.make_word(&mut rand::rng());
        assert!(word.is_empty());
    }

    #[test]
    fn generates_nonempty_word() {
        let mut chain = MarkovChain::new(2);
        chain.add_word("sakura");
        chain.add_word("takeshi");
        chain.add_word("haruki");
        chain.add_word("kazuki");
        chain.add_word("naomi");
        let word = chain.make_word(&mut rand::rng());
        assert!(!word.is_empty());
    }

    #[test]
    fn respects_length_bounds() {
        let mut chain = MarkovChain::new(2);
        for w in ["tokyo", "osaka", "kyoto", "nagoya", "sapporo", "sendai"] {
            chain.add_word(w);
        }
        let words = chain.make_words(5, 3, 8, &mut rand::rng());
        for word in &words {
            assert!(
                word.len() >= 3 && word.len() <= 8,
                "word '{}' out of bounds",
                word
            );
        }
    }

    #[test]
    fn rejects_blacklisted_words() {
        let mut chain = MarkovChain::new(2);
        chain.add_word("the");
        chain.add_word("them");
        chain.add_reject_words(["the".to_string()]);
        let words = chain.make_words(10, 2, 5, &mut rand::rng());
        assert!(!words.contains(&"the".to_string()));
    }

    #[test]
    fn train_text_splits_words() {
        let mut chain = MarkovChain::new(2);
        chain.train("hello world foo bar baz");
        assert!(!chain.is_empty());
        let word = chain.make_word(&mut rand::rng());
        assert!(!word.is_empty());
    }

    #[test]
    fn blended_corpora_produce_novel_words() {
        let mut chain = MarkovChain::new(2);
        // Train on Japanese-style names
        for w in ["sakura", "takeshi", "haruki", "kazuki", "naomi", "hikaru"] {
            chain.add_word(w);
        }
        // Blend with Korean-style names
        for w in ["kim", "park", "choi", "jung", "kang", "yoon"] {
            chain.add_word(w);
        }
        let words = chain.make_words(10, 3, 10, &mut rand::rng());
        assert!(!words.is_empty());
        // Words should exist but not necessarily match either source
    }

    #[test]
    fn lookback_3_produces_smoother_words() {
        let mut chain = MarkovChain::new(3);
        for w in [
            "sakura", "takeshi", "haruki", "kazuki", "naomi", "hikaru", "kenji", "sayuri",
            "hiroshi", "midori", "daichi", "aiko",
        ] {
            chain.add_word(w);
        }
        let words = chain.make_words(5, 3, 10, &mut rand::rng());
        assert!(!words.is_empty());
    }
}

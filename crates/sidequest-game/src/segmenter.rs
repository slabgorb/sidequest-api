//! Sentence segmenter — split narrative text into sentence-level chunks for TTS streaming.
//!
//! Ported from Python `sidequest_daemon/voice/segmenter.py`. Splits narration into
//! sentence-level segments preserving punctuation, with metadata for streaming delivery.

use std::collections::HashSet;
use std::sync::LazyLock;

/// A single segment of narration text, ready for TTS synthesis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    /// The sentence text, trimmed.
    pub text: String,
    /// Zero-based index of this segment in the sequence.
    pub index: usize,
    /// Byte offset of this segment's start in the original text.
    pub byte_offset: usize,
    /// Whether this is the last segment in the sequence.
    pub is_last: bool,
}

/// Title abbreviations — these NEVER end a sentence, even before a capital letter.
/// They always prefix a name or noun (e.g., "Mr. Smith", "Dr. Jones").
static TITLE_ABBREVIATIONS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "mr", "mrs", "ms", "dr", "prof", "sr", "jr", "gen", "gov", "sgt", "cpl", "pvt", "capt",
        "lt", "col",
    ]
    .into_iter()
    .collect()
});

/// Non-title abbreviations — these don't split mid-sentence, but CAN end a sentence
/// when followed by whitespace + capital letter (e.g., "etc. The next thing...").
static OTHER_ABBREVIATIONS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "st", "ave", "etc", "vs", "vol", "dept", "est", "approx", "inc", "ltd",
    ]
    .into_iter()
    .collect()
});

/// Break narrative text into sentence-level semantic units.
pub struct SentenceSegmenter;

impl SentenceSegmenter {
    /// Create a new segmenter.
    pub fn new() -> Self {
        Self
    }

    /// Split `text` into sentences, preserving punctuation.
    ///
    /// Returns a `Vec<Segment>` with metadata for streaming delivery.
    /// Empty or whitespace-only input yields an empty vec.
    pub fn segment(&self, text: &str) -> Vec<Segment> {
        if text.trim().is_empty() {
            return Vec::new();
        }

        let split_points = find_split_points(text);
        let mut segments: Vec<(String, usize)> = Vec::new();
        let mut last = 0;

        for end in split_points {
            if let Some(seg) = trimmed_segment(text, last, end) {
                segments.push(seg);
            }
            last = end;
        }

        // Remainder after the last split point.
        if let Some(seg) = trimmed_segment(text, last, text.len()) {
            segments.push(seg);
        }

        let total = segments.len();
        segments
            .into_iter()
            .enumerate()
            .map(|(i, (text, byte_offset))| Segment {
                text,
                index: i,
                byte_offset,
                is_last: i == total - 1,
            })
            .collect()
    }
}

impl Default for SentenceSegmenter {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract a trimmed segment from `text[start..end]`, returning the text and its byte offset.
fn trimmed_segment(text: &str, start: usize, end: usize) -> Option<(String, usize)> {
    let slice = &text[start..end];
    let trimmed = slice.trim();
    if trimmed.is_empty() {
        return None;
    }
    let leading = slice.len() - slice.trim_start().len();
    Some((trimmed.to_string(), start + leading))
}

/// Find all sentence-boundary byte positions in `text`.
///
/// Returns a sorted list of byte offsets where splits should occur (the end
/// of the sentence-terminating punctuation). Mirrors the Python regex logic
/// but without lookahead/lookbehind.
fn find_split_points(text: &str) -> Vec<usize> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let len = chars.len();
    let mut splits = Vec::new();

    for (ci, &(byte_pos, ch)) in chars.iter().enumerate() {
        match ch {
            '.' => {
                // Check for ellipsis (three dots or unicode …)
                if ci + 2 < len && chars[ci + 1].1 == '.' && chars[ci + 2].1 == '.' {
                    // Ellipsis: split only if followed by whitespace + capital/opening quote
                    let after_ellipsis = byte_pos + 3; // 3 ASCII dots
                    if is_followed_by_ws_and_capital(text, after_ellipsis) {
                        splits.push(after_ellipsis);
                    }
                    // Skip — don't also check as single period
                    continue;
                }

                // Skip if this dot is part of an ellipsis (we're at pos 2 or 3 of "...")
                if ci > 0 && chars[ci - 1].1 == '.' {
                    continue;
                }

                // Single period — check abbreviation
                if let Some(word) = word_before_dot(text, byte_pos) {
                    let lower = word.to_lowercase();
                    // Title abbreviations (Mr., Dr.) never end a sentence
                    if TITLE_ABBREVIATIONS.contains(lower.as_str()) {
                        continue;
                    }
                    // Other abbreviations (etc., vs.) only skip if NOT followed
                    // by whitespace + capital (which signals a new sentence)
                    if OTHER_ABBREVIATIONS.contains(lower.as_str()) {
                        let after = byte_pos + 1;
                        if !is_followed_by_ws_and_capital(text, after) {
                            continue;
                        }
                    }
                }

                // Period + optional closing quote
                let mut end = byte_pos + 1;
                if let Some(next_ch) = text[end..].chars().next() {
                    if next_ch == '"' || next_ch == '\u{201d}' {
                        end += next_ch.len_utf8();
                    }
                }

                // Must be followed by whitespace or end-of-string
                if end >= text.len() || text[end..].starts_with(|c: char| c.is_whitespace()) {
                    splits.push(end);
                }
            }
            '\u{2026}' => {
                // Unicode ellipsis — split only if followed by whitespace + capital
                let after = byte_pos + ch.len_utf8();
                if is_followed_by_ws_and_capital(text, after) {
                    splits.push(after);
                }
            }
            '!' | '?' => {
                let mut end = byte_pos + 1;

                // Check for closing quote after ! or ?
                let has_closing_quote = match text[end..].chars().next() {
                    Some(c) if c == '"' || c == '\u{201d}' => {
                        end += c.len_utf8();
                        true
                    }
                    _ => false,
                };

                if has_closing_quote {
                    // Pattern 3: !?" followed by whitespace + opening quote
                    if is_followed_by_ws_and_opening_quote(text, end) {
                        splits.push(end);
                        continue;
                    }
                    // Pattern 5: !?" at end-of-string
                    if end >= text.len() {
                        splits.push(end);
                        continue;
                    }
                    // Also split if followed by whitespace (general case)
                    if text[end..].starts_with(|c: char| c.is_whitespace()) {
                        splits.push(end);
                    }
                } else {
                    // Pattern 4: bare ! or ? followed by whitespace or end
                    if end >= text.len() || text[end..].starts_with(|c: char| c.is_whitespace()) {
                        splits.push(end);
                    }
                }
            }
            _ => {}
        }
    }

    debug_assert!(
        splits.windows(2).all(|w| w[0] < w[1]),
        "split points must be strictly increasing"
    );
    splits
}

/// Check if text at `pos` starts with whitespace followed by a char matching `predicate`.
fn is_followed_by_ws_and(text: &str, pos: usize, predicate: impl Fn(char) -> bool) -> bool {
    if pos >= text.len() {
        return false;
    }
    let rest = &text[pos..];
    let trimmed = rest.trim_start();
    if trimmed.is_empty() || trimmed.len() == rest.len() {
        return false;
    }
    trimmed.chars().next().is_some_and(&predicate)
}

/// Check if text at `pos` starts with whitespace followed by a capital letter or opening quote.
fn is_followed_by_ws_and_capital(text: &str, pos: usize) -> bool {
    is_followed_by_ws_and(text, pos, |c| {
        c.is_uppercase() || c == '"' || c == '\u{201c}'
    })
}

/// Check if text at `pos` starts with whitespace followed by an opening quote.
fn is_followed_by_ws_and_opening_quote(text: &str, pos: usize) -> bool {
    is_followed_by_ws_and(text, pos, |c| c == '"' || c == '\u{201c}')
}

/// Return the word immediately before the dot at `dot_pos`.
fn word_before_dot(text: &str, dot_pos: usize) -> Option<String> {
    let before = &text[..dot_pos];
    let word: String = before
        .chars()
        .rev()
        .take_while(|c| c.is_alphabetic())
        .collect();
    let word: String = word.chars().rev().collect();
    if word.is_empty() {
        None
    } else {
        Some(word)
    }
}

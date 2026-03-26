//! JSON extraction from LLM responses.
//!
//! Port lesson #2: Single JsonExtractor with 3-tier extraction logic
//! (direct parse → markdown fence → freeform search).

use regex::Regex;
use serde::de::DeserializeOwned;

/// Errors from JSON extraction attempts.
#[derive(Debug)]
#[non_exhaustive]
pub enum ExtractionError {
    /// No JSON found in the input after trying all three tiers.
    NoJsonFound,
    /// JSON was found but failed to parse into the target type.
    ParseFailed {
        /// The raw JSON string that failed to parse.
        raw: String,
        /// The parse error message.
        source: String,
    },
}

impl std::fmt::Display for ExtractionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExtractionError::NoJsonFound => write!(f, "no JSON found in input"),
            ExtractionError::ParseFailed { raw, source } => {
                write!(f, "JSON parse failed: {source} (raw: {raw})")
            }
        }
    }
}

impl std::error::Error for ExtractionError {}

/// Stateless JSON extractor with 3-tier extraction.
///
/// Tries in order:
/// 1. Direct JSON parse of the entire input
/// 2. Extract from markdown code fences (```json ... ``` or ``` ... ```)
/// 3. Freeform search for JSON objects/arrays in the text
pub struct JsonExtractor;

impl JsonExtractor {
    /// Extract and deserialize JSON from LLM output.
    ///
    /// Tries three tiers in order: direct parse, fence extraction, freeform search.
    /// Emits a tracing span with extraction_tier, target_type, and success fields (story 3-1).
    pub fn extract<T: DeserializeOwned>(input: &str) -> Result<T, ExtractionError> {
        let trimmed = input.trim();
        let target_type = std::any::type_name::<T>();

        // Try all three tiers, tracking which succeeded
        let (result, tier): (Result<T, ExtractionError>, u8) = if let Ok(value) =
            serde_json::from_str::<T>(trimmed)
        {
            (Ok(value), 1)
        } else if let Some(fenced) = Self::extract_from_fence(trimmed) {
            let r = serde_json::from_str::<T>(&fenced).map_err(|e| ExtractionError::ParseFailed {
                raw: fenced,
                source: e.to_string(),
            });
            (r, 2)
        } else if let Some(found) = Self::find_json_in_text(trimmed) {
            let r = serde_json::from_str::<T>(&found).map_err(|e| ExtractionError::ParseFailed {
                raw: found,
                source: e.to_string(),
            });
            (r, 3)
        } else {
            (Err(ExtractionError::NoJsonFound), 0)
        };

        let success = result.is_ok();
        let span = tracing::info_span!(
            "extract",
            extraction_tier = tier,
            target_type = target_type,
            success = success,
        );
        let _guard = span.enter();

        result
    }

    /// Extract content from markdown code fences.
    fn extract_from_fence(input: &str) -> Option<String> {
        let re = Regex::new(r"```(?:json)?\s*\n([\s\S]*?)\n```").ok()?;
        re.captures(input)
            .and_then(|caps| caps.get(1))
            .map(|m| m.as_str().trim().to_string())
    }

    /// Search for a JSON object or array in freeform text.
    fn find_json_in_text(input: &str) -> Option<String> {
        // Find the first { or [ and try to parse from there
        for (i, ch) in input.char_indices() {
            if ch == '{' || ch == '[' {
                let closing = if ch == '{' { '}' } else { ']' };
                if let Some(end) = Self::find_matching_bracket(input, i, ch, closing) {
                    let candidate = &input[i..=end];
                    // Verify it's valid JSON
                    if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
                        return Some(candidate.to_string());
                    }
                }
            }
        }
        None
    }

    /// Find the matching closing bracket, accounting for nesting.
    fn find_matching_bracket(input: &str, start: usize, open: char, close: char) -> Option<usize> {
        let mut depth = 0;
        let mut in_string = false;
        let mut escape_next = false;

        for (i, ch) in input[start..].char_indices() {
            if escape_next {
                escape_next = false;
                continue;
            }
            if ch == '\\' && in_string {
                escape_next = true;
                continue;
            }
            if ch == '"' {
                in_string = !in_string;
                continue;
            }
            if in_string {
                continue;
            }
            if ch == open {
                depth += 1;
            } else if ch == close {
                depth -= 1;
                if depth == 0 {
                    return Some(start + i);
                }
            }
        }
        None
    }
}

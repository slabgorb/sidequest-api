//! Input sanitization for player-authored text.
//!
//! Direct port of Python's `comms/sanitize.py`. Strips prompt injection
//! vectors before player text reaches any agent prompt.
//!
//! ## Why this matters
//!
//! Players type free-form text that gets injected into Claude's prompt.
//! Without sanitization, a player could type `<system>ignore rules</system>`
//! and the LLM might treat it as a system instruction. This module strips
//! dangerous patterns while preserving normal gameplay text.

use regex::Regex;
use std::sync::LazyLock;

// ---------------------------------------------------------------------------
// Patterns — compiled once, reused forever
// ---------------------------------------------------------------------------

// In Python, these were module-level `re.compile()` calls.
// In Rust, we use `LazyLock` (stable since 1.80) — the regex is compiled
// on first use and cached for the lifetime of the program. Same semantics
// as Python's module-level compilation, but thread-safe by default.

static DANGEROUS_TAGS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)<\s*/?\s*(?:system|context|user-input|instructions|assistant|human_turn|ai_turn)(?:\s[^>]*)?\s*/?\s*>"
    ).unwrap()
});

static BRACKET_MARKERS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\[\s*/?\s*(?:SYSTEM(?:\s+PROMPT)?|INST)\s*\]").unwrap());

static OVERRIDE_PREAMBLES: LazyLock<[Regex; 5]> = LazyLock::new(|| {
    [
        Regex::new(r"(?i)ignore\s+(?:all\s+)?previous\s+instructions").unwrap(),
        Regex::new(r"(?i)disregard\s+your\s+system\s+prompt").unwrap(),
        Regex::new(r"(?i)you\s+are\s+now\s+DAN").unwrap(),
        Regex::new(r"(?i)forget\s+everything\s+above").unwrap(),
        Regex::new(r"(?i)ignore\s+previous\s+instructions").unwrap(),
    ]
});

static DOUBLE_SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"  +").unwrap());

// ---------------------------------------------------------------------------
// Unicode confusable replacements
// ---------------------------------------------------------------------------

/// Maps unicode confusable characters to their ASCII equivalents.
/// Attackers use fullwidth brackets (＜＞) to sneak past naive tag detection.
const UNICODE_REPLACEMENTS: &[(char, char)] = &[
    ('\u{ff1c}', '<'), // fullwidth <
    ('\u{ff1e}', '>'), // fullwidth >
    ('\u{27e8}', '<'), // mathematical left angle ⟨
    ('\u{27e9}', '>'), // mathematical right angle ⟩
    ('\u{fe64}', '<'), // small form variant <
    ('\u{fe65}', '>'), // small form variant >
];

/// Zero-width characters that can be used to bypass pattern matching.
const ZERO_WIDTH_CHARS: &[char] = &[
    '\u{200b}', // zero-width space
    '\u{200c}', // zero-width non-joiner
    '\u{200d}', // zero-width joiner
    '\u{2060}', // word joiner
    '\u{feff}', // byte order mark
];

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Sanitize player-authored text before injection into agent prompts.
///
/// Strips:
/// - XML-like tags used in prompt structure (`<system>`, `<context>`, etc.)
/// - Bracket notation markers (`[SYSTEM]`, `[INST]`, `[/INST]`)
/// - Common prompt override preambles
/// - Unicode tricks (fullwidth brackets, zero-width chars)
///
/// Preserves normal player text unchanged.
pub fn sanitize_player_text(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }

    // Step 1: Normalize unicode confusables
    let mut result = normalize_unicode(text);

    // Step 2: Strip dangerous XML-like tags
    result = DANGEROUS_TAGS.replace_all(&result, "").to_string();

    // Step 3: Strip bracket notation markers
    result = BRACKET_MARKERS.replace_all(&result, "").to_string();

    // Step 4: Replace prompt override preambles with [blocked]
    for pattern in OVERRIDE_PREAMBLES.iter() {
        result = pattern.replace_all(&result, "[blocked]").to_string();
    }

    // Step 5: Collapse double spaces and trim
    result = DOUBLE_SPACES.replace_all(&result, " ").to_string();
    result.trim().to_string()
}

fn normalize_unicode(text: &str) -> String {
    text.chars()
        .filter(|c| !ZERO_WIDTH_CHARS.contains(c))
        .map(|c| {
            for &(from, to) in UNICODE_REPLACEMENTS {
                if c == from {
                    return to;
                }
            }
            c
        })
        .collect()
}

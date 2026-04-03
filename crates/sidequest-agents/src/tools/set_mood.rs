//! Scene mood validation tool (ADR-057 Phase 2).
//!
//! Validates a string argument against the `SceneMood` enum.
//! This replaces the narrator's `scene_mood` JSON field with a typed tool call.

use std::fmt;

/// Scene mood — the overall emotional tone of a scene for music selection.
///
/// These are distinct from the narrator's old free-text mood values.
/// Each variant maps to a specific audio/atmosphere profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SceneMood {
    /// Suspense, danger nearby, uncertainty.
    Tension,
    /// Awe, discovery, beauty.
    Wonder,
    /// Sadness, loss, reflection.
    Melancholy,
    /// Victory, achievement, celebration.
    Triumph,
    /// Dread, impending doom, dark omens.
    Foreboding,
    /// Peace, rest, safety.
    Calm,
    /// Excitement, speed, thrill.
    Exhilaration,
    /// Sacred, solemn, spiritual.
    Reverence,
}

impl SceneMood {
    /// Convert to the canonical lowercase string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            SceneMood::Tension => "tension",
            SceneMood::Wonder => "wonder",
            SceneMood::Melancholy => "melancholy",
            SceneMood::Triumph => "triumph",
            SceneMood::Foreboding => "foreboding",
            SceneMood::Calm => "calm",
            SceneMood::Exhilaration => "exhilaration",
            SceneMood::Reverence => "reverence",
        }
    }
}

impl fmt::Display for SceneMood {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Error returned when a mood string doesn't match any known variant.
#[derive(Debug, thiserror::Error)]
#[error("invalid scene mood: \"{0}\" — expected one of: tension, wonder, melancholy, triumph, foreboding, calm, exhilaration, reverence")]
pub struct InvalidMood(String);

/// Validate a string against the `SceneMood` enum (case-insensitive).
#[tracing::instrument(name = "tool.set_mood", skip_all, fields(input = %input))]
pub fn validate_mood(input: &str) -> Result<SceneMood, InvalidMood> {
    let result = match input.to_lowercase().as_str() {
        "tension" => Ok(SceneMood::Tension),
        "wonder" => Ok(SceneMood::Wonder),
        "melancholy" => Ok(SceneMood::Melancholy),
        "triumph" => Ok(SceneMood::Triumph),
        "foreboding" => Ok(SceneMood::Foreboding),
        "calm" => Ok(SceneMood::Calm),
        "exhilaration" => Ok(SceneMood::Exhilaration),
        "reverence" => Ok(SceneMood::Reverence),
        _ => Err(InvalidMood(input.to_string())),
    };

    match &result {
        Ok(mood) => tracing::info!(valid = true, value = mood.as_str(), "mood validated"),
        Err(_) => tracing::warn!(valid = false, "mood validation failed"),
    }

    result
}

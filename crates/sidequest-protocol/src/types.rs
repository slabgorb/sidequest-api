//! Validated newtypes for the protocol layer.
//!
//! ## Why newtypes?
//!
//! In Python, validation happens at runtime via Pydantic's `@field_validator`:
//! ```python
//! @field_validator("name")
//! def name_must_not_be_blank(cls, v):
//!     if not v.strip():
//!         raise ValueError("name must not be empty")
//!     return v
//! ```
//!
//! In Rust, we use the **newtype pattern**: a wrapper struct with a private inner
//! field and a validating constructor. Once you have a `NonBlankString`, you *know*
//! it's valid — the type system guarantees it. No runtime check needed at point of use.
//!
//! The `#[serde(try_from)]` attribute ensures deserialization goes through the same
//! validation as `new()`, preventing rule #8 (Deserialize bypass).

use serde::{Deserialize, Serialize};
use std::fmt;

/// A string that is guaranteed to be non-empty after trimming.
///
/// The inner value is private (rule #9) — access via `as_str()`.
/// Construction validates (rule #5) — `new("")` returns `Err`.
/// Deserialization validates (rule #8) — `#[serde(try_from)]` calls `new()`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct NonBlankString(String);

impl NonBlankString {
    /// Create a new `NonBlankString`, rejecting empty or whitespace-only input.
    ///
    /// The input is trimmed before validation, matching Python's behavior.
    pub fn new(s: &str) -> Result<Self, NonBlankStringError> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err(NonBlankStringError::Blank);
        }
        Ok(Self(trimmed.to_string()))
    }

    /// Access the validated string value.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Deserialization goes through `new()` so validation is consistent (rule #13).
///
/// `#[serde(try_from = "String")]` tells serde: "deserialize as a plain String,
/// then convert via TryFrom". If TryFrom fails, deserialization fails.
/// This prevents creating an invalid NonBlankString via JSON.
impl TryFrom<String> for NonBlankString {
    type Error = NonBlankStringError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::new(&s)
    }
}

impl<'de> Deserialize<'de> for NonBlankString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::try_from(s).map_err(serde::de::Error::custom)
    }
}

impl fmt::Display for NonBlankString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Error returned when attempting to create a `NonBlankString` from blank input.
#[derive(Debug, Clone, thiserror::Error)]
#[non_exhaustive]
pub enum NonBlankStringError {
    /// The input was empty or contained only whitespace.
    #[error("string must not be blank")]
    Blank,
}

/// A canonicalized ability/stat name for dice requests.
///
/// Normalized to UPPERCASE at construction. This stops narrator casing drift:
/// the LLM emits `"Influence"` in one session and `"INFLUENCE"` in another
/// (story 37-17), but both canonicalize to the same wire form and compare
/// equal. Genre packs define the valid set per-genre via `rules.yaml`'s
/// `ability_score_names`; this type does not enumerate variants because the
/// set varies wildly by genre (`STR/DEX/CON/...` in `caverns_and_claudes`,
/// `Brawn/Reflexes/Toughness/...` in `mutant_wasteland`,
/// `Physique/Reflex/Intellect/Cunning/Resolve/Influence` in `space_opera`,
/// etc.). Hardcoding a closed enum would couple `sidequest-protocol` to every
/// genre pack and violate SOUL.md's "Crunch in the Genre, Flavor in the World".
///
/// Construction rejects blank/whitespace-only input. Equality and hashing work
/// on the canonical (uppercase, trimmed) form, so `"strength"`, `"Strength"`,
/// and `"STRENGTH"` are all the same `Stat`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct Stat(String);

impl Stat {
    /// Canonicalize a stat name: trim whitespace, uppercase, reject if blank.
    pub fn new(s: &str) -> Result<Self, StatError> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err(StatError::Blank);
        }
        Ok(Self(trimmed.to_uppercase()))
    }

    /// Access the canonical (uppercase) stat name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Stat {
    type Error = StatError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::new(&s)
    }
}

impl TryFrom<&str> for Stat {
    type Error = StatError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::new(s)
    }
}

impl<'de> Deserialize<'de> for Stat {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::try_from(s).map_err(serde::de::Error::custom)
    }
}

impl fmt::Display for Stat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Error returned when constructing a `Stat` from invalid input.
#[derive(Debug, Clone, thiserror::Error)]
#[non_exhaustive]
pub enum StatError {
    /// The input was empty or contained only whitespace.
    #[error("stat name must not be blank")]
    Blank,
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn display_shows_inner_value() {
        let nbs = NonBlankString::new("hello").unwrap();
        assert_eq!(format!("{nbs}"), "hello");
    }

    #[test]
    fn stat_canonicalizes_to_uppercase() {
        assert_eq!(Stat::new("strength").unwrap().as_str(), "STRENGTH");
        assert_eq!(Stat::new("Strength").unwrap().as_str(), "STRENGTH");
        assert_eq!(Stat::new("STRENGTH").unwrap().as_str(), "STRENGTH");
        assert_eq!(Stat::new("  influence  ").unwrap().as_str(), "INFLUENCE");
    }

    #[test]
    fn stat_equal_across_casing() {
        let a = Stat::new("Influence").unwrap();
        let b = Stat::new("INFLUENCE").unwrap();
        let c = Stat::new("influence").unwrap();
        assert_eq!(a, b);
        assert_eq!(b, c);
    }

    #[test]
    fn stat_rejects_blank() {
        assert!(Stat::new("").is_err());
        assert!(Stat::new("   ").is_err());
        assert!(Stat::new("\t\n").is_err());
    }

    #[test]
    fn stat_roundtrips_through_json_canonical() {
        let stat = Stat::new("Influence").unwrap();
        let json = serde_json::to_string(&stat).unwrap();
        assert_eq!(json, "\"INFLUENCE\"");
        let back: Stat = serde_json::from_str(&json).unwrap();
        assert_eq!(back, stat);
    }

    #[test]
    fn stat_deserializes_case_insensitively() {
        let a: Stat = serde_json::from_str("\"NERVE\"").unwrap();
        let b: Stat = serde_json::from_str("\"Nerve\"").unwrap();
        let c: Stat = serde_json::from_str("\"nerve\"").unwrap();
        assert_eq!(a, b);
        assert_eq!(b, c);
        assert_eq!(a.as_str(), "NERVE");
    }

    #[test]
    fn stat_deserialize_rejects_blank() {
        let result: Result<Stat, _> = serde_json::from_str("\"\"");
        assert!(result.is_err());
        let result: Result<Stat, _> = serde_json::from_str("\"   \"");
        assert!(result.is_err());
    }
}

//! Validated newtype for genre pack codes.
//!
//! Genre codes are snake_case identifiers like `"mutant_wasteland"` or `"low_fantasy"`.
//! They must be lowercase, use only `[a-z0-9_]`, and cannot start or end with underscore.

use serde::{Deserialize, Serialize};
use std::fmt;

/// A validated genre pack code (e.g., `"mutant_wasteland"`).
///
/// The inner value is private — access via `as_str()`.
/// Construction validates — `new("")` returns `Err`.
/// Deserialization validates — custom `Deserialize` calls `new()`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct GenreCode(String);

impl GenreCode {
    /// Create a new `GenreCode`, validating snake_case format.
    ///
    /// Valid codes: lowercase alphanumeric with underscores, no leading/trailing underscores.
    pub fn new(code: &str) -> Result<Self, GenreCodeError> {
        let trimmed = code.trim();
        if trimmed.is_empty() {
            return Err(GenreCodeError::Empty);
        }
        if trimmed.starts_with('_') || trimmed.ends_with('_') {
            return Err(GenreCodeError::InvalidFormat(trimmed.to_string()));
        }
        for ch in trimmed.chars() {
            if !ch.is_ascii_lowercase() && !ch.is_ascii_digit() && ch != '_' {
                return Err(GenreCodeError::InvalidFormat(trimmed.to_string()));
            }
        }
        Ok(Self(trimmed.to_string()))
    }

    /// Access the validated code string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for GenreCode {
    type Error = GenreCodeError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::new(&s)
    }
}

impl<'de> Deserialize<'de> for GenreCode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::try_from(s).map_err(serde::de::Error::custom)
    }
}

impl fmt::Display for GenreCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Error returned when a genre code is invalid.
#[derive(Debug, Clone, thiserror::Error)]
#[non_exhaustive]
pub enum GenreCodeError {
    /// The input was empty or whitespace-only.
    #[error("genre code must not be empty")]
    Empty,
    /// The input contains invalid characters or format.
    #[error("invalid genre code format: '{0}' (must be lowercase snake_case)")]
    InvalidFormat(String),
}

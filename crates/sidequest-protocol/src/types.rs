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

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn display_shows_inner_value() {
        let nbs = NonBlankString::new("hello").unwrap();
        assert_eq!(format!("{nbs}"), "hello");
    }
}

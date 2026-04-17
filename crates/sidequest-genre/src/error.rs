//! Error types for the genre pack loader.

use std::fmt;

/// Errors that can occur when loading, resolving, or validating genre packs.
///
/// This enum is `#[non_exhaustive]` so downstream crates must use a wildcard
/// arm when matching, allowing new variants to be added without breaking changes.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum GenreError {
    /// A YAML file could not be read or parsed.
    #[error("failed to load {path}: {detail}")]
    LoadError {
        /// Path to the file that failed to load.
        path: String,
        /// Description of what went wrong.
        detail: String,
    },

    /// A trope `extends` chain contains a cycle.
    #[error("cycle detected in trope inheritance: {trope}")]
    CycleDetected {
        /// The trope name where the cycle was detected.
        trope: String,
    },

    /// A trope references a parent via `extends` that does not exist.
    #[error("trope '{trope}' extends '{parent}' which does not exist")]
    MissingParent {
        /// The trope with the dangling reference.
        trope: String,
        /// The parent name that was not found.
        parent: String,
    },

    /// Cross-reference validation failed.
    #[error("validation error: {message}")]
    ValidationError {
        /// Description of the validation failure.
        message: String,
    },

    /// An I/O error occurred while reading a tier file.
    #[error("I/O error: {message}")]
    IoError {
        /// Description of the I/O failure.
        message: String,
    },

    /// Genre pack not found in any search path.
    #[error("genre pack '{code}' not found; searched: {}", searched.join(", "))]
    NotFound {
        /// The genre code that was searched for.
        code: String,
        /// The paths that were searched.
        searched: Vec<String>,
    },
}

/// A collection of validation errors, supporting error aggregation.
///
/// Instead of failing on the first error, validation collects all errors
/// and reports them together.
#[derive(Debug)]
pub struct ValidationErrors {
    errors: Vec<GenreError>,
}

impl ValidationErrors {
    /// Create an empty error collector.
    pub fn new() -> Self {
        Self { errors: Vec::new() }
    }

    /// Add a validation error to the collection.
    pub fn push(&mut self, error: GenreError) {
        self.errors.push(error);
    }

    /// Returns the number of collected errors.
    pub fn len(&self) -> usize {
        self.errors.len()
    }

    /// Returns true if no errors have been collected.
    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    /// Convert into a `Result`: `Ok(())` if empty, `Err(self)` if non-empty.
    pub fn into_result(self) -> Result<(), Self> {
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(self)
        }
    }

    /// Access the collected errors as a slice.
    pub fn errors(&self) -> &[GenreError] {
        &self.errors
    }
}

impl Default for ValidationErrors {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ValidationErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} validation error(s):", self.errors.len())?;
        for (i, err) in self.errors.iter().enumerate() {
            write!(f, "\n  {}: {err}", i + 1)?;
        }
        Ok(())
    }
}

impl std::error::Error for ValidationErrors {}

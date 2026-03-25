//! Error types for the genre pack loader.

use std::fmt;

/// Errors that can occur when loading, resolving, or validating genre packs.
///
/// This enum is `#[non_exhaustive]` so downstream crates must use a wildcard
/// arm when matching, allowing new variants to be added without breaking changes.
#[derive(Debug)]
#[non_exhaustive]
pub enum GenreError {
    /// A YAML file could not be read or parsed.
    LoadError {
        /// Path to the file that failed to load.
        path: String,
        /// Description of what went wrong.
        source: String,
    },

    /// A trope `extends` chain contains a cycle.
    CycleDetected {
        /// The trope name where the cycle was detected.
        trope: String,
    },

    /// A trope references a parent via `extends` that does not exist.
    MissingParent {
        /// The trope with the dangling reference.
        trope: String,
        /// The parent name that was not found.
        parent: String,
    },

    /// Cross-reference validation failed.
    ValidationError {
        /// Description of the validation failure.
        message: String,
    },

    /// Genre pack not found in any search path.
    NotFound {
        /// The genre code that was searched for.
        code: String,
        /// The paths that were searched.
        searched: Vec<String>,
    },
}

impl fmt::Display for GenreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GenreError::LoadError { path, source } => {
                write!(f, "failed to load {path}: {source}")
            }
            GenreError::CycleDetected { trope } => {
                write!(f, "cycle detected in trope inheritance: {trope}")
            }
            GenreError::MissingParent { trope, parent } => {
                write!(f, "trope '{trope}' extends '{parent}' which does not exist")
            }
            GenreError::ValidationError { message } => {
                write!(f, "validation error: {message}")
            }
            GenreError::NotFound { code, searched } => {
                write!(
                    f,
                    "genre pack '{code}' not found; searched: {}",
                    searched.join(", ")
                )
            }
        }
    }
}

impl std::error::Error for GenreError {}

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

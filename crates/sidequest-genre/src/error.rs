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
        }
    }
}

impl std::error::Error for GenreError {}

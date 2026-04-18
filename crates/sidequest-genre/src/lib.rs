//! SideQuest Genre — YAML genre pack loader and models.
//!
//! This crate loads and validates genre pack YAML files, providing a strongly-typed
//! interface to the narrative structures defined by the genre packs.
//!
//! # Usage
//!
//! ```no_run
//! use std::path::Path;
//! let pack = sidequest_genre::load_genre_pack(Path::new("genre_packs/mutant_wasteland")).unwrap();
//! pack.validate().unwrap();
//! ```

#![warn(missing_docs)]

// Alias this crate under its external name so the `Layered` derive macro's
// absolute paths (`::sidequest_genre::resolver::LayeredMerge`) resolve when
// the derive is used from within this crate's own source files.
extern crate self as sidequest_genre;

/// Archetype resolution on the Layered Content Model framework.
pub mod archetype;
mod cache;
mod error;
mod genre_code;
mod loader;
/// Character-level Markov chain for fantasy word generation.
pub mod markov;
/// Genre pack model structs — types for all YAML-declared game data.
pub mod models;
/// Template-based name generator with corpus blending.
pub mod names;
mod resolve;
/// Four-tier content resolver: Global → Genre → World → Culture provenance tracking.
pub mod resolver;
pub use resolver::*;
/// Per-tier content schemas with `deny_unknown_fields` enforcement.
pub mod schema;
mod util;
mod validate;

pub use sidequest_protocol;

/// `#[derive(Layered)]` proc macro, re-exported for consumers.
pub use sidequest_genre_layered_derive::Layered;

// Re-export the public API
pub use cache::GenreCache;
pub use error::{GenreError, ValidationErrors};
pub use genre_code::{GenreCode, GenreCodeError};
pub use loader::{load_genre_pack, load_interaction_table, load_rules_config, GenreLoader};
pub use models::*;
pub use resolve::resolve_trope_inheritance;

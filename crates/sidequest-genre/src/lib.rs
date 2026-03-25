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

mod error;
mod loader;
mod models;
mod resolve;
mod util;
mod validate;

pub use sidequest_protocol;

// Re-export the public API
pub use error::GenreError;
pub use loader::load_genre_pack;
pub use models::*;
pub use resolve::resolve_trope_inheritance;

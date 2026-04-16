//! Scene harness fixture loader.
//!
//! Hydrates a `GameSnapshot` from a YAML fixture, producing a save file that
//! the normal `dispatch_connect` restore path will pick up. Used by the
//! `DEV_SCENES=1` dev route and the `sidequest-fixture` CLI to drop directly
//! into an active encounter for iteration.
//!
//! Anti-scope: fixtures reference `encounter.type` by key; beats, metrics,
//! and thresholds always come from the genre pack's `ConfrontationDef`. No
//! inline beat authoring.

pub mod error;
pub mod hydrate;
pub mod schema;

pub use error::FixtureError;
pub use hydrate::{hydrate_fixture, load_fixture, save_path_for};
pub use schema::{Fixture, FixtureEncounter, FixtureNpc};

//! Genre pack model structs.
//!
//! Structs use `#[serde(deny_unknown_fields)]` where appropriate to catch YAML
//! typos. Content structs that genre packs extend use `#[serde(flatten)]` extras
//! bags instead, allowing genre-specific fields without breaking deserialization.

mod audio;
mod axes;
/// Base archetype axis definitions — Jungian archetypes, RPG roles, NPC narrative roles.
pub mod archetype_axes;
mod character;
mod culture;
mod inventory;
mod legends;
mod lore;
mod narrative;
pub mod ocean;
mod pack;
mod progression;
mod rules;
mod scenario;
mod theme;
mod tropes;
/// World configuration, cartography, and room graph types.
pub mod world;

pub use audio::*;
pub use axes::*;
pub use character::*;
pub use culture::*;
pub use inventory::*;
pub use legends::*;
pub use lore::*;
pub use narrative::*;
pub use ocean::*;
pub use pack::*;
pub use progression::*;
pub use rules::*;
pub use scenario::*;
pub use theme::*;
pub use tropes::*;
pub use world::*;

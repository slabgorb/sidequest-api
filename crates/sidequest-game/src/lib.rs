//! SideQuest Game — Game state, characters, combat, and chase engines.
//!
//! This crate implements the core game simulation, including state management,
//! character models, combat resolution, and chase sequences.

#![warn(missing_docs)]

pub mod character;
pub mod combatant;
pub mod disposition;
pub mod hp;
pub mod inventory;
pub mod npc;

pub use character::Character;
pub use combatant::Combatant;
pub use disposition::{Attitude, Disposition};
pub use hp::clamp_hp;
pub use inventory::{Inventory, InventoryError, Item};
pub use npc::Npc;

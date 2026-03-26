//! SideQuest Game — Game state, characters, combat, and chase engines.
//!
//! This crate implements the core game simulation, including state management,
//! character models, combat resolution, and chase sequences.

#![warn(missing_docs)]

pub mod builder;
pub mod character;
pub mod chase;
pub mod combat;
pub mod combatant;
pub mod creature_core;
pub mod delta;
pub mod disposition;
pub mod hp;
pub mod inventory;
pub mod narrative;
pub mod npc;
pub mod persistence;
pub mod progression;
pub mod session;
pub mod state;
pub mod turn;

pub use character::Character;
pub use chase::{ChaseRound, ChaseState, ChaseType};
pub use combat::{CombatState, DamageEvent, RoundResult, StatusEffect, StatusEffectKind};
pub use combatant::Combatant;
pub use creature_core::CreatureCore;
pub use delta::{StateDelta, StateSnapshot};
pub use disposition::{Attitude, Disposition};
pub use hp::clamp_hp;
pub use inventory::{Inventory, InventoryError, Item};
pub use narrative::NarrativeEntry;
pub use npc::Npc;
pub use persistence::{GameStore, PersistenceError, SaveInfo};
pub use persistence::{PersistError, SavedSession, SessionMeta, SessionStore, SqliteStore};
pub use progression::{level_to_damage, level_to_defense, level_to_hp, xp_for_level};
pub use session::SessionManager;
pub use state::{ChasePatch, CombatPatch, GameSnapshot, WorldStatePatch};
pub use turn::{TurnManager, TurnPhase};

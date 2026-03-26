//! SideQuest Game — Game state, characters, combat, and chase engines.
//!
//! This crate implements the core game simulation, including state management,
//! character models, combat resolution, and chase sequences.

#![warn(missing_docs)]

pub mod ability;
pub mod barrier;
pub mod beat_filter;
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
pub mod multiplayer;
pub mod narrative;
pub mod npc;
pub mod persistence;
pub mod render_queue;
pub mod progression;
pub mod segmenter;
pub mod session;
pub mod state;
pub mod subject;
pub mod trope;
pub mod turn;
pub mod turn_mode;

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
pub use render_queue::{
    compute_content_hash, tier_to_dimensions, EnqueueResult, ImageDimensions, QueueError,
    RenderJobResult, RenderQueue, RenderQueueConfig, RenderStatus, DEFAULT_CACHE_TTL,
    MAX_QUEUE_DEPTH,
};
pub use progression::{level_to_damage, level_to_defense, level_to_hp, xp_for_level};
pub use session::SessionManager;
pub use state::{broadcast_state_changes, ChasePatch, CombatPatch, GameSnapshot, NpcPatch, WorldStatePatch};
pub use subject::{
    ExtractionContext, RenderSubject, SceneType, SubjectExtractor, SubjectTier, TierRules,
};
pub use beat_filter::{BeatFilter, BeatFilterConfig, FilterContext, FilterDecision};
pub use segmenter::{Segment, SentenceSegmenter};
pub use turn::{TurnManager, TurnPhase};

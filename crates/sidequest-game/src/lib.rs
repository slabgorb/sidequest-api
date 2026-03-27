//! SideQuest Game — Game state, characters, combat, and chase engines.
//!
//! This crate implements the core game simulation, including state management,
//! character models, combat resolution, and chase sequences.

#![warn(missing_docs)]

pub mod ability;
pub mod audio_mixer;
pub mod barrier;
pub mod beat_filter;
pub mod builder;
pub mod catch_up;
pub mod character;
pub mod chase;
pub mod combat;
pub mod combatant;
pub mod creature_core;
pub mod delta;
pub mod disposition;
pub mod guest_npc;
pub mod hp;
pub mod inventory;
pub mod multiplayer;
pub mod music_director;
pub mod narrative;
pub mod npc;
pub mod perception;
pub mod persistence;
pub mod prerender;
pub mod progression;
pub mod render_queue;
pub mod segmenter;
pub mod state;
pub mod subject;
pub mod theme_rotator;
pub mod trope;
pub mod tension_tracker;
pub mod tts_stream;
pub mod turn;
pub mod turn_mode;
pub mod turn_reminder;
pub mod voice_router;

pub use audio_mixer::{AudioMixer, DuckConfig};
pub use beat_filter::{BeatFilter, BeatFilterConfig, FilterContext, FilterDecision};
pub use character::Character;
pub use chase::{ChaseRound, ChaseState, ChaseType};
pub use combat::{CombatState, DamageEvent, RoundResult, StatusEffect, StatusEffectKind};
pub use combatant::Combatant;
pub use creature_core::CreatureCore;
pub use delta::{StateDelta, StateSnapshot};
pub use disposition::{Attitude, Disposition};
pub use hp::clamp_hp;
pub use inventory::{Inventory, InventoryError, Item};
pub use music_director::{AudioAction, AudioChannel, AudioCue, Mood, MoodClassification, MoodContext, MusicDirector};
pub use narrative::NarrativeEntry;
pub use npc::Npc;
pub use persistence::{
    PersistError, PersistenceHandle, PersistenceWorker, SaveListEntry,
    SavedSession, SessionMeta, SessionStore, SqliteStore,
};
pub use prerender::{PrerenderConfig, PrerenderContext, PrerenderScheduler, WasteTracker};
pub use progression::{level_to_damage, level_to_defense, level_to_hp, xp_for_level};
pub use render_queue::{
    compute_content_hash, tier_to_dimensions, EnqueueResult, ImageDimensions, QueueError,
    RenderJobResult, RenderQueue, RenderQueueConfig, RenderStatus, DEFAULT_CACHE_TTL,
    MAX_QUEUE_DEPTH,
};
pub use segmenter::{Segment, SentenceSegmenter};
pub use state::{
    broadcast_state_changes, ChasePatch, CombatPatch, GameSnapshot, NpcPatch, WorldStatePatch,
};
pub use subject::{
    ExtractionContext, RenderSubject, SceneType, SubjectExtractor, SubjectTier, TierRules,
};
pub use tension_tracker::{
    CombatEvent, DeliveryMode, DramaThresholds, PacingHint, TensionTracker,
};
pub use theme_rotator::{RotationConfig, ThemeRotator};
pub use turn::{TurnManager, TurnPhase};
pub use voice_router::{VoiceAssignment, VoiceRouter};

//! SideQuest Game — Game state, characters, combat, and chase engines.
//!
//! This crate implements the core game simulation, including state management,
//! character models, combat resolution, and chase sequences.

#![warn(missing_docs)]

pub mod ability;
pub mod accusation;
pub mod achievement;
pub mod affinity;
pub mod belief_state;
pub mod audio_mixer;
pub mod axis;
pub mod barrier;
pub mod beat_filter;
pub mod builder;
pub mod catch_up;
pub mod character;
pub mod chase;
pub mod chase_depth;
pub mod clue_activation;
pub mod combat;
pub mod combatant;
pub mod commands;
pub mod conlang;
pub mod consequence;
pub mod continuity;
pub mod creature_core;
pub mod delta;
pub mod disposition;
pub mod encounter;
pub mod engagement;
pub mod faction_agenda;
pub mod gossip;
pub mod guest_npc;
pub mod hp;
pub mod inventory;
pub mod journal;
pub mod known_fact;
pub mod lore;
pub mod merchant;
pub mod multiplayer;
pub mod music_director;
pub mod narrative;
pub mod narrative_sheet;
pub mod npc;
pub mod npc_actions;
pub mod ocean;
pub mod ocean_shift_proposals;
pub mod perception;
pub mod persistence;
pub mod preprocessor;
pub mod prerender;
pub mod progression;
pub mod render_queue;
pub mod room_movement;
pub mod scenario_state;
pub mod scene_relevance;
pub mod scene_directive;
pub mod session_restore;
pub mod segmenter;
pub mod slash_router;
pub mod state;
pub mod subject;
pub mod tension_tracker;
pub mod theme_rotator;
pub mod trope;
pub mod tts_stream;
pub mod turn;
pub mod turn_mode;
pub mod turn_reminder;
pub mod voice_router;
pub mod world_materialization;

pub use accusation::{
    Accusation, AccusationResult, EvidenceSummary, EvidenceQuality, evaluate_accusation,
};
pub use achievement::{Achievement, AchievementTracker};
pub use belief_state::{Belief, BeliefSource, BeliefState, Credibility};
pub use affinity::{
    AffinityState, AffinityTierUpEvent, check_affinity_thresholds,
    format_abilities_context, increment_affinity_progress, resolve_abilities, MAX_TIER, TIER_NAMES,
};
pub use audio_mixer::{AudioMixer, DuckConfig};
pub use axis::{format_tone_context, AxisValue, ToneCommand};
pub use beat_filter::{BeatFilter, BeatFilterConfig, FilterContext, FilterDecision};
pub use character::Character;
pub use chase::{ChaseRound, ChaseState, ChaseType};
pub use chase_depth::{
    apply_terrain_to_rig, camera_for_phase, check_outcome, cinematography_for_phase,
    danger_for_beat, format_chase_context, phase_for_beat, sentence_range_for_drama,
    terrain_modifiers, BeatDecision, CameraMode, ChaseActor, ChaseBeat, ChaseCinematography,
    ChaseOutcome, ChasePhase, ChaseRole, RigDamageTier, RigStats, RigType, TerrainModifiers,
};
pub use clue_activation::{
    ClueActivation, ClueGraph, ClueNode, ClueType, ClueVisibility, DiscoveryMethod,
};
pub use combat::{CombatOutcome, CombatState, DamageEvent, RoundResult, StatusEffect, StatusEffectKind};
pub use combatant::Combatant;
pub use conlang::{
    format_name_bank_for_prompt, GeneratedName, Morpheme, MorphemeCategory, MorphemeGlossary,
    NameBank, NameGenConfig, NamePattern,
};
pub use consequence::{ConsequenceCategory, GenieWish, WishConsequenceEngine, WishStatus};
pub use continuity::{Contradiction, ContradictionCategory, ValidationResult};
pub use creature_core::CreatureCore;
pub use delta::{StateDelta, StateSnapshot};
pub use disposition::{Attitude, Disposition};
pub use encounter::{
    EncounterActor, EncounterMetric, EncounterPhase, MetricDirection, SecondaryStats, StatValue,
    StructuredEncounter,
};
pub use faction_agenda::{AgendaUrgency, FactionAgenda, FactionAgendaError};
pub use gossip::{GossipEngine, PropagationResult};
pub use hp::clamp_hp;
pub use inventory::{Inventory, InventoryError, Item};
pub use merchant::{
    calculate_price, execute_buy, execute_sell, format_merchant_context,
    MerchantError, MerchantTransaction, MerchantTransactionRequest, TransactionType,
};
pub use known_fact::{Confidence, DiscoveredFact, FactSource, KnownFact};
pub use lore::{
    accumulate_lore, accumulate_lore_batch, cosine_similarity, format_lore_context,
    query_language_knowledge, record_language_knowledge, record_name_knowledge,
    seed_lore_from_char_creation, seed_lore_from_genre_pack, select_lore_for_prompt,
    summarize_lore_retrieval, FragmentSummary, LoreCategory, LoreFragment,
    LoreRetrievalSummary, LoreSource, LoreStore,
};
pub use music_director::{
    AudioAction, AudioChannel, AudioCue, Mood, MoodClassification, MoodClassificationWithReason,
    MoodContext, MusicDirector, MusicTelemetry,
};
pub use sidequest_genre::TrackVariation;
pub use narrative::NarrativeEntry;
pub use npc::{enrich_registry_from_npcs, Npc, NpcRegistryEntry};
pub use npc_actions::{available_actions, select_npc_action, NpcAction, ScenarioRole};
pub use ocean::{OceanDimension, OceanProfile, OceanShift, OceanShiftLog};
pub use ocean_shift_proposals::{
    apply_ocean_shifts, propose_ocean_shifts, OceanShiftProposal,
    PersonalityEvent,
};
pub use persistence::{
    PersistError, PersistenceHandle, PersistenceWorker, SaveListEntry, SavedSession, SessionMeta,
    SessionStore, SqliteStore,
};
pub use preprocessor::PreprocessedAction;
pub use prerender::{PrerenderConfig, PrerenderContext, PrerenderScheduler, WasteTracker};
pub use progression::{level_to_damage, level_to_defense, level_to_hp, xp_for_level};
pub use render_queue::{
    compute_content_hash, tier_to_dimensions, EnqueueResult, ImageDimensions, QueueError,
    RenderJobResult, RenderQueue, RenderQueueConfig, RenderStatus, DEFAULT_CACHE_TTL,
    MAX_QUEUE_DEPTH,
};
pub use scenario_state::{ScenarioEvent, ScenarioEventType, ScenarioState};
pub use scene_relevance::{ImagePromptVerdict, SceneRelevanceValidator};
pub use segmenter::{Segment, SentenceSegmenter};
pub use room_movement::{apply_validated_move, build_room_graph_explored, init_room_graph_location, validate_room_transition, DispatchError, RoomTransition};
pub use state::{
    broadcast_state_changes, build_protocol_delta, ChasePatch, CombatPatch, DiscoveredRooms,
    GameSnapshot, NpcPatch, WorldStatePatch,
};
pub use subject::{
    ExtractionContext, RenderSubject, SceneType, SubjectExtractor, SubjectTier, TierRules,
};
pub use tension_tracker::{CombatEvent, DeliveryMode, DramaThresholds, PacingHint, TensionTracker};
pub use theme_rotator::{RotationConfig, ThemeRotator};
pub use turn::{TurnManager, TurnPhase};
pub use voice_router::{VoiceAssignment, VoiceRouter};
pub use world_materialization::{
    materialize_from_genre_pack, materialize_world, parse_history_chapters, CampaignMaturity,
    ChapterCharacter, ChapterNarrativeEntry, ChapterNpc, ChapterTrope, HistoryChapter,
    WorldBuilder,
};

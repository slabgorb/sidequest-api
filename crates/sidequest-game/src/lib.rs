//! SideQuest Game — Game state, characters, combat, and chase engines.
//!
//! This crate implements the core game simulation, including state management,
//! character models, combat resolution, and chase sequences.

#![warn(missing_docs)]

pub mod ability;
pub mod accusation;
pub mod achievement;
pub mod affinity;
pub mod audio_mixer;
pub mod axis;
pub mod barrier;
pub mod beat_filter;
pub mod belief_state;
pub mod builder;
pub mod catch_up;
pub mod character;
pub mod chase_depth;
pub mod clue_activation;
pub mod combatant;
pub mod commands;
pub mod conlang;
pub mod consequence;
pub mod continuity;
pub mod creature_core;
pub mod delta;
pub mod dice;
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
pub mod monster_manual;
pub mod multiplayer;
pub mod music_director;
pub mod narrative;
pub mod narrative_sheet;
pub mod npc;
pub mod npc_actions;
pub mod ocean;
pub mod ocean_shift_proposals;
pub mod party_reconciliation;
pub mod perception;
pub mod persistence;
pub mod preprocessor;
pub mod prerender;
pub mod progression;
pub mod render_queue;
pub mod resource_pool;
pub mod room_movement;
pub mod scenario_archiver;
pub mod scenario_scoring;
pub mod scenario_state;
pub mod scene_directive;
pub mod scene_relevance;
pub mod sealed_round;
pub mod session_restore;
pub mod slash_router;
pub mod state;
pub mod subject;
/// Tactical grid maps — ASCII room geometry for dungeon combat (ADR-071).
pub mod tactical;
pub mod tension_tracker;
pub mod theme_rotator;
pub mod treasure_xp;
pub mod trope;
pub mod turn;
pub mod turn_mode;
pub mod turn_reminder;
pub mod world_materialization;

pub use accusation::{
    evaluate_accusation, Accusation, AccusationResult, EvidenceQuality, EvidenceSummary,
};
pub use achievement::{Achievement, AchievementTracker};
pub use affinity::{
    check_affinity_thresholds, format_abilities_context, increment_affinity_progress,
    resolve_abilities, AffinityState, AffinityTierUpEvent, MAX_TIER, TIER_NAMES,
};
pub use audio_mixer::AudioMixer;
pub use axis::{format_tone_context, AxisValue, ToneCommand};
pub use beat_filter::{BeatFilter, BeatFilterConfig, FilterContext, FilterDecision};
pub use belief_state::{Belief, BeliefSource, BeliefState, Credibility};
pub use character::Character;
pub use chase_depth::{
    apply_terrain_to_rig, camera_for_phase, check_outcome, cinematography_for_phase,
    danger_for_beat, format_chase_context, phase_for_beat, sentence_range_for_drama,
    terrain_modifiers, BeatDecision, CameraMode, ChaseActor, ChaseBeat, ChaseCinematography,
    ChaseOutcome, ChasePhase, ChaseRole, RigDamageTier, RigStats, RigType, TerrainModifiers,
};
pub use clue_activation::{
    ClueActivation, ClueGraph, ClueNode, ClueType, ClueVisibility, DiscoveryMethod,
};
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
pub use inventory::{Inventory, InventoryError, Item, ItemState};
pub use known_fact::{Confidence, DiscoveredFact, FactSource, KnownFact};
pub use lore::{
    accumulate_lore, accumulate_lore_batch, cosine_similarity,
    format_language_knowledge_for_prompt, format_lore_context, query_all_language_knowledge,
    query_language_knowledge, record_language_knowledge, record_name_knowledge,
    seed_lore_from_char_creation, seed_lore_from_genre_pack, select_lore_for_prompt,
    summarize_lore_retrieval, FragmentSummary, LoreCategory, LoreFragment, LoreRetrievalSummary,
    LoreSource, LoreStore,
};
pub use merchant::{
    calculate_price, execute_buy, execute_sell, format_merchant_context, MerchantError,
    MerchantTransaction, MerchantTransactionRequest, TransactionType,
};
pub use music_director::{
    AudioAction, AudioChannel, AudioCue, FactionContext, Mood, MoodClassification,
    MoodClassificationWithReason, MoodContext, MoodKey, MusicDirector, MusicEvalResult,
    MusicTelemetry,
};
pub use narrative::NarrativeEntry;
pub use npc::{enrich_registry_from_npcs, Npc, NpcRegistryEntry};
pub use npc_actions::{available_actions, select_npc_action, NpcAction, ScenarioRole};
pub use ocean::{OceanDimension, OceanProfile, OceanShift, OceanShiftLog};
pub use ocean_shift_proposals::{
    apply_ocean_shifts, propose_ocean_shifts, OceanShiftProposal, PersonalityEvent,
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
    RenderJobParams, RenderJobResult, RenderQueue, RenderQueueConfig, RenderStatus,
    DEFAULT_CACHE_TTL, MAX_QUEUE_DEPTH,
};
pub use resource_pool::{
    mint_threshold_lore, ResourcePatch, ResourcePatchError, ResourcePatchOp, ResourcePatchResult,
    ResourcePool, ResourceThreshold,
};
pub use room_movement::{
    apply_validated_move, build_room_graph_explored, init_room_graph_location,
    validate_room_transition, DispatchError, RoomTransition,
};
pub use scenario_scoring::{
    score_scenario, DeductionQuality, ScenarioGrade, ScenarioScore, ScenarioScoreInput,
};
pub use scenario_state::{ScenarioEvent, ScenarioEventType, ScenarioState};
pub use scene_relevance::{ImagePromptVerdict, SceneRelevanceValidator};
pub use sidequest_genre::TrackVariation;
pub use state::{
    broadcast_state_changes, build_protocol_delta, DiscoveredRooms, GameSnapshot, NpcPatch,
    WorldStatePatch,
};
pub use subject::{
    ExtractionContext, RenderSubject, SceneType, SubjectExtractor, SubjectTier, TierRules,
};
pub use tension_tracker::{
    CombatEvent, DamageEvent, DeliveryMode, DramaThresholds, PacingHint, RoundResult,
    TensionTracker,
};
pub use theme_rotator::{RotationConfig, ThemeRotator};
pub use treasure_xp::{apply_treasure_xp, TreasureXpConfig, TreasureXpResult};
pub use turn::{TurnManager, TurnPhase};
pub use world_materialization::{
    materialize_from_genre_pack, materialize_world, parse_history_chapters, CampaignMaturity,
    ChapterCharacter, ChapterNarrativeEntry, ChapterNpc, ChapterTrope, HistoryChapter,
    WorldBuilder,
};

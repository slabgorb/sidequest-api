//! GameMessage — the core protocol enum.
//!
//! ## Tagged enums in serde
//!
//! Python's protocol uses a `type` field to identify message kind:
//! ```json
//! { "type": "PLAYER_ACTION", "payload": { "action": "..." }, "player_id": "" }
//! ```
//!
//! In Rust, `#[serde(tag = "type")]` makes serde look at the `type` field to decide
//! which variant to deserialize into. Each variant carries its own typed payload —
//! no more `payload: dict` with runtime KeyError.
//!
//! ## Struct variants vs tuple variants
//!
//! The tests construct messages like:
//! ```text
//! GameMessage::PlayerAction { payload: PlayerActionPayload { .. }, player_id: "".into() }
//! ```
//! That's a **struct variant** — named fields inside the enum variant.
//! This is different from a tuple variant (`PlayerAction(PayloadType)`).
//! Struct variants serialize each field into the JSON object alongside `type`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// NarratorVerbosity — per-session narrator verbosity control (story 14-3)
// ---------------------------------------------------------------------------

/// Controls how verbose the narrator's prose output should be.
///
/// Serializes as lowercase strings for wire compatibility with the React UI.
/// Default is `Standard`. Solo sessions default to `Verbose` via
/// `default_for_player_count()`.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum NarratorVerbosity {
    /// Keep descriptions to 1-2 sentences. Prioritize action over atmosphere.
    Concise,
    /// Standard descriptive prose — balanced detail and pacing.
    #[default]
    Standard,
    /// Elaborate with sensory details, world-building, and atmospheric prose.
    Verbose,
}

impl NarratorVerbosity {
    /// Returns the default verbosity for a given player count.
    ///
    /// Solo sessions (1 player) default to Verbose for immersive storytelling.
    /// Multiplayer sessions (2+) default to Standard for pacing.
    pub fn default_for_player_count(player_count: usize) -> Self {
        if player_count <= 1 {
            Self::Verbose
        } else {
            Self::Standard
        }
    }
}

// ---------------------------------------------------------------------------
// NarratorVocabulary — per-session narrator vocabulary/complexity control (story 14-4)
// ---------------------------------------------------------------------------

/// Controls the prose complexity and diction of narrator output.
///
/// Works alongside `NarratorVerbosity` (which controls length). Vocabulary
/// controls word choice and sentence complexity. Serializes as lowercase strings
/// for wire compatibility with the React UI. Default is `Literary`.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum NarratorVocabulary {
    /// Simple, direct language. Approximately 8th-grade reading level.
    Accessible,
    /// Rich but clear prose. Varied vocabulary without being obscure.
    #[default]
    Literary,
    /// Elevated, archaic, or mythic diction. Unrestricted complexity.
    Epic,
}

// ---------------------------------------------------------------------------
// GameMessage — the tagged enum
// ---------------------------------------------------------------------------

/// The core protocol message. Every WebSocket frame carries one of these as JSON.
///
/// `#[serde(tag = "type")]` means the JSON object's `"type"` field determines
/// which variant to use. The `#[serde(rename = "PLAYER_ACTION")]` on each variant
/// maps Rust's PascalCase to the SCREAMING_CASE the React UI expects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[non_exhaustive]
pub enum GameMessage {
    /// Player submits an action or aside.
    #[serde(rename = "PLAYER_ACTION")]
    PlayerAction {
        /// The typed payload for this message.
        payload: PlayerActionPayload,
        /// The player who sent this message (empty string from client).
        player_id: String,
    },

    /// Narrative text from the AI, optionally with state changes.
    #[serde(rename = "NARRATION")]
    Narration {
        /// The typed payload for this message.
        payload: NarrationPayload,
        /// The player who sent this message.
        player_id: String,
    },

    /// Turn-completion marker. Carries the final `StateDelta` when one exists.
    ///
    /// Emitted by the dispatch layer at the end of every narration turn.
    /// The UI processes this message through its normal state-mirror
    /// pipeline — React's automatic batching applies any final state delta
    /// in the same render commit as the preceding `Narration`, with no
    /// explicit buffering required on the client side (ADR-076 — post-TTS
    /// protocol collapse).
    #[serde(rename = "NARRATION_END")]
    NarrationEnd {
        /// The typed payload for this message.
        payload: NarrationEndPayload,
        /// The player who sent this message.
        player_id: String,
    },

    /// Server is processing (shows spinner on client).
    #[serde(rename = "THINKING")]
    Thinking {
        /// The typed payload for this message.
        payload: ThinkingPayload,
        /// The player who sent this message.
        player_id: String,
    },

    /// Session lifecycle events (connect, ready, theme).
    #[serde(rename = "SESSION_EVENT")]
    SessionEvent {
        /// The typed payload for this message.
        payload: SessionEventPayload,
        /// The player who sent this message.
        player_id: String,
    },

    /// Character creation flow (scenes, confirmation, complete).
    #[serde(rename = "CHARACTER_CREATION")]
    CharacterCreation {
        /// The typed payload for this message.
        payload: CharacterCreationPayload,
        /// The player who sent this message.
        player_id: String,
    },

    /// Turn/round tracking with optional state delta.
    #[serde(rename = "TURN_STATUS")]
    TurnStatus {
        /// The typed payload for this message.
        payload: TurnStatusPayload,
        /// The player who sent this message.
        player_id: String,
    },

    /// Full party snapshot.
    #[serde(rename = "PARTY_STATUS")]
    PartyStatus {
        /// The typed payload for this message.
        payload: PartyStatusPayload,
        /// The player who sent this message.
        player_id: String,
    },

    // NOTE: CHARACTER_SHEET and INVENTORY variants were removed in 2026-04.
    // Per-character sheet and inventory state now live on `PartyMember`
    // (`sheet` and `inventory` fields) and are broadcast via PARTY_STATUS.
    // This collapses three message types into one, eliminates the
    // observer-null race condition, and makes teammate gear visible.
    /// World map state for map overlay.
    #[serde(rename = "MAP_UPDATE")]
    MapUpdate {
        /// The typed payload for this message.
        payload: MapUpdatePayload,
        /// The player who sent this message.
        player_id: String,
    },

    // CombatEvent variant removed in story 28-9. Confrontation replaces it.
    /// Structured encounter state for confrontation overlay (standoffs, chases, negotiations).
    #[serde(rename = "CONFRONTATION")]
    Confrontation {
        /// The typed payload for this message.
        payload: ConfrontationPayload,
        /// The player who sent this message.
        player_id: String,
    },

    /// Render job queued — tells UI to show a placeholder shimmer.
    #[serde(rename = "RENDER_QUEUED")]
    RenderQueued {
        /// The typed payload for this message.
        payload: RenderQueuedPayload,
        /// The player who sent this message.
        player_id: String,
    },

    /// Image delivery (portraits, handouts, scene art).
    #[serde(rename = "IMAGE")]
    Image {
        /// The typed payload for this message.
        payload: ImagePayload,
        /// The player who sent this message.
        player_id: String,
    },

    /// Background music and sound effects.
    #[serde(rename = "AUDIO_CUE")]
    AudioCue {
        /// The typed payload for this message.
        payload: AudioCuePayload,
        /// The player who sent this message.
        player_id: String,
    },

    /// WebRTC signaling relay.
    #[serde(rename = "VOICE_SIGNAL")]
    VoiceSignal {
        /// The typed payload for this message.
        payload: VoiceSignalPayload,
        /// The player who sent this message.
        player_id: String,
    },

    /// TTS text companion (displayed alongside audio).
    #[serde(rename = "VOICE_TEXT")]
    VoiceText {
        /// The typed payload for this message.
        payload: VoiceTextPayload,
        /// The player who sent this message.
        player_id: String,
    },

    /// Queued player actions.
    #[serde(rename = "ACTION_QUEUE")]
    ActionQueue {
        /// The typed payload for this message.
        payload: ActionQueuePayload,
        /// The player who sent this message.
        player_id: String,
    },

    /// Chapter/scene boundary marker.
    #[serde(rename = "CHAPTER_MARKER")]
    ChapterMarker {
        /// The typed payload for this message.
        payload: ChapterMarkerPayload,
        /// The player who sent this message.
        player_id: String,
    },

    /// Error message to client.
    #[serde(rename = "ERROR")]
    Error {
        /// The typed payload for this message.
        payload: ErrorPayload,
        /// The player who sent this message.
        player_id: String,
    },

    /// Broadcast of all submitted player actions when a sealed-letter turn resolves.
    #[serde(rename = "ACTION_REVEAL")]
    ActionReveal {
        /// The typed payload for this message.
        payload: ActionRevealPayload,
        /// The player who sent this message (typically "server").
        player_id: String,
    },

    /// Scenario system event (Epic 7 — clue discovery, NPC actions, gossip, accusations).
    #[serde(rename = "SCENARIO_EVENT")]
    ScenarioEvent {
        /// The typed payload for this message.
        payload: ScenarioEventPayload,
        /// The player who sent this message (typically "server").
        player_id: String,
    },

    /// Achievement earned — broadcast when a trope transition triggers an achievement (story 15-13).
    #[serde(rename = "ACHIEVEMENT_EARNED")]
    AchievementEarned {
        /// The typed payload for this message.
        payload: AchievementEarnedPayload,
        /// The player who sent this message (typically "server").
        player_id: String,
    },

    /// Client requests accumulated journal entries (story 9-13).
    #[serde(rename = "JOURNAL_REQUEST")]
    JournalRequest {
        /// The typed payload for this message.
        payload: JournalRequestPayload,
        /// The player who sent this message.
        player_id: String,
    },

    /// Server responds with journal entries (story 9-13).
    #[serde(rename = "JOURNAL_RESPONSE")]
    JournalResponse {
        /// The typed payload for this message.
        payload: JournalResponsePayload,
        /// The player who sent this message (typically "server").
        player_id: String,
    },

    /// A consumable item was fully depleted (story 19-10).
    #[serde(rename = "ITEM_DEPLETED")]
    ItemDepleted {
        /// The typed payload for this message.
        payload: ItemDepletedPayload,
        /// The player who sent this message (typically "server").
        player_id: String,
    },

    /// A resource reached its minimum value (story 19-6).
    #[serde(rename = "RESOURCE_MIN_REACHED")]
    ResourceMinReached {
        /// The typed payload for this message.
        payload: ResourceMinReachedPayload,
        /// The player who sent this message (typically "server").
        player_id: String,
    },

    /// Tactical grid state for the current room (story 29-5).
    /// Sent on room entry when the room has ASCII grid data.
    #[serde(rename = "TACTICAL_STATE")]
    TacticalState {
        /// The typed payload for this message.
        payload: TacticalStatePayload,
        /// The player who sent this message.
        player_id: String,
    },

    /// Player tactical action (move, target, inspect) on the grid (story 29-5).
    #[serde(rename = "TACTICAL_ACTION")]
    TacticalAction {
        /// The typed payload for this message.
        payload: TacticalActionPayload,
        /// The player who sent this message.
        player_id: String,
    },

    /// Server asks a player to throw dice for a confrontation check (story 34-2 / ADR-074).
    ///
    /// Broadcast to all clients during the reveal phase, after the narrator sets
    /// the scene. Contains the DC (revealed NOW, not during sealed phase), the
    /// dice pool, the stat modifier, and narrator flavor. The `payload.player_id`
    /// identifies who must throw; other clients watch.
    #[serde(rename = "DICE_REQUEST")]
    DiceRequest {
        /// The typed payload for this message.
        payload: DiceRequestPayload,
        /// The player who sent this message (typically "server").
        player_id: String,
    },

    /// Rolling player submits a throw gesture to the server (story 34-2 / ADR-074).
    ///
    /// Contains physics parameters (velocity, angular, position) captured from the
    /// drag-and-flick gesture. The server authority model uses these for animation
    /// replay only — the outcome is determined server-side from an independent RNG
    /// seed. Sent by the rolling player only; spectators cannot submit throws.
    #[serde(rename = "DICE_THROW")]
    DiceThrow {
        /// The typed payload for this message.
        payload: DiceThrowPayload,
        /// The player who sent this message.
        player_id: String,
    },

    /// Server broadcasts the resolved dice outcome to all clients (story 34-2 / ADR-074).
    ///
    /// Contains per-group die faces (via `DieGroupResult`), total, outcome
    /// classification, and the physics seed plus throw parameters needed for
    /// deterministic client-side replay. All clients run identical Rapier
    /// physics from the same seed + throw params, producing visually identical
    /// animations regardless of who threw. The envelope `player_id` is the
    /// sender (typically `"server"`), while `payload.rolling_player_id`
    /// identifies whose character rolled — see `DiceRequest` for the same
    /// distinction.
    #[serde(rename = "DICE_RESULT")]
    DiceResult {
        /// The typed payload for this message.
        payload: DiceResultPayload,
        /// The player who sent this message (typically "server").
        player_id: String,
    },

    /// Player selects a structured beat action during an encounter.
    ///
    /// Sent by the client when the player clicks a beat action button in the
    /// Confrontation panel (Ram, Brake, Make Your Case, Concede, etc.).
    /// Carries the exact `beat_id` from `ConfrontationDef.beats` — NO text
    /// synthesis, NO label fallback, NO narrator interpretation.
    ///
    /// The server validates `beat_id` against the active encounter's
    /// `ConfrontationDef` and dispatches the beat directly via
    /// `beat::apply_beat_dispatch()`. The narrator is then called to describe
    /// the mechanical outcome in prose — it does NOT choose the beat.
    #[serde(rename = "BEAT_SELECTION")]
    BeatSelection {
        /// The typed payload for this message.
        payload: BeatSelectionPayload,
        /// The player who selected this beat.
        player_id: String,
    },
}

// ---------------------------------------------------------------------------
// Payload structs — one per message type
// ---------------------------------------------------------------------------

/// Player action payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerActionPayload {
    /// The action text the player typed.
    pub action: String,
    /// True if this is an out-of-character aside.
    #[serde(default)]
    pub aside: bool,
}

/// Narration payload with optional state delta and structured footnotes.
///
/// Story 9-11: Extended with `footnotes` for knowledge extraction pipeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NarrationPayload {
    /// The narrative text from the AI.
    pub text: String,
    /// Optional state changes resulting from this narration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_delta: Option<StateDelta>,
    /// Structured footnotes — new discoveries and callbacks to prior knowledge.
    /// Empty when narrator reveals nothing new and references nothing.
    #[serde(default, deserialize_with = "deserialize_null_as_empty_vec")]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub footnotes: Vec<Footnote>,
}

/// Deserialize null or missing values as empty Vec.
fn deserialize_null_as_empty_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    let opt: Option<Vec<T>> = Option::deserialize(deserializer)?;
    Ok(opt.unwrap_or_default())
}

/// A structured footnote from narrator output.
///
/// Footnotes capture discoveries and callbacks to prior knowledge.
/// New discoveries (`is_new: true`) become KnownFact entries.
/// Callbacks (`is_new: false`) link to existing KnownFacts via `fact_id`.
///
/// Story 9-11: Part of the knowledge extraction pipeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Footnote {
    /// Marker number matching `[N]` superscript in prose.
    /// Optional because the LLM sometimes omits or nulls it.
    #[serde(default)]
    pub marker: Option<u32>,
    /// Links to existing KnownFact if this is a callback (is_new: false).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fact_id: Option<String>,
    /// One-sentence description of the fact.
    pub summary: String,
    /// Classification category for the footnote.
    pub category: FactCategory,
    /// True if this is a new revelation, false if referencing prior knowledge.
    #[serde(alias = "isnew")]
    pub is_new: bool,
}

/// Classification category for narrator footnotes.
///
/// Story 9-11: Categorizes what kind of knowledge the footnote represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum FactCategory {
    /// World history, mythology, or cosmology.
    Lore,
    /// Geographic location or landmark.
    Place,
    /// NPC, faction, or named individual.
    Person,
    /// Quest objective, task, or mission.
    Quest,
    /// Character ability, skill, or power.
    Ability,
}

/// Turn-completion payload, optionally carrying the final state delta.
///
/// Sent at the end of every narration turn. The UI applies the optional
/// `state_delta` through its normal state-mirror pipeline — no explicit
/// buffering is required on the client side because React's automatic
/// batching already coalesces consecutive `setState` calls into a single
/// commit (ADR-076).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NarrationEndPayload {
    /// Optional state changes at end of narration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_delta: Option<StateDelta>,
}

/// Thinking indicator (empty payload — just shows spinner).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThinkingPayload {}

/// Session lifecycle events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionEventPayload {
    /// Event type: "connect", "connected", "ready", "theme_css".
    pub event: String,
    /// Player name (on connect).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub player_name: Option<String>,
    /// Genre slug (on connect).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    /// World slug (on connect).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub world: Option<String>,
    /// Whether player has a character (on connected).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_character: Option<bool>,
    /// Initial game state (on ready).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial_state: Option<InitialState>,
    /// Genre CSS content (on theme_css event).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub css: Option<String>,
    /// Narrator verbosity setting (story 14-3).
    /// Optional for backward compatibility — old clients that don't send it
    /// deserialize as None, and the server applies a default based on player count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub narrator_verbosity: Option<NarratorVerbosity>,

    /// Narrator vocabulary/complexity setting (story 14-4).
    /// Optional for backward compatibility — old clients that don't send it
    /// deserialize as None, and the server applies a default (Literary).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub narrator_vocabulary: Option<NarratorVocabulary>,

    /// Image generation cooldown in seconds (story 14-6).
    /// Optional for backward compatibility — old clients that don't send it
    /// deserialize as None, and the server applies a default based on player count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_cooldown_seconds: Option<u32>,
}

/// Character creation flow payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterCreationPayload {
    /// Creation phase: "scene", "confirmation", "complete".
    pub phase: String,
    /// Current scene index (1-based).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene_index: Option<u32>,
    /// Total number of scenes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_scenes: Option<u32>,
    /// Prompt text for the player.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// Recap of previous choices.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Flavor text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Available choices.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub choices: Option<Vec<CreationChoice>>,
    /// Whether freeform text input is allowed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allows_freeform: Option<bool>,
    /// Input type hint ("text", "select", etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_type: Option<String>,
    /// Genre-aware loading text for the spinner between scenes.
    /// E.g. "The ripperdoc considers your words..."
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loading_text: Option<String>,
    /// Preview of the character being created.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub character_preview: Option<serde_json::Value>,
    /// Rolled ability scores in genre-defined order. When present, the UI
    /// should render them as a structured stat block alongside the narration
    /// instead of asking the player to parse them out of inline prose.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rolled_stats: Option<Vec<RolledStat>>,
    /// Player's choice (client → server).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub choice: Option<String>,
    /// Completed character data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub character: Option<serde_json::Value>,
}

/// One rolled ability score: ability name + value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RolledStat {
    /// Ability name as defined by the genre's `ability_score_names`
    /// (e.g. "STR", "Cunning", "Grit").
    pub name: String,
    /// Rolled value (typically 3-18 for 3d6 strict).
    pub value: i32,
}

/// Turn/round tracking.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnStatusPayload {
    /// Which player this turn status is about.
    pub player_name: String,
    /// "active" = this player's turn, "resolved" = turn complete.
    pub status: String,
    /// Optional state delta.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_delta: Option<StateDelta>,
}

/// Full party snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartyStatusPayload {
    /// All party members.
    pub members: Vec<PartyMember>,
}

// CharacterSheetPayload removed 2026-04. See `CharacterSheetDetails` (nested
// inside `PartyMember.sheet`) for the replacement.

/// Full inventory snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InventoryPayload {
    /// All inventory items.
    pub items: Vec<InventoryItem>,
    /// Gold/currency amount.
    pub gold: i64,
}

/// Map update for the map overlay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapUpdatePayload {
    /// Current player location.
    pub current_location: String,
    /// Current region name.
    pub region: String,
    /// Explored locations.
    pub explored: Vec<ExploredLocation>,
    /// Fog of war bounds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fog_bounds: Option<FogBounds>,
    /// Cartography metadata from genre pack — navigation structure, regions, routes.
    /// Sent on session connect and location changes so the UI can render the world map.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cartography: Option<CartographyMetadata>,
}

/// Cartography metadata for the map overlay (story 26-10).
/// Wire-format subset of the genre pack's CartographyConfig — carries only
/// the fields the UI needs to render the world map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CartographyMetadata {
    /// Navigation mode — "region", "room_graph", or "hierarchical".
    pub navigation_mode: String,
    /// Starting region slug.
    #[serde(default)]
    pub starting_region: String,
    /// Regions keyed by slug.
    #[serde(default)]
    pub regions: HashMap<String, CartographyRegion>,
    /// Routes between regions.
    #[serde(default)]
    pub routes: Vec<CartographyRoute>,
}

/// A region in the cartography metadata (wire format for UI).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CartographyRegion {
    /// Display name.
    pub name: String,
    /// Description.
    #[serde(default)]
    pub description: String,
    /// Adjacent region slugs.
    #[serde(default)]
    pub adjacent: Vec<String>,
}

/// A route between regions in the cartography metadata (wire format for UI).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CartographyRoute {
    /// Route name.
    pub name: String,
    /// Description.
    #[serde(default)]
    pub description: String,
    /// Source region slug.
    #[serde(default)]
    pub from_id: Option<String>,
    /// Destination region slug.
    #[serde(default)]
    pub to_id: Option<String>,
}

// CombatEventPayload deleted in story 28-9 — ConfrontationPayload replaces it.

/// Render job queued — sent when a render is submitted to the daemon.
/// The UI can show a shimmer placeholder while waiting for the actual IMAGE.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderQueuedPayload {
    /// Unique render job ID (matches render_id on the eventual IMAGE message).
    pub render_id: String,
    /// Image tier ("portrait", "landscape", "scene_illustration", etc.).
    pub tier: String,
    /// Expected width in pixels.
    pub width: u32,
    /// Expected height in pixels.
    pub height: u32,
}

/// Structured encounter state for the confrontation overlay.
///
/// Maps directly to the UI's ConfrontationData interface. Sent when
/// a structured encounter (standoff, chase, negotiation) starts, updates,
/// or ends. Null/empty actors signals encounter end.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfrontationPayload {
    /// Encounter type key (e.g., "chase", "standoff", "negotiation").
    #[serde(rename = "type")]
    pub encounter_type: String,
    /// Display label (e.g., "High-Speed Chase").
    pub label: String,
    /// Category (e.g., "combat", "social", "pursuit").
    pub category: String,
    /// Participants and their roles.
    pub actors: Vec<ConfrontationActor>,
    /// Primary metric being tracked.
    pub metric: ConfrontationMetric,
    /// Available beat options for the player.
    #[serde(default)]
    pub beats: Vec<ConfrontationBeat>,
    /// Optional secondary stats (vehicle stats, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary_stats: Option<serde_json::Value>,
    /// Genre pack slug (for theming).
    pub genre_slug: String,
    /// Current mood (for atmosphere).
    #[serde(default)]
    pub mood: String,
    /// Whether the encounter is active (false = overlay should dismiss).
    #[serde(default = "default_true")]
    pub active: bool,
}

fn default_true() -> bool {
    true
}

/// A participant in a confrontation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfrontationActor {
    /// Display name of the actor (e.g., "Sheriff Reyes").
    pub name: String,
    /// Narrative role this actor plays in the confrontation
    /// (e.g., "antagonist", "witness", "ally").
    pub role: String,
    /// Optional URL to a portrait image for the actor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub portrait_url: Option<String>,
}

/// Primary metric being tracked in a confrontation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfrontationMetric {
    /// Display name of the metric (e.g., "Suspicion", "Distance").
    pub name: String,
    /// Current value of the metric.
    pub current: i32,
    /// Starting value of the metric at the beginning of the confrontation.
    pub starting: i32,
    /// Direction of progress: "ascending", "descending", or "bidirectional".
    pub direction: String,
    /// Optional upper threshold that triggers a confrontation outcome
    /// when crossed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold_high: Option<i32>,
    /// Optional lower threshold that triggers a confrontation outcome
    /// when crossed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold_low: Option<i32>,
}

/// A beat option the player can choose during a confrontation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfrontationBeat {
    /// Stable identifier for the beat (used in `BeatSelected` messages).
    pub id: String,
    /// Player-facing label describing the beat (e.g., "Stand your ground").
    pub label: String,
    /// Amount the metric changes when this beat is selected.
    #[serde(default)]
    pub metric_delta: i32,
    /// Stat check that gates the beat (empty = no check).
    #[serde(default)]
    pub stat_check: String,
    /// Optional narrative risk descriptor (e.g., "high", "fatal").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk: Option<String>,
    /// Whether selecting this beat resolves the confrontation.
    #[serde(default)]
    pub resolution: bool,
}

/// Image delivery payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImagePayload {
    /// Image URL.
    pub url: String,
    /// Alt text / description.
    pub description: String,
    /// Whether this is a journal handout.
    pub handout: bool,
    /// Unique render identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub render_id: Option<String>,
    /// Subject tier (e.g. "portrait", "scene", "landscape", "abstract").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
    /// Scene type (e.g. "combat", "dialogue", "exploration").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene_type: Option<String>,
    /// Image generation time in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation_ms: Option<u64>,
}

/// Audio cue payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioCuePayload {
    /// Music mood.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mood: Option<String>,
    /// Music track identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub music_track: Option<String>,
    /// Sound effect triggers.
    #[serde(default)]
    pub sfx_triggers: Vec<String>,
    /// Audio channel: "music", "sfx", "ambience".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    /// Audio action: "play", "fade_in", "fade_out", "duck", "restore", "stop", "configure".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// Volume level (0.0–1.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<f32>,
    /// Genre-pack mixer config: music channel volume (0.0–1.0).
    /// Sent with action "configure" on session connect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub music_volume: Option<f32>,
    /// Genre-pack mixer config: SFX channel volume (0.0–1.0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sfx_volume: Option<f32>,
    /// Genre-pack mixer config: voice channel volume (0.0–1.0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice_volume: Option<f32>,
    /// Genre-pack mixer config: crossfade duration in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crossfade_ms: Option<u32>,
}

/// WebRTC voice signaling payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VoiceSignalPayload {
    /// Target peer (outbound).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// Source peer (inbound).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// WebRTC signaling data.
    pub signal: serde_json::Value,
}

/// TTS text companion payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VoiceTextPayload {
    /// The spoken text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// Action queue payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionQueuePayload {
    /// Queued actions.
    #[serde(default)]
    pub actions: Vec<serde_json::Value>,
}

/// Chapter/scene marker payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChapterMarkerPayload {
    /// Chapter title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Current location name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
}

/// Error payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ErrorPayload {
    /// Human-readable error message.
    pub message: String,
    /// When true, the client must re-send a SESSION_EVENT{connect} before
    /// retrying.  Set when the server has no session for this connection
    /// (e.g. after a server restart).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reconnect_required: Option<bool>,
}

/// Action reveal payload — broadcast when a sealed-letter turn resolves.
///
/// Story 13-3: Carries each player's submitted action for the full party to see.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionRevealPayload {
    /// Individual player actions submitted during the turn.
    pub actions: Vec<PlayerActionEntry>,
    /// Turn number this reveal belongs to.
    pub turn_number: u32,
    /// Character names of players who were auto-resolved (timed out).
    #[serde(default)]
    pub auto_resolved: Vec<String>,
}

/// A single player's submitted action in an action reveal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerActionEntry {
    /// Character name (display name, not player ID).
    pub character_name: String,
    /// Player identifier.
    pub player_id: String,
    /// The action text the player submitted.
    pub action: String,
}

// ---------------------------------------------------------------------------
// Shared sub-types used across payloads
// ---------------------------------------------------------------------------

/// State changes carried in NARRATION and TURN_STATUS.
///
/// All fields are optional — only changed state is included.
/// This maps to the TypeScript `StateDelta` interface in the React UI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateDelta {
    /// New location, if changed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    /// Updated character states, merged by name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub characters: Option<Vec<CharacterState>>,
    /// Updated quest statuses, merged by key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quests: Option<HashMap<String, String>>,
    /// Items gained by the player this turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items_gained: Option<Vec<ItemGained>>,
}

/// An item the player gained during narration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemGained {
    /// Short item name (e.g., "sealed matte-black case").
    pub name: String,
    /// One-sentence description.
    #[serde(default = "default_item_description")]
    pub description: String,
    /// Category (weapon, armor, tool, consumable, quest, misc).
    #[serde(default = "default_item_category")]
    pub category: String,
}

fn default_item_description() -> String {
    "An item found during adventure.".to_string()
}

fn default_item_category() -> String {
    "misc".to_string()
}

/// Character state as seen by the client (UI-facing).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterState {
    /// Character name (merge key).
    pub name: String,
    /// Current hit points.
    pub hp: i32,
    /// Maximum hit points.
    pub max_hp: i32,
    /// Character level.
    #[serde(default)]
    pub level: u32,
    /// Character class (e.g., "Ranger", "Mage").
    #[serde(default)]
    pub class: String,
    /// Active status effects.
    pub statuses: Vec<String>,
    /// Inventory item names.
    pub inventory: Vec<String>,
}

/// Initial game state sent on session ready.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InitialState {
    /// Party characters.
    pub characters: Vec<CharacterState>,
    /// Current location.
    pub location: String,
    /// Quest log.
    pub quests: HashMap<String, String>,
    /// Current turn count (persisted across sessions).
    #[serde(default)]
    pub turn_count: u32,
}

/// A choice in the character creation flow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreationChoice {
    /// Display label.
    pub label: String,
    /// Description text.
    pub description: String,
}

/// A party member in PARTY_STATUS.
///
/// PARTY_STATUS is the single source of truth for all per-character state,
/// including the character sheet (`sheet`) and inventory (`inventory`) facets.
/// Observers receive the full sheet and inventory for every member, which is
/// what enables "look at your teammate's gear" affordances and removes the
/// old reactive-null race condition where client-side state was gated on
/// separate CHARACTER_SHEET / INVENTORY messages that never reached observers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartyMember {
    /// Player identifier.
    pub player_id: String,
    /// Player lobby name (what the user typed at connect — used for identity matching).
    pub name: String,
    /// In-game character name (for display in party panel).
    #[serde(default)]
    pub character_name: String,
    /// Current HP.
    pub current_hp: i32,
    /// Maximum HP.
    pub max_hp: i32,
    /// Active statuses.
    pub statuses: Vec<String>,
    /// Character class.
    pub class: String,
    /// Character level.
    pub level: u32,
    /// Portrait URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub portrait_url: Option<String>,
    /// Current location name (for party panel display).
    #[serde(default)]
    pub current_location: String,
    /// Full character sheet — `None` until the member completes chargen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sheet: Option<CharacterSheetDetails>,
    /// Full inventory snapshot — `None` until the member has a loadout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inventory: Option<InventoryPayload>,
}

/// Character sheet details nested inside `PartyMember`.
///
/// Fields that already exist on `PartyMember` (`name`, `class`, `level`,
/// `portrait_url`, `current_location`) are intentionally NOT duplicated here —
/// the party member fields remain the single place those values live.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterSheetDetails {
    /// Character race/origin.
    #[serde(default)]
    pub race: String,
    /// Ability scores / stats.
    pub stats: HashMap<String, i32>,
    /// Known abilities.
    pub abilities: Vec<String>,
    /// Character backstory.
    #[serde(default)]
    pub backstory: String,
    /// Personality trait.
    #[serde(default)]
    pub personality: String,
    /// Pronouns.
    #[serde(default)]
    pub pronouns: String,
    /// Equipped/carried items as display strings.
    #[serde(default)]
    pub equipment: Vec<String>,
}

/// An inventory item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InventoryItem {
    /// Item name.
    pub name: String,
    /// Item category (weapon, armor, consumable, etc.).
    #[serde(rename = "type")]
    pub item_type: String,
    /// Whether the item is equipped.
    pub equipped: bool,
    /// Stack count.
    pub quantity: u32,
    /// Item description.
    pub description: String,
}

/// A location on the explored map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExploredLocation {
    /// Stable location identifier. In room-graph mode this is the `RoomDef`
    /// slug that `room_exits[].target` references — the UI joins exits back
    /// to rooms by this id. In region/cartography mode this equals `name`
    /// (no distinct slug exists). Always populated; `#[serde(default)]`
    /// allows older saves to deserialize cleanly.
    #[serde(default)]
    pub id: String,
    /// Display name (human-readable).
    pub name: String,
    /// X coordinate on map (0 when no coordinate data available).
    #[serde(default)]
    pub x: i32,
    /// Y coordinate on map (0 when no coordinate data available).
    #[serde(default)]
    pub y: i32,
    /// Location type (dungeon, town, etc.).
    #[serde(rename = "type", default)]
    pub location_type: String,
    /// Connected location names.
    #[serde(default)]
    pub connections: Vec<String>,
    /// Room exits with target and type info (room graph mode only).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub room_exits: Vec<RoomExitInfo>,
    /// Room type from RoomDef (room graph mode only).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub room_type: String,
    /// Room dimensions (width, height) from RoomDef (room graph mode only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<(u32, u32)>,
    /// Whether this is the player's current room (room graph mode only).
    #[serde(default)]
    pub is_current_room: bool,
    /// Tactical grid data for rooms with ASCII grids (room graph mode only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tactical_grid: Option<TacticalGridPayload>,
}

/// Exit descriptor for room graph mode — target room and exit type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoomExitInfo {
    /// Target room ID this exit leads to.
    pub target: String,
    /// Exit type: "door", "corridor", "chute_down", "chute_up", "secret".
    pub exit_type: String,
}

/// Fog of war bounds for map overlay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FogBounds {
    /// Map width.
    pub width: i32,
    /// Map height.
    pub height: i32,
}

/// Status effect info for the combat overlay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatusEffectInfo {
    /// Effect kind (e.g. "Poison", "Stun", "Bless", "Curse").
    pub kind: String,
    /// Rounds remaining.
    pub remaining_rounds: u32,
}

/// An enemy in combat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatEnemy {
    /// Enemy name.
    pub name: String,
    /// Current HP.
    pub hp: i32,
    /// Maximum HP.
    pub max_hp: i32,
    /// Armor class (optional for some enemies).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ac: Option<i32>,
    /// Active status effects on this enemy.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub status_effects: Vec<StatusEffectInfo>,
}

// ---------------------------------------------------------------------------
// Scenario system (Epic 7)
// ---------------------------------------------------------------------------

/// Payload for scenario system events.
///
/// Carries structured scenario data (clue discoveries, NPC actions, gossip
/// propagation results, accusation outcomes) to the client for UI rendering.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScenarioEventPayload {
    /// The type of scenario event.
    pub event_type: String,
    /// Human-readable description for display or narrator context.
    pub description: String,
    /// Structured event details (varies by event_type).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// Payload for achievement earned events (story 15-13).
///
/// Broadcast to all session players when a trope status transition
/// triggers an achievement. The UI can display a toast or achievement panel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AchievementEarnedPayload {
    /// Unique achievement identifier.
    pub achievement_id: String,
    /// Display name of the achievement.
    pub name: String,
    /// Flavor text shown on unlock.
    pub description: String,
    /// The trope that triggered this achievement.
    pub trope_id: String,
    /// What triggered it: "activated", "progressing", "resolved", "subverted".
    pub trigger: String,
    /// Optional emoji for UI display.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji: Option<String>,
}

// ---------------------------------------------------------------------------
// Journal browse (Story 9-13)
// ---------------------------------------------------------------------------

/// Sort order for journal entries.
///
/// Story 9-13: Controls how journal entries are ordered in the response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum JournalSortOrder {
    /// Sort by learned_turn, newest first.
    Time,
    /// Group by FactCategory, newest first within each group.
    Category,
}

/// Request payload for journal browse (client → server).
///
/// Story 9-13: Client requests accumulated KnownFacts, optionally filtered
/// by category and sorted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JournalRequestPayload {
    /// Optional category filter. None = all categories.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<FactCategory>,
    /// Sort order for results.
    pub sort_by: JournalSortOrder,
}

/// Response payload for journal browse (server → client).
///
/// Story 9-13: Server returns accumulated journal entries from character's
/// KnownFacts, filtered and sorted per request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JournalResponsePayload {
    /// Journal entries matching the request filter/sort.
    pub entries: Vec<JournalEntry>,
}

/// A single journal entry for the browse view.
///
/// Story 9-13: Wire representation of a KnownFact for the React client.
/// Source and confidence are strings (Display format of their Rust enums)
/// for UI simplicity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JournalEntry {
    /// Unique identifier for this fact.
    pub fact_id: String,
    /// The fact content in genre voice.
    pub content: String,
    /// Classification category.
    pub category: FactCategory,
    /// How the fact was acquired (e.g., "Observation", "Dialogue", "Discovery").
    pub source: String,
    /// Confidence level (e.g., "Certain", "Suspected", "Rumored").
    pub confidence: String,
    /// Turn number when this fact was learned.
    pub learned_turn: u64,
}

/// Item depletion payload — sent when a consumable item is fully exhausted.
///
/// Story 19-10: Fired when `deplete_light_on_transition()` exhausts a light source
/// during a room transition in room-graph mode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemDepletedPayload {
    /// Display name of the depleted item.
    pub item_name: String,
    /// How many uses the item had before this final depletion (typically 1).
    pub remaining_before: u32,
}

/// Resource minimum reached payload — sent when a resource decays to its min value.
///
/// Story 19-6: Fired when `apply_decay_per_turn()` causes a resource to reach its
/// declared minimum. Not re-fired if the resource was already at min.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceMinReachedPayload {
    /// Name of the resource that reached its minimum.
    pub resource_name: String,
    /// The minimum value the resource reached.
    pub min_value: f64,
}

// ---------------------------------------------------------------------------
// Tactical grid payload structs (story 29-5)
// ---------------------------------------------------------------------------

/// Full tactical state for a room — grid, entities, and effect zones.
/// Sent as TACTICAL_STATE on room entry when the room has ASCII grid data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TacticalStatePayload {
    /// Room ID this tactical state belongs to.
    pub room_id: String,
    /// The parsed grid layout.
    pub grid: TacticalGridPayload,
    /// Entities positioned on the grid (players, NPCs, creatures).
    pub entities: Vec<TacticalEntityPayload>,
    /// Active effect zones (spell areas, hazards, barriers).
    pub zones: Vec<EffectZonePayload>,
}

/// Grid layout — cell types as strings for JSON simplicity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TacticalGridPayload {
    /// Grid width in cells.
    pub width: u32,
    /// Grid height in cells.
    pub height: u32,
    /// 2D grid of cell type strings (e.g., "floor", "wall", "water").
    pub cells: Vec<Vec<String>>,
    /// Named features placed on the grid via legend.
    pub features: Vec<TacticalFeaturePayload>,
}

/// A named feature placed on the grid via legend glyph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TacticalFeaturePayload {
    /// The uppercase letter glyph (A-Z) from the ASCII grid.
    pub glyph: char,
    /// Feature type (cover, hazard, difficult_terrain, atmosphere, interactable, door).
    pub feature_type: String,
    /// Human-readable label for UI tooltip.
    pub label: String,
    /// Grid positions where this feature appears ([x, y] pairs).
    pub positions: Vec<[u32; 2]>,
}

/// An entity positioned on the tactical grid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TacticalEntityPayload {
    /// Unique entity identifier.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Grid x position (column).
    pub x: u32,
    /// Grid y position (row).
    pub y: u32,
    /// Size in cells (1 = medium, 2 = large, etc.).
    pub size: u32,
    /// Faction: "player", "hostile", "neutral", "ally".
    pub faction: String,
}

/// An effect zone overlay on the tactical grid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectZonePayload {
    /// Unique zone identifier.
    pub id: String,
    /// Zone shape type: "circle", "cone", "line", "rect".
    pub zone_type: String,
    /// Shape-specific parameters (center, radius, etc.).
    pub params: serde_json::Value,
    /// Human-readable label.
    pub label: String,
    /// Optional display color override.
    pub color: Option<String>,
}

/// Player tactical action on the grid (move, target, inspect).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TacticalActionPayload {
    /// Action type: "move", "target", "inspect".
    pub action_type: String,
    /// Entity performing the action (for move/target).
    pub entity_id: Option<String>,
    /// Target grid position [x, y].
    pub target: Option<[u32; 2]>,
    /// Ability being used (for target actions).
    pub ability: Option<String>,
}

// ---------------------------------------------------------------------------
// Dice resolution protocol (story 34-2, ADR-074)
// ---------------------------------------------------------------------------

/// Supported die sizes (faces per die) per ADR-074 §3.
///
/// The seven values are the tabletop standard. Making this a bounded enum
/// (rather than a raw `u32`) means invalid sides like `3` (not a real tabletop
/// die) do not materialize as a usable variant — they fall through to
/// `Unknown` via the `From<u32>` bridge below.
///
/// **Wire format: unsigned integer face count** (`4`, `6`, `8`, `10`, `12`,
/// `20`, `100`). Implemented via `#[serde(from = "u32", into = "u32")]` — the
/// JSON shape is a bare integer (`"sides": 20`) matching ADR-074's fixture
/// exactly, and the From/Into u32 bridge handles the enum ↔ integer mapping.
///
/// **Forward-compatibility — two orthogonal mechanisms:**
///
/// - `#[non_exhaustive]` is the compile-time guard. It forces downstream
///   `match` arms to include a wildcard, so adding a new die size (e.g., `D30`)
///   does not break downstream builds.
/// - `From<u32>` maps every unknown integer to `Unknown`. This is the wire-level
///   guard: any integer from a newer server that the older client does not know
///   about deserializes to `Unknown` rather than failing with a serde error.
///
/// Older revisions used `#[serde(rename = "4")] + #[serde(other)]` which
/// actually produced **string**-tagged wire output (`"sides": "20"`). The doc
/// said integer but the code emitted strings. Cycle-2 review caught this; the
/// current `From`/`Into<u32>` pattern actually delivers the integer wire
/// format the doc claims.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(from = "u32", into = "u32")]
#[non_exhaustive]
pub enum DieSides {
    /// Four-sided die (d4).
    D4,
    /// Six-sided die (d6).
    D6,
    /// Eight-sided die (d8).
    D8,
    /// Ten-sided die (d10).
    D10,
    /// Twelve-sided die (d12).
    D12,
    /// Twenty-sided die (d20) — the primary check die in ADR-074.
    D20,
    /// Percentile die (d100).
    D100,
    /// Unknown die size from a newer wire protocol. Deserialization of any
    /// integer not in the supported set maps here. Downstream code should
    /// refuse the roll and surface a "client needs upgrade" signal rather than
    /// guessing at face count — see `DieSides::faces()` which returns `None`
    /// for this variant.
    Unknown,
}

impl DieSides {
    /// Returns the face count as a `u32` for use in RNG modular arithmetic.
    ///
    /// Returns `None` for `Unknown` (caller must decide how to handle an
    /// unrecognized die from a newer protocol — typically: refuse the roll).
    pub fn faces(self) -> Option<u32> {
        match self {
            Self::D4 => Some(4),
            Self::D6 => Some(6),
            Self::D8 => Some(8),
            Self::D10 => Some(10),
            Self::D12 => Some(12),
            Self::D20 => Some(20),
            Self::D100 => Some(100),
            Self::Unknown => None,
        }
    }
}

impl From<u32> for DieSides {
    /// Infallible mapping from the wire-format integer to the variant.
    ///
    /// Any value not in the ADR-074 set of `{4, 6, 8, 10, 12, 20, 100}`
    /// (including `0`, `1`, `3`, `7`, `u32::MAX`) maps to `Unknown`. This is
    /// the wire-level forward-compat path — downstream `match` sites see
    /// `Unknown` and decide policy, rather than serde erroring at parse time.
    fn from(value: u32) -> Self {
        match value {
            4 => Self::D4,
            6 => Self::D6,
            8 => Self::D8,
            10 => Self::D10,
            12 => Self::D12,
            20 => Self::D20,
            100 => Self::D100,
            _ => Self::Unknown,
        }
    }
}

impl From<DieSides> for u32 {
    /// Maps a `DieSides` back to its face count for wire serialization.
    ///
    /// `Unknown` serializes as `0` — a deliberate sentinel that any sane
    /// downstream consumer should reject before rolling. `0` is not in the
    /// `From<u32>` accepted set, so round-tripping `Unknown → 0 → Unknown` is
    /// stable (the wire value maps back to `Unknown` on deserialize).
    fn from(value: DieSides) -> Self {
        match value {
            DieSides::D4 => 4,
            DieSides::D6 => 6,
            DieSides::D8 => 8,
            DieSides::D10 => 10,
            DieSides::D12 => 12,
            DieSides::D20 => 20,
            DieSides::D100 => 100,
            DieSides::Unknown => 0,
        }
    }
}

/// Specification for one group of dice in a roll (story 34-2).
///
/// A dice pool is `Vec<DieSpec>` — e.g., `[{sides: "20", count: 1}]` for a
/// single d20, or `[{sides: "6", count: 4}, {sides: "10", count: 2}]` for
/// 4d6 + 2d10 thrown together in one gesture.
///
/// `count` uses `NonZeroU8` (max 255 dice, minimum 1) — prevents a malicious
/// client from requesting `count: u32::MAX` as an allocation DoS, and makes
/// `count == 0` unrepresentable.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DieSpec {
    /// Which die to throw (bounded by `DieSides`).
    pub sides: DieSides,
    /// How many dice of this type to throw. `NonZeroU8` caps at 255 and
    /// rejects zero — both serde and direct construction enforce it.
    pub count: std::num::NonZeroU8,
}

/// Throw gesture parameters captured from the drag-and-flick interaction (story 34-2).
///
/// Server authority model (ADR-074): these parameters control animation aesthetics
/// — angle, force, tumble path — but NOT the outcome. The outcome is determined
/// entirely by the independently-generated seed on `DiceResultPayload.seed`
/// (see that field for the cheat-resistance rationale). All clients run
/// identical Rapier physics from the same seed + throw params, producing
/// identical visual animation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThrowParams {
    /// Initial linear velocity vector `[x, y, z]`.
    pub velocity: [f32; 3],
    /// Initial angular velocity `[x, y, z]` — spin around each axis.
    pub angular: [f32; 3],
    /// Release point on screen, normalized `[x, y]` in `[0.0, 1.0]`.
    pub position: [f32; 2],
}

/// Outcome classification for a resolved dice roll (story 34-2, ADR-074).
///
/// The narrator uses this to shape prose tone — crit successes produce triumphant
/// narration, crit fails produce dread or comedy depending on context. `CritFail`
/// is distinguishable from plain `Fail` on the wire so the narrator can pick a
/// different register.
///
/// **Forward-compatibility model — two orthogonal mechanisms:**
///
/// - `#[non_exhaustive]` is a *compile-time* guard. It forces downstream crates
///   matching on `RollOutcome` to include a wildcard arm, so adding a new variant
///   (e.g., `NearMiss`) does not break their builds.
/// - `#[serde(other)] Unknown` is the *wire-level* guard. Without it, serde would
///   hard-reject any variant string it doesn't know about — an older client
///   talking to a newer server would `Err` on deserialization. With it, unknown
///   variants silently fall through to `Unknown`, which downstream code can then
///   treat however it prefers (typically: refuse the outcome and request a retry).
///
/// Both are needed. Prior revisions of this doc claimed `#[non_exhaustive]` alone
/// delivered forward-compat; it does not.
///
/// `Eq`/`Hash` are intentionally NOT derived — deriving `Hash` on a
/// `#[non_exhaustive]` enum ties the hash surface to the public variant list,
/// which breaks wire stability when new variants are added. If you need
/// outcome-keyed aggregation, match to an integer discriminant instead.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum RollOutcome {
    /// Natural maximum on the primary die (e.g., nat 20 on d20). Always succeeds
    /// dramatically regardless of DC. "Primary die" is resolved from the
    /// `DieGroupResult` with the largest face count — typically the d20 in a
    /// skill-check pool.
    CritSuccess,
    /// Total (sum of rolls + modifier) meets or exceeds the DC.
    Success,
    /// Total falls short of the DC but was not a critical failure.
    Fail,
    /// Natural minimum on the primary die (e.g., nat 1 on d20). Always fails
    /// dramatically regardless of modifier.
    CritFail,
    /// Unknown outcome from a newer wire protocol. Older clients fall through
    /// to this variant via `#[serde(other)]` instead of hard-erroring on
    /// deserialization. Downstream code should refuse the outcome and surface
    /// a "client needs upgrade" error to the user rather than guessing.
    #[serde(other)]
    Unknown,
}

/// Resolved face values for one group in a pool roll (story 34-2).
///
/// Paired with the originating `DieSpec` so downstream consumers (narrator tone,
/// physics replay, UI rendering) can attribute each rolled face back to its
/// die type. Before this type existed, the result payload used a flat
/// `Vec<u32>` of face values, which made `RollOutcome::CritSuccess` ("natural
/// max on the primary die") formally unresolvable for mixed pools like 4d6+2d10.
///
/// **Invariant: `faces.len() == spec.count.get() as usize`.** Enforced at the
/// wire boundary via `DiceResultPayload`'s validated deserialization path (see
/// `dice_payload_raw::DiceResultPayloadRaw::try_from`). A payload where the
/// face count and the declared die count disagree is rejected with
/// `DiceResultPayloadError::FaceCountMismatch` — this invariant used to be a
/// docstring-only claim (cycle-2 review finding #2), it is now actually
/// enforced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DieGroupResult {
    /// The die this group was rolled against (echoes the request's `DieSpec`).
    pub spec: DieSpec,
    /// The raw face values in roll order. Length must match `spec.count` —
    /// enforced by `DiceResultPayload`'s `TryFrom` guard at deserialization.
    pub faces: Vec<u32>,
}

/// Server -> client: request a dice roll during the reveal phase (story 34-2).
///
/// Broadcast to all clients. The `rolling_player_id` identifies who must
/// throw; other clients watch. Note that this is distinct from the envelope
/// `player_id` on `GameMessage::DiceRequest` which identifies the sender of
/// the frame (typically `"server"`).
///
/// **Deserialization is validated** via the private
/// `dice_payload_raw::DiceRequestPayloadRaw` intermediary. Rejects empty dice
/// pools (`dice == []`) — a modifier-only "roll" with no dice actually thrown
/// — and whitespace-only stat names. `difficulty` uses `NonZeroU32` to reject
/// `difficulty == 0`, which would guarantee `Success` on any roll.
///
/// The Raw intermediary type is private to the `dice_payload_raw` module to
/// prevent direct construction/deserialization bypassing the validation (see
/// cycle-2 review finding #3).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(into = "dice_payload_raw::DiceRequestPayloadRaw")]
pub struct DiceRequestPayload {
    /// Correlation ID matching the eventual `DiceResult`. Also used by the
    /// rolling player when submitting a `DiceThrow`.
    pub request_id: String,
    /// Player who must throw the dice. (Distinct from the envelope `player_id`
    /// on the `GameMessage::DiceRequest` variant — that identifies the sender
    /// of the frame, typically `"server"`. This field identifies the thrower.)
    pub rolling_player_id: String,
    /// Display name of the character making the check.
    pub character_name: String,
    /// Dice pool to throw (one or more `DieSpec` groups). Guaranteed non-empty
    /// via the validated deserialization path.
    pub dice: Vec<DieSpec>,
    /// Stat modifier applied to the sum of rolls. Can be negative (penalties).
    pub modifier: i32,
    /// Ability name for the check (e.g., `"dexterity"`, `"strength"`). Set by
    /// the narrator from the beat definition. Must be non-blank and non-empty;
    /// whitespace-only values are rejected at the wire boundary.
    pub stat: String,
    /// Difficulty class the total must meet or exceed. Revealed HERE, not during
    /// the sealed phase (ADR-074: DC-reveal-at-roll-time tension mechanic).
    /// Uses `NonZeroU32` to reject `0`, which would otherwise make every roll
    /// a guaranteed `Success` — a silent footgun if a narrator prompt omits DC.
    pub difficulty: std::num::NonZeroU32,
    /// Narrator flavor text for the dice tray UI — sets the scene for the throw.
    pub context: String,
}

/// Private deserialization intermediaries for the dice request and result
/// payloads. Kept inside a module so the Raw types are `pub(super)` rather
/// than crate-public — serde's `#[serde(into = "...")]` attribute requires
/// the target type to be name-resolvable from the attribute site, and a
/// submodule satisfies that while preventing direct `pub` bypass (cycle-2
/// finding #3).
///
/// The Raw types mirror their public counterparts field-for-field, derive
/// plain `Serialize`/`Deserialize` with `#[serde(deny_unknown_fields)]`, and
/// `TryFrom` implementations that enforce the cross-field invariants:
///
/// - `DiceRequestPayloadRaw::try_into()`: dice pool non-empty, stat non-blank.
/// - `DiceResultPayloadRaw::try_into()`: every `DieGroupResult.faces.len() ==
///   spec.count.get() as usize`. This is the enforcement of the invariant
///   that the `DieGroupResult` doc claims (cycle-2 finding #2).
mod dice_payload_raw {
    use super::{
        DiceRequestPayload, DiceResultPayload, DieGroupResult, DieSpec, RollOutcome, ThrowParams,
    };
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub(super) struct DiceRequestPayloadRaw {
        pub request_id: String,
        pub rolling_player_id: String,
        pub character_name: String,
        pub dice: Vec<DieSpec>,
        pub modifier: i32,
        pub stat: String,
        pub difficulty: std::num::NonZeroU32,
        pub context: String,
    }

    impl From<DiceRequestPayload> for DiceRequestPayloadRaw {
        fn from(p: DiceRequestPayload) -> Self {
            Self {
                request_id: p.request_id,
                rolling_player_id: p.rolling_player_id,
                character_name: p.character_name,
                dice: p.dice,
                modifier: p.modifier,
                stat: p.stat,
                difficulty: p.difficulty,
                context: p.context,
            }
        }
    }

    impl TryFrom<DiceRequestPayloadRaw> for DiceRequestPayload {
        type Error = super::DiceRequestPayloadError;
        fn try_from(raw: DiceRequestPayloadRaw) -> Result<Self, Self::Error> {
            if raw.dice.is_empty() {
                return Err(super::DiceRequestPayloadError::EmptyDicePool);
            }
            if raw.stat.trim().is_empty() {
                return Err(super::DiceRequestPayloadError::BlankStat);
            }
            Ok(Self {
                request_id: raw.request_id,
                rolling_player_id: raw.rolling_player_id,
                character_name: raw.character_name,
                dice: raw.dice,
                modifier: raw.modifier,
                stat: raw.stat,
                difficulty: raw.difficulty,
                context: raw.context,
            })
        }
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub(super) struct DiceResultPayloadRaw {
        pub request_id: String,
        pub rolling_player_id: String,
        pub character_name: String,
        pub rolls: Vec<DieGroupResult>,
        pub modifier: i32,
        pub total: i32,
        pub difficulty: std::num::NonZeroU32,
        pub outcome: RollOutcome,
        pub seed: u64,
        pub throw_params: ThrowParams,
    }

    impl From<DiceResultPayload> for DiceResultPayloadRaw {
        fn from(p: DiceResultPayload) -> Self {
            Self {
                request_id: p.request_id,
                rolling_player_id: p.rolling_player_id,
                character_name: p.character_name,
                rolls: p.rolls,
                modifier: p.modifier,
                total: p.total,
                difficulty: p.difficulty,
                outcome: p.outcome,
                seed: p.seed,
                throw_params: p.throw_params,
            }
        }
    }

    impl TryFrom<DiceResultPayloadRaw> for DiceResultPayload {
        type Error = super::DiceResultPayloadError;
        fn try_from(raw: DiceResultPayloadRaw) -> Result<Self, Self::Error> {
            // Enforce the DieGroupResult invariant that was previously
            // documentary-only (cycle-2 review finding #2).
            for (idx, group) in raw.rolls.iter().enumerate() {
                let declared = group.spec.count.get() as usize;
                let actual = group.faces.len();
                if declared != actual {
                    return Err(super::DiceResultPayloadError::FaceCountMismatch {
                        group_index: idx,
                        declared,
                        actual,
                    });
                }
            }
            Ok(Self {
                request_id: raw.request_id,
                rolling_player_id: raw.rolling_player_id,
                character_name: raw.character_name,
                rolls: raw.rolls,
                modifier: raw.modifier,
                total: raw.total,
                difficulty: raw.difficulty,
                outcome: raw.outcome,
                seed: raw.seed,
                throw_params: raw.throw_params,
            })
        }
    }
}

impl<'de> Deserialize<'de> for DiceRequestPayload {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = dice_payload_raw::DiceRequestPayloadRaw::deserialize(deserializer)?;
        Self::try_from(raw).map_err(serde::de::Error::custom)
    }
}

/// Validation errors produced by `DiceRequestPayload::try_from(raw)`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DiceRequestPayloadError {
    /// The dice pool was empty. A roll with zero dice is a nonsensical game
    /// state — the modifier and difficulty would compare against a total of
    /// `0`, making every sufficiently-low DC a guaranteed success.
    EmptyDicePool,
    /// The `stat` field was blank or whitespace-only. Stat names must be
    /// non-empty at the wire boundary; downstream consumers expect a real
    /// ability name to feed into narrator prompts.
    BlankStat,
}

impl std::fmt::Display for DiceRequestPayloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyDicePool => f.write_str("dice pool is empty"),
            Self::BlankStat => f.write_str("stat field is blank or whitespace-only"),
        }
    }
}

impl std::error::Error for DiceRequestPayloadError {}

/// Validation errors produced by `DiceResultPayload::try_from(raw)`.
///
/// Currently only surfaces the per-group face-count invariant. Future
/// consistency checks (e.g., `total == sum(flat_rolls) + modifier`) can be
/// added here without breaking the public API.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DiceResultPayloadError {
    /// A `DieGroupResult` declared a different `spec.count` than the number
    /// of `faces` actually present. This invariant is load-bearing for
    /// `RollOutcome::CritSuccess` detection — the "natural max on the primary
    /// die" needs every declared die to have a rolled face.
    FaceCountMismatch {
        /// Zero-based index into `DiceResultPayload.rolls`.
        group_index: usize,
        /// The count declared by `spec.count.get()`.
        declared: usize,
        /// The actual `faces.len()` value.
        actual: usize,
    },
}

impl std::fmt::Display for DiceResultPayloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FaceCountMismatch {
                group_index,
                declared,
                actual,
            } => write!(
                f,
                "rolls[{group_index}] declared count={declared} but got {actual} face value(s)"
            ),
        }
    }
}

impl std::error::Error for DiceResultPayloadError {}

impl<'de> Deserialize<'de> for DiceResultPayload {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = dice_payload_raw::DiceResultPayloadRaw::deserialize(deserializer)?;
        Self::try_from(raw).map_err(serde::de::Error::custom)
    }
}

/// Client -> server: rolling player submits a throw after local physics settles
/// (story 34-12, physics-is-the-roll).
///
/// Matched to the original `DiceRequest` via `request_id`. The client runs
/// Rapier physics locally to visual completion, reads the settled face from
/// each die, and submits the face values alongside the throw parameters.
/// The server uses `face` as the authoritative roll result (no server-side
/// RNG on this path) and echoes `throw_params` so spectators can replay the
/// same physics deterministically.
///
/// `face` carries one entry per physical die in the pool, in flat iteration
/// order matching the `DieSpec`s in the triggering `DiceRequest` — e.g. a
/// `[{sides:20, count:1}, {sides:6, count:3}]` pool yields `face.len() == 4`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiceThrowPayload {
    /// Correlation ID matching the triggering `DiceRequest`.
    pub request_id: String,
    /// Physics parameters captured from the drag-and-flick gesture, echoed
    /// to spectators for deterministic animation replay.
    pub throw_params: ThrowParams,
    /// Client-reported settled face values (physics-is-the-roll).
    /// One per die, flat order matching the pool's `DieSpec` iteration.
    pub face: Vec<u32>,
}

/// Server -> all clients: resolved dice roll outcome (story 34-2).
///
/// Contains everything needed to replay identical physics and display the
/// outcome: per-group die faces (so consumers can attribute rolls back to
/// their `DieSpec`), total, DC, outcome classification, physics seed, and
/// the echoed throw parameters.
///
/// Note that the envelope `player_id` on `GameMessage::DiceResult` identifies
/// the sender (typically `"server"`), while `rolling_player_id` identifies the
/// player whose character rolled. See the `DiceRequest` doc for the same
/// distinction — the two `player_id`-shaped fields serve different roles and
/// the payload field is renamed to make the ambiguity impossible.
///
/// **Deserialization is validated** via the private
/// `dice_payload_raw::DiceResultPayloadRaw` intermediary. The `DieGroupResult`
/// invariant `faces.len() == spec.count.get() as usize` is enforced at the
/// wire boundary — cycle-2 review finding #2 (this was previously
/// documentary only).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(into = "dice_payload_raw::DiceResultPayloadRaw")]
pub struct DiceResultPayload {
    /// Correlation ID matching the original `DiceRequest`.
    pub request_id: String,
    /// Player whose character threw the dice. (Distinct from the envelope
    /// `player_id` on `GameMessage::DiceResult`, which is the sender — typically
    /// `"server"`.)
    pub rolling_player_id: String,
    /// Display name of the character that rolled.
    pub character_name: String,
    /// Per-group face values. Each `DieGroupResult` pairs its originating
    /// `DieSpec` with the rolled faces, so downstream consumers can identify
    /// which roll came from which die (e.g., the d20 in a 4d6+1d20 pool).
    /// This is required for `RollOutcome::CritSuccess` / `CritFail` detection
    /// on the "primary die" and for per-group UI rendering.
    pub rolls: Vec<DieGroupResult>,
    /// Stat modifier applied to the sum (can be negative).
    pub modifier: i32,
    /// `sum(rolls_flat) + modifier` — final check total. Uses `i32` because a
    /// sufficiently negative modifier can push the total below zero
    /// (e.g., rolls=[1], modifier=-5 → total=-4), and silently wrapping that
    /// to `u32::MAX - 3` would be a catastrophic mechanical lie.
    pub total: i32,
    /// Difficulty class echoed from the `DiceRequest` for UI display. Uses
    /// `NonZeroU32` to match the request-side invariant.
    pub difficulty: std::num::NonZeroU32,
    /// Outcome classification — feeds the narrator prompt for tone shaping.
    pub outcome: RollOutcome,
    /// Deterministic physics seed derived from `(session_id, round)`. Under
    /// story 34-12 (physics-is-the-roll) this seed no longer drives the roll
    /// outcome — the rolling player's client-reported `face` values from
    /// `DiceThrowPayload` are authoritative, and the server echoes them in
    /// `rolls[].faces`. This seed now drives spectator replay animation only:
    /// every non-rolling client runs Rapier locally with this seed +
    /// `throw_params` so the visual playout is consistent across clients.
    /// Producing the same final face from a different client physics engine
    /// is NOT guaranteed — visual consistency is the goal, not re-verification
    /// of the face value.
    pub seed: u64,
    /// Throw gesture parameters echoed back for client-side replay.
    pub throw_params: ThrowParams,
}

// ---------------------------------------------------------------------------
// BeatSelection payload — structured encounter beat dispatch
// ---------------------------------------------------------------------------

/// Client -> server: player selects a beat action during an encounter.
///
/// Carries the structured `beat_id` from the genre pack's `ConfrontationDef`.
/// The server validates the id against the active encounter, dispatches
/// `apply_beat()` deterministically, and then invokes the narrator to describe
/// the outcome. The narrator does NOT choose the player's beat.
///
/// The `actor` field defaults to `"player"` for the controlling player. In
/// multiplayer, guest-NPC controllers set `actor` to their NPC's name so the
/// server can attribute the beat to the correct `EncounterActor`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BeatSelectionPayload {
    /// The exact `id` from `ConfrontationDef.beats[].id`. Matched strictly —
    /// no label fallback, no snake_case normalization.
    pub beat_id: String,
    /// Which actor is taking the action. `"player"` for the controlling player.
    /// In multiplayer guest-NPC mode, this is the NPC's name.
    #[serde(default = "default_player_actor")]
    pub actor: String,
}

fn default_player_actor() -> String {
    "player".to_string()
}

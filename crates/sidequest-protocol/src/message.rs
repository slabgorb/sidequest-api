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

    /// Partial narration text (streaming).
    #[serde(rename = "NARRATION_CHUNK")]
    NarrationChunk {
        /// The typed payload for this message.
        payload: NarrationChunkPayload,
        /// The player who sent this message.
        player_id: String,
    },

    /// End of narration stream.
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

    /// Full character details for sheet overlay.
    #[serde(rename = "CHARACTER_SHEET")]
    CharacterSheet {
        /// The typed payload for this message.
        payload: CharacterSheetPayload,
        /// The player who sent this message.
        player_id: String,
    },

    /// Full inventory snapshot.
    #[serde(rename = "INVENTORY")]
    Inventory {
        /// The typed payload for this message.
        payload: InventoryPayload,
        /// The player who sent this message.
        player_id: String,
    },

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

    /// TTS stream start — sent before first audio chunk.
    #[serde(rename = "TTS_START")]
    TtsStart {
        /// The typed payload for this message.
        payload: TtsStartPayload,
        /// The player who sent this message.
        player_id: String,
    },

    /// TTS audio chunk — base64-encoded audio for one narration segment.
    #[serde(rename = "TTS_CHUNK")]
    TtsChunk {
        /// The typed payload for this message.
        payload: TtsChunkPayload,
        /// The player who sent this message.
        player_id: String,
    },

    /// TTS stream end — sent after last audio chunk.
    #[serde(rename = "TTS_END")]
    TtsEnd {
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

/// Partial narration text (streaming chunk).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NarrationChunkPayload {
    /// The partial text being streamed.
    pub text: String,
}

/// End of narration stream, optionally with final state delta.
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
    /// Player's choice (client → server).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub choice: Option<String>,
    /// Completed character data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub character: Option<serde_json::Value>,
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

/// Character sheet details.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterSheetPayload {
    /// Character name.
    pub name: String,
    /// Character class.
    pub class: String,
    /// Character race/origin.
    #[serde(default)]
    pub race: String,
    /// Character level.
    pub level: u32,
    /// Ability scores / stats.
    pub stats: HashMap<String, i32>,
    /// Known abilities.
    pub abilities: Vec<String>,
    /// Character backstory.
    pub backstory: String,
    /// Personality trait.
    #[serde(default)]
    pub personality: String,
    /// Pronouns.
    #[serde(default)]
    pub pronouns: String,
    /// Equipped/carried items.
    #[serde(default)]
    pub equipment: Vec<String>,
    /// Portrait image URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub portrait_url: Option<String>,
    /// Current location name (for character sheet display).
    #[serde(default)]
    pub current_location: String,
}

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

fn default_true() -> bool { true }

/// A participant in a confrontation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfrontationActor {
    pub name: String,
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub portrait_url: Option<String>,
}

/// Primary metric being tracked in a confrontation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfrontationMetric {
    pub name: String,
    pub current: i32,
    pub starting: i32,
    pub direction: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold_high: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold_low: Option<i32>,
}

/// A beat option the player can choose during a confrontation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfrontationBeat {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub metric_delta: i32,
    #[serde(default)]
    pub stat_check: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk: Option<String>,
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

/// TTS stream start payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TtsStartPayload {
    /// Total number of audio segments to expect.
    pub total_segments: usize,
}

/// TTS audio chunk payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TtsChunkPayload {
    /// Base64-encoded audio bytes.
    pub audio_base64: String,
    /// Segment index in the narration.
    pub segment_index: usize,
    /// Whether this is the last chunk.
    pub is_last_chunk: bool,
    /// Speaker identity (character name or "narrator").
    pub speaker: String,
    /// Audio format ("wav" or "opus").
    pub format: String,
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
    /// Location name.
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

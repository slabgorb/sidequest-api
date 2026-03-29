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

    /// Combat state for combat overlay.
    #[serde(rename = "COMBAT_EVENT")]
    CombatEvent {
        /// The typed payload for this message.
        payload: CombatEventPayload,
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
    /// Character level.
    pub level: u32,
    /// Ability scores / stats.
    pub stats: HashMap<String, i32>,
    /// Known abilities.
    pub abilities: Vec<String>,
    /// Character backstory.
    pub backstory: String,
    /// Portrait image URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub portrait_url: Option<String>,
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
}

/// Combat state for the combat overlay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatEventPayload {
    /// Whether combat is active.
    pub in_combat: bool,
    /// Active enemies.
    pub enemies: Vec<CombatEnemy>,
    /// Initiative order.
    pub turn_order: Vec<String>,
    /// Who's acting now.
    pub current_turn: String,
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
    /// Audio action: "play", "fade_in", "fade_out", "duck", "restore", "stop".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// Volume level (0.0–1.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<f32>,
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
    /// Character name.
    pub name: String,
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
}

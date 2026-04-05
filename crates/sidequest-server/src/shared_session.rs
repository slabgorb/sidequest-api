//! Shared multiplayer game session — world-level state shared across players.
//!
//! A `SharedGameSession` holds the world state (location, NPCs, narration
//! history, music, tropes) that is common to all players in the same
//! genre:world instance. Per-player state lives in `PlayerState`.

use std::collections::HashMap;

use tokio::sync::broadcast;

use sidequest_game::barrier::TurnBarrier;
use sidequest_game::builder::CharacterBuilder;
use sidequest_game::multiplayer::MultiplayerSession;

/// Server-internal wrapper for targeted broadcast messages.
/// When `target_player_id` is Some, only that player receives the message.
/// When None, all session members receive it (standard broadcast).
#[derive(Debug, Clone)]
pub struct TargetedMessage {
    pub msg: GameMessage,
    /// If set, only deliver to this player. None = broadcast to all.
    pub target_player_id: Option<String>,
}
use sidequest_game::perception::{PerceptionFilter, PerceptionRewriter};
use sidequest_game::turn_mode::TurnMode;
use sidequest_protocol::GameMessage;

use crate::NpcRegistryEntry;
use crate::Session;

// ---------------------------------------------------------------------------
// Session key — genre:world (NOT player-scoped)
// ---------------------------------------------------------------------------

/// Build the shared session key for a genre/world pair.
///
/// Unlike the per-player `session_key()`, this is player-agnostic so that
/// multiple connections to the same genre:world join the same session.
pub fn game_session_key(genre: &str, world: &str) -> String {
    format!("{}:{}", genre, world)
}

// ---------------------------------------------------------------------------
// Per-player state
// ---------------------------------------------------------------------------

/// Per-player state within a shared session.
///
/// These fields were formerly local variables in `handle_ws_connection`.
/// They remain per-player because each player has their own character,
/// inventory, and combat stance.
pub struct PlayerState {
    pub player_name: String,
    pub session: Session,
    pub builder: Option<CharacterBuilder>,
    pub character_json: Option<serde_json::Value>,
    pub character_name: Option<String>,
    pub character_hp: i32,
    pub character_max_hp: i32,
    pub character_level: u32,
    pub character_class: String,
    pub character_xp: u32,
    /// Resolved region ID from cartography (used for co-location comparison).
    pub region_id: String,
    /// Raw narrator location string (display text for UI).
    pub display_location: String,
    pub inventory: sidequest_game::Inventory,
    pub combat_state: sidequest_game::combat::CombatState,
    pub chase_state: Option<sidequest_game::ChaseState>,
}

impl PlayerState {
    /// Create a new player state with defaults.
    pub fn new(player_name: String) -> Self {
        Self {
            player_name,
            session: Session::new(),
            builder: None,
            character_json: None,
            character_name: None,
            character_hp: 10,
            character_max_hp: 10,
            character_level: 1,
            character_class: String::new(),
            character_xp: 0,
            region_id: String::new(),
            display_location: String::new(),
            inventory: sidequest_game::Inventory::default(),
            combat_state: sidequest_game::combat::CombatState::default(),
            chase_state: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Shared game session
// ---------------------------------------------------------------------------

/// World-level state shared across all players in the same genre:world.
///
/// Protected by `tokio::sync::Mutex` at the registry level — callers lock
/// the session, read/write fields, then drop the guard.
pub struct SharedGameSession {
    // --- Identity ---
    pub genre_slug: String,
    pub world_slug: String,

    // --- World state (shared) ---
    pub world_context: String,
    pub visual_style: Option<sidequest_genre::VisualStyle>,
    pub trope_defs: Vec<sidequest_genre::TropeDefinition>,
    pub trope_states: Vec<sidequest_game::trope::TropeState>,
    pub npc_registry: Vec<NpcRegistryEntry>,
    pub narration_history: Vec<String>,
    pub discovered_regions: Vec<String>,
    pub current_location: String,
    pub music_director: Option<sidequest_game::MusicDirector>,
    pub audio_mixer: Option<sidequest_game::AudioMixer>,
    pub prerender_scheduler: Option<sidequest_game::PrerenderScheduler>,

    // --- Multiplayer coordination ---
    pub multiplayer: MultiplayerSession,
    pub turn_mode: TurnMode,
    pub turn_barrier: Option<TurnBarrier>,
    /// Per-player perception filters (player_id → filter).
    /// When populated, narration is rewritten per-player based on
    /// active perceptual effects (blinded, charmed, etc.).
    pub perception_filters: HashMap<String, PerceptionFilter>,

    // --- Scenario ---
    /// Active scenario pack for pressure events and scene budget (multiplayer-shared).
    pub active_scenario: Option<sidequest_genre::ScenarioPack>,
    /// Scene counter for pressure event triggering.
    pub scene_count: u32,

    // --- Cartography ---
    /// Region registry from cartography.yaml: region_id → display name (lowercase for matching).
    pub region_names: Vec<(String, String)>,

    // --- Per-player state ---
    pub players: HashMap<String, PlayerState>,

    // --- Session-scoped broadcast (narration to all members) ---
    pub session_tx: broadcast::Sender<TargetedMessage>,
}

impl SharedGameSession {
    /// Create a new shared session for a genre:world pair.
    pub fn new(genre_slug: String, world_slug: String) -> Self {
        let (session_tx, _) = broadcast::channel::<TargetedMessage>(64);
        let multiplayer = MultiplayerSession::new(HashMap::new());
        Self {
            genre_slug,
            world_slug,
            world_context: String::new(),
            visual_style: None,
            trope_defs: vec![],
            trope_states: vec![],
            npc_registry: vec![],
            narration_history: vec![],
            discovered_regions: vec![],
            current_location: String::new(),
            music_director: None,
            audio_mixer: None,
            prerender_scheduler: None,
            multiplayer,
            turn_mode: TurnMode::default(),
            turn_barrier: None,
            perception_filters: HashMap::new(),
            active_scenario: None,
            scene_count: 0,
            region_names: vec![],
            players: HashMap::new(),
            session_tx,
        }
    }

    /// Number of connected players.
    pub fn player_count(&self) -> usize {
        self.players.len()
    }

    /// Subscribe to the session broadcast channel.
    pub fn subscribe(&self) -> broadcast::Receiver<TargetedMessage> {
        self.session_tx.subscribe()
    }

    /// Broadcast a message to all session members.
    pub fn broadcast(&self, msg: GameMessage) {
        // Ignore send errors (no active receivers is fine)
        let _ = self.session_tx.send(TargetedMessage {
            msg,
            target_player_id: None,
        });
    }

    /// Send a message to a specific player via the session channel.
    /// The writer task filters based on `target_player_id`.
    pub fn send_to_player(&self, msg: GameMessage, target: String) {
        let _ = self.session_tx.send(TargetedMessage {
            msg,
            target_player_id: Some(target),
        });
    }

    /// Return player IDs of other players in the same cartography region.
    /// Empty region_id never matches (players with no resolved region are not co-located).
    pub fn co_located_players(&self, player_id: &str) -> Vec<String> {
        let my_region = self
            .players
            .get(player_id)
            .map(|p| p.region_id.as_str())
            .unwrap_or("");
        if my_region.is_empty() {
            return vec![];
        }
        self.players
            .iter()
            .filter(|(pid, ps)| pid.as_str() != player_id && ps.region_id == my_region)
            .map(|(pid, _)| pid.clone())
            .collect()
    }

    /// Resolve a narrator-generated location string to a cartography region_id.
    /// Uses case-insensitive contains matching against region display names.
    /// Returns the region_id if a match is found.
    pub fn resolve_region(&self, location_text: &str) -> Option<String> {
        if location_text.is_empty() {
            return None;
        }
        let loc_lower = location_text.to_lowercase();
        // Try exact-ish match first: region name contained in location text
        // (handles "The Gutter, Coyote Reach Station" matching region "The Gutter")
        let mut best: Option<(&str, usize)> = None;
        for (region_id, name_lower) in &self.region_names {
            if loc_lower.contains(name_lower.as_str()) {
                // Prefer longest match to avoid "The" matching everything
                let len = name_lower.len();
                if best.map_or(true, |(_, prev_len)| len > prev_len) {
                    best = Some((region_id.as_str(), len));
                }
            }
        }
        best.map(|(id, _)| id.to_string())
    }

    /// Load region names from a cartography config for region resolution.
    pub fn load_cartography(&mut self, regions: &HashMap<String, sidequest_genre::Region>) {
        self.region_names = regions
            .iter()
            .map(|(id, region)| (id.clone(), region.name.to_lowercase()))
            .collect();
        tracing::info!(
            region_count = self.region_names.len(),
            "Loaded cartography regions for co-location"
        );
    }

    /// Check if any players have active perceptual effects that would
    /// require narration rewriting.
    ///
    /// Returns true if at least one player has effects. The actual
    /// rewriting requires a `PerceptionRewriter` with a configured
    /// strategy (currently RED phase / stub — no production strategy yet).
    pub fn has_perception_effects(&self) -> bool {
        self.perception_filters.values().any(|f| f.has_effects())
    }

    /// Describe active perceptual effects for a player (for prompt composition).
    /// Returns None if the player has no effects.
    pub fn describe_player_effects(&self, player_id: &str) -> Option<String> {
        self.perception_filters
            .get(player_id)
            .filter(|f| f.has_effects())
            .map(|f| PerceptionRewriter::describe_effects(f.effects()))
    }

    /// Copy world-level state FROM the shared session INTO local variables.
    /// Used at the start of dispatch_player_action so existing code works unchanged.
    pub fn sync_to_locals(
        &self,
        current_location: &mut String,
        npc_registry: &mut Vec<NpcRegistryEntry>,
        narration_history: &mut Vec<String>,
        discovered_regions: &mut Vec<String>,
        trope_states: &mut Vec<sidequest_game::trope::TropeState>,
    ) {
        *current_location = self.current_location.clone();
        *npc_registry = self.npc_registry.clone();
        *narration_history = self.narration_history.clone();
        *discovered_regions = self.discovered_regions.clone();
        *trope_states = self.trope_states.clone();
    }

    /// Sync per-player state FROM PlayerState INTO per-connection locals.
    /// Called at the start of dispatch_player_action to pick up changes
    /// made by the barrier path (which can't access per-connection locals).
    pub fn sync_player_to_locals(
        &self,
        player_id: &str,
        hp: &mut i32,
        max_hp: &mut i32,
        level: &mut u32,
        xp: &mut u32,
        inventory: &mut sidequest_game::Inventory,
        combat_state: &mut sidequest_game::combat::CombatState,
        chase_state: &mut Option<sidequest_game::ChaseState>,
        character_json: &mut Option<serde_json::Value>,
    ) {
        if let Some(ps) = self.players.get(player_id) {
            *hp = ps.character_hp;
            *max_hp = ps.character_max_hp;
            *level = ps.character_level;
            *xp = ps.character_xp;
            *inventory = ps.inventory.clone();
            *combat_state = ps.combat_state.clone();
            *chase_state = ps.chase_state.clone();
            if let Some(ref cj) = ps.character_json {
                *character_json = Some(cj.clone());
            }
        }
    }

    /// Copy world-level state FROM local variables BACK INTO the shared session.
    /// Used at the end of dispatch_player_action after the narrator has run.
    pub fn sync_from_locals(
        &mut self,
        current_location: &str,
        npc_registry: &[NpcRegistryEntry],
        narration_history: &[String],
        discovered_regions: &[String],
        trope_states: &[sidequest_game::trope::TropeState],
        player_id: &str,
    ) {
        self.current_location = current_location.to_string();
        self.npc_registry = npc_registry.to_vec();
        self.narration_history = narration_history.to_vec();
        self.discovered_regions = discovered_regions.to_vec();
        self.trope_states = trope_states.to_vec();
        // Resolve region before mutably borrowing players
        let resolved = self.resolve_region(current_location).unwrap_or_default();
        if let Some(ps) = self.players.get_mut(player_id) {
            ps.display_location = current_location.to_string();
            ps.region_id = resolved;
        }
    }
}


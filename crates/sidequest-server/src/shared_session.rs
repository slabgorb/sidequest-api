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
    pub character_xp: u32,
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
            character_xp: 0,
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

    // --- Per-player state ---
    pub players: HashMap<String, PlayerState>,

    // --- Session-scoped broadcast (narration to all members) ---
    pub session_tx: broadcast::Sender<GameMessage>,
}

impl SharedGameSession {
    /// Create a new shared session for a genre:world pair.
    pub fn new(genre_slug: String, world_slug: String) -> Self {
        let (session_tx, _) = broadcast::channel(64);
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
            players: HashMap::new(),
            session_tx,
        }
    }

    /// Number of connected players.
    pub fn player_count(&self) -> usize {
        self.players.len()
    }

    /// Subscribe to the session broadcast channel.
    pub fn subscribe(&self) -> broadcast::Receiver<GameMessage> {
        self.session_tx.subscribe()
    }

    /// Broadcast a message to all session members.
    pub fn broadcast(&self, msg: GameMessage) {
        // Ignore send errors (no active receivers is fine)
        let _ = self.session_tx.send(msg);
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

    /// Copy world-level state FROM local variables BACK INTO the shared session.
    /// Used at the end of dispatch_player_action after the narrator has run.
    pub fn sync_from_locals(
        &mut self,
        current_location: &str,
        npc_registry: &[NpcRegistryEntry],
        narration_history: &[String],
        discovered_regions: &[String],
        trope_states: &[sidequest_game::trope::TropeState],
    ) {
        self.current_location = current_location.to_string();
        self.npc_registry = npc_registry.to_vec();
        self.narration_history = narration_history.to_vec();
        self.discovered_regions = discovered_regions.to_vec();
        self.trope_states = trope_states.to_vec();
    }
}

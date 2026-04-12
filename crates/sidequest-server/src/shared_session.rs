//! Shared multiplayer game session — world-level state shared across players.
//!
//! A `SharedGameSession` holds the world state (location, NPCs, narration
//! history, music, tropes) that is common to all players in the same
//! genre:world instance. Per-player state lives in `PlayerState`.

use std::collections::HashMap;

use tokio::sync::broadcast;
use uuid::Uuid;

use sidequest_game::barrier::TurnBarrier;
use sidequest_game::builder::CharacterBuilder;
use sidequest_game::guest_npc::PlayerRole;
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
use sidequest_game::perception::PerceptionFilter;
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
    /// Player role for multiplayer permission gating (story 35-6).
    ///
    /// Defaults to `PlayerRole::Full` — full agency, no action restrictions.
    /// Guest NPC players (ADR-029) get `PlayerRole::GuestNpc { .. }` with a
    /// restricted `allowed_actions` set. The dispatch pipeline reads this
    /// field via `role()` after intent classification and calls
    /// `can_perform()` to enforce the restriction, emitting OTEL watcher
    /// events on every decision.
    ///
    /// **Private with crate-only setter** to satisfy rust.md rule #9
    /// (security-critical fields must be private with getters). The
    /// `pub(crate) set_role` method is the only sanctioned write site —
    /// it should be called from the connect handshake when assigning a
    /// guest NPC role. Direct field mutation is impossible from outside
    /// the crate, preventing accidental privilege escalation by future
    /// code that holds `&mut PlayerState`.
    role: PlayerRole,
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
    /// Cached character sheet details (race/stats/abilities/backstory/etc.)
    /// populated at the end of chargen. `None` before chargen completes.
    /// This is the single source of truth the PARTY_STATUS builder reads from;
    /// there is no longer a separate CHARACTER_SHEET message to fall back on.
    pub sheet: Option<sidequest_protocol::CharacterSheetDetails>,
}

impl PlayerState {
    /// Create a new player state with defaults.
    ///
    /// `role` defaults to `PlayerRole::Full` — guest NPC roles must be set
    /// explicitly by the connect handshake (future story — 35-6 wires the
    /// enforcement path but leaves role selection at connect time out of scope).
    pub fn new(player_name: String) -> Self {
        Self {
            player_name,
            role: PlayerRole::Full,
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
            sheet: None,
        }
    }

    /// Read the player's role for permission gating.
    ///
    /// Used by `dispatch_player_action` to look up whether the player is
    /// a guest NPC and what action categories they may perform. Story 35-6.
    pub fn role(&self) -> &PlayerRole {
        &self.role
    }

    /// Assign the player's role. **Crate-only** — the only sanctioned write
    /// site is the connect handshake when binding a guest NPC to a player.
    ///
    /// Marked `pub(crate)` to satisfy rust.md rule #9 (security-critical
    /// fields must not be mutable from outside the crate). Direct field
    /// assignment is impossible because the field itself is private.
    /// Story 35-6.
    ///
    /// Currently has no callers — the connect handshake protocol extension
    /// for guest NPCs is a future story (35-6 wires the enforcement gate
    /// but leaves role assignment at connect time out of scope). The
    /// `#[allow(dead_code)]` documents that the setter is intentionally
    /// part of the API surface awaiting the connect handshake story.
    #[allow(dead_code)]
    pub(crate) fn set_role(&mut self, role: PlayerRole) {
        self.role = role;
    }
}

// ---------------------------------------------------------------------------
// PartyMember construction helpers
// ---------------------------------------------------------------------------
//
// PARTY_STATUS is now the single source of truth for per-character state, so
// every PartyMember construction site should go through these helpers. Doing
// it in one place means adding a field to PartyMember can't silently skip a
// construction site.

/// Convert an `Inventory` into the wire-format `InventoryPayload`.
pub fn inventory_payload_from(
    inv: &sidequest_game::Inventory,
) -> sidequest_protocol::InventoryPayload {
    sidequest_protocol::InventoryPayload {
        items: inv
            .carried()
            .map(|item| sidequest_protocol::InventoryItem {
                name: item.name.as_str().to_string(),
                item_type: item.category.as_str().to_string(),
                equipped: item.equipped,
                quantity: item.quantity,
                description: item.description.as_str().to_string(),
            })
            .collect(),
        gold: inv.gold,
    }
}

/// Build a `PartyMember` for an observer from their `PlayerState`.
///
/// Used by the PARTY_STATUS broadcast path in session_sync and everywhere
/// else that iterates `SharedGameSession::players`. The acting player in
/// dispatch/mod.rs builds its own PartyMember inline because the live turn
/// data on `DispatchContext` is fresher than `PlayerState` until
/// `sync_from_locals` runs — but it uses this same helper to populate the
/// `sheet` facet from the cached per-player detail.
pub fn party_member_from(pid: &str, ps: &PlayerState) -> sidequest_protocol::PartyMember {
    sidequest_protocol::PartyMember {
        player_id: pid.to_string(),
        name: ps.player_name.clone(),
        character_name: ps
            .character_name
            .clone()
            .unwrap_or_else(|| ps.player_name.clone()),
        current_hp: ps.character_hp,
        max_hp: ps.character_max_hp,
        statuses: vec![],
        class: ps.character_class.clone(),
        level: ps.character_level,
        portrait_url: None,
        current_location: ps.display_location.clone(),
        sheet: ps.sheet.clone(),
        inventory: Some(inventory_payload_from(&ps.inventory)),
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
    /// Unique identifier for this game session instance. Distinguishes
    /// sequential games on the same genre:world pair. Generated as UUID v4
    /// on session creation. Used for OTEL tracing and stale-session detection.
    pub session_id: String,

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

    // --- Dice ---
    /// Pending DiceRequests awaiting DiceThrow from the rolling player.
    /// Keyed by `request_id`. Inserted when DiceRequest is broadcast,
    /// consumed when DiceThrow arrives and resolution completes.
    pub pending_dice_requests: HashMap<String, sidequest_protocol::DiceRequestPayload>,
    /// Last resolved dice outcome (story 34-9). Set by DiceThrow handler
    /// after resolution, consumed (taken) by the next narration dispatch
    /// to inject into TurnContext for narrator prompt tone shaping.
    pub last_roll_outcome: Option<sidequest_protocol::RollOutcome>,

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
        let session_id = Uuid::new_v4().to_string();
        Self {
            genre_slug,
            world_slug,
            session_id,
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
            pending_dice_requests: HashMap::new(),
            last_roll_outcome: None,
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
                if best.is_none_or(|(_, prev_len)| len > prev_len) {
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

    /// Copy world-level state FROM the shared session INTO local variables.
    /// Used at the start of dispatch_player_action so existing code works unchanged.
    ///
    /// Note: only overwrites `current_location` when the shared session has a
    /// non-empty value. This prevents the initial location set during chargen
    /// (before sync_from_locals has run) from being blanked by the default empty.
    pub fn sync_to_locals(
        &self,
        current_location: &mut String,
        npc_registry: &mut Vec<NpcRegistryEntry>,
        narration_history: &mut Vec<String>,
        discovered_regions: &mut Vec<String>,
        trope_states: &mut Vec<sidequest_game::trope::TropeState>,
    ) {
        if !self.current_location.is_empty() {
            *current_location = self.current_location.clone();
        }
        *npc_registry = self.npc_registry.clone();
        *narration_history = self.narration_history.clone();
        if !self.discovered_regions.is_empty() {
            *discovered_regions = self.discovered_regions.clone();
        }
        *trope_states = self.trope_states.clone();
    }

    /// Sync per-player state FROM PlayerState INTO per-connection locals.
    /// Called at the start of dispatch_player_action to pick up changes
    /// made by the barrier path (which can't access per-connection locals).
    // 8 args — fold into a `PlayerLocals` struct in the dispatch refactor.
    #[allow(clippy::too_many_arguments)]
    pub fn sync_player_to_locals(
        &self,
        player_id: &str,
        hp: &mut i32,
        max_hp: &mut i32,
        level: &mut u32,
        xp: &mut u32,
        inventory: &mut sidequest_game::Inventory,
        character_json: &mut Option<serde_json::Value>,
    ) {
        if let Some(ps) = self.players.get(player_id) {
            *hp = ps.character_hp;
            *max_hp = ps.character_max_hp;
            *level = ps.character_level;
            *xp = ps.character_xp;
            *inventory = ps.inventory.clone();
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

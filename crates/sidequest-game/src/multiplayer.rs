//! Multiplayer session — coordinate multiple players with barrier-sync turns.
//!
//! Story 8-1: Port of Python `MultiplayerSession`. Tracks player→character
//! mapping, collects per-player actions, and resolves turns when all players
//! have submitted (barrier sync). Kept synchronous — async coordination
//! happens at the server layer.

use std::collections::{HashMap, HashSet};

use crate::character::Character;
use crate::combatant::Combatant;
use crate::turn_mode::TurnMode;

/// Status of a turn after an action submission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnStatus {
    /// Still waiting for other players to submit.
    Waiting,
    /// All players submitted — turn resolved.
    Resolved,
}

/// Result of submitting a player action.
#[derive(Debug)]
pub struct TurnResult {
    /// Whether the turn resolved or is still waiting.
    pub status: TurnStatus,
    /// Per-player narration stubs, present only when status is Resolved.
    /// Keyed by player_id.
    pub narration: Option<HashMap<String, String>>,
}

/// Error type for multiplayer session operations.
#[derive(Debug, thiserror::Error)]
pub enum MultiplayerError {
    /// Player ID already exists in the session.
    #[error("player '{0}' already in session")]
    DuplicatePlayer(String),
    /// Player ID is empty.
    #[error("player id cannot be empty")]
    EmptyPlayerId,
    /// Player ID not found.
    #[error("player '{0}' not found")]
    PlayerNotFound(String),
    /// Session is at max capacity.
    #[error("session full (max {0} players)")]
    SessionFull(usize),
}

/// Coordinates multiple WebSocket clients in a single game session.
///
/// Manages player→character mapping, barrier-sync turn collection, and
/// provides snapshot generation for late joiners.
pub struct MultiplayerSession {
    /// player_id → Character
    players: HashMap<String, Character>,
    /// Current turn number (starts at 1).
    turn: u32,
    /// Actions collected this turn: player_id → action string.
    actions: HashMap<String, String>,
    /// Actions from the last resolved turn (kept for queries after resolution).
    last_resolved_actions: HashMap<String, String>,
    /// Narration from the last resolved turn.
    last_narration: Option<HashMap<String, String>>,
}

impl MultiplayerSession {
    /// Maximum number of players in a session.
    pub const MAX_PLAYERS: usize = 6;

    /// Create a new multiplayer session with the given player→character map.
    pub fn new(players: HashMap<String, Character>) -> Self {
        Self {
            players,
            turn: 1,
            actions: HashMap::new(),
            last_resolved_actions: HashMap::new(),
            last_narration: None,
        }
    }

    /// Create a session from player IDs only (no Character data).
    ///
    /// Used by `TurnBarrier` which only needs player count and ID tracking
    /// for barrier-met checks, not full Character objects. Character names
    /// are generated as "Character {id}" to distinguish them from player IDs.
    pub fn with_player_ids(player_ids: impl IntoIterator<Item = String>) -> Self {
        use crate::creature_core::CreatureCore;
        use crate::inventory::Inventory;
        use sidequest_protocol::NonBlankString;

        let players: HashMap<String, Character> = player_ids
            .into_iter()
            .map(|id| {
                let char_name = format!("Character {}", id);
                let name = NonBlankString::new(&char_name)
                    .unwrap_or_else(|_| NonBlankString::new("unknown").unwrap());
                let character = Character {
                    core: CreatureCore {
                        name: name.clone(),
                        description: NonBlankString::new("barrier placeholder").unwrap(),
                        personality: NonBlankString::new("n/a").unwrap(),
                        level: 1,
                        hp: 1,
                        max_hp: 1,
                        ac: 10,
                        xp: 0,
                        statuses: vec![],
                        inventory: Inventory::default(),
                    },
                    backstory: NonBlankString::new("n/a").unwrap(),
                    narrative_state: String::new(),
                    hooks: vec![],
                    char_class: NonBlankString::new("barrier").unwrap(),
                    race: NonBlankString::new("barrier").unwrap(),
                    pronouns: String::new(),
                    stats: HashMap::new(),
                    abilities: vec![],
                    known_facts: vec![],
                    affinities: vec![],
                    is_friendly: true,
                    resolved_archetype: None,
                };
                (id, character)
            })
            .collect();
        Self::new(players)
    }

    /// Number of players currently in the session.
    pub fn player_count(&self) -> usize {
        self.players.len()
    }

    /// Current turn number (starts at 1, increments on resolution).
    pub fn turn_number(&self) -> u32 {
        self.turn
    }

    /// Player IDs that haven't submitted an action this turn.
    pub fn pending_players(&self) -> HashSet<String> {
        self.players
            .keys()
            .filter(|id| !self.actions.contains_key(id.as_str()))
            .cloned()
            .collect()
    }

    /// Add a player to the session. Returns new player count.
    pub fn add_player(
        &mut self,
        player_id: String,
        character: Character,
    ) -> Result<usize, MultiplayerError> {
        if player_id.is_empty() {
            return Err(MultiplayerError::EmptyPlayerId);
        }
        if self.players.contains_key(&player_id) {
            return Err(MultiplayerError::DuplicatePlayer(player_id));
        }
        if self.players.len() >= Self::MAX_PLAYERS {
            return Err(MultiplayerError::SessionFull(Self::MAX_PLAYERS));
        }
        self.players.insert(player_id, character);
        Ok(self.players.len())
    }

    /// Remove a player from the session. Returns remaining player count.
    ///
    /// If the removed player had a pending action, it's discarded.
    /// If removing this player means all remaining players have submitted,
    /// the turn auto-resolves.
    pub fn remove_player(&mut self, player_id: &str) -> Result<usize, MultiplayerError> {
        if self.players.remove(player_id).is_none() {
            return Err(MultiplayerError::PlayerNotFound(player_id.to_string()));
        }
        self.actions.remove(player_id);
        // Check if barrier is now met after removal
        self.try_resolve();
        Ok(self.players.len())
    }

    /// Submit an action for a player. Returns a `TurnResult` indicating
    /// whether the turn resolved.
    ///
    /// Unknown players are ignored (returns Waiting). Duplicate submissions
    /// from the same player are idempotent (first action wins).
    pub fn submit_action(&mut self, player_id: &str, action: &str) -> TurnResult {
        // Unknown player — ignore
        if !self.players.contains_key(player_id) {
            return TurnResult {
                status: TurnStatus::Waiting,
                narration: None,
            };
        }

        // Duplicate — idempotent, first action wins
        if self.actions.contains_key(player_id) {
            return TurnResult {
                status: TurnStatus::Waiting,
                narration: None,
            };
        }

        self.actions
            .insert(player_id.to_string(), action.to_string());

        // Check barrier
        if self.actions.len() >= self.players.len() {
            let narration = self.resolve_turn();
            TurnResult {
                status: TurnStatus::Resolved,
                narration: Some(narration),
            }
        } else {
            TurnResult {
                status: TurnStatus::Waiting,
                narration: None,
            }
        }
    }

    /// Raw actions map: player_id → action string (only submitted actions).
    pub fn current_actions(&self) -> HashMap<String, String> {
        self.actions.clone()
    }

    /// Actions keyed by character name instead of player_id.
    ///
    /// Returns current turn's actions if any have been submitted,
    /// otherwise returns the last resolved turn's actions.
    pub fn named_actions(&self) -> HashMap<String, String> {
        let source = if self.actions.is_empty() {
            &self.last_resolved_actions
        } else {
            &self.actions
        };
        source
            .iter()
            .filter_map(|(pid, action)| {
                self.players
                    .get(pid)
                    .map(|ch| (ch.name().to_string(), action.clone()))
            })
            .collect()
    }

    /// Actions with full identity: `(player_id, character_name, action)`.
    ///
    /// Same source selection as [`named_actions`], but preserves the player
    /// id so that downstream `PlayerActionEntry` construction can populate
    /// `player_id` without falling back to empty-string sentinels.
    pub fn identified_actions(&self) -> Vec<(String, String, String)> {
        let source = if self.actions.is_empty() {
            &self.last_resolved_actions
        } else {
            &self.actions
        };
        source
            .iter()
            .filter_map(|(pid, action)| {
                self.players
                    .get(pid)
                    .map(|ch| (pid.clone(), ch.name().to_string(), action.clone()))
            })
            .collect()
    }

    /// Generate a catch-up snapshot string for a player (e.g., late joiner).
    pub fn generate_snapshot(&self, player_id: &str) -> Result<String, MultiplayerError> {
        let character = self
            .players
            .get(player_id)
            .ok_or_else(|| MultiplayerError::PlayerNotFound(player_id.to_string()))?;

        let others: Vec<String> = self
            .players
            .iter()
            .filter(|(id, _)| id.as_str() != player_id)
            .map(|(_, ch)| ch.name().to_string())
            .collect();

        let others_str = if others.is_empty() {
            "no other players".to_string()
        } else {
            others.join(", ")
        };

        Ok(format!(
            "You are {} on turn {}. Party members: {}.",
            character.name(),
            self.turn,
            others_str,
        ))
    }

    /// Returns a map of player_id → character_name for players who haven't
    /// submitted an action yet (useful for sending reminders).
    pub fn check_reminders(&self) -> HashMap<String, String> {
        self.players
            .iter()
            .filter(|(id, _)| !self.actions.contains_key(id.as_str()))
            .map(|(id, ch)| (id.clone(), ch.name().to_string()))
            .collect()
    }

    /// Record an action without triggering auto-resolution.
    /// Returns true if the action was recorded (player exists and hasn't
    /// already submitted). Used by `TurnBarrier` to decouple submission
    /// from resolution.
    pub fn record_action(&mut self, player_id: &str, action: &str) -> bool {
        if !self.players.contains_key(player_id) {
            return false;
        }
        if self.actions.contains_key(player_id) {
            return false;
        }
        self.actions
            .insert(player_id.to_string(), action.to_string());
        true
    }

    /// Check whether all players have submitted actions (barrier met).
    pub fn is_barrier_met(&self) -> bool {
        !self.players.is_empty() && self.actions.len() >= self.players.len()
    }

    /// Remove a player without triggering auto-resolution.
    /// Used by `TurnBarrier` which manages resolution itself.
    pub fn remove_player_no_resolve(&mut self, player_id: &str) -> Result<usize, MultiplayerError> {
        if self.players.remove(player_id).is_none() {
            return Err(MultiplayerError::PlayerNotFound(player_id.to_string()));
        }
        self.actions.remove(player_id);
        Ok(self.players.len())
    }

    /// Force-resolve the current turn, filling in "hesitates" for missing
    /// players. Used by the turn barrier on timeout.
    pub fn force_resolve_turn(&mut self) -> HashMap<String, String> {
        self.force_resolve_turn_for_mode(&TurnMode::FreePlay)
    }

    /// Force-resolve the current turn with a mode-contextual default action
    /// for missing players.
    ///
    /// - Structured / FreePlay → "hesitates, waiting to see what happens"
    /// - Cinematic → "remains silent"
    pub fn force_resolve_turn_for_mode(&mut self, mode: &TurnMode) -> HashMap<String, String> {
        let default_action = match mode {
            TurnMode::Cinematic { .. } => "remains silent",
            _ => "hesitates, waiting to see what happens",
        };
        for pid in self.players.keys().cloned().collect::<Vec<_>>() {
            self.actions
                .entry(pid)
                .or_insert_with(|| default_action.to_string());
        }
        self.resolve_turn()
    }

    /// Resolve the current turn: generate narration stubs and advance.
    fn resolve_turn(&mut self) -> HashMap<String, String> {
        let narration: HashMap<String, String> = self
            .players
            .keys()
            .map(|pid| {
                let action = self.actions.get(pid).cloned().unwrap_or_default();
                let char_name = self
                    .players
                    .get(pid)
                    .map(|ch| ch.name().to_string())
                    .unwrap_or_default();
                (pid.clone(), format!("{char_name}: {action}"))
            })
            .collect();

        self.last_resolved_actions = std::mem::take(&mut self.actions);
        self.last_narration = Some(narration.clone());
        self.turn += 1;
        narration
    }

    /// Check if all remaining players have submitted and auto-resolve if so.
    fn try_resolve(&mut self) {
        if !self.players.is_empty() && self.actions.len() >= self.players.len() {
            self.resolve_turn();
        }
    }
}

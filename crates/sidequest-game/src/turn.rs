//! Turn management — phase tracking, round counting, and barrier semantics.
//!
//! Two-tier turn model:
//! - `interaction` (granular): increments every player-narrator exchange.
//!   Used for fact/item discovery chronology. Monotonic, never resets.
//! - `round` (display): advances on meaningful narrative beats — location
//!   changes, chapter markers, trope escalations. This is what the player sees.
//!
//! ADR-006: Both counters always increment, never reset.
//! Persisted across sessions — loading a save restores the exact counts.
//!
//! Story 1-8: Barrier semantics — single-player advances immediately,
//! multi-player waits for all players to submit input before advancing.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

/// The phases of a game turn (ADR-006).
///
/// Turns progress: InputCollection -> IntentRouting -> AgentExecution ->
/// StatePatch -> Broadcast.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[derive(Default)]
pub enum TurnPhase {
    /// Collecting player input.
    #[default]
    InputCollection,
    /// Routing intents to agents.
    IntentRouting,
    /// Agents executing actions.
    AgentExecution,
    /// Applying state patches.
    StatePatch,
    /// Broadcasting results.
    Broadcast,
}

/// Tracks the current turn round, phase, and player input barrier.
///
/// Two-tier model:
/// - `interaction`: monotonic counter for every player-narrator exchange.
///   Powers fact chronology, item discovery timestamps, NPC last-seen tracking.
/// - `round`: display counter for meaningful narrative beats (location changes,
///   chapter markers, trope escalations). Shown to the player.
///
/// Both counters always increment, never reset. Persisted across sessions.
/// Barrier semantics: all players must submit input before the turn advances
/// past InputCollection. Duplicate submissions from the same player are ignored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnManager {
    /// Display round — advances on meaningful narrative beats only.
    #[serde(default = "default_round")]
    round: u32,
    /// Granular interaction counter — increments every player-narrator exchange.
    #[serde(default = "default_interaction")]
    interaction: u64,
    #[serde(default)]
    phase: TurnPhase,
    #[serde(default)]
    player_count: usize,
    #[serde(skip)]
    submitted: HashSet<String>,
}

fn default_round() -> u32 {
    1
}
fn default_interaction() -> u64 {
    1
}

impl TurnManager {
    /// Create a new turn manager starting at round 1, interaction 1.
    pub fn new() -> Self {
        Self {
            round: 1,
            interaction: 1,
            phase: TurnPhase::InputCollection,
            player_count: 1,
            submitted: HashSet::new(),
        }
    }

    /// Current display round number (starts at 1, increments on narrative beats).
    pub fn round(&self) -> u32 {
        self.round
    }

    /// Current granular interaction number (starts at 1, increments every exchange).
    pub fn interaction(&self) -> u64 {
        self.interaction
    }

    /// Current phase within the turn.
    pub fn phase(&self) -> TurnPhase {
        self.phase
    }

    /// Set the number of players required to advance past InputCollection.
    pub fn set_player_count(&mut self, count: usize) {
        self.player_count = count;
    }

    /// Submit input for a player. If all players have submitted, advances
    /// to IntentRouting. Duplicate submissions from the same player are ignored.
    pub fn submit_input(&mut self, player_id: &str) {
        if self.phase != TurnPhase::InputCollection {
            return;
        }
        self.submitted.insert(player_id.to_string());
        if self.submitted.len() >= self.player_count {
            self.phase = TurnPhase::IntentRouting;
            self.submitted.clear();
        }
    }

    /// Record a player-narrator interaction (granular counter).
    /// Call this after every narration response. Resets phase to InputCollection.
    pub fn record_interaction(&mut self) {
        self.interaction += 1;
        self.phase = TurnPhase::InputCollection;
        self.submitted.clear();
    }

    /// Advance the display round — call on meaningful narrative beats
    /// (location changes, chapter markers, trope escalations).
    pub fn advance_round(&mut self) {
        self.round += 1;
    }

    /// Advance to the next round (legacy — increments display round + resets phase).
    /// Prefer `record_interaction()` + `advance_round()` for the two-tier model.
    pub fn advance(&mut self) {
        self.round += 1;
        self.phase = TurnPhase::InputCollection;
        self.submitted.clear();
    }

    /// Advance to the next phase within the current round.
    pub fn advance_phase(&mut self) {
        let from = self.phase;
        self.phase = match self.phase {
            TurnPhase::InputCollection => TurnPhase::IntentRouting,
            TurnPhase::IntentRouting => TurnPhase::AgentExecution,
            TurnPhase::AgentExecution => TurnPhase::StatePatch,
            TurnPhase::StatePatch => TurnPhase::Broadcast,
            TurnPhase::Broadcast => TurnPhase::Broadcast, // stays at last phase
        };
        let span = tracing::info_span!(
            "turn.phase_transition",
            from_phase = ?from,
            to_phase = ?self.phase,
            round = self.round,
        );
        let _guard = span.enter();
    }
}

impl Default for TurnManager {
    fn default() -> Self {
        Self::new()
    }
}

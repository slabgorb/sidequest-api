//! Turn management — phase tracking, round counting, and barrier semantics.
//!
//! ADR-006: Turns advance in discrete phases. Round counts always
//! increment, never reset.
//!
//! Story 1-8: Barrier semantics — single-player advances immediately,
//! multi-player waits for all players to submit input before advancing.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

/// The phases of a game turn (ADR-006).
///
/// Turns progress: InputCollection → IntentRouting → AgentExecution →
/// StatePatch → Broadcast.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum TurnPhase {
    /// Collecting player input.
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

impl Default for TurnPhase {
    fn default() -> Self {
        Self::InputCollection
    }
}

/// Tracks the current turn round, phase, and player input barrier.
///
/// Round counter always increments, never resets.
/// Barrier semantics: all players must submit input before the turn advances
/// past InputCollection. Duplicate submissions from the same player are ignored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnManager {
    #[serde(default)]
    round: u32,
    #[serde(default)]
    phase: TurnPhase,
    #[serde(default)]
    player_count: usize,
    #[serde(skip)]
    submitted: HashSet<String>,
}

impl TurnManager {
    /// Create a new turn manager starting at round 1, InputCollection phase.
    pub fn new() -> Self {
        Self {
            round: 1,
            phase: TurnPhase::InputCollection,
            player_count: 1,
            submitted: HashSet::new(),
        }
    }

    /// Current round number (starts at 1, always increases).
    pub fn round(&self) -> u32 {
        self.round
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

    /// Advance to the next round (increments counter, resets phase to InputCollection).
    pub fn advance(&mut self) {
        self.round += 1;
        self.phase = TurnPhase::InputCollection;
        self.submitted.clear();
    }

    /// Advance to the next phase within the current round.
    pub fn advance_phase(&mut self) {
        self.phase = match self.phase {
            TurnPhase::InputCollection => TurnPhase::IntentRouting,
            TurnPhase::IntentRouting => TurnPhase::AgentExecution,
            TurnPhase::AgentExecution => TurnPhase::StatePatch,
            TurnPhase::StatePatch => TurnPhase::Broadcast,
            TurnPhase::Broadcast => TurnPhase::Broadcast, // stays at last phase
        };
    }
}

impl Default for TurnManager {
    fn default() -> Self {
        Self::new()
    }
}

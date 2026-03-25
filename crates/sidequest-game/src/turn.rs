//! Turn management — phase tracking and round counting.
//!
//! ADR-006: Turns advance in discrete phases. Round counts always
//! increment, never reset.

/// The phases of a game turn (ADR-006).
///
/// Turns progress: InputCollection → IntentRouting → AgentExecution →
/// StatePatch → Broadcast.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

/// Tracks the current turn round and phase.
///
/// Round counter always increments, never resets.
pub struct TurnManager {
    round: u32,
    phase: TurnPhase,
}

impl TurnManager {
    /// Create a new turn manager starting at round 1, InputCollection phase.
    pub fn new() -> Self {
        Self {
            round: 1,
            phase: TurnPhase::InputCollection,
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

    /// Advance to the next round (increments counter, resets phase to InputCollection).
    pub fn advance(&mut self) {
        self.round += 1;
        self.phase = TurnPhase::InputCollection;
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

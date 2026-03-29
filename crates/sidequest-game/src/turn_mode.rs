//! Turn mode state machine — controls action collection and resolution.
//!
//! Story 8-5: Three modes determine how the multiplayer session coordinates
//! player actions:
//!
//! - **FreePlay** — Actions resolve immediately. No barrier. Default mode.
//! - **Structured** — Blind simultaneous submission. Barrier waits for all.
//! - **Cinematic** — Narrator-paced. Players respond to a prompt.

/// The current turn mode for a multiplayer session.
///
/// Determines whether actions are collected via barrier sync or resolved
/// immediately. Transitions are driven by game state changes (combat
/// start/end, cutscene start/end) via [`TurnModeTransition`].
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub enum TurnMode {
    /// Actions resolve immediately. No barrier sync. Default mode.
    #[default]
    FreePlay,
    /// Blind simultaneous submission. Barrier waits for all players.
    Structured,
    /// Narrator-paced. Players respond to a prompt before advancing.
    Cinematic {
        /// The narrator prompt for this cinematic moment, if any.
        prompt: Option<String>,
    },
}

impl TurnMode {
    /// Apply a state transition. Returns the new mode.
    ///
    /// Invalid transitions are no-ops — the current mode is returned
    /// unchanged. This is intentional: the orchestrator may fire transitions
    /// speculatively, and the state machine silently ignores inapplicable ones.
    pub fn apply(self, transition: TurnModeTransition) -> TurnMode {
        match (self, transition) {
            (TurnMode::FreePlay, TurnModeTransition::CombatStarted) => TurnMode::Structured,
            (TurnMode::FreePlay, TurnModeTransition::CutsceneStarted { prompt }) => {
                TurnMode::Cinematic {
                    prompt: Some(prompt),
                }
            }
            // Multiplayer: 2+ players → Structured (sealed envelope pattern).
            // All players submit blindly, barrier collects, narrator resolves as one scene.
            (TurnMode::FreePlay, TurnModeTransition::PlayerJoined { player_count }) if player_count > 1 => TurnMode::Structured,
            (TurnMode::FreePlay, TurnModeTransition::PlayerJoined { .. }) => TurnMode::FreePlay,
            // Revert to FreePlay when back to solo
            (TurnMode::Structured, TurnModeTransition::PlayerLeft { player_count }) if player_count <= 1 => {
                TurnMode::FreePlay
            }
            (TurnMode::Structured, TurnModeTransition::CombatEnded) => TurnMode::FreePlay,
            (TurnMode::Cinematic { .. }, TurnModeTransition::SceneEnded) => TurnMode::FreePlay,
            // All other combinations are no-ops.
            (mode, _) => mode,
        }
    }

    /// Whether this mode requires barrier synchronization.
    ///
    /// FreePlay resolves actions immediately (no barrier). Structured and
    /// Cinematic both wait for coordinated submission.
    pub fn should_use_barrier(&self) -> bool {
        !matches!(self, TurnMode::FreePlay)
    }
}

/// A state transition that can be applied to a [`TurnMode`].
///
/// The orchestrator fires these based on game state changes — combat
/// starting, ending, cutscene triggers, etc.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum TurnModeTransition {
    /// Combat has started — switch to blind simultaneous submission.
    CombatStarted,
    /// Combat has ended — return to free-form play.
    CombatEnded,
    /// A cutscene or dramatic moment has started.
    CutsceneStarted {
        /// The narrator prompt for this cinematic moment.
        prompt: String,
    },
    /// The current scene has ended — return to free-form play.
    SceneEnded,
    /// A player joined the session (carries new total player count).
    PlayerJoined {
        /// Total player count after join.
        player_count: usize,
    },
    /// A player left the session (carries remaining player count).
    PlayerLeft {
        /// Total player count after leave.
        player_count: usize,
    },
}

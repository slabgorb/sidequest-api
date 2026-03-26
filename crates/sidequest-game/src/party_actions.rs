//! Party action composition — compose multi-character PARTY ACTIONS block.
//!
//! Story 8-4: After the barrier resolves, compose the collected actions into
//! a structured `[PARTY ACTIONS]` block for the orchestrator. Maps player IDs
//! to character names, fills defaults for timed-out players, and renders
//! the block as a formatted string.

use std::collections::HashMap;

use crate::character::Character;
use crate::combatant::Combatant;

/// A single character's action within a party turn.
#[derive(Debug, Clone)]
pub struct CharacterAction {
    character_name: String,
    input: String,
    is_default: bool,
}

impl CharacterAction {
    /// The character's display name.
    pub fn character_name(&self) -> &str {
        &self.character_name
    }

    /// The action text submitted (or default text if timed out).
    pub fn input(&self) -> &str {
        &self.input
    }

    /// Whether this is a default action (player timed out).
    pub fn is_default(&self) -> bool {
        self.is_default
    }
}

/// Composed party actions for a single turn, ready for the orchestrator.
#[derive(Debug)]
pub struct PartyActions {
    actions: Vec<CharacterAction>,
    turn_number: u64,
}

impl PartyActions {
    /// Compose party actions from raw action data.
    ///
    /// - `actions`: player_id → action text (only submitted actions)
    /// - `players`: player_id → Character (the full player roster)
    /// - `missing`: player IDs that timed out (get default action)
    /// - `turn_number`: which turn these actions belong to
    ///
    /// Players in `players` that are neither in `actions` nor `missing`
    /// are treated as missing. Actions for player IDs not in `players`
    /// are silently ignored.
    pub fn compose(
        actions: &HashMap<String, String>,
        players: &HashMap<String, Character>,
        _missing: &[String],
        turn_number: u64,
    ) -> Self {
        let mut result = Vec::new();

        for (player_id, character) in players {
            let name = character.name().to_string();

            if let Some(action_text) = actions.get(player_id) {
                result.push(CharacterAction {
                    character_name: name,
                    input: action_text.clone(),
                    is_default: false,
                });
            } else {
                // Player didn't submit — either explicitly timed out
                // or implicitly missing. Either way, default action.
                result.push(CharacterAction {
                    character_name: name,
                    input: "hesitates, waiting".to_string(),
                    is_default: true,
                });
            }
        }

        Self {
            actions: result,
            turn_number,
        }
    }

    /// The character actions for this turn.
    pub fn actions(&self) -> &[CharacterAction] {
        &self.actions
    }

    /// Which turn these actions belong to.
    pub fn turn_number(&self) -> u64 {
        self.turn_number
    }

    /// Render the `[PARTY ACTIONS]` block for the orchestrator.
    ///
    /// Format:
    /// ```text
    /// [PARTY ACTIONS]
    /// - CharName: action text
    /// - CharName: action text (waiting)
    /// ```
    pub fn render(&self) -> String {
        let mut block = String::from("[PARTY ACTIONS]\n");
        for action in &self.actions {
            let suffix = if action.is_default { " (waiting)" } else { "" };
            block.push_str(&format!(
                "- {}: {}{}\n",
                action.character_name, action.input, suffix
            ));
        }
        block
    }
}

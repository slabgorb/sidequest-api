//! Preprocessed player action — three-perspective rewrite of raw player input.
//!
//! STT cleanup produces disfluency-free text in three forms:
//! - `you`: second-person ("You draw your sword")
//! - `named`: third-person with character name ("{Name} draws their sword")
//! - `intent`: neutral, no pronouns ("draw sword")

use serde::{Deserialize, Serialize};

/// A player action cleaned of STT disfluencies and rewritten into three perspectives.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PreprocessedAction {
    /// Second-person form: "You draw your sword"
    pub you: String,
    /// Third-person with character name: "{CharName} draws their sword"
    pub named: String,
    /// Neutral intent, no pronouns: "draw sword"
    pub intent: String,
    /// Whether the LLM classified this action as a power-grab attempt.
    #[serde(default)]
    pub is_power_grab: bool,
    /// Whether the player references using/checking/equipping items.
    #[serde(default)]
    pub references_inventory: bool,
    /// Whether the player addresses or mentions an NPC by name.
    #[serde(default)]
    pub references_npc: bool,
    /// Whether the player invokes a power, mutation, or supernatural ability.
    #[serde(default)]
    pub references_ability: bool,
    /// Whether the player mentions a specific place or attempts travel.
    #[serde(default)]
    pub references_location: bool,
}

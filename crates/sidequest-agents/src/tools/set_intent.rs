//! Scene intent validation tool (ADR-057 Phase 2).
//!
//! Validates a string argument against the `SceneIntent` enum.
//! This replaces the narrator's `scene_intent` JSON field with a typed tool call.

use std::fmt;

/// Scene intent — what the next player action is likely to be.
///
/// These are distinct from the orchestrator's `Intent` enum used for routing.
/// Scene intent is a prediction for UI/audio preparation, not agent dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SceneIntent {
    /// Conversation with NPCs.
    Dialogue,
    /// Moving through and observing the world.
    Exploration,
    /// Preparing for combat (not yet in combat).
    CombatPrep,
    /// Sneaking, hiding, avoiding detection.
    Stealth,
    /// Bargaining, persuading, making deals.
    Negotiation,
    /// Fleeing, running away, evading pursuit.
    Escape,
    /// Examining clues, solving puzzles, gathering information.
    Investigation,
    /// Social interaction, parties, ceremonies, non-transactional NPC time.
    Social,
}

impl SceneIntent {
    /// Convert to the canonical lowercase string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            SceneIntent::Dialogue => "dialogue",
            SceneIntent::Exploration => "exploration",
            SceneIntent::CombatPrep => "combat_prep",
            SceneIntent::Stealth => "stealth",
            SceneIntent::Negotiation => "negotiation",
            SceneIntent::Escape => "escape",
            SceneIntent::Investigation => "investigation",
            SceneIntent::Social => "social",
        }
    }
}

impl fmt::Display for SceneIntent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Error returned when an intent string doesn't match any known variant.
#[derive(Debug, thiserror::Error)]
#[error("invalid scene intent: \"{0}\" — expected one of: dialogue, exploration, combat_prep, stealth, negotiation, escape, investigation, social")]
pub struct InvalidIntent(String);

/// Validate a string against the `SceneIntent` enum (case-insensitive).
#[tracing::instrument(name = "tool.set_intent", skip_all, fields(input = %input))]
pub fn validate_intent(input: &str) -> Result<SceneIntent, InvalidIntent> {
    let result = match input.to_lowercase().as_str() {
        "dialogue" => Ok(SceneIntent::Dialogue),
        "exploration" => Ok(SceneIntent::Exploration),
        "combat_prep" => Ok(SceneIntent::CombatPrep),
        "stealth" => Ok(SceneIntent::Stealth),
        "negotiation" => Ok(SceneIntent::Negotiation),
        "escape" => Ok(SceneIntent::Escape),
        "investigation" => Ok(SceneIntent::Investigation),
        "social" => Ok(SceneIntent::Social),
        _ => Err(InvalidIntent(input.to_string())),
    };

    match &result {
        Ok(intent) => tracing::info!(valid = true, value = intent.as_str(), "intent validated"),
        Err(_) => tracing::warn!(valid = false, "intent validation failed"),
    }

    result
}

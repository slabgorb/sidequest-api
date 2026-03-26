//! Ability definitions — genre-voiced descriptions with mechanical effects.
//!
//! Story 9-1: Each ability carries a narrative description for the player
//! ("Your bond with ancient roots lets you sense corruption") and a
//! mechanical effect for game logic ("+2 Nature, detect corruption 30ft").
//! The `involuntary` flag marks abilities the narrator can trigger without
//! the player asking.

use serde::{Deserialize, Serialize};

/// A character ability with dual-voice representation.
///
/// Genre packs define abilities with narrative descriptions for the player
/// and mechanical effects for game logic. The player sees the genre voice;
/// the game engine uses the mechanical effect.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbilityDefinition {
    /// Ability name (e.g., "Root-Bonding", "Fireball").
    pub name: String,
    /// Narrative description in genre voice — what the player sees.
    pub genre_description: String,
    /// Mechanical game effect — what the engine uses.
    pub mechanical_effect: String,
    /// Whether this ability triggers without player action.
    /// Used by the narrator to inject involuntary ability context.
    pub involuntary: bool,
    /// How the character acquired this ability.
    pub source: AbilitySource,
}

impl AbilityDefinition {
    /// Returns the genre-voiced description for player display.
    pub fn display(&self) -> &str {
        &self.genre_description
    }

    /// Whether this ability triggers without player action.
    pub fn is_involuntary(&self) -> bool {
        self.involuntary
    }
}

/// How a character acquired an ability.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum AbilitySource {
    /// Innate to the character's race/species.
    Race,
    /// Granted by the character's class/archetype.
    Class,
    /// Bestowed by an item or artifact.
    Item,
    /// Acquired during gameplay through experience.
    Play,
}

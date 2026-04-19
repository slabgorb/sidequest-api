//! Narrative character sheet — genre-voiced player-facing display.
//!
//! Story 9-5: The character sheet reads like a passage from a novel, not a
//! stat block. Abilities use genre descriptions, status uses narrative voice,
//! and no raw numbers are exposed in player-facing fields.

use serde::{Deserialize, Serialize};

use crate::known_fact::Confidence;

/// A genre-voiced character sheet for player display.
///
/// Structured as typed sections so the UI can format each independently.
/// Serializes to JSON for protocol transmission (WebSocket CHARACTER_SHEET message).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NarrativeSheet {
    /// Genre-voiced identity line (name, race, class — no raw stats).
    pub identity: String,
    /// Character abilities with genre descriptions (never mechanical effects).
    pub abilities: Vec<AbilityEntry>,
    /// Known facts with confidence tags.
    pub knowledge: Vec<KnowledgeEntry>,
    /// Current status in narrative voice (no raw HP numbers).
    pub status: CharacterStatus,
}

/// A single ability entry for the narrative sheet.
///
/// Uses `genre_description` from AbilityDefinition, never `mechanical_effect`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbilityEntry {
    /// Ability name.
    pub name: String,
    /// Genre-voiced description — what the player sees.
    pub description: String,
    /// Whether this ability triggers without player action.
    pub involuntary: bool,
}

/// A single knowledge entry for the narrative sheet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeEntry {
    /// The fact content in genre voice.
    pub content: String,
    /// How confident the character is in this fact.
    pub confidence: Confidence,
}

/// Character status in narrative voice — no raw numbers exposed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterStatus {
    /// Narrative description of health (e.g., "badly wounded", "in good health").
    pub health: String,
    /// Active conditions in narrative voice.
    pub conditions: Vec<String>,
}

/// Convert raw edge ratio to a narrative composure description.
///
/// Epic 39 content (story 39-6) replaces this HP-flavored vocabulary
/// with genre-authored composure thresholds; until then the wording is
/// driven off the same edge/max values the EdgePool exposes.
fn describe_health(edge: i32, max_edge: i32) -> String {
    if max_edge <= 0 {
        return "in unknown condition".to_string();
    }
    let ratio = edge as f64 / max_edge as f64;
    if ratio >= 1.0 {
        "in good health".to_string()
    } else if ratio >= 0.75 {
        "lightly wounded".to_string()
    } else if ratio >= 0.5 {
        "wounded".to_string()
    } else if ratio >= 0.25 {
        "badly wounded".to_string()
    } else if ratio > 0.0 {
        "near death".to_string()
    } else {
        "fallen".to_string()
    }
}

impl CharacterStatus {
    /// Build a narrative status from creature stats.
    pub(crate) fn from_creature(edge: i32, max_edge: i32, statuses: &[String]) -> Self {
        Self {
            health: describe_health(edge, max_edge),
            conditions: statuses.to_vec(),
        }
    }
}

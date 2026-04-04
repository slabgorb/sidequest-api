//! Trope definitions from `tropes.yaml`.

use serde::{Deserialize, Serialize};
use sidequest_protocol::NonBlankString;

/// A narrative trope definition (genre-level or world-level).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TropeDefinition {
    /// Trope identifier (optional — some tropes use name-based slugs).
    #[serde(default)]
    pub id: Option<String>,
    /// Display name.
    pub name: NonBlankString,
    /// Optional description.
    #[serde(default)]
    pub description: Option<String>,
    /// Narrative category (conflict, revelation, recurring, climax, etc.).
    #[serde(default)]
    pub category: String,
    /// Player actions or events that activate this trope.
    #[serde(default)]
    pub triggers: Vec<String>,
    /// Narrator guidance when this trope is active.
    #[serde(default)]
    pub narrative_hints: Vec<String>,
    /// Base tension level (0.0–1.0). None means "inherit from parent" during merge.
    #[serde(default)]
    pub tension_level: Option<f64>,
    /// Suggested ways the trope can resolve.
    #[serde(default)]
    pub resolution_hints: Option<Vec<String>>,
    /// Resolution patterns (used by abstract tropes).
    #[serde(default)]
    pub resolution_patterns: Option<Vec<String>>,
    /// Categorization tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Escalation steps keyed by progression value.
    #[serde(default)]
    pub escalation: Vec<TropeEscalation>,
    /// Passive progression configuration.
    #[serde(default)]
    pub passive_progression: Option<PassiveProgression>,
    /// Whether this is an abstract archetype (must be extended by world tropes).
    #[serde(default, rename = "abstract")]
    pub is_abstract: bool,
    /// Parent trope slug to inherit from.
    #[serde(default)]
    pub extends: Option<String>,
}

/// A single escalation step within a trope.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TropeEscalation {
    /// Progression threshold (0.0–1.0) at which this fires.
    pub at: f64,
    /// Narrative event description.
    pub event: String,
    /// NPCs involved in this escalation.
    #[serde(default)]
    pub npcs_involved: Vec<String>,
    /// What's at stake.
    #[serde(default)]
    pub stakes: String,
}

/// Passive progression configuration for a trope.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PassiveProgression {
    /// Progression per game turn.
    #[serde(default)]
    pub rate_per_turn: f64,
    /// Progression per in-game day.
    #[serde(default)]
    pub rate_per_day: f64,
    /// Keywords that accelerate progression.
    #[serde(default)]
    pub accelerators: Vec<String>,
    /// Keywords that decelerate progression.
    #[serde(default)]
    pub decelerators: Vec<String>,
    /// Bonus per accelerator match.
    #[serde(default)]
    pub accelerator_bonus: f64,
    /// Penalty per decelerator match.
    #[serde(default)]
    pub decelerator_penalty: f64,
}

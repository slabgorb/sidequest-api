//! Narrative support types: prompts, openings, beat vocabulary, achievements, power tiers.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════
// prompts.yaml
// ═══════════════════════════════════════════════════════════

/// LLM prompt templates for different agent roles.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Prompts {
    /// Narrator system prompt.
    pub narrator: String,
    /// Combat narrator prompt.
    pub combat: String,
    /// NPC behavior prompt.
    pub npc: String,
    /// World state tracking prompt.
    pub world_state: String,
    /// Chase scene prompt.
    #[serde(default)]
    pub chase: Option<String>,
    /// Scene transition hint templates.
    #[serde(default)]
    pub transition_hints: HashMap<String, String>,
}

// ═══════════════════════════════════════════════════════════
// openings.yaml
// ═══════════════════════════════════════════════════════════

/// An opening scenario hook that constrains the narrator's first turn.
///
/// Each genre pack can define multiple opening hooks to ensure variety.
/// One is selected randomly at session start and injected into the
/// narrator's first-turn context.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpeningHook {
    /// Unique identifier within the genre (e.g. "arena_challenge").
    pub id: String,
    /// Archetype category (e.g. "challenge", "mystery", "chase", "survival", "standoff", "arrival").
    pub archetype: String,
    /// Situation description injected as narrator guidance — what's happening, what the vibe is.
    pub situation: String,
    /// Tone directive (e.g. "tense, competitive").
    pub tone: String,
    /// Patterns the narrator must avoid in this opening.
    #[serde(default)]
    pub avoid: Vec<String>,
    /// Synthetic first-turn action that replaces the generic "I look around".
    pub first_turn_seed: String,
}

// ═══════════════════════════════════════════════════════════
// beat_vocabulary.yaml
// ═══════════════════════════════════════════════════════════

/// Chase/beat vocabulary configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BeatVocabulary {
    /// Obstacles that can appear during chases.
    pub obstacles: Vec<BeatObstacle>,
}

/// A chase obstacle.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BeatObstacle {
    /// Obstacle name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Stat used for the check.
    pub stat_check: String,
    /// Penalty on failure.
    pub failure_penalty: String,
    /// Categorization tags.
    pub tags: Vec<String>,
}

// ═══════════════════════════════════════════════════════════
// achievements.yaml
// ═══════════════════════════════════════════════════════════

/// An achievement linked to trope progression.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Achievement {
    /// Achievement identifier.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Trope that triggers this achievement.
    pub trope_id: String,
    /// Trope status that triggers (activated, progressing, resolved).
    pub trigger_status: String,
    /// Display emoji.
    pub emoji: String,
}

// ═══════════════════════════════════════════════════════════
// power_tiers.yaml
// ═══════════════════════════════════════════════════════════

/// A power tier description for a character class at a level range.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PowerTier {
    /// Level range [min, max].
    pub level_range: [u32; 2],
    /// Tier label.
    pub label: String,
    /// Player appearance description.
    pub player: String,
    /// NPC appearance description (absent for max-level tiers — no level-10 NPCs).
    #[serde(default)]
    pub npc: Option<String>,
}

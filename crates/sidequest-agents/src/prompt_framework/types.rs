//! Core prompt framework types: AttentionZone, SectionCategory, RuleTier, PromptSection.

use serde::{Deserialize, Serialize};

/// Attention zones ordered from highest-primacy to highest-recency.
///
/// Maps to the proven attention pattern from ADR-009:
/// - Primacy/Early: high attention (identity, SOUL, critical rules)
/// - Valley: lower attention (lore, game state, background)
/// - Late/Recency: high attention (checklist, user input)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttentionZone {
    /// Highest attention — agent identity, agency rules.
    Primacy,
    /// High attention — SOUL principles, genre tone, critical rules.
    Early,
    /// Lower attention — lore, game state, character data.
    Valley,
    /// High attention — per-turn state, output format.
    Late,
    /// Highest attention — before-you-respond checklist, user input.
    Recency,
}

impl AttentionZone {
    /// Returns the sort order index (0 = first in prompt).
    pub fn order(&self) -> u8 {
        match self {
            AttentionZone::Primacy => 0,
            AttentionZone::Early => 1,
            AttentionZone::Valley => 2,
            AttentionZone::Late => 3,
            AttentionZone::Recency => 4,
        }
    }

    /// Returns all zones in prompt assembly order.
    pub fn all_ordered() -> Vec<AttentionZone> {
        vec![
            AttentionZone::Primacy,
            AttentionZone::Early,
            AttentionZone::Valley,
            AttentionZone::Late,
            AttentionZone::Recency,
        ]
    }
}

impl PartialOrd for AttentionZone {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for AttentionZone {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.order().cmp(&other.order())
    }
}

/// Prompt section categories — extensible as new agent types are added.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SectionCategory {
    /// Agent identity (name, persona, core purpose).
    Identity,
    /// Safety guardrails (agency, format, no-metagame).
    Guardrail,
    /// SOUL.md principles.
    Soul,
    /// Genre pack content (tone, rules, lore).
    Genre,
    /// Game state (characters, location, tropes).
    State,
    /// Player action / input.
    Action,
    /// Output format instructions.
    Format,
    /// General context sections (location, NPCs, quests).
    Context,
    /// Agent role definition.
    Role,
}

/// Three-tier rule taxonomy for agent system prompts.
///
/// Maps to the Python `RuleTier` / `RuleTaxonomy`:
/// - Critical: always enforced, all agents (agency, output-format, no-metagame)
/// - Firm: agent-specific behavioral rules (living-world, genre-truth)
/// - Coherence: stylistic guidelines (brevity, sensory-grounding)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleTier {
    /// Always enforced across all agents.
    Critical,
    /// Agent-specific behavioral rules.
    Firm,
    /// Stylistic guidelines.
    Coherence,
}

/// A named, categorized, zone-labeled unit of prompt content.
///
/// Frozen (immutable) after construction. Token count is derived from content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PromptSection {
    /// Section name (e.g., "soul_principles", "genre_tone").
    pub name: String,
    /// What kind of content this section carries.
    pub category: SectionCategory,
    /// Which attention zone this section belongs to.
    pub zone: AttentionZone,
    /// The actual text content of this section.
    pub content: String,
    /// Optional provenance tag (e.g., "genre_pack", "soul_md").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

impl PromptSection {
    /// Create a new prompt section.
    pub fn new(
        name: impl Into<String>,
        content: impl Into<String>,
        zone: AttentionZone,
        category: SectionCategory,
    ) -> Self {
        Self {
            name: name.into(),
            category,
            zone,
            content: content.into(),
            source: None,
        }
    }

    /// Create a new prompt section with a source tag.
    pub fn with_source(
        name: impl Into<String>,
        content: impl Into<String>,
        zone: AttentionZone,
        category: SectionCategory,
        source: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            category,
            zone,
            content: content.into(),
            source: Some(source.into()),
        }
    }

    /// Approximate token count (word count as proxy).
    pub fn token_estimate(&self) -> usize {
        self.content.split_whitespace().count()
    }

    /// Returns true if the section has no content.
    pub fn is_empty(&self) -> bool {
        self.content.trim().is_empty()
    }
}

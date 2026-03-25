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
        todo!("AttentionZone::order")
    }

    /// Returns all zones in prompt assembly order.
    pub fn all_ordered() -> Vec<AttentionZone> {
        todo!("AttentionZone::all_ordered")
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

/// Closed set of prompt section categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionCategory {
    /// Agent identity and role definition.
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
        category: SectionCategory,
        zone: AttentionZone,
        content: impl Into<String>,
    ) -> Self {
        todo!("PromptSection::new")
    }

    /// Create a new prompt section with a source tag.
    pub fn with_source(
        name: impl Into<String>,
        category: SectionCategory,
        zone: AttentionZone,
        content: impl Into<String>,
        source: impl Into<String>,
    ) -> Self {
        todo!("PromptSection::with_source")
    }

    /// Approximate token count (word count as proxy).
    pub fn token_estimate(&self) -> usize {
        todo!("PromptSection::token_estimate")
    }

    /// Returns true if the section has no content.
    pub fn is_empty(&self) -> bool {
        todo!("PromptSection::is_empty")
    }
}

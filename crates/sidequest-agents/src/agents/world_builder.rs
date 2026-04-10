//! WorldBuilder agent — progressive world materialization based on campaign maturity.
//!
//! As a campaign matures (Fresh -> Early -> Mid -> Veteran), the WorldBuilder
//! generates increasingly detailed world content: locations, NPCs, lore,
//! faction developments, and political intrigue.
//!
//! Ported from sq-2/sidequest/game/world_builder.py (~500 LOC builder pattern).
//! The Rust version delegates materialization mechanics to
//! sidequest-game::world_materialization and uses the LLM for creative content
//! generation appropriate to the current maturity level.

use tracing::info_span;

use crate::agent::Agent;
use crate::context_builder::ContextBuilder;
use crate::prompt_framework::{AttentionZone, PromptSection, SectionCategory};
use sidequest_game::faction_agenda::{AgendaUrgency, FactionAgenda};
use sidequest_game::world_materialization::{CampaignMaturity, HistoryChapter};

/// System prompt for the WorldBuilder agent.
const WORLD_BUILDER_SYSTEM_PROMPT: &str = "\
<system>
You are the WORLD_BUILDER agent in SideQuest, a collaborative AI Dungeon Master.

Your role: progressive world materialization. You generate world content that
enriches the narrator's context, scaled to campaign maturity.

MATURITY TIERS — content density scales with campaign age:

FRESH (turns 0-5):
- Basic location descriptions (1-2 sentences, sensory impressions)
- Simple NPCs (name, role, one distinguishing trait)
- No deep lore — the world is new and unknown
- Atmosphere and mood, not history

EARLY (turns 6-20):
- Named NPCs with personalities and motivations
- Location details: what's notable, what's hidden
- Faction names surface — who holds power here?
- Rumors and hints of deeper history

MID (turns 21-50):
- Faction agendas: what each faction wants and how urgently
- Inter-NPC relationships: alliances, rivalries, debts
- Hidden lore fragments that reward exploration
- Consequences of earlier player actions ripple through the world

VETERAN (turns 51+):
- Deep history: why things are the way they are
- Political intrigue: faction betrayals, shifting alliances
- World-shaking events: prophecies, invasions, cataclysms
- NPCs reference shared history with the player

RULES:
- Output a JSON object with the fields appropriate to the maturity tier.
- Always include a \"maturity\" field echoing the current tier.
- Include \"locations\", \"npcs\", \"lore\", and \"faction_developments\" arrays as appropriate.
- Each entry should be a compact object with enough detail for the narrator to weave in.
- Do NOT generate content above the current maturity tier.
- Build on existing world state — extend, don't contradict.

OUTPUT FORMAT:
```json
{
  \"maturity\": \"early\",
  \"locations\": [{\"name\": \"...\", \"description\": \"...\"}],
  \"npcs\": [{\"name\": \"...\", \"role\": \"...\", \"trait\": \"...\"}],
  \"lore\": [\"...\"],
  \"faction_developments\": [{\"faction\": \"...\", \"development\": \"...\"}]
}
```

Omit empty arrays. Keep entries concise — the narrator will expand them.
</system>";

/// The WorldBuilder agent — progressive world materialization.
///
/// Reads campaign maturity from the game state and generates appropriately
/// detailed world content for narrator context injection.
pub struct WorldBuilderAgent {
    system_prompt: String,
    /// Current campaign maturity level, set before invocation.
    maturity: CampaignMaturity,
    /// Existing locations for context (avoid contradiction).
    known_locations: Vec<String>,
    /// Existing NPC names for context (avoid duplication).
    known_npcs: Vec<String>,
    /// Established lore fragments for context.
    known_lore: Vec<String>,
    /// Active faction agendas for context injection.
    faction_summaries: Vec<String>,
    /// History chapters from the genre pack for progressive materialization.
    history_chapters: Vec<HistoryChapter>,
}

impl WorldBuilderAgent {
    /// Create a new WorldBuilder agent with default (Fresh) maturity.
    pub fn new() -> Self {
        Self {
            system_prompt: WORLD_BUILDER_SYSTEM_PROMPT.to_string(),
            maturity: CampaignMaturity::default(),
            known_locations: Vec::new(),
            known_npcs: Vec::new(),
            known_lore: Vec::new(),
            faction_summaries: Vec::new(),
            history_chapters: Vec::new(),
        }
    }

    /// Set the campaign maturity level for context generation.
    pub fn with_maturity(mut self, maturity: CampaignMaturity) -> Self {
        self.maturity = maturity;
        self
    }

    /// Set known locations to avoid contradicting existing world state.
    pub fn with_locations(mut self, locations: Vec<String>) -> Self {
        self.known_locations = locations;
        self
    }

    /// Set known NPC names to avoid duplication.
    pub fn with_npcs(mut self, npcs: Vec<String>) -> Self {
        self.known_npcs = npcs;
        self
    }

    /// Set established lore fragments for continuity.
    pub fn with_lore(mut self, lore: Vec<String>) -> Self {
        self.known_lore = lore;
        self
    }

    /// Set active faction agendas for context injection.
    pub fn with_factions(mut self, factions: &[FactionAgenda]) -> Self {
        self.faction_summaries = factions
            .iter()
            .filter(|f| f.urgency() != AgendaUrgency::Dormant)
            .map(|f| format!("{} ({:?}): {}", f.faction_name(), f.urgency(), f.goal()))
            .collect();
        self
    }

    /// Provide history chapters for progressive world materialization.
    ///
    /// Chapters are filtered by maturity during `build_context()` — only
    /// chapters at or below the current maturity level are included.
    pub fn with_chapters(mut self, chapters: Vec<HistoryChapter>) -> Self {
        self.history_chapters = chapters;
        self
    }

    /// Current maturity level.
    pub fn maturity(&self) -> &CampaignMaturity {
        &self.maturity
    }

    /// Inject only narrator-facing world context (maturity + materialized content)
    /// into the prompt builder. Does NOT include the world builder's own system
    /// prompt / identity — that would be agent cross-contamination.
    ///
    /// Use this instead of `build_context()` when composing the narrator's prompt.
    pub fn inject_world_context(&self, builder: &mut ContextBuilder) {
        // World state section (Early zone) — maturity + existing world knowledge
        builder.add_section(PromptSection::new(
            "world_maturity",
            self.maturity_context(),
            AttentionZone::Early,
            SectionCategory::State,
        ));

        // Materialized world description (Early zone) — progressive content
        // from history chapters filtered by current maturity level.
        if let Some(materialized) = self.materialized_world_context() {
            builder.add_section(PromptSection::new(
                "world_materialization",
                materialized,
                AttentionZone::Early,
                SectionCategory::State,
            ));
        }
    }

    /// Format the maturity tier as a lowercase string for prompt injection.
    fn maturity_label(&self) -> &str {
        match &self.maturity {
            CampaignMaturity::Fresh => "fresh",
            CampaignMaturity::Early => "early",
            CampaignMaturity::Mid => "mid",
            CampaignMaturity::Veteran => "veteran",
            _ => "fresh", // non_exhaustive fallback
        }
    }

    /// Build the maturity-aware context section for prompt injection.
    fn maturity_context(&self) -> String {
        let mut parts = vec![format!(
            "<world_maturity tier=\"{}\">",
            self.maturity_label()
        )];

        parts.push(format!(
            "Campaign maturity: {}",
            self.maturity_label().to_uppercase()
        ));

        if !self.known_locations.is_empty() {
            parts.push(format!(
                "Known locations: {}",
                self.known_locations.join(", ")
            ));
        }

        if !self.known_npcs.is_empty() {
            parts.push(format!("Known NPCs: {}", self.known_npcs.join(", ")));
        }

        if !self.known_lore.is_empty() {
            parts.push("Established lore:".to_string());
            for lore in &self.known_lore {
                parts.push(format!("- {}", lore));
            }
        }

        if !self.faction_summaries.is_empty() {
            parts.push("Active faction agendas:".to_string());
            for faction in &self.faction_summaries {
                parts.push(format!("- {}", faction));
            }
        }

        parts.push("</world_maturity>".to_string());
        parts.join("\n")
    }

    /// Filter history chapters by current maturity and format as materialized
    /// world description for prompt injection. Returns None if no applicable
    /// chapters exist.
    ///
    /// Emits OTEL span: `world.materialized` with maturity_level, chapter_count,
    /// and description_tokens fields.
    fn materialized_world_context(&self) -> Option<String> {
        // Filter chapters: include all at or below current maturity
        let applicable: Vec<&HistoryChapter> = self
            .history_chapters
            .iter()
            .filter(|ch| {
                // Reuse CampaignMaturity ordering — chapter id maps to a maturity level,
                // and we include it if that level <= our current maturity.
                match ch.id.as_str() {
                    "fresh" => CampaignMaturity::Fresh <= self.maturity,
                    "early" => CampaignMaturity::Early <= self.maturity,
                    "mid" => CampaignMaturity::Mid <= self.maturity,
                    "veteran" => CampaignMaturity::Veteran <= self.maturity,
                    _ => false, // Unknown chapter IDs are excluded
                }
            })
            .collect();

        if applicable.is_empty() {
            return None;
        }

        let mut parts = vec!["<world_materialization>".to_string()];

        for chapter in &applicable {
            parts.push(format!("## {} ({})", chapter.label, chapter.id));
            for lore in &chapter.lore {
                parts.push(format!("- {}", lore));
            }
        }

        parts.push("</world_materialization>".to_string());
        let description = parts.join("\n");

        // Approximate token count (~4 chars per token)
        let description_tokens = description.len() / 4;

        let _span = info_span!(
            "world.materialized",
            maturity_level = self.maturity_label(),
            chapter_count = applicable.len(),
            description_tokens = description_tokens,
        )
        .entered();

        Some(description)
    }
}

impl Default for WorldBuilderAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl Agent for WorldBuilderAgent {
    fn name(&self) -> &str {
        "world_builder"
    }

    fn system_prompt(&self) -> &str {
        &self.system_prompt
    }

    fn build_context(&self, builder: &mut ContextBuilder) {
        // Identity section (Primacy zone) — who this agent is
        builder.add_section(PromptSection::new(
            "world_builder_identity",
            self.system_prompt(),
            AttentionZone::Primacy,
            SectionCategory::Identity,
        ));

        // World state section (Early zone) — maturity + existing world knowledge
        builder.add_section(PromptSection::new(
            "world_maturity",
            self.maturity_context(),
            AttentionZone::Early,
            SectionCategory::State,
        ));

        // Materialized world description (Situational zone) — progressive content
        // from history chapters filtered by current maturity level.
        if let Some(materialized) = self.materialized_world_context() {
            builder.add_section(PromptSection::new(
                "world_materialization",
                materialized,
                AttentionZone::Early,
                SectionCategory::State,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_world_builder_has_fresh_maturity() {
        let agent = WorldBuilderAgent::new();
        assert_eq!(agent.maturity(), &CampaignMaturity::Fresh);
    }

    #[test]
    fn world_builder_name_is_correct() {
        let agent = WorldBuilderAgent::new();
        assert_eq!(agent.name(), "world_builder");
    }

    #[test]
    fn world_builder_system_prompt_is_non_empty() {
        let agent = WorldBuilderAgent::new();
        assert!(!agent.system_prompt().is_empty());
        assert!(agent.system_prompt().contains("<system>"));
    }

    #[test]
    fn world_builder_system_prompt_describes_maturity_tiers() {
        let agent = WorldBuilderAgent::new();
        let prompt = agent.system_prompt();
        assert!(prompt.contains("FRESH"));
        assert!(prompt.contains("EARLY"));
        assert!(prompt.contains("MID"));
        assert!(prompt.contains("VETERAN"));
    }

    #[test]
    fn with_maturity_sets_level() {
        let agent = WorldBuilderAgent::new().with_maturity(CampaignMaturity::Veteran);
        assert_eq!(agent.maturity(), &CampaignMaturity::Veteran);
    }

    #[test]
    fn maturity_label_maps_correctly() {
        let cases = vec![
            (CampaignMaturity::Fresh, "fresh"),
            (CampaignMaturity::Early, "early"),
            (CampaignMaturity::Mid, "mid"),
            (CampaignMaturity::Veteran, "veteran"),
        ];
        for (maturity, expected) in cases {
            let agent = WorldBuilderAgent::new().with_maturity(maturity);
            assert_eq!(agent.maturity_label(), expected);
        }
    }

    #[test]
    fn maturity_context_includes_tier() {
        let agent = WorldBuilderAgent::new().with_maturity(CampaignMaturity::Mid);
        let ctx = agent.maturity_context();
        assert!(ctx.contains("tier=\"mid\""));
        assert!(ctx.contains("Campaign maturity: MID"));
    }

    #[test]
    fn maturity_context_includes_known_locations() {
        let agent = WorldBuilderAgent::new()
            .with_locations(vec!["The Rusted Tavern".into(), "Collapsed Bridge".into()]);
        let ctx = agent.maturity_context();
        assert!(ctx.contains("The Rusted Tavern"));
        assert!(ctx.contains("Collapsed Bridge"));
    }

    #[test]
    fn maturity_context_includes_known_npcs() {
        let agent =
            WorldBuilderAgent::new().with_npcs(vec!["Gorm the Smith".into(), "Reva".into()]);
        let ctx = agent.maturity_context();
        assert!(ctx.contains("Gorm the Smith"));
        assert!(ctx.contains("Reva"));
    }

    #[test]
    fn maturity_context_includes_lore() {
        let agent =
            WorldBuilderAgent::new().with_lore(vec!["The old king fell to corruption".into()]);
        let ctx = agent.maturity_context();
        assert!(ctx.contains("The old king fell to corruption"));
    }

    #[test]
    fn with_factions_filters_dormant() {
        let active = FactionAgenda::try_new(
            "Iron Collective".into(),
            "Control the water supply".into(),
            AgendaUrgency::Pressing,
            "The Iron Collective tightens its grip".into(),
        )
        .unwrap();
        let dormant = FactionAgenda::try_new(
            "Silent Order".into(),
            "Observe from shadows".into(),
            AgendaUrgency::Dormant,
            "The Silent Order watches".into(),
        )
        .unwrap();

        let agent = WorldBuilderAgent::new().with_factions(&[active, dormant]);
        assert_eq!(agent.faction_summaries.len(), 1);
        assert!(agent.faction_summaries[0].contains("Iron Collective"));
    }

    #[test]
    fn build_context_adds_identity_and_state_sections() {
        let agent = WorldBuilderAgent::new()
            .with_maturity(CampaignMaturity::Early)
            .with_locations(vec!["Market Square".into()]);
        let mut builder = ContextBuilder::new();
        agent.build_context(&mut builder);

        let identity = builder.sections_by_category(SectionCategory::Identity);
        assert_eq!(identity.len(), 1);
        assert!(identity[0].content.contains("WORLD_BUILDER"));

        let state = builder.sections_by_category(SectionCategory::State);
        assert_eq!(state.len(), 1);
        assert!(state[0].content.contains("Market Square"));
        assert!(state[0].content.contains("EARLY"));
    }

    #[test]
    fn inject_world_context_excludes_identity() {
        let agent = WorldBuilderAgent::new()
            .with_maturity(CampaignMaturity::Early)
            .with_locations(vec!["Market Square".into()]);
        let mut builder = ContextBuilder::new();
        agent.inject_world_context(&mut builder);

        // No identity section — world builder system prompt must not leak
        let identity = builder.sections_by_category(SectionCategory::Identity);
        assert_eq!(
            identity.len(),
            0,
            "inject_world_context must not include identity section"
        );

        // State section should still have maturity + locations
        let state = builder.sections_by_category(SectionCategory::State);
        assert_eq!(state.len(), 1);
        assert!(state[0].content.contains("Market Square"));
        assert!(state[0].content.contains("EARLY"));
        // Must not contain WORLD_BUILDER agent instructions
        assert!(!state[0].content.contains("WORLD_BUILDER agent"));
        assert!(!state[0].content.contains("OUTPUT FORMAT"));
        assert!(!state[0].content.contains("```json"));
    }

    #[test]
    fn default_builds_same_as_new() {
        let d = WorldBuilderAgent::default();
        let n = WorldBuilderAgent::new();
        assert_eq!(d.name(), n.name());
        assert_eq!(d.maturity(), n.maturity());
    }
}

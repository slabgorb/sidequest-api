//! Prompt framework — attention-zone prompt composition for Claude CLI agents.
//!
//! Ports the Python `prompt_composer.py` attention-zone system (ADR-009) to Rust.
//! Provides [`PromptSection`], [`AttentionZone`], [`RuleTier`], [`SoulData`],
//! and the [`PromptComposer`] trait for assembling structured prompts.

pub mod soul;
mod types;

#[cfg(test)]
mod tests;

pub use soul::{parse_soul_md, SoulData, SoulPrinciple};
pub use types::{AttentionZone, PromptSection, RuleTier, SectionCategory};

use sidequest_game::character::Character;
use sidequest_game::known_fact::Confidence;
use sidequest_game::npc::Npc;
use sidequest_game::scene_directive::SceneDirective;
use sidequest_protocol::{NarratorVerbosity, NarratorVocabulary};

/// Render a `SceneDirective` into its prompt text representation.
///
/// Returns `None` if the directive has no mandatory elements (empty suppression).
/// The rendered block uses `[SCENE DIRECTIVES — MANDATORY]` header with MUST-weave
/// language and numbered elements labeled by source (Story 6-2).
pub fn render_scene_directive(directive: &SceneDirective) -> Option<String> {
    if directive.mandatory_elements.is_empty() {
        return None;
    }

    let mut block = String::from(
        "[SCENE DIRECTIVES — MANDATORY]\n\
         You MUST weave at least one of the following into your response.\n\
         These are not suggestions — they are active story elements.\n\n",
    );

    for (i, elem) in directive.mandatory_elements.iter().enumerate() {
        block.push_str(&format!(
            "{}. [{}] {}\n",
            i + 1,
            elem.source.label(),
            elem.content
        ));
    }

    if !directive.narrative_hints.is_empty() {
        block.push_str("\nNarrative hints (weave if natural):\n");
        for hint in &directive.narrative_hints {
            block.push_str(&format!("- {}\n", hint));
        }
    }

    Some(block)
}

/// Concrete implementation of PromptComposer — stores sections per agent, composes in zone order.
pub struct PromptRegistry {
    sections: std::collections::HashMap<String, Vec<PromptSection>>,
}

impl PromptRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            sections: std::collections::HashMap::new(),
        }
    }
}

impl Default for PromptRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Agents that receive pacing guidance in their prompts.
const PACING_AGENTS: &[&str] = &["narrator", "creature_smith"];

/// Agents that receive narrator verbosity instructions.
/// Same set as pacing — only agents that produce narrative prose.
const NARRATING_AGENTS: &[&str] = &["narrator", "creature_smith"];

impl PromptRegistry {
    /// Inject pacing guidance into the prompt for narrating agents.
    /// Non-narrating agents (ensemble, dialectician, etc.) are silently skipped.
    pub fn register_pacing_section(
        &mut self,
        agent_name: &str,
        hint: &sidequest_game::tension_tracker::PacingHint,
    ) {
        if !PACING_AGENTS.contains(&agent_name) {
            return;
        }

        let mut content = format!("## Pacing Guidance\n{}", hint.narrator_directive());
        if let Some(ref beat) = hint.escalation_beat {
            content.push_str(&format!("\n\n## Escalation Beat\n{}", beat));
        }

        self.register_section(
            agent_name,
            PromptSection::new("pacing", content, AttentionZone::Late, SectionCategory::Context),
        );
    }

    /// Inject narrator verbosity instructions into the system prompt.
    ///
    /// Only applies to narrating agents (narrator, creature_smith). Non-narrating
    /// agents are silently skipped. Placed in Late zone, Format category — same
    /// position as footnote protocol, so the LLM sees it with high recency attention.
    ///
    /// Story 14-3: Per-session verbosity control.
    pub fn register_verbosity_section(
        &mut self,
        agent_name: &str,
        verbosity: NarratorVerbosity,
    ) {
        if !NARRATING_AGENTS.contains(&agent_name) {
            return;
        }

        let content = match verbosity {
            NarratorVerbosity::Concise => {
                "<length-limit>\n\
                 HARD LIMIT: 2-3 sentences, under 200 characters of prose. \
                 Action and consequence only. No scene-setting. No paragraphs. \
                 If your prose exceeds 200 characters, DELETE and rewrite shorter.\n\
                 </length-limit>"
            }
            NarratorVerbosity::Standard => {
                "<length-limit>\n\
                 HARD LIMIT: 2-3 short paragraphs, under 400 characters of prose. \
                 One action, one scene beat. If your prose exceeds 400 characters, \
                 DELETE and rewrite shorter. Most turns should be 2-3 sentences.\n\
                 </length-limit>"
            }
            NarratorVerbosity::Verbose => {
                "<length-limit>\n\
                 HARD LIMIT: 2-3 paragraphs, under 600 characters of prose. \
                 Richer atmosphere and sensory detail, but still concise. \
                 If your prose exceeds 600 characters, DELETE and rewrite shorter.\n\
                 </length-limit>"
            }
        };

        self.register_section(
            agent_name,
            PromptSection::new(
                "narrator_verbosity",
                content,
                AttentionZone::Recency,
                SectionCategory::Guardrail,
            ),
        );
    }

    /// Inject narrator vocabulary/complexity instructions into the system prompt.
    ///
    /// Only applies to narrating agents (narrator, creature_smith). Non-narrating
    /// agents are silently skipped. Placed in Late zone, Format category — same
    /// position as verbosity, so the LLM sees both length and complexity guidance
    /// with high recency attention.
    ///
    /// Story 14-4: Per-session vocabulary control.
    pub fn register_vocabulary_section(
        &mut self,
        agent_name: &str,
        vocabulary: NarratorVocabulary,
    ) {
        if !NARRATING_AGENTS.contains(&agent_name) {
            return;
        }

        let content = match vocabulary {
            NarratorVocabulary::Accessible => {
                "[NARRATION VOCABULARY]\n\
                 Use simple, direct language. Prefer common words over obscure \
                 ones. Keep sentences short and clear. Aim for approximately \
                 8th-grade reading level. No archaic constructions or elaborate \
                 metaphors."
            }
            NarratorVocabulary::Literary => {
                "[NARRATION VOCABULARY]\n\
                 Use rich but clear prose. Employ varied vocabulary and literary \
                 devices where they serve the narrative. Balance elegance with \
                 accessibility — vivid but not purple."
            }
            NarratorVocabulary::Epic => {
                "[NARRATION VOCABULARY]\n\
                 Use elevated, archaic, or mythic diction. Embrace elaborate \
                 sentence structures, rare words, and poetic constructions. \
                 Channel the cadence of sagas, epics, and high fantasy prose. \
                 Unrestricted complexity."
            }
        };

        self.register_section(
            agent_name,
            PromptSection::new(
                "narrator_vocabulary",
                content,
                AttentionZone::Late,
                SectionCategory::Format,
            ),
        );
    }

    /// Inject OCEAN personality summaries for NPCs into the narrator prompt.
    ///
    /// Filters to NPCs with an `ocean` profile, builds a labeled personality
    /// block per NPC, and registers it in the Valley zone with Context category.
    /// Does nothing if no NPCs have OCEAN profiles.
    ///
    /// Story 10-4: Parallel to `register_pacing_section()`.
    pub fn register_ocean_personalities_section(&mut self, agent_name: &str, npcs: &[Npc]) {
        let entries: Vec<String> = npcs
            .iter()
            .filter_map(|npc| {
                npc.ocean.as_ref().map(|ocean| {
                    format!("- {}: {}", npc.core.name, ocean.behavioral_summary())
                })
            })
            .collect();

        if entries.is_empty() {
            return;
        }

        let content = format!(
            "## NPC Personalities\n\
             Use these personality descriptions to shape each NPC's dialogue and behavior.\n\n\
             {}",
            entries.join("\n")
        );

        self.register_section(
            agent_name,
            PromptSection::new(
                "ocean_personalities",
                content,
                AttentionZone::Valley,
                SectionCategory::Context,
            ),
        );
    }

    /// Inject involuntary ability context for characters into the narrator prompt.
    ///
    /// Filters to involuntary abilities only, uses genre_description (not mechanical_effect),
    /// and includes natural triggering instructions. Characters with no involuntary abilities
    /// are omitted. Does nothing if no characters have involuntary abilities.
    ///
    /// Story 9-2: Parallel to `register_knowledge_section()`.
    pub fn register_ability_context(&mut self, agent_name: &str, characters: &[Character]) {
        let mut entries: Vec<String> = Vec::new();

        for character in characters {
            let involuntary: Vec<_> = character
                .abilities
                .iter()
                .filter(|a| a.involuntary)
                .collect();

            if involuntary.is_empty() {
                continue;
            }

            let mut char_block = format!("{}:", character.core.name);
            for ability in &involuntary {
                char_block.push_str(&format!("\n  - {}: {}", ability.name, ability.genre_description));
            }
            entries.push(char_block);
        }

        if entries.is_empty() {
            return;
        }

        let content = format!(
            "[CHARACTER ABILITIES]\n\
             Weave these abilities naturally when relevant. Do not force triggers every turn.\n\n\
             {}",
            entries.join("\n\n")
        );

        self.register_section(
            agent_name,
            PromptSection::new(
                "ability_context",
                content,
                AttentionZone::Valley,
                SectionCategory::Context,
            ),
        );
    }

    /// Inject a scene directive into the narrator prompt with narrative primacy.
    ///
    /// The directive is placed in the `Early` attention zone — after agent identity
    /// but before game state — ensuring the narrator sees it with high attention.
    /// Empty directives are suppressed (no section registered).
    ///
    /// Story 6-2: Parallel to `register_pacing_section()`.
    pub fn register_scene_directive(
        &mut self,
        agent_name: &str,
        directive: &SceneDirective,
    ) {
        if let Some(content) = render_scene_directive(directive) {
            self.register_section(
                agent_name,
                PromptSection::new(
                    "scene_directive",
                    content,
                    AttentionZone::Early,
                    SectionCategory::Context,
                ),
            );
        }
    }

    /// Inject a character's known facts into the narrator prompt.
    ///
    /// Builds a `[CHARACTER KNOWLEDGE]` section with facts tagged by confidence
    /// level (certain/suspected/rumored), sorted most-recent-first, capped at 20.
    /// Does nothing if the character has no known facts.
    ///
    /// Story 9-4: Parallel to `register_ocean_personalities_section()`.
    pub fn register_knowledge_section(
        &mut self,
        agent_name: &str,
        character: &Character,
    ) {
        if character.known_facts.is_empty() {
            return;
        }

        // Sort by learned_turn descending (most recent first), cap at 20
        let mut facts: Vec<&sidequest_game::known_fact::KnownFact> =
            character.known_facts.iter().collect();
        facts.sort_by(|a, b| b.learned_turn.cmp(&a.learned_turn));
        let facts: Vec<_> = facts.into_iter().take(20).collect();

        let mut content = format!("[{}'s KNOWLEDGE]\n", character.core.name);
        for fact in &facts {
            let confidence_tag = match fact.confidence {
                Confidence::Certain => "certain",
                Confidence::Suspected => "suspected",
                Confidence::Rumored => "rumored",
            };
            content.push_str(&format!("- {} ({})\n", fact.content, confidence_tag));
        }

        self.register_section(
            agent_name,
            PromptSection::new(
                "character_knowledge",
                content,
                AttentionZone::Valley,
                SectionCategory::Context,
            ),
        );
    }

    /// Inject the footnote protocol instruction into the narrator prompt.
    ///
    /// Tells the narrator to emit `[N]` markers in prose when revealing or
    /// referencing knowledge, with structured footnote entries in the response.
    /// Placed in the Late zone (Format category) — output instructions near
    /// the end of the prompt for high recency attention.
    ///
    /// Story 9-11: Parallel to `register_knowledge_section()`.
    pub fn register_footnote_protocol_section(&mut self, agent_name: &str) {
        let content = "\
[FOOTNOTE PROTOCOL]
When you reveal new information or reference something the party previously learned,
include a numbered marker in your prose like [1], [2], etc.

For each marker, emit a footnote in your structured output with:
- summary: one-sentence description of the fact
- category: one of Lore, Place, Person, Quest, Ability
- is_new: true if this is a new revelation, false if referencing prior knowledge

Example prose: \"As you enter the grove, Reva feels a deep wrongness [1].\"
Example footnote: { \"marker\": 1, \"summary\": \"Corruption detected in the grove\", \"category\": \"Place\", \"is_new\": true }

If you reference something the party already knows, set is_new to false and include the fact_id.
If nothing new is revealed and nothing prior is referenced, omit the footnotes array entirely.";

        self.register_section(
            agent_name,
            PromptSection::new(
                "footnote_protocol",
                content,
                AttentionZone::Late,
                SectionCategory::Format,
            ),
        );
    }

    /// Inject genre resource state into the narrator prompt (story 16-1).
    ///
    /// Serializes current resource values into a human-readable block in the Valley zone.
    /// Empty declarations produce no section (genres without resources are unaffected).
    /// Falls back to `starting` value when resource state is missing.
    pub fn register_resource_section(
        &mut self,
        agent_name: &str,
        declarations: &[sidequest_genre::ResourceDeclaration],
        state: &std::collections::HashMap<String, f64>,
    ) {
        if declarations.is_empty() {
            return;
        }

        let mut lines = vec!["## GENRE RESOURCES — Current State".to_string()];
        for decl in declarations {
            let current = state.get(&decl.name).copied().unwrap_or(decl.starting);
            let vol_label = if decl.voluntary {
                "voluntary"
            } else {
                "involuntary"
            };
            let mut line = format!(
                "{}: {}/{} ({})",
                decl.label, current, decl.max, vol_label
            );
            if decl.decay_per_turn.abs() > f64::EPSILON {
                line.push_str(&format!(", decay {}/turn", decl.decay_per_turn.abs()));
            }
            lines.push(line);
        }

        self.register_section(
            agent_name,
            PromptSection::new(
                "genre_resources",
                lines.join("\n"),
                AttentionZone::Valley,
                SectionCategory::State,
            ),
        );
    }
}

impl PromptComposer for PromptRegistry {
    fn register_section(&mut self, agent_name: &str, section: PromptSection) {
        self.sections
            .entry(agent_name.to_string())
            .or_default()
            .push(section);
    }

    fn registry(&self, agent_name: &str) -> Vec<&PromptSection> {
        self.sections
            .get(agent_name)
            .map(|sections| sections.iter().collect())
            .unwrap_or_default()
    }

    fn get_sections(
        &self,
        agent_name: &str,
        category: Option<SectionCategory>,
        zone: Option<AttentionZone>,
    ) -> Vec<&PromptSection> {
        self.sections
            .get(agent_name)
            .map(|sections| {
                sections
                    .iter()
                    .filter(|s| category.map_or(true, |c| s.category == c))
                    .filter(|s| zone.map_or(true, |z| s.zone == z))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn compose(&self, agent_name: &str) -> String {
        let mut sections: Vec<&PromptSection> = self.registry(agent_name);
        sections.sort_by_key(|s| s.zone.order());
        sections
            .iter()
            .filter(|s| !s.is_empty())
            .map(|s| s.content.as_str())
            .collect::<Vec<&str>>()
            .join("\n\n")
    }

    fn clear(&mut self, agent_name: &str) {
        self.sections.remove(agent_name);
    }
}

/// Trait for assembling prompt sections into a final prompt string.
///
/// Implementors register sections and compose them in attention-optimal zone order.
pub trait PromptComposer {
    /// Register a section for a given agent.
    fn register_section(&mut self, agent_name: &str, section: PromptSection);

    /// Return sections for an agent in insertion order.
    fn registry(&self, agent_name: &str) -> Vec<&PromptSection>;

    /// Return sections filtered by optional category and/or zone.
    fn get_sections(
        &self,
        agent_name: &str,
        category: Option<SectionCategory>,
        zone: Option<AttentionZone>,
    ) -> Vec<&PromptSection>;

    /// Compose all registered sections for an agent into a final prompt string.
    fn compose(&self, agent_name: &str) -> String;

    /// Clear all sections for an agent.
    fn clear(&mut self, agent_name: &str);
}

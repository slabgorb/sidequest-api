//! Prompt framework — attention-zone prompt composition for Claude CLI agents.
//!
//! Ports the Python `prompt_composer.py` attention-zone system (ADR-009) to Rust.
//! Provides [`PromptSection`], [`AttentionZone`], [`RuleTier`], [`SoulData`],
//! and the [`PromptComposer`] trait for assembling structured prompts.

mod soul;
mod types;

#[cfg(test)]
mod tests;

pub use soul::{parse_soul_md, SoulData, SoulPrinciple};
pub use types::{AttentionZone, PromptSection, RuleTier, SectionCategory};

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

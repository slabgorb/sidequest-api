//! Prompt framework — attention-zone prompt composition for Claude CLI agents.
//!
//! Ports the Python `prompt_composer.py` attention-zone system (ADR-009) to Rust.
//! Provides [`PromptSection`], [`AttentionZone`], [`RuleTier`], [`SoulData`],
//! and the [`PromptComposer`] trait for assembling structured prompts.

mod soul;
mod types;

#[cfg(test)]
mod tests;

pub use soul::{parse_soul_md, SoulPrinciple, SoulData};
pub use types::{AttentionZone, PromptSection, RuleTier, SectionCategory};

/// Trait for assembling prompt sections into a final prompt string.
///
/// Implementors register sections and compose them in attention-optimal zone order.
pub trait PromptComposer {
    /// Register a section for a given agent.
    fn register_section(&mut self, agent_name: &str, section: PromptSection);

    /// Return the ordered section list for an agent.
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

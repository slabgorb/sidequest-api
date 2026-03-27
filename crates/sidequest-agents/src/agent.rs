//! Agent trait and response types.
//!
//! Port lesson #7: Define a proper Agent trait so all agents share
//! a consistent interface.

use crate::context_builder::ContextBuilder;
use crate::prompt_framework::{AttentionZone, PromptSection, SectionCategory};

/// Response from an agent execution.
#[derive(Debug, Clone)]
pub struct AgentResponse {
    /// Parsed/cleaned text output.
    pub text: String,
    /// Raw output from Claude CLI.
    pub raw_output: String,
}

/// Trait defining the agent interface.
///
/// All game agents (Narrator, Combat, NPC, etc.) implement this trait
/// to provide a consistent interface for the orchestrator.
pub trait Agent {
    /// Agent's display name.
    fn name(&self) -> &str;

    /// The system prompt for this agent.
    fn system_prompt(&self) -> &str;

    /// Add this agent's sections to a ContextBuilder.
    ///
    /// Default implementation wraps system_prompt() as a Primacy/Identity section.
    /// Agents can override for more granular section composition.
    fn build_context(&self, builder: &mut ContextBuilder) {
        builder.add_section(PromptSection::new(
            format!("{}_identity", self.name()),
            self.system_prompt(),
            AttentionZone::Primacy,
            SectionCategory::Identity,
        ));
    }
}

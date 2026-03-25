//! Narrator agent — handles exploration, description, and story progression.
//!
//! Ported from sq-2/sidequest/agents/narrator.py.

use crate::agent::Agent;
use crate::context_builder::ContextBuilder;
use crate::prompt_framework::{AttentionZone, PromptSection, SectionCategory};

/// System prompt for the Narrator agent.
const NARRATOR_SYSTEM_PROMPT: &str = "\
<system>
You are the NARRATOR agent in a collaborative AI Dungeon Master system called SideQuest.

Your role:
- Describe environments, scenery, and atmosphere
- Handle exploration, movement, and investigation actions
- Advance the story based on player choices
- Describe consequences of non-combat, non-dialogue actions

Agency Rules:
- NEVER control the player character — the player controls their character's actions, thoughts, and feelings.
- Do not speak for the player or assume player actions.
- Present options and consequences, never force outcomes.

Respond ONLY with the narrative description. No meta-commentary, no dice rolls.
</system>";

/// The Narrator agent — exploration, description, story progression.
pub struct NarratorAgent {
    system_prompt: String,
}

impl NarratorAgent {
    /// Create a new Narrator agent.
    pub fn new() -> Self {
        Self {
            system_prompt: NARRATOR_SYSTEM_PROMPT.to_string(),
        }
    }

    /// Build context for a narrator prompt using the ContextBuilder.
    pub fn build_context(&self, builder: &mut ContextBuilder) {
        builder.add_section(PromptSection::new(
            "narrator_identity",
            SectionCategory::Identity,
            AttentionZone::Primacy,
            &self.system_prompt,
        ));
    }
}

impl Default for NarratorAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl Agent for NarratorAgent {
    fn name(&self) -> &str {
        "narrator"
    }

    fn system_prompt(&self) -> &str {
        &self.system_prompt
    }
}

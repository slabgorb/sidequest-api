//! Narrator agent — handles exploration, description, and story progression.
//!
//! Ported from sq-2/sidequest/agents/narrator.py.

use crate::agent::Agent;
use crate::context_builder::ContextBuilder;
use crate::prompt_framework::{AttentionZone, PromptSection, SectionCategory};

/// System prompt for the Narrator agent.
const NARRATOR_SYSTEM_PROMPT: &str = "\
<system>
You are the NARRATOR in SideQuest, a collaborative AI Dungeon Master.

Your role: describe environments, advance the story, show consequences.

PACING — THIS IS CRITICAL:
- Most turns: 2-3 sentences. Movement, dialogue, simple actions = SHORT.
- Big moments only (arrivals, reveals, combat start): up to 5-6 sentences.
- VARY your length. Not every turn is the same size.
- Fast action = short sentences. Quiet moments can breathe.
- Dialogue is snappy, not embedded in description paragraphs.
- End on a hook the player can react to. Not a prose flourish.
- Think tweet-length beats, not novel paragraphs.

Format:
- First line: location header like **The Collapsed Overpass**
- Blank line, then prose.

Agency:
- NEVER control the player character's actions, thoughts, or feelings.
- Present situations. Let the player decide.

Output ONLY narrative prose. No meta-commentary, no dice rolls, no OOC.
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
            &self.system_prompt,
            AttentionZone::Primacy,
            SectionCategory::Identity,
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

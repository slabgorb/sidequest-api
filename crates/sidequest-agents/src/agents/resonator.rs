//! Resonator agent — character perception, hook refinement, ability filtering.
//!
//! Ported from sq-2/sidequest/agents/hook_refiner.py + perception_rewriter.py.

use crate::agent::Agent;

/// System prompt for the Resonator agent.
const RESONATOR_SYSTEM_PROMPT: &str = "\
<system>
You are the RESONATOR agent in SideQuest.

Your role:
- Refine character hooks and backstory elements
- Rewrite narration through the lens of each character's perception
- Filter abilities and skills based on character context
- Maintain character-specific narrative consistency

Enhance the narrative to reflect each character's unique perspective.
</system>";

/// The Resonator agent — character perception, ability filtering.
pub struct ResonatorAgent {
    system_prompt: String,
}

impl ResonatorAgent {
    /// Create a new Resonator agent.
    pub fn new() -> Self {
        Self {
            system_prompt: RESONATOR_SYSTEM_PROMPT.to_string(),
        }
    }
}

impl Default for ResonatorAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl Agent for ResonatorAgent {
    fn name(&self) -> &str {
        "resonator"
    }

    fn system_prompt(&self) -> &str {
        &self.system_prompt
    }
}

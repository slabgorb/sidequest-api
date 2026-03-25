//! WorldBuilder agent — maintains game truth and continuity via state patches.
//!
//! Ported from sq-2/sidequest/agents/world_state.py.

use crate::agent::Agent;

/// System prompt for the WorldBuilder agent.
const WORLD_BUILDER_SYSTEM_PROMPT: &str = "\
<system>
You are the WORLD_BUILDER agent in SideQuest.

Your role:
- Maintain the authoritative game world state
- Produce structured JSON patches for state mutations
- Track location changes, NPC attitudes, quest progress
- Ensure world consistency across turns

Output a JSON patch with only the fields that changed.
</system>";

/// The WorldBuilder agent — game truth, state patches, continuity.
pub struct WorldBuilderAgent {
    system_prompt: String,
}

impl WorldBuilderAgent {
    /// Create a new WorldBuilder agent.
    pub fn new() -> Self {
        Self {
            system_prompt: WORLD_BUILDER_SYSTEM_PROMPT.to_string(),
        }
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
}

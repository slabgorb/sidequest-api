//! Ensemble agent — NPC dialogue and social encounters.
//!
//! Ported from sq-2/sidequest/agents/npc.py.

use crate::agent::Agent;

/// System prompt for the Ensemble agent.
const ENSEMBLE_SYSTEM_PROMPT: &str = "\
<system>
You are the ENSEMBLE agent in SideQuest.

Your role:
- Voice NPCs in dialogue and social encounters
- Reflect each NPC's personality, disposition, and knowledge
- Track conversation context and relationship dynamics
- Generate dialogue that advances story and reveals character

Each NPC has unique voice, knowledge, and motivations.
</system>";

/// The Ensemble agent — NPC dialogue, social encounters.
pub struct EnsembleAgent {
    system_prompt: String,
}

impl EnsembleAgent {
    /// Create a new Ensemble agent.
    pub fn new() -> Self {
        Self {
            system_prompt: ENSEMBLE_SYSTEM_PROMPT.to_string(),
        }
    }
}

impl Default for EnsembleAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl Agent for EnsembleAgent {
    fn name(&self) -> &str {
        "ensemble"
    }

    fn system_prompt(&self) -> &str {
        &self.system_prompt
    }
}

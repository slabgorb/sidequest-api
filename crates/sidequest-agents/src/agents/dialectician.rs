//! Dialectician agent — chase sequences, pursuit, decision points.
//!
//! Ported from sq-2/sidequest/agents/chase.py. ADR-017.

use crate::agent::Agent;

/// System prompt for the Dialectician agent.
const DIALECTICIAN_SYSTEM_PROMPT: &str = "\
<system>
You are the DIALECTICIAN agent in SideQuest.

Your role:
- Narrate chase sequences with cinematic tension
- Track separation distance between pursuer and quarry
- Present decision points with meaningful choices
- Manage the five-phase chase arc

Output a JSON chase patch with state changes.
</system>";

/// The Dialectician agent — chase sequences, pursuit, decision points.
pub struct DialecticianAgent {
    system_prompt: String,
}

impl DialecticianAgent {
    /// Create a new Dialectician agent.
    pub fn new() -> Self {
        Self {
            system_prompt: DIALECTICIAN_SYSTEM_PROMPT.to_string(),
        }
    }
}

impl Default for DialecticianAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl Agent for DialecticianAgent {
    fn name(&self) -> &str {
        "dialectician"
    }

    fn system_prompt(&self) -> &str {
        &self.system_prompt
    }
}

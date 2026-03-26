//! CreatureSmith agent — combat resolution, dice, tactical encounters.
//!
//! Ported from sq-2/sidequest/agents/combat.py.

use crate::agent::Agent;

/// System prompt for the CreatureSmith agent.
const CREATURE_SMITH_SYSTEM_PROMPT: &str = "\
<system>
You are the CREATURE_SMITH agent in SideQuest.

Your role:
- Resolve attacks, spells, and combat actions
- Track initiative order and turn economy
- Simulate dice rolls and apply rules fairly
- Wrap every mechanical outcome in narrative description
- Manage enemy HP, conditions, and tactics

Output a JSON combat patch with state changes.
</system>";

/// The CreatureSmith agent — combat resolution, tactical encounters.
pub struct CreatureSmithAgent {
    system_prompt: String,
}

impl CreatureSmithAgent {
    /// Create a new CreatureSmith agent.
    pub fn new() -> Self {
        Self {
            system_prompt: CREATURE_SMITH_SYSTEM_PROMPT.to_string(),
        }
    }
}

impl Default for CreatureSmithAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl Agent for CreatureSmithAgent {
    fn name(&self) -> &str {
        "creature_smith"
    }

    fn system_prompt(&self) -> &str {
        &self.system_prompt
    }
}

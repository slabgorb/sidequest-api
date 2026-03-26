//! Dialectician agent — chase sequences, pursuit, decision points.
//!
//! Ported from sq-2/sidequest/agents/chase.py. ADR-017.

use crate::agent::Agent;

/// System prompt for the Dialectician agent.
const DIALECTICIAN_SYSTEM_PROMPT: &str = "\
<system>
You are the CHASE NARRATOR in SideQuest, a collaborative AI Dungeon Master.

Your role: narrate chase sequences — pursuit, escape, obstacles, split-second decisions.

PACING — THIS IS CRITICAL:
- 2-3 sentences. FAST. Breathless. Urgent.
- Short sentences for sprinting. Fragments are fine.
- \"Left. The alley narrows. Something crashes behind you.\"
- Each beat is a decision point — fork in the road, obstacle, closing gap.
- End on the choice: \"The fence or the fire escape?\"

CHASE RULES:
- Tension builds through environment, not description.
- Obstacles are physical: fences, crowds, rubble, locked doors.
- The pursuer is always close. Make the player feel it.
- Every turn the gap changes — closing or opening.

Format:
- First line: location header like **The Collapsed Overpass**
- Blank line, then chase narration.

Agency:
- NEVER decide the player's escape route or action.
- Describe the situation and threat. Let the player choose.

Output ONLY narrative prose. No JSON. No meta-commentary.
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

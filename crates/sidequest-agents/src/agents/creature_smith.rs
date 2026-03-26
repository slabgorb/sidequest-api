//! CreatureSmith agent — combat resolution, dice, tactical encounters.
//!
//! Ported from sq-2/sidequest/agents/combat.py.

use crate::agent::Agent;

/// System prompt for the CreatureSmith agent.
const CREATURE_SMITH_SYSTEM_PROMPT: &str = "\
<system>
You are the COMBAT NARRATOR in SideQuest, a collaborative AI Dungeon Master.

Your role: narrate combat — attacks, spells, abilities, wounds, and tactical moments.

PACING — THIS IS CRITICAL:
- 2-4 sentences per combat beat. Fast, kinetic, visceral.
- Describe the action, the impact, the consequence. No preamble.
- Vary intensity: a punch is one sentence, a critical hit is three.
- Sound, motion, pain. Not poetry.
- End on what's happening NOW — the next threat, the opening, the choice.

COMBAT RULES:
- Describe what happens mechanically through narration, not stats.
- \"The blade catches your shoulder — you feel the sting\" not \"You take 4 damage\".
- Show enemy reactions — they dodge, stagger, snarl, flee.
- Make the player feel the weight of their choices.

Format:
- First line: location header like **The Collapsed Overpass**
- Blank line, then combat narration.

Agency:
- NEVER control the player character's actions, thoughts, or feelings.
- Describe what enemies do. Let the player decide their response.

Output ONLY narrative prose. No JSON. No dice rolls. No meta-commentary.
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

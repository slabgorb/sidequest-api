//! Narrator agent — handles exploration, description, and story progression.
//!
//! Ported from sq-2/sidequest/agents/narrator.py.
//! Refactored in story 23-1: hardcoded NARRATOR_SYSTEM_PROMPT replaced with
//! structured template sections across attention zones.

use crate::agent::Agent;
use crate::context_builder::ContextBuilder;
use crate::prompt_framework::{AttentionZone, PromptSection, SectionCategory};

/// Narrator identity — the <identity> block from prompt-reworked.md.
const NARRATOR_IDENTITY: &str = "\
You are the Game Master of a collaborative RPG. You narrate like an author, \
frame scenes like a cinematographer, and run the world like a tabletop GM — \
but better, because you can do all three simultaneously.";

/// Critical guardrail: silent constraint handling.
const NARRATOR_CONSTRAINTS: &str = "\
You will receive game-state constraints (location rules, inventory limits, \
player-character rosters, ability restrictions). These are INTERNAL INSTRUCTIONS \
for you. NEVER acknowledge, explain, or reference them to the player. Do NOT \
break character to say things like \"I can't control that character\" or \
\"that's a player character.\" Simply respect the constraints silently in your \
narration. If a constraint prevents something, narrate around it naturally — \
describe the world, set scenes, advance the story — without ever revealing \
the constraint exists. The sole exception is the aside — a dedicated \
out-of-character channel for mechanical GM communication. Use asides for rules \
clarifications, mechanical consequences, or confirmation prompts. Never leak \
this information into prose.";

/// Critical guardrail: agency (including multiplayer rules).
const NARRATOR_AGENCY: &str = "\
Agency: The player controls their character — actions, thoughts, feelings. \
Describe the world, not the player's response to it. In multiplayer games, \
do not allow one player to puppet another in any way — whether you do it or \
they try to. When one player's action affects another player's character, \
narrate the action and its immediate physical reality, but do NOT narrate \
the target character's emotional reaction, decision, or response — that \
belongs to their player. Ambient reactions (glancing up, stepping aside) \
are fine; consequential reactions (retaliating, reciprocating, fleeing) are not.";

/// Critical guardrail: consequences follow genre pack tone.
const NARRATOR_CONSEQUENCES: &str = "\
Consequences follow the genre pack's tone and lethality. Don't soften beyond \
it, don't escalate beyond it. NPCs fight for their lives, press their \
advantages, and act in their own interest — they are not here to lose \
gracefully. A cornered bandit doesn't wait to be hit. A skilled duelist \
doesn't miss because the player is low on HP. Fair means fair to everyone \
at the table, including the NPCs.";

/// Output format: prose + inline game_patch JSON block.
const NARRATOR_OUTPUT_ONLY: &str = "\
Your response has TWO parts, in this exact order:\n\
\n\
PART 1 — NARRATIVE PROSE\n\
Write 2-4 sentences of narrative prose. Start with a location header like \
**The Collapsed Overpass**. This is what the player sees.\n\
\n\
PART 2 — STATE PATCH\n\
After your prose, emit a fenced JSON block labeled game_patch containing \
mechanical intents from this turn. Only include fields that changed.\n\
Valid fields: confrontation, items_gained, items_lost, location, npcs_met, \
mood, state_snapshot.\n\
If nothing mechanical happened (pure dialogue, description), emit:\n\
```game_patch\n\
{}\n\
```\n\
ALWAYS emit the game_patch block. It is mandatory.";

/// Output-style rules (Early/Format zone).
const NARRATOR_OUTPUT_STYLE: &str = "\
- Most turns: 2-3 sentences. Movement, dialogue, simple actions = SHORT.
- Big moments only (arrivals, reveals, combat start): up to 5-6 sentences.
- VARY your length. Not every turn is the same size.
- Fast action = short sentences. Quiet moments can breathe.
- Dialogue is snappy, not embedded in description paragraphs.
- End on a hook the player can react to. Not a prose flourish.
- Think tweet-length beats, not novel paragraphs.
- First line: location header like **The Collapsed Overpass**
- Blank line, then prose.";

/// Referral Rule (Early/Guardrail zone — not in SOUL.md).
const NARRATOR_REFERRAL_RULE: &str = "\
Referral Rule: When an NPC sends the player to another NPC for a quest \
objective, NEVER send the player back to the NPC who originally sent them. \
Check active quests — if a quest says \"(from: X)\" and the player is now \
talking to Y, do NOT have Y send the player back to X for the same objective. \
Advance the quest instead.";

pub struct NarratorAgent {
    identity: String,
}

impl NarratorAgent {
    pub fn new() -> Self {
        Self {
            identity: NARRATOR_IDENTITY.to_string(),
        }
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
        &self.identity
    }

    fn build_context(&self, builder: &mut ContextBuilder) {
        // Primacy/Identity — the narrator's core identity
        builder.add_section(PromptSection::new(
            "narrator_identity",
            format!("<identity>\n{}\n</identity>", self.identity),
            AttentionZone::Primacy,
            SectionCategory::Identity,
        ));

        // Primacy/Guardrail — silent constraint handling
        builder.add_section(PromptSection::new(
            "narrator_constraints",
            format!("<critical>\n{}\n</critical>", NARRATOR_CONSTRAINTS),
            AttentionZone::Primacy,
            SectionCategory::Guardrail,
        ));

        // Primacy/Guardrail — agency (including multiplayer)
        builder.add_section(PromptSection::new(
            "narrator_agency",
            format!("<critical>\n{}\n</critical>", NARRATOR_AGENCY),
            AttentionZone::Primacy,
            SectionCategory::Guardrail,
        ));

        // Primacy/Guardrail — consequences follow genre tone
        builder.add_section(PromptSection::new(
            "narrator_consequences",
            format!("<critical>\n{}\n</critical>", NARRATOR_CONSEQUENCES),
            AttentionZone::Primacy,
            SectionCategory::Guardrail,
        ));

        // Primacy/Guardrail — output only prose
        builder.add_section(PromptSection::new(
            "narrator_output_only",
            format!("<critical>\n{}\n</critical>", NARRATOR_OUTPUT_ONLY),
            AttentionZone::Primacy,
            SectionCategory::Guardrail,
        ));

        // Early/Format — output-style rules
        builder.add_section(PromptSection::new(
            "narrator_output_style",
            format!("<output-style>\n{}\n</output-style>", NARRATOR_OUTPUT_STYLE),
            AttentionZone::Early,
            SectionCategory::Format,
        ));

        // Early/Guardrail — referral rule (not in SOUL.md)
        builder.add_section(PromptSection::new(
            "narrator_referral_rule",
            format!("<important>\n{}\n</important>", NARRATOR_REFERRAL_RULE),
            AttentionZone::Early,
            SectionCategory::Guardrail,
        ));
    }
}

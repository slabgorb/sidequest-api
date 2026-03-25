//! Troper agent — trope progression, beat injection.
//!
//! Manages the trope engine lifecycle (ADR-018).

use crate::agent::Agent;

/// System prompt for the Troper agent.
const TROPER_SYSTEM_PROMPT: &str = "\
<system>
You are the TROPER agent in SideQuest.

Your role:
- Monitor active tropes and their progression
- Inject escalation beats at narrative thresholds
- Evaluate semantic triggers for trope activation
- Track trope lifecycle: DORMANT → ACTIVE → RESOLVED

Suggest trope-driven narrative hooks without forcing outcomes.
</system>";

/// The Troper agent — trope progression, beat injection.
pub struct TroperAgent {
    system_prompt: String,
}

impl TroperAgent {
    /// Create a new Troper agent.
    pub fn new() -> Self {
        Self {
            system_prompt: TROPER_SYSTEM_PROMPT.to_string(),
        }
    }
}

impl Default for TroperAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl Agent for TroperAgent {
    fn name(&self) -> &str {
        "troper"
    }

    fn system_prompt(&self) -> &str {
        &self.system_prompt
    }
}

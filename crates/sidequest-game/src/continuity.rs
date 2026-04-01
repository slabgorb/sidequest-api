//! Continuity Validator — post-narration state consistency checks.
//!
//! Runs AFTER narration is received. Detects contradictions between what the
//! narrator said and what the game state actually is. Corrections are injected
//! into the NEXT turn's narrator prompt so the LLM self-corrects.

use serde::{Deserialize, Serialize};

/// Category of state contradiction detected in narrator text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContradictionCategory {
    /// Narrator described a location that doesn't match canonical state.
    Location,
    /// Narrator mentioned an NPC who is dead (hp == 0).
    DeadNpc,
    /// Narrator described the player using an item they don't have.
    Inventory,
    /// Narrator used time-of-day language inconsistent with current period.
    TimeOfDay,
}

/// A single contradiction between narrator text and game state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contradiction {
    /// What kind of contradiction.
    pub category: ContradictionCategory,
    /// Human-readable description of the contradiction.
    pub detail: String,
    /// What the game state says the correct value is.
    pub expected: String,
}

/// Result of running all continuity checks against a narration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ValidationResult {
    /// All contradictions found.
    pub contradictions: Vec<Contradiction>,
}

impl ValidationResult {
    /// Returns true if no contradictions were found.
    pub fn is_clean(&self) -> bool {
        self.contradictions.is_empty()
    }

    /// Format contradictions as a `[STATE CORRECTIONS]` block for narrator prompt injection.
    pub fn format_corrections(&self) -> String {
        if self.contradictions.is_empty() {
            return String::new();
        }
        let mut lines = vec!["[STATE CORRECTIONS]".to_string()];
        lines.push("The following contradictions were detected in your previous narration. Correct these in your next response:".to_string());
        for c in &self.contradictions {
            lines.push(format!(
                "- {:?}: {} (expected: {})",
                c.category, c.detail, c.expected
            ));
        }
        lines.push("[/STATE CORRECTIONS]".to_string());
        lines.join("\n")
    }
}

// Keyword-based validation functions have been removed. Continuity checking
// is now done via Haiku LLM classification in sidequest-agents::continuity_validator.
// The types above (Contradiction, ContradictionCategory, ValidationResult) are
// still used by the LLM validator.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_corrections_empty_when_clean() {
        let result = ValidationResult::default();
        assert_eq!(result.format_corrections(), "");
    }

    #[test]
    fn format_corrections_produces_block() {
        let result = ValidationResult {
            contradictions: vec![Contradiction {
                category: ContradictionCategory::Location,
                detail: "Wrong location".to_string(),
                expected: "The Tavern".to_string(),
            }],
        };
        let formatted = result.format_corrections();
        assert!(formatted.contains("[STATE CORRECTIONS]"));
        assert!(formatted.contains("[/STATE CORRECTIONS]"));
        assert!(formatted.contains("Wrong location"));
        assert!(formatted.contains("The Tavern"));
    }
}

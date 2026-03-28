//! Preprocessed player action — three-perspective rewrite of raw player input.
//!
//! STT cleanup produces disfluency-free text in three forms:
//! - `you`: second-person ("You draw your sword")
//! - `named`: third-person with character name ("{Name} draws their sword")
//! - `intent`: neutral, no pronouns ("draw sword")

use serde::{Deserialize, Serialize};

/// A player action cleaned of STT disfluencies and rewritten into three perspectives.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PreprocessedAction {
    /// Second-person form: "You draw your sword"
    pub you: String,
    /// Third-person with character name: "{CharName} draws their sword"
    pub named: String,
    /// Neutral intent, no pronouns: "draw sword"
    pub intent: String,
}

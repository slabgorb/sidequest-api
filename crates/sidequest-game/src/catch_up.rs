//! Catch-up narration — generate arrival snapshot for mid-session joining players.
//!
//! Story 8-8: When a player joins a session in progress, they receive a concise
//! catch-up summary. The generator uses a pluggable strategy (trait object) for
//! the actual LLM call, allowing test doubles.

/// A lightweight per-turn summary maintained by the session.
///
/// One line per turn, updated as turns resolve. Avoids sending full narration
/// history to the LLM.
#[derive(Debug, Clone)]
pub struct TurnSummary {
    turn_number: u32,
    summary: String,
}

impl TurnSummary {
    /// Create a new turn summary.
    pub fn new(turn_number: u32, summary: String) -> Self {
        todo!()
    }

    /// The turn number.
    pub fn turn_number(&self) -> u32 {
        todo!()
    }

    /// The summary text.
    pub fn summary(&self) -> &str {
        todo!()
    }
}

/// Error type for catch-up narration generation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CatchUpError {
    /// The generation strategy failed (e.g., Claude unavailable).
    #[error("catch-up generation failed: {0}")]
    GenerationFailed(String),
    /// No turn history available to summarize.
    #[error("no turn history available")]
    NoHistory,
}

/// Strategy trait for generating catch-up narration text.
///
/// Allows injecting test doubles in place of the real Claude CLI call.
pub trait GenerationStrategy {
    /// Generate narration text from a composed prompt.
    fn generate(&self, prompt: &str) -> Result<String, CatchUpError>;
}

/// Result of a catch-up narration generation.
#[derive(Debug)]
pub struct CatchUpResult {
    narration: String,
    is_fallback: bool,
    target_player_id: Option<String>,
}

impl CatchUpResult {
    /// Create a result from successful generation.
    pub fn generated(narration: String) -> Self {
        todo!()
    }

    /// Create a fallback result (used when generation fails).
    pub fn fallback(narration: String) -> Self {
        todo!()
    }

    /// The narration text.
    pub fn narration(&self) -> &str {
        todo!()
    }

    /// Whether this result is a fallback (not LLM-generated).
    pub fn is_fallback(&self) -> bool {
        todo!()
    }

    /// Set the target player ID for targeted delivery.
    pub fn for_player(self, player_id: String) -> Self {
        todo!()
    }

    /// The target player ID, if set.
    pub fn target_player_id(&self) -> Option<&str> {
        todo!()
    }
}

/// Generates catch-up narration for late-joining players.
///
/// Uses a pluggable `GenerationStrategy` for the actual LLM call,
/// enabling test doubles.
pub struct CatchUpGenerator {
    strategy: Box<dyn GenerationStrategy>,
}

impl CatchUpGenerator {
    /// Create a new generator with the given strategy.
    pub fn new(strategy: Box<dyn GenerationStrategy>) -> Self {
        todo!()
    }

    /// Generate catch-up narration for a joining player.
    ///
    /// Composes a prompt from the character, recent turn summaries, location,
    /// and genre voice, then delegates to the strategy.
    pub fn generate_catch_up(
        &self,
        character: &crate::character::Character,
        recent_turns: &[TurnSummary],
        location: &str,
        genre_voice: &str,
    ) -> Result<CatchUpResult, CatchUpError> {
        todo!()
    }

    /// Generate catch-up with automatic fallback on failure.
    ///
    /// If the strategy fails, returns a basic location description
    /// instead of propagating the error.
    pub fn generate_catch_up_with_fallback(
        &self,
        character: &crate::character::Character,
        recent_turns: &[TurnSummary],
        location: &str,
        genre_voice: &str,
    ) -> Result<CatchUpResult, CatchUpError> {
        todo!()
    }

    /// Format recent turn summaries for the prompt.
    ///
    /// Takes the last 5 turns (most recent first) and formats them
    /// as a bulleted list.
    pub fn format_recent(turns: &[TurnSummary]) -> String {
        todo!()
    }

    /// Build the prompt string for catch-up generation.
    ///
    /// Includes character name, location, genre voice, and recent events.
    pub fn build_prompt(
        character: &crate::character::Character,
        recent_turns: &[TurnSummary],
        location: &str,
        genre_voice: &str,
    ) -> String {
        todo!()
    }

    /// Generate a brief join notification for existing players.
    pub fn join_notification(character_name: &str) -> String {
        todo!()
    }
}

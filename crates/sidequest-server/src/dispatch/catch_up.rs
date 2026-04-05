//! Catch-up narration — generate arrival snapshots for mid-session joining players.
//!
//! Implements `GenerationStrategy` from `sidequest_game::catch_up` using
//! `ClaudeClient` for the actual LLM call.

use sidequest_agents::client::ClaudeClient;
use sidequest_game::catch_up::{CatchUpError, CatchUpGenerator, CatchUpResult, GenerationStrategy, TurnSummary};
use sidequest_game::character::Character;
use sidequest_protocol::{GameMessage, NarrationEndPayload, NarrationPayload};

/// Claude CLI-backed generation strategy for catch-up narration.
pub(crate) struct ClaudeGenerationStrategy {
    client: ClaudeClient,
}

impl ClaudeGenerationStrategy {
    pub fn new(client: ClaudeClient) -> Self {
        Self { client }
    }
}

impl GenerationStrategy for ClaudeGenerationStrategy {
    fn generate(&self, prompt: &str) -> Result<String, CatchUpError> {
        let response = self.client.send_with_model(prompt, "haiku");
        match response {
            Ok(r) => Ok(r.text),
            Err(e) => Err(CatchUpError::GenerationFailed(e.to_string())),
        }
    }
}

/// Generate catch-up narration for a player joining an in-progress session.
///
/// Returns `None` if no narration history exists (first player, nothing to catch up on).
/// Returns catch-up narration messages targeted at the joining player.
pub(crate) fn generate_catch_up_messages(
    state: &crate::AppState,
    character: &Character,
    narration_history: &[String],
    location: &str,
    genre_voice: &str,
    player_id: &str,
) -> Option<Vec<GameMessage>> {
    if narration_history.is_empty() {
        return None;
    }

    let span = tracing::info_span!(
        "catch_up.generate",
        player_id = %player_id,
        history_len = narration_history.len(),
    );
    let _guard = span.enter();

    // Convert narration history to TurnSummary objects
    let summaries: Vec<TurnSummary> = narration_history
        .iter()
        .enumerate()
        .map(|(i, text)| TurnSummary::new(i as u32 + 1, text.clone()))
        .collect();

    let client = state.create_claude_client();
    let strategy = ClaudeGenerationStrategy::new(client);
    let generator = CatchUpGenerator::new(Box::new(strategy));

    match generator.generate_catch_up_with_fallback(character, &summaries, location, genre_voice) {
        Ok(result) => {
            tracing::info!(
                is_fallback = result.is_fallback(),
                narration_len = result.narration().len(),
                "catch_up.generated"
            );

            crate::WatcherEventBuilder::new("catch_up", crate::WatcherEventType::StateTransition)
                .field("event", "catch_up_generated")
                .field("player_id", player_id)
                .field("is_fallback", result.is_fallback())
                .field("history_turns", narration_history.len())
                .send(state);

            Some(vec![
                GameMessage::Narration {
                    payload: NarrationPayload {
                        text: result.narration().to_string(),
                        state_delta: None,
                        footnotes: vec![],
                    },
                    player_id: player_id.to_string(),
                },
                GameMessage::NarrationEnd {
                    payload: NarrationEndPayload { state_delta: None },
                    player_id: player_id.to_string(),
                },
            ])
        }
        Err(e) => {
            tracing::warn!(error = %e, "catch_up.generation_failed");
            None
        }
    }
}

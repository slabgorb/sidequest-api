//! Action Preprocessor — STT cleanup before player input reaches agents.
//!
//! Calls a haiku-tier LLM via ClaudeClient to clean speech-to-text disfluencies
//! (uh, um, like, you know, false starts, repetitions) and rewrite player input
//! into three perspectives: second-person, named third-person, and neutral intent.
//!
//! On LLM failure or timeout, falls back to mechanical string manipulation so
//! the game loop never blocks on preprocessing.

use std::time::Duration;

use sidequest_game::PreprocessedAction;
use tracing::{info, warn};

use crate::client::ClaudeClient;

/// Haiku model identifier for fast preprocessing.
const HAIKU_MODEL: &str = "haiku";

/// Timeout for preprocessing — must be fast to not delay the game loop.
const PREPROCESS_TIMEOUT: Duration = Duration::from_secs(15);

/// Preprocess a raw player action into three perspectives via LLM.
///
/// Fails loudly if Haiku is unavailable — no silent fallbacks.
pub fn preprocess_action(raw_input: &str, char_name: &str) -> Result<PreprocessedAction, PreprocessError> {
    let client = ClaudeClient::with_timeout(PREPROCESS_TIMEOUT);

    let prompt = build_prompt(raw_input, char_name);

    let llm_span = tracing::info_span!("turn.preprocess.llm", model = HAIKU_MODEL);
    let llm_result = {
        let _llm_guard = llm_span.enter();
        client.send_with_model(&prompt, HAIKU_MODEL)
    };

    match llm_result {
        Ok(resp) => {
            let response = &resp.text;
            let parse_span = tracing::info_span!("turn.preprocess.parse", response_len = response.len());
            let _parse_guard = parse_span.enter();
            match parse_response(response) {
                Some(action) => {
                    let max_len = raw_input.len() * 2;
                    if action.you.len() > max_len
                        || action.named.len() > max_len
                        || action.intent.len() > max_len
                    {
                        Err(PreprocessError::OutputTooLong {
                            raw_len: raw_input.len(),
                            you_len: action.you.len(),
                            named_len: action.named.len(),
                            intent_len: action.intent.len(),
                        })
                    } else {
                        info!(
                            you = %action.you,
                            named = %action.named,
                            intent = %action.intent,
                            "Action preprocessed via LLM"
                        );
                        Ok(action)
                    }
                }
                None => Err(PreprocessError::ParseFailed(response.clone())),
            }
        }
        Err(e) => Err(PreprocessError::LlmFailed(e.to_string())),
    }
}

/// Errors from preprocessing — no silent fallbacks.
#[derive(Debug, thiserror::Error)]
pub enum PreprocessError {
    #[error("Haiku LLM call failed: {0}")]
    LlmFailed(String),
    #[error("Failed to parse Haiku response as PreprocessedAction: {0}")]
    ParseFailed(String),
    #[error("Preprocessor output exceeded 2x input length (raw={raw_len}, you={you_len}, named={named_len}, intent={intent_len})")]
    OutputTooLong {
        raw_len: usize,
        you_len: usize,
        named_len: usize,
        intent_len: usize,
    },
}

/// Async wrapper around [`preprocess_action`] for use in the dispatch pipeline.
///
/// Runs the sync preprocessor on a blocking thread via `spawn_blocking` so it
/// doesn't block the tokio runtime. Propagates errors — no silent fallbacks.
pub async fn preprocess_action_async(raw_input: &str, char_name: &str) -> Result<PreprocessedAction, PreprocessError> {
    let raw = raw_input.to_string();
    let name = char_name.to_string();
    match tokio::task::spawn_blocking(move || preprocess_action(&raw, &name)).await {
        Ok(result) => result,
        Err(e) => Err(PreprocessError::LlmFailed(format!("spawn_blocking panicked: {e}"))),
    }
}

/// Build the LLM prompt for action preprocessing.
fn build_prompt(raw_input: &str, char_name: &str) -> String {
    format!(
        r#"You are a speech-to-text cleanup preprocessor for a tabletop RPG game.

Clean the following player input of STT disfluencies (uh, um, like, you know, false starts, repetitions).

Rules:
- Preserve all quoted dialogue VERBATIM.
- Do NOT add adjectives, adverbs, or emotions that weren't in the original.
- Each output field must be no longer than 2x the input length.
- Output ONLY valid JSON, no markdown fences, no explanation.

Character name: {char_name}

Player input: "{raw_input}"

Respond with JSON having exactly eight keys:
- "you": second-person rewrite (e.g., "You draw your sword")
- "named": third-person with character name (e.g., "{char_name} draws their sword")
- "intent": neutral, no pronouns (e.g., "draw sword")
- "is_power_grab": true ONLY if the player is genuinely attempting to seize extraordinary power
  (unlimited resources, godlike abilities, time control, invincibility, summoning weapons from
  nothing, killing everyone). The test: would a tabletop DM say "you can't just do that"?
  Casual mention does NOT count: "I wish I hadn't eaten that" = false.
  "I wish for unlimited gold from the genie" = true.
- "references_inventory": true if the player mentions using, checking, equipping, trading,
  dropping, or interacting with items, equipment, or possessions. "I look around" = false.
  "I use my healing potion" = true. "I check what I'm carrying" = true.
- "references_npc": true if the player addresses or mentions a specific character by name
  or role. "I explore the cave" = false. "I talk to the bartender" = true.
- "references_ability": true if the player invokes or activates a power, mutation, skill,
  spell, or supernatural ability. "I walk north" = false. "I use my psychic echo" = true.
- "references_location": true if the player mentions a specific place by name or attempts
  to travel somewhere. "I look around" = false. "I head to the market" = true."#
    )
}

/// Parse the LLM response as a PreprocessedAction JSON object.
fn parse_response(response: &str) -> Option<PreprocessedAction> {
    // Try direct parse first
    if let Ok(action) = serde_json::from_str::<PreprocessedAction>(response) {
        return Some(action);
    }

    // Try extracting JSON from markdown fences or surrounding text
    if let Some(start) = response.find('{') {
        if let Some(end) = response.rfind('}') {
            let json_str = &response[start..=end];
            if let Ok(action) = serde_json::from_str::<PreprocessedAction>(json_str) {
                return Some(action);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_response_direct_json() {
        let json = r#"{"you":"You draw your sword","named":"Kael draws their sword","intent":"draw sword"}"#;
        let result = parse_response(json).unwrap();
        assert_eq!(result.you, "You draw your sword");
        assert_eq!(result.named, "Kael draws their sword");
        assert_eq!(result.intent, "draw sword");
    }

    #[test]
    fn test_parse_response_with_markdown() {
        let response = "Here is the result:\n```json\n{\"you\":\"You look\",\"named\":\"Kael looks\",\"intent\":\"look\"}\n```";
        let result = parse_response(response).unwrap();
        assert_eq!(result.you, "You look");
    }

    #[test]
    fn test_parse_response_garbage() {
        assert!(parse_response("not json at all").is_none());
    }

    #[test]
    fn test_build_prompt_contains_key_elements() {
        let prompt = build_prompt("uh I like draw my sword", "Kael");
        assert!(prompt.contains("Kael"));
        assert!(prompt.contains("uh I like draw my sword"));
        assert!(prompt.contains("\"you\""));
        assert!(prompt.contains("\"named\""));
        assert!(prompt.contains("\"intent\""));
    }

    #[test]
    fn test_third_person_single_word() {
        assert_eq!(action_text_to_third_person("run"), "runs");
        assert_eq!(action_text_to_third_person("watch"), "watches");
        assert_eq!(action_text_to_third_person("go"), "goes");
    }
}

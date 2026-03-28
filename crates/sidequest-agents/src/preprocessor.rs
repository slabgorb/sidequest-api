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
const HAIKU_MODEL: &str = "claude-haiku-4-5-20250514";

/// Timeout for preprocessing — must be fast to not delay the game loop.
const PREPROCESS_TIMEOUT: Duration = Duration::from_secs(15);

/// Preprocess a raw player action into three perspectives via LLM.
///
/// On any failure (timeout, parse error, LLM error), returns a mechanical fallback.
pub fn preprocess_action(raw_input: &str, char_name: &str) -> PreprocessedAction {
    let client = ClaudeClient::with_timeout(PREPROCESS_TIMEOUT);

    let prompt = build_prompt(raw_input, char_name);

    match client.send_with_model(&prompt, HAIKU_MODEL) {
        Ok(response) => {
            match parse_response(&response) {
                Some(action) => {
                    // Validate output length constraint: each field <= 2x input length
                    let max_len = raw_input.len() * 2;
                    if action.you.len() > max_len
                        || action.named.len() > max_len
                        || action.intent.len() > max_len
                    {
                        warn!(
                            raw_len = raw_input.len(),
                            you_len = action.you.len(),
                            named_len = action.named.len(),
                            intent_len = action.intent.len(),
                            "Preprocessor output exceeded 2x input length, using fallback"
                        );
                        fallback(raw_input, char_name)
                    } else {
                        info!(
                            you = %action.you,
                            named = %action.named,
                            intent = %action.intent,
                            "Action preprocessed via LLM"
                        );
                        action
                    }
                }
                None => {
                    warn!(response = %response, "Failed to parse preprocessor LLM response, using fallback");
                    fallback(raw_input, char_name)
                }
            }
        }
        Err(e) => {
            warn!(error = %e, "Preprocessor LLM call failed, using fallback");
            fallback(raw_input, char_name)
        }
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

Respond with JSON having exactly three keys:
- "you": second-person rewrite (e.g., "You draw your sword")
- "named": third-person with character name (e.g., "{char_name} draws their sword")
- "intent": neutral, no pronouns (e.g., "draw sword")"#
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

/// Mechanical fallback when LLM is unavailable.
///
/// Strips "I " or "i " prefix and constructs three perspectives from the remainder.
pub fn fallback(raw_input: &str, char_name: &str) -> PreprocessedAction {
    let trimmed = raw_input.trim();

    // Strip first-person prefix
    let action_text = if trimmed.starts_with("I ") {
        &trimmed[2..]
    } else if trimmed.starts_with("i ") {
        &trimmed[2..]
    } else {
        trimmed
    };

    let action_text = action_text.trim();

    PreprocessedAction {
        you: format!("You {}", action_text),
        named: format!("{} {}", char_name, action_text_to_third_person(action_text)),
        intent: action_text.to_string(),
    }
}

/// Minimal verb conjugation for third-person fallback.
///
/// Handles common patterns: "draw" -> "draws", "look around" -> "looks around".
/// Not exhaustive — this is a best-effort fallback, not a grammar engine.
fn action_text_to_third_person(text: &str) -> String {
    // Split into first word (verb) and rest
    let mut parts = text.splitn(2, ' ');
    let verb = match parts.next() {
        Some(v) => v,
        None => return text.to_string(),
    };
    let rest = parts.next().unwrap_or("");

    // Simple third-person -s suffix
    let conjugated = if verb.ends_with("sh") || verb.ends_with("ch") || verb.ends_with("ss")
        || verb.ends_with('x') || verb.ends_with('z') || verb.ends_with('o')
    {
        format!("{}es", verb)
    } else if verb.ends_with('y')
        && !verb.ends_with("ay")
        && !verb.ends_with("ey")
        && !verb.ends_with("oy")
        && !verb.ends_with("uy")
    {
        format!("{}ies", &verb[..verb.len() - 1])
    } else {
        format!("{}s", verb)
    };

    if rest.is_empty() {
        conjugated
    } else {
        format!("{} {}", conjugated, rest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fallback_strips_i_prefix() {
        let result = fallback("I draw my sword", "Kael");
        assert_eq!(result.you, "You draw my sword");
        assert_eq!(result.named, "Kael draws my sword");
        assert_eq!(result.intent, "draw my sword");
    }

    #[test]
    fn test_fallback_lowercase_i_prefix() {
        let result = fallback("i look around", "Lyra");
        assert_eq!(result.you, "You look around");
        assert_eq!(result.named, "Lyra looks around");
        assert_eq!(result.intent, "look around");
    }

    #[test]
    fn test_fallback_no_i_prefix() {
        let result = fallback("attack the goblin", "Thorne");
        assert_eq!(result.you, "You attack the goblin");
        assert_eq!(result.named, "Thorne attacks the goblin");
        assert_eq!(result.intent, "attack the goblin");
    }

    #[test]
    fn test_fallback_trims_whitespace() {
        let result = fallback("  I  search the room  ", "Anya");
        assert_eq!(result.you, "You search the room");
        assert_eq!(result.named, "Anya searches the room");
        assert_eq!(result.intent, "search the room");
    }

    #[test]
    fn test_fallback_verb_conjugation_sh() {
        let result = fallback("push the door", "Rex");
        assert_eq!(result.named, "Rex pushes the door");
    }

    #[test]
    fn test_fallback_verb_conjugation_y() {
        let result = fallback("try to open the chest", "Ivy");
        assert_eq!(result.named, "Ivy tries to open the chest");
    }

    #[test]
    fn test_fallback_verb_conjugation_ay() {
        // "play" should become "plays", not "plaies"
        let result = fallback("play the lute", "Bard");
        assert_eq!(result.named, "Bard plays the lute");
    }

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

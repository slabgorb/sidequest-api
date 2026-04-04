//! LLM-based continuity validation — replaces keyword substring matching.
//!
//! Uses a Haiku classification call to detect contradictions between narrator
//! prose and canonical game state. Returns structured ContradictionCategory
//! results that feed into the next turn's prompt corrections.

use std::time::Duration;

use sidequest_game::continuity::{Contradiction, ContradictionCategory, ValidationResult};
use tracing::{info, warn};

use crate::client::ClaudeClient;

/// Haiku model for fast validation.
const HAIKU_MODEL: &str = "haiku";

/// Timeout — validation must not block the game loop.
const VALIDATE_TIMEOUT: Duration = Duration::from_secs(30);

/// Validate narrator text against game state using Haiku classification.
///
/// Returns a `ValidationResult` with any contradictions found. On LLM failure,
/// returns an empty result (no corrections) rather than blocking.
pub fn validate_continuity_llm(
    narration: &str,
    location: &str,
    dead_npcs: &[String],
    inventory_items: &[String],
    time_of_day: &str,
) -> ValidationResult {
    let client = ClaudeClient::with_timeout(VALIDATE_TIMEOUT);

    let prompt = build_validation_prompt(narration, location, dead_npcs, inventory_items, time_of_day);

    let span = tracing::info_span!("continuity.llm_validation", model = HAIKU_MODEL);
    let result = {
        let _guard = span.enter();
        client.send_with_model(&prompt, HAIKU_MODEL)
    };

    match result {
        Ok(resp) => {
            match parse_validation_response(&resp.text) {
                Some(contradictions) => {
                    info!(
                        contradictions = contradictions.len(),
                        "continuity.llm_validation_complete"
                    );
                    ValidationResult { contradictions }
                }
                None => {
                    // Clean parse failure — likely means "no contradictions found"
                    info!("continuity.llm_validation_clean");
                    ValidationResult::default()
                }
            }
        }
        Err(e) => {
            warn!(error = %e, "continuity.llm_validation_failed — skipping corrections this turn");
            ValidationResult::default()
        }
    }
}

/// Async wrapper for use in the dispatch pipeline.
pub async fn validate_continuity_llm_async(
    narration: &str,
    location: &str,
    dead_npcs: &[String],
    inventory_items: &[String],
    time_of_day: &str,
) -> ValidationResult {
    let narration = narration.to_string();
    let location = location.to_string();
    let dead_npcs = dead_npcs.to_vec();
    let inventory_items = inventory_items.to_vec();
    let time_of_day = time_of_day.to_string();

    match tokio::task::spawn_blocking(move || {
        validate_continuity_llm(&narration, &location, &dead_npcs, &inventory_items, &time_of_day)
    })
    .await
    {
        Ok(result) => result,
        Err(e) => {
            warn!(error = %e, "continuity.spawn_blocking_failed");
            ValidationResult::default()
        }
    }
}

fn build_validation_prompt(
    narration: &str,
    location: &str,
    dead_npcs: &[String],
    inventory_items: &[String],
    time_of_day: &str,
) -> String {
    let mut state_facts = Vec::new();

    if !location.is_empty() {
        state_facts.push(format!("Current location: {location}"));
    }
    if !dead_npcs.is_empty() {
        state_facts.push(format!("Dead NPCs (hp=0, should not act or speak): {}", dead_npcs.join(", ")));
    }
    if !inventory_items.is_empty() {
        state_facts.push(format!("Player inventory: {}", inventory_items.join(", ")));
    }
    if !time_of_day.is_empty() {
        state_facts.push(format!("Time of day: {time_of_day}"));
    }

    if state_facts.is_empty() {
        return String::new();
    }

    format!(
        r#"You are a continuity checker for a tabletop RPG narration engine.

Given the NARRATION and the canonical GAME STATE below, identify any contradictions.
A contradiction is when the narration describes something that conflicts with the game state.

GAME STATE:
{}

NARRATION:
{}

Respond with a JSON array of contradictions. Each entry:
{{"category": "<location|dead_npc|inventory|time_of_day>", "detail": "<what's wrong>", "expected": "<what state says>"}}

If there are NO contradictions, respond with an empty array: []

IMPORTANT:
- The narrator does NOT need to explicitly name the location every turn. Only flag if the narration describes being in a DIFFERENT place.
- A dead NPC being MENTIONED is fine (memories, references). Flag only if they ACT, SPEAK, or are described as alive.
- Inventory: only flag if the narration has the player USING an item they don't possess.
- Time: only flag if narration describes daylight during night or vice versa."#,
        state_facts.join("\n"),
        narration
    )
}

/// Parse the Haiku response into Contradiction structs.
fn parse_validation_response(response: &str) -> Option<Vec<Contradiction>> {
    // Try direct parse
    if let Ok(entries) = serde_json::from_str::<Vec<RawContradiction>>(response) {
        if entries.is_empty() {
            return None; // no contradictions
        }
        return Some(entries.into_iter().map(|e| e.into()).collect());
    }

    // Try extracting JSON array from fenced block or surrounding text
    if let Some(start) = response.find('[') {
        if let Some(end) = response.rfind(']') {
            let json_str = &response[start..=end];
            if let Ok(entries) = serde_json::from_str::<Vec<RawContradiction>>(json_str) {
                if entries.is_empty() {
                    return None;
                }
                return Some(entries.into_iter().map(|e| e.into()).collect());
            }
        }
    }

    None
}

#[derive(serde::Deserialize)]
struct RawContradiction {
    category: String,
    detail: String,
    expected: String,
}

impl From<RawContradiction> for Contradiction {
    fn from(raw: RawContradiction) -> Self {
        let category = match raw.category.as_str() {
            "location" => ContradictionCategory::Location,
            "dead_npc" => ContradictionCategory::DeadNpc,
            "inventory" => ContradictionCategory::Inventory,
            "time_of_day" => ContradictionCategory::TimeOfDay,
            _ => ContradictionCategory::Location, // fallback
        };
        Contradiction {
            category,
            detail: raw.detail,
            expected: raw.expected,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_array_returns_none() {
        assert!(parse_validation_response("[]").is_none());
    }

    #[test]
    fn parse_single_contradiction() {
        let json = r#"[{"category": "dead_npc", "detail": "Gork speaks despite being dead", "expected": "Gork is dead (hp=0)"}]"#;
        let result = parse_validation_response(json).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].category, ContradictionCategory::DeadNpc);
    }

    #[test]
    fn parse_fenced_json() {
        let response = "Here are the contradictions:\n```json\n[{\"category\": \"location\", \"detail\": \"Narration says forest but player is in tavern\", \"expected\": \"The Rusty Tankard\"}]\n```";
        let result = parse_validation_response(response).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].category, ContradictionCategory::Location);
    }

    #[test]
    fn parse_garbage_returns_none() {
        assert!(parse_validation_response("I found no issues with the narration.").is_none());
    }

    #[test]
    fn prompt_includes_state_facts() {
        let prompt = build_validation_prompt(
            "You enter the dark forest.",
            "The Rusty Tankard",
            &["Gork".to_string()],
            &["Rusty Sword".to_string()],
            "night",
        );
        assert!(prompt.contains("The Rusty Tankard"));
        assert!(prompt.contains("Gork"));
        assert!(prompt.contains("Rusty Sword"));
        assert!(prompt.contains("night"));
    }

    #[test]
    fn prompt_skips_empty_facts() {
        let prompt = build_validation_prompt("test", "", &[], &[], "");
        assert!(prompt.is_empty());
    }
}

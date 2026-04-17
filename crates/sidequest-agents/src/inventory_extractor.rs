//! Two-pass inventory mutation extractor.
//!
//! On Turn N+1, classifies the previous turn's narration + action to detect
//! inventory state transitions. Uses Haiku for fast classification — no regex.
//! Returns structured mutations that the dispatch pipeline applies via
//! `Inventory::transition()`.
//!
//! Graceful degradation: on timeout or parse failure, returns empty (no mutations).

use std::time::Duration;

use tracing::{info, warn};

use crate::client::ClaudeClient;

/// Haiku model for fast extraction.
const HAIKU_MODEL: &str = "haiku";

/// Timeout — extraction must not block the game loop.
/// Haiku CLI cold starts can take 10-15s, so 20s gives headroom.
const EXTRACT_TIMEOUT: Duration = Duration::from_secs(20);

/// OTEL event name emitted when a mutation is successfully extracted.
pub const OTEL_MUTATION_EXTRACTED: &str = "inventory.mutation_extracted";

/// OTEL event name emitted on extraction failure (timeout or parse error).
pub const OTEL_MUTATION_MISSED: &str = "inventory.mutation_missed";

/// OTEL event name emitted when the LLM response cannot be parsed as JSON.
/// Distinct from OTEL_MUTATION_MISSED (which covers timeout/subprocess errors).
pub const OTEL_EXTRACTION_PARSE_FAILED: &str = "inventory.extraction_parse_failed";

/// A single inventory mutation extracted from narration.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct InventoryMutation {
    /// Item name (fuzzy — matched against inventory by the caller).
    pub item_name: String,
    /// What happened to the item.
    pub action: MutationAction,
    /// Who or what was involved (merchant name, NPC name, cause of loss).
    #[serde(default)]
    pub detail: String,
    /// Item category for acquired items (weapon, armor, tool, consumable, treasure, misc).
    #[serde(default)]
    pub category: Option<String>,
    /// Gold amount for currency acquisitions (caps, coins, credits).
    #[serde(default)]
    pub gold: Option<i64>,
}

/// The result of parsing a Claude extraction response.
///
/// Three-state return: mutations found, clean (no mutations), or parse failure.
/// This replaces the previous `Option<Vec<InventoryMutation>>` which made parse
/// failures indistinguishable from clean extractions — the core 37-10 bug.
#[derive(Debug, Clone)]
pub enum ExtractionOutcome {
    /// Successfully parsed one or more inventory mutations.
    Mutations(Vec<InventoryMutation>),
    /// Claude explicitly returned an empty array — no mutations detected.
    Clean,
    /// The response could not be parsed as a JSON array of mutations.
    ParseFailed { raw_response: String },
}

/// The type of inventory mutation.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum MutationAction {
    /// Item was consumed (eaten, drunk, used up, spent).
    Consumed,
    /// Item was sold to someone.
    Sold,
    /// Item was given to someone.
    Given,
    /// Item was lost (stolen, dropped, confiscated).
    Lost,
    /// Item was destroyed (broken, burned, disintegrated).
    Destroyed,
    /// Item was acquired (found, looted, received, bought, picked up).
    Acquired,
}

impl std::fmt::Display for MutationAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Consumed => write!(f, "consumed"),
            Self::Sold => write!(f, "sold"),
            Self::Given => write!(f, "given"),
            Self::Lost => write!(f, "lost"),
            Self::Destroyed => write!(f, "destroyed"),
            Self::Acquired => write!(f, "acquired"),
        }
    }
}

/// Extract inventory mutations from the previous turn's narration.
///
/// Synchronous — call via `spawn_blocking` from async code.
pub fn extract_inventory_mutations(
    previous_action: &str,
    previous_narration: &str,
    carried_items: &[String],
) -> Vec<InventoryMutation> {
    if previous_narration.is_empty() {
        return vec![];
    }

    let client = ClaudeClient::with_timeout(EXTRACT_TIMEOUT);
    let prompt = build_extraction_prompt(previous_action, previous_narration, carried_items);

    let span = tracing::info_span!("inventory.extraction", model = HAIKU_MODEL);
    let result = {
        let _guard = span.enter();
        client.send_with_model(&prompt, HAIKU_MODEL)
    };

    match result {
        Ok(resp) => match parse_extraction_response(&resp.text) {
            ExtractionOutcome::Mutations(mutations) => {
                info!(mutations = mutations.len(), "inventory.extraction_complete");
                for mutation in &mutations {
                    info!(
                        item_name = %mutation.item_name,
                        action = %mutation.action,
                        category = ?mutation.category,
                        detail = %mutation.detail,
                        OTEL_MUTATION_EXTRACTED
                    );
                }
                mutations
            }
            ExtractionOutcome::Clean => {
                info!("inventory.extraction_clean — no mutations detected");
                vec![]
            }
            ExtractionOutcome::ParseFailed { raw_response } => {
                warn!(
                    otel_event = OTEL_EXTRACTION_PARSE_FAILED,
                    raw_response_len = raw_response.len(),
                    raw_response_preview = %&raw_response[..raw_response.len().min(200)],
                    reason = "extraction_parse_failed",
                    "inventory.extraction_parse_failed — could not parse LLM response as JSON"
                );
                vec![]
            }
        },
        Err(e) => {
            warn!(
                error = %e,
                otel_event = OTEL_MUTATION_MISSED,
                reason = "extraction_timeout_or_error",
                "inventory.extraction_failed — skipping this turn"
            );
            vec![]
        }
    }
}

/// Async wrapper for use in the dispatch pipeline.
pub async fn extract_inventory_mutations_async(
    previous_action: &str,
    previous_narration: &str,
    carried_items: &[String],
) -> Vec<InventoryMutation> {
    let action = previous_action.to_string();
    let narration = previous_narration.to_string();
    let items = carried_items.to_vec();

    match tokio::task::spawn_blocking(move || {
        extract_inventory_mutations(&action, &narration, &items)
    })
    .await
    {
        Ok(result) => result,
        Err(e) => {
            warn!(error = %e, "inventory.extraction_spawn_failed");
            vec![]
        }
    }
}

/// Builds the Haiku extraction prompt for detecting inventory mutations from narration.
///
/// `action` is the raw player input, `narration` is the narrator response, and
/// `carried_items` is the current inventory. Returns a prompt string for `claude -p`.
pub fn build_extraction_prompt(action: &str, narration: &str, carried_items: &[String]) -> String {
    let inventory_section = if carried_items.is_empty() {
        "(empty)".to_string()
    } else {
        carried_items.join("\n")
    };
    format!(
        r#"You are an inventory tracker for a tabletop RPG. Given a player's ACTION, the NARRATOR's response, and the player's current INVENTORY, identify items that changed state OR new items acquired.

INVENTORY (items the player currently carries):
{inventory_section}

ACTION:
{action}

NARRATION:
{narration}

Report TWO kinds of changes:

1. **Existing items that changed state** (from the INVENTORY LIST):
- **consumed**: eaten, drunk, injected, used up, spent, applied
- **sold**: traded to a merchant for gold/currency
- **given**: handed to an NPC or another player voluntarily
- **lost**: stolen, confiscated, dropped accidentally, fell into a pit
- **destroyed**: broken, burned, shattered, disintegrated

2. **New items acquired** (NOT in the inventory list):
- **acquired**: found, looted, received, bought, picked up, taken from a body, picks up, grabs, takes, pockets it, tucks it away, stows it, adds it to their pack

Respond with a JSON array. Each entry:
{{"item_name": "<item name>", "action": "<consumed|sold|given|lost|destroyed|acquired>", "detail": "<who/what/why>", "category": "<weapon|armor|tool|consumable|treasure|misc>", "gold": <number or null>}}

For existing items (consumed/sold/given/lost/destroyed): category can be null.
For acquired items: category is required.

CURRENCY/GOLD RULES:
- Currency GAINED (found, looted, received, fished out): action "acquired", gold = positive amount.
- Currency SPENT (tossed, donated, paid, lost, given away, used as offering): action "lost" (or "given"/"consumed" as appropriate), gold = positive amount spent.
- If the item is currency (caps, coins, gold, credits), set gold to the amount and item_name to the currency name.

If NOTHING changed, respond with: []

RULES:
- For state changes: only report items IN THE INVENTORY LIST.
- For acquisitions: only report if the narration CONFIRMS the player received/found/took the item. "I search the body" is not acquisition unless the narrator describes finding something.
- Equipping, examining, or brandishing is NOT a state change.
- Using a weapon to attack does NOT consume it (unless the narration says it broke)."#
    )
}

/// Parses a Haiku extraction response into inventory mutations.
///
/// Handles both raw JSON arrays and markdown-fenced ```json blocks.
/// Returns a three-state `ExtractionOutcome`:
/// - `Mutations(vec)` — successfully parsed mutations
/// - `Clean` — Claude explicitly returned `[]` (no mutations)
/// - `ParseFailed { raw_response }` — could not parse the response as JSON
pub fn parse_extraction_response(response: &str) -> ExtractionOutcome {
    // Try direct parse
    if let Ok(entries) = serde_json::from_str::<Vec<InventoryMutation>>(response) {
        if entries.is_empty() {
            return ExtractionOutcome::Clean;
        }
        return ExtractionOutcome::Mutations(entries);
    }

    // Try extracting JSON array from fenced block or surrounding text
    if let Some(start) = response.find('[') {
        if let Some(end) = response.rfind(']') {
            let json_str = &response[start..=end];
            if let Ok(entries) = serde_json::from_str::<Vec<InventoryMutation>>(json_str) {
                if entries.is_empty() {
                    return ExtractionOutcome::Clean;
                }
                return ExtractionOutcome::Mutations(entries);
            }
        }
    }

    ExtractionOutcome::ParseFailed {
        raw_response: response.to_string(),
    }
}

/// Validates that an item name will produce a non-empty ID after sanitization.
///
/// The dispatch layer sanitizes item names to IDs via `to_lowercase() + replace`.
/// If the name contains only special characters (e.g., "—???—"), the sanitized
/// ID is empty and `NonBlankString::new("")` fails silently. This function
/// catches that before the silent failure.
pub fn validate_mutation_item_name(name: &str) -> Result<(), String> {
    let sanitized = name
        .to_lowercase()
        .replace(' ', "_")
        .replace(|c: char| !c.is_alphanumeric() && c != '_', "");
    if sanitized.is_empty() {
        Err(format!(
            "Item name '{name}' produces empty ID after sanitization"
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_array_returns_clean() {
        assert!(matches!(
            parse_extraction_response("[]"),
            ExtractionOutcome::Clean
        ));
    }

    #[test]
    fn parse_single_mutation() {
        let json = r#"[{"item_name": "Healing Potion", "action": "consumed", "detail": "drank during combat"}]"#;
        match parse_extraction_response(json) {
            ExtractionOutcome::Mutations(result) => {
                assert_eq!(result.len(), 1);
                assert_eq!(result[0].item_name, "Healing Potion");
                assert_eq!(result[0].action, MutationAction::Consumed);
            }
            other => panic!("Expected Mutations, got: {other:?}"),
        }
    }

    #[test]
    fn parse_multiple_mutations() {
        let json = r#"[
            {"item_name": "Rusty Sword", "action": "sold", "detail": "sold to Patchwork"},
            {"item_name": "Compass", "action": "given", "detail": "gave to Shirley"}
        ]"#;
        match parse_extraction_response(json) {
            ExtractionOutcome::Mutations(result) => {
                assert_eq!(result.len(), 2);
                assert_eq!(result[0].action, MutationAction::Sold);
                assert_eq!(result[1].action, MutationAction::Given);
                assert_eq!(result[1].detail, "gave to Shirley");
            }
            other => panic!("Expected Mutations, got: {other:?}"),
        }
    }

    #[test]
    fn parse_fenced_json() {
        let response = "Here are the mutations:\n```json\n[{\"item_name\": \"Torch\", \"action\": \"destroyed\", \"detail\": \"burned out\"}]\n```";
        match parse_extraction_response(response) {
            ExtractionOutcome::Mutations(result) => {
                assert_eq!(result.len(), 1);
                assert_eq!(result[0].action, MutationAction::Destroyed);
            }
            other => panic!("Expected Mutations, got: {other:?}"),
        }
    }

    #[test]
    fn parse_garbage_returns_parse_failed() {
        assert!(matches!(
            parse_extraction_response("No items changed state."),
            ExtractionOutcome::ParseFailed { .. }
        ));
    }

    #[test]
    fn prompt_includes_inventory_and_narration() {
        let prompt = build_extraction_prompt(
            "I drink the healing potion",
            "You gulp down the potion. Warmth spreads through your limbs.",
            &["Healing Potion".to_string(), "Iron Sword".to_string()],
        );
        assert!(prompt.contains("Healing Potion"));
        assert!(prompt.contains("Iron Sword"));
        assert!(prompt.contains("drink the healing potion"));
        assert!(prompt.contains("gulp down the potion"));
    }

    #[test]
    #[ignore] // calls real Claude CLI — run with cargo test -- --ignored
    fn empty_inventory_acquires_nothing_from_bland_narration() {
        let result = extract_inventory_mutations("I look around", "You see a dusty room.", &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn empty_narration_short_circuits() {
        let result = extract_inventory_mutations("I look around", "", &["Sword".to_string()]);
        assert!(result.is_empty());
    }

    #[test]
    fn parse_acquired_item() {
        let json = r#"[{"item_name": "Rusty Caps", "action": "acquired", "detail": "looted from body", "category": "treasure", "gold": null}]"#;
        match parse_extraction_response(json) {
            ExtractionOutcome::Mutations(result) => {
                assert_eq!(result.len(), 1);
                assert_eq!(result[0].action, MutationAction::Acquired);
                assert_eq!(result[0].category.as_deref(), Some("treasure"));
                assert_eq!(result[0].gold, None);
            }
            other => panic!("Expected Mutations, got: {other:?}"),
        }
    }

    #[test]
    fn parse_gold_acquisition() {
        let json = r#"[{"item_name": "caps", "action": "acquired", "detail": "found on body", "category": "treasure", "gold": 11}]"#;
        match parse_extraction_response(json) {
            ExtractionOutcome::Mutations(result) => {
                assert_eq!(result.len(), 1);
                assert_eq!(result[0].action, MutationAction::Acquired);
                assert_eq!(result[0].gold, Some(11));
            }
            other => panic!("Expected Mutations, got: {other:?}"),
        }
    }

    #[test]
    fn parse_gold_loss() {
        let json = r#"[{"item_name": "gold", "action": "lost", "detail": "tossed into fountain", "gold": 1}]"#;
        match parse_extraction_response(json) {
            ExtractionOutcome::Mutations(result) => {
                assert_eq!(result.len(), 1);
                assert_eq!(result[0].action, MutationAction::Lost);
                assert_eq!(result[0].gold, Some(1));
            }
            other => panic!("Expected Mutations, got: {other:?}"),
        }
    }

    #[test]
    fn parse_gold_given() {
        let json = r#"[{"item_name": "gold", "action": "given", "detail": "donated to beggar", "gold": 5}]"#;
        match parse_extraction_response(json) {
            ExtractionOutcome::Mutations(result) => {
                assert_eq!(result.len(), 1);
                assert_eq!(result[0].action, MutationAction::Given);
                assert_eq!(result[0].gold, Some(5));
            }
            other => panic!("Expected Mutations, got: {other:?}"),
        }
    }

    #[test]
    fn parse_mixed_mutations() {
        let json = r#"[
            {"item_name": "Healing Potion", "action": "consumed", "detail": "drank it"},
            {"item_name": "Old Key", "action": "acquired", "detail": "found in chest", "category": "quest"}
        ]"#;
        match parse_extraction_response(json) {
            ExtractionOutcome::Mutations(result) => {
                assert_eq!(result.len(), 2);
                assert_eq!(result[0].action, MutationAction::Consumed);
                assert_eq!(result[1].action, MutationAction::Acquired);
            }
            other => panic!("Expected Mutations, got: {other:?}"),
        }
    }
}

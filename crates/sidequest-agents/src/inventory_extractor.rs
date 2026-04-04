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
const EXTRACT_TIMEOUT: Duration = Duration::from_secs(8);

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

/// The type of inventory mutation.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
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
            Some(mutations) => {
                info!(
                    mutations = mutations.len(),
                    "inventory.extraction_complete"
                );
                mutations
            }
            None => {
                info!("inventory.extraction_clean — no mutations detected");
                vec![]
            }
        },
        Err(e) => {
            warn!(error = %e, "inventory.extraction_failed — skipping this turn");
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

fn build_extraction_prompt(
    action: &str,
    narration: &str,
    carried_items: &[String],
) -> String {
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
- **acquired**: found, looted, received, bought, picked up, taken from a body

Respond with a JSON array. Each entry:
{{"item_name": "<item name>", "action": "<consumed|sold|given|lost|destroyed|acquired>", "detail": "<who/what/why>", "category": "<weapon|armor|tool|consumable|treasure|misc>", "gold": <number or null>}}

For existing items (consumed/sold/given/lost/destroyed): category and gold can be null.
For acquired items: category is required. If the item is currency (caps, coins, gold, credits), set gold to the amount and item_name to the currency name.

If NOTHING changed, respond with: []

RULES:
- For state changes: only report items IN THE INVENTORY LIST.
- For acquisitions: only report if the narration CONFIRMS the player received/found/took the item. "I search the body" is not acquisition unless the narrator describes finding something.
- Equipping, examining, or brandishing is NOT a state change.
- Using a weapon to attack does NOT consume it (unless the narration says it broke).
- Currency/gold found should use the "acquired" action with a gold amount."#
    )
}

fn parse_extraction_response(response: &str) -> Option<Vec<InventoryMutation>> {
    // Try direct parse
    if let Ok(entries) = serde_json::from_str::<Vec<InventoryMutation>>(response) {
        if entries.is_empty() {
            return None;
        }
        return Some(entries);
    }

    // Try extracting JSON array from fenced block or surrounding text
    if let Some(start) = response.find('[') {
        if let Some(end) = response.rfind(']') {
            let json_str = &response[start..=end];
            if let Ok(entries) = serde_json::from_str::<Vec<InventoryMutation>>(json_str) {
                if entries.is_empty() {
                    return None;
                }
                return Some(entries);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_array_returns_none() {
        assert!(parse_extraction_response("[]").is_none());
    }

    #[test]
    fn parse_single_mutation() {
        let json = r#"[{"item_name": "Healing Potion", "action": "consumed", "detail": "drank during combat"}]"#;
        let result = parse_extraction_response(json).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].item_name, "Healing Potion");
        assert_eq!(result[0].action, MutationAction::Consumed);
    }

    #[test]
    fn parse_multiple_mutations() {
        let json = r#"[
            {"item_name": "Rusty Sword", "action": "sold", "detail": "sold to Patchwork"},
            {"item_name": "Compass", "action": "given", "detail": "gave to Shirley"}
        ]"#;
        let result = parse_extraction_response(json).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].action, MutationAction::Sold);
        assert_eq!(result[1].action, MutationAction::Given);
        assert_eq!(result[1].detail, "gave to Shirley");
    }

    #[test]
    fn parse_fenced_json() {
        let response = "Here are the mutations:\n```json\n[{\"item_name\": \"Torch\", \"action\": \"destroyed\", \"detail\": \"burned out\"}]\n```";
        let result = parse_extraction_response(response).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].action, MutationAction::Destroyed);
    }

    #[test]
    fn parse_garbage_returns_none() {
        assert!(parse_extraction_response("No items changed state.").is_none());
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
        let result = extract_inventory_mutations(
            "I look around",
            "You see a dusty room.",
            &[],
        );
        assert!(result.is_empty());
    }

    #[test]
    fn empty_narration_short_circuits() {
        let result = extract_inventory_mutations(
            "I look around",
            "",
            &["Sword".to_string()],
        );
        assert!(result.is_empty());
    }

    #[test]
    fn parse_acquired_item() {
        let json = r#"[{"item_name": "Rusty Caps", "action": "acquired", "detail": "looted from body", "category": "treasure", "gold": null}]"#;
        let result = parse_extraction_response(json).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].action, MutationAction::Acquired);
        assert_eq!(result[0].category.as_deref(), Some("treasure"));
        assert_eq!(result[0].gold, None);
    }

    #[test]
    fn parse_gold_acquisition() {
        let json = r#"[{"item_name": "caps", "action": "acquired", "detail": "found on body", "category": "treasure", "gold": 11}]"#;
        let result = parse_extraction_response(json).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].action, MutationAction::Acquired);
        assert_eq!(result[0].gold, Some(11));
    }

    #[test]
    fn parse_mixed_mutations() {
        let json = r#"[
            {"item_name": "Healing Potion", "action": "consumed", "detail": "drank it"},
            {"item_name": "Old Key", "action": "acquired", "detail": "found in chest", "category": "quest"}
        ]"#;
        let result = parse_extraction_response(json).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].action, MutationAction::Consumed);
        assert_eq!(result[1].action, MutationAction::Acquired);
    }
}

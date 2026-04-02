//! Item acquisition validation tool (ADR-057 Phase 3).
//!
//! Validates an item_acquire tool call from the narrator sidecar.
//! Accepts catalog-style IDs or narrator-described free-text references.
//! Rejects empty/whitespace fields. Produces `ItemGained` for the protocol layer.

use std::fmt;

/// Validated result of an `item_acquire` tool call.
///
/// Fields are private with getters to enforce validation invariants
/// (non-empty, trimmed). Constructed only through `validate_item_acquire`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ItemAcquireResult {
    item_ref: String,
    name: String,
    category: String,
}

impl ItemAcquireResult {
    /// The item reference — catalog ID or narrator description.
    pub fn item_ref(&self) -> &str {
        &self.item_ref
    }

    /// Display name for the item.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Item category (weapon, armor, tool, consumable, quest, misc, etc.).
    pub fn category(&self) -> &str {
        &self.category
    }

    /// Convert to `ItemGained` for the protocol layer.
    ///
    /// Uses `item_ref` as the description — the narrator's reference text
    /// serves as a reasonable one-line description.
    pub fn to_item_gained(&self) -> sidequest_protocol::ItemGained {
        sidequest_protocol::ItemGained {
            name: self.name.clone(),
            description: self.item_ref.clone(),
            category: self.category.clone(),
        }
    }
}

/// Error returned when an item_acquire tool call has invalid fields.
#[derive(Debug)]
pub struct InvalidItemAcquire(String);

impl fmt::Display for InvalidItemAcquire {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid item_acquire: {}", self.0)
    }
}

impl std::error::Error for InvalidItemAcquire {}

/// Validate an item_acquire tool call.
///
/// All three fields are required and must be non-empty after trimming.
/// Accepts any non-empty string for `item_ref` — both catalog IDs
/// (`"iron_sword"`) and narrator descriptions (`"a rusty sword with runes"`).
#[tracing::instrument(name = "tool.item_acquire", skip_all, fields(
    item_ref = %item_ref,
    name = %name,
    category = %category,
))]
pub fn validate_item_acquire(
    item_ref: &str,
    name: &str,
    category: &str,
) -> Result<ItemAcquireResult, InvalidItemAcquire> {
    let item_ref = item_ref.trim();
    let name = name.trim();
    let category = category.trim();

    if item_ref.is_empty() {
        tracing::warn!(valid = false, "item_ref is empty");
        return Err(InvalidItemAcquire("item_ref is empty".to_string()));
    }
    if name.is_empty() {
        tracing::warn!(valid = false, "name is empty");
        return Err(InvalidItemAcquire("name is empty".to_string()));
    }
    if category.is_empty() {
        tracing::warn!(valid = false, "category is empty");
        return Err(InvalidItemAcquire("category is empty".to_string()));
    }

    let result = ItemAcquireResult {
        item_ref: item_ref.to_string(),
        name: name.to_string(),
        category: category.to_string(),
    };

    tracing::info!(
        valid = true,
        item_ref = result.item_ref(),
        name = result.name(),
        category = result.category(),
        "item_acquire validated"
    );

    Ok(result)
}

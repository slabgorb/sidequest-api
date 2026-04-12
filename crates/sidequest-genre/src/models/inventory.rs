//! Inventory and economy types from `inventory.yaml`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Complete inventory configuration from `inventory.yaml`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InventoryConfig {
    /// Currency system (optional — some genre packs don't define one).
    #[serde(default)]
    pub currency: Option<CurrencyConfig>,
    /// Full item catalog.
    #[serde(default)]
    pub item_catalog: Vec<CatalogItem>,
    /// Starting equipment per archetype/class. Key = archetype/class name, value = item IDs.
    #[serde(default)]
    pub starting_equipment: HashMap<String, Vec<String>>,
    /// Starting gold per archetype/class.
    #[serde(default)]
    pub starting_gold: HashMap<String, u32>,
    /// Inventory philosophy (carry limits, restrictions).
    #[serde(default)]
    pub philosophy: Option<InventoryPhilosophy>,
}

/// Currency system definition.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CurrencyConfig {
    /// Currency name (e.g., "gold", "credits", "Dollars").
    pub name: String,
    /// Denomination names or name→multiplier map.
    /// Accepts either a list of strings or a map of name→value.
    #[serde(default)]
    pub denominations: serde_json::Value,
}

/// A single item in the genre pack's item catalog.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CatalogItem {
    /// Unique item identifier (e.g., "sword_iron").
    pub id: String,
    /// Display name.
    pub name: String,
    /// Item description.
    pub description: String,
    /// Category: weapon, armor, tool, consumable, treasure, misc.
    pub category: String,
    /// Base value in currency.
    #[serde(default)]
    pub value: u32,
    /// Weight in abstract units.
    #[serde(default)]
    pub weight: f64,
    /// Rarity: common, uncommon, rare, legendary.
    #[serde(default)]
    pub rarity: String,
    /// Power level (0-5 scale).
    #[serde(default)]
    pub power_level: u32,
    /// Searchable tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Flavor text / lore.
    #[serde(default)]
    pub lore: String,
    /// Narrative weight for how much the narrator should mention this item.
    #[serde(default)]
    pub narrative_weight: serde_json::Value,
    /// Number of room transitions before this item is consumed.
    /// Maps to `uses_remaining` on the game `Item` struct.
    /// E.g., a torch with `resource_ticks: 6` lasts 6 room transitions.
    #[serde(default)]
    pub resource_ticks: Option<u32>,
}

/// Whether inventory limits are enforced by item count or total weight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
#[derive(Default)]
pub enum CarryMode {
    /// Limit by number of carried items (existing behavior).
    #[default]
    Count,
    /// Limit by total weight of carried items.
    Weight,
}

/// Inventory philosophy configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InventoryPhilosophy {
    /// Maximum number of carried items (count-based limit).
    #[serde(default)]
    pub carry_limit: Option<u32>,
    /// Whether to enforce count-based or weight-based limits.
    #[serde(default)]
    pub carry_mode: CarryMode,
    /// Maximum total weight when carry_mode is Weight.
    #[serde(default)]
    pub weight_limit: Option<f64>,
    /// Item categories that are restricted.
    #[serde(default)]
    pub restricted_categories: Vec<String>,
    /// Progression gates for item access.
    #[serde(default)]
    pub progression_gates: HashMap<String, serde_json::Value>,
}

//! Inventory and Item types.
//!
//! ADR-021 Track 3: Items evolve via narrative_weight thresholds.
//! Genre packs define item categories and inventory limits.

use serde::{Deserialize, Serialize};
use sidequest_protocol::NonBlankString;

/// An item in the game world.
///
/// Items gain identity as `narrative_weight` increases (ADR-021):
/// - 0.0..0.5: unnamed utility items ("coal")
/// - 0.5..0.7: named items with identity
/// - 0.7+: mechanically significant ("diamond")
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    /// Unique item identifier (e.g., "sword_iron").
    pub id: NonBlankString,
    /// Display name.
    pub name: NonBlankString,
    /// Item description.
    pub description: NonBlankString,
    /// Category: weapon, armor, consumable, tool, treasure.
    pub category: NonBlankString,
    /// Gold value.
    pub value: i32,
    /// Encumbrance weight.
    pub weight: f64,
    /// Rarity: common, uncommon, rare, legendary.
    pub rarity: NonBlankString,
    /// Narrative weight (0.0 to 1.0) — controls item evolution stage.
    pub narrative_weight: f64,
    /// Tags for item classification (melee, blade, fire, magic, etc.).
    pub tags: Vec<String>,
    /// Whether the item is currently equipped.
    pub equipped: bool,
    /// Stack count (consumables can stack).
    pub quantity: u32,
}

impl Item {
    /// Whether this item has gained a proper name (narrative_weight >= 0.5).
    pub fn is_named(&self) -> bool {
        self.narrative_weight >= 0.5
    }

    /// Whether this item has full mechanical power (narrative_weight >= 0.7).
    pub fn is_evolved(&self) -> bool {
        self.narrative_weight >= 0.7
    }
}

/// Error when an inventory operation fails.
#[derive(Debug, Clone, thiserror::Error)]
pub enum InventoryError {
    /// Inventory is at capacity.
    #[error("inventory full: {current}/{limit} items")]
    Full {
        /// Current item count.
        current: usize,
        /// Maximum capacity.
        limit: usize,
    },
    /// Item not found by ID.
    #[error("item not found: {0}")]
    NotFound(String),
}

/// A character's inventory — items and gold.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Inventory {
    /// Carried items.
    pub items: Vec<Item>,
    /// Gold currency.
    pub gold: i64,
}

impl Default for Inventory {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            gold: 0,
        }
    }
}

impl Inventory {
    /// Number of items currently held.
    pub fn item_count(&self) -> usize {
        self.items.len()
    }

    /// Add an item to the inventory. Returns error if at capacity.
    pub fn add(&mut self, item: Item, carry_limit: usize) -> Result<(), InventoryError> {
        if self.items.len() >= carry_limit {
            return Err(InventoryError::Full {
                current: self.items.len(),
                limit: carry_limit,
            });
        }
        self.items.push(item);
        Ok(())
    }

    /// Remove an item by ID. Returns the removed item or error if not found.
    pub fn remove(&mut self, id: &str) -> Result<Item, InventoryError> {
        let pos = self
            .items
            .iter()
            .position(|item| item.id.as_str() == id)
            .ok_or_else(|| InventoryError::NotFound(id.to_string()))?;
        Ok(self.items.remove(pos))
    }

    /// Find an item by ID.
    pub fn find(&self, id: &str) -> Option<&Item> {
        self.items.iter().find(|item| item.id.as_str() == id)
    }

    /// Get all equipped items.
    pub fn equipped(&self) -> Vec<&Item> {
        self.items.iter().filter(|item| item.equipped).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sword() -> Item {
        Item {
            id: NonBlankString::new("sword_iron").unwrap(),
            name: NonBlankString::new("Iron Sword").unwrap(),
            description: NonBlankString::new("A sturdy iron blade").unwrap(),
            category: NonBlankString::new("weapon").unwrap(),
            value: 50,
            weight: 3.0,
            rarity: NonBlankString::new("common").unwrap(),
            narrative_weight: 0.3,
            tags: vec!["melee".to_string(), "blade".to_string()],
            equipped: true,
            quantity: 1,
        }
    }

    fn potion() -> Item {
        Item {
            id: NonBlankString::new("healing_potion").unwrap(),
            name: NonBlankString::new("Healing Potion").unwrap(),
            description: NonBlankString::new("Restores health").unwrap(),
            category: NonBlankString::new("consumable").unwrap(),
            value: 25,
            weight: 0.5,
            rarity: NonBlankString::new("common").unwrap(),
            narrative_weight: 0.1,
            tags: vec!["healing".to_string()],
            equipped: false,
            quantity: 3,
        }
    }

    fn legendary_blade() -> Item {
        Item {
            id: NonBlankString::new("blade_of_dawn").unwrap(),
            name: NonBlankString::new("Blade of Dawn").unwrap(),
            description: NonBlankString::new("A radiant sword that hums with power").unwrap(),
            category: NonBlankString::new("weapon").unwrap(),
            value: 500,
            weight: 2.5,
            rarity: NonBlankString::new("legendary").unwrap(),
            narrative_weight: 0.85,
            tags: vec!["melee".to_string(), "blade".to_string(), "radiant".to_string()],
            equipped: false,
            quantity: 1,
        }
    }

    // === Item narrative evolution (ADR-021 Track 3) ===

    #[test]
    fn low_narrative_weight_not_named() {
        let item = sword(); // 0.3
        assert!(!item.is_named());
        assert!(!item.is_evolved());
    }

    #[test]
    fn named_at_threshold() {
        let mut item = sword();
        item.narrative_weight = 0.5;
        assert!(item.is_named());
        assert!(!item.is_evolved());
    }

    #[test]
    fn evolved_at_threshold() {
        let mut item = sword();
        item.narrative_weight = 0.7;
        assert!(item.is_named());
        assert!(item.is_evolved());
    }

    #[test]
    fn legendary_is_fully_evolved() {
        let item = legendary_blade(); // 0.85
        assert!(item.is_named());
        assert!(item.is_evolved());
    }

    // === Inventory: add ===

    #[test]
    fn add_item_to_empty_inventory() {
        let mut inv = Inventory::default();
        assert!(inv.add(sword(), 10).is_ok());
        assert_eq!(inv.item_count(), 1);
    }

    #[test]
    fn add_item_at_capacity_fails() {
        let mut inv = Inventory::default();
        inv.add(sword(), 1).unwrap();
        let result = inv.add(potion(), 1);
        assert!(result.is_err());
        match result.unwrap_err() {
            InventoryError::Full { current, limit } => {
                assert_eq!(current, 1);
                assert_eq!(limit, 1);
            }
            _ => panic!("expected Full error"),
        }
    }

    #[test]
    fn add_multiple_items() {
        let mut inv = Inventory::default();
        inv.add(sword(), 10).unwrap();
        inv.add(potion(), 10).unwrap();
        inv.add(legendary_blade(), 10).unwrap();
        assert_eq!(inv.item_count(), 3);
    }

    // === Inventory: remove ===

    #[test]
    fn remove_existing_item() {
        let mut inv = Inventory::default();
        inv.add(sword(), 10).unwrap();
        let removed = inv.remove("sword_iron").unwrap();
        assert_eq!(removed.id.as_str(), "sword_iron");
        assert_eq!(inv.item_count(), 0);
    }

    #[test]
    fn remove_nonexistent_item_fails() {
        let mut inv = Inventory::default();
        let result = inv.remove("nonexistent");
        assert!(result.is_err());
        match result.unwrap_err() {
            InventoryError::NotFound(id) => assert_eq!(id, "nonexistent"),
            _ => panic!("expected NotFound error"),
        }
    }

    #[test]
    fn remove_from_empty_inventory_fails() {
        let mut inv = Inventory::default();
        assert!(inv.remove("anything").is_err());
    }

    // === Inventory: find ===

    #[test]
    fn find_existing_item() {
        let mut inv = Inventory::default();
        inv.add(sword(), 10).unwrap();
        let found = inv.find("sword_iron");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name.as_str(), "Iron Sword");
    }

    #[test]
    fn find_nonexistent_returns_none() {
        let inv = Inventory::default();
        assert!(inv.find("nonexistent").is_none());
    }

    // === Inventory: equipped ===

    #[test]
    fn equipped_returns_only_equipped_items() {
        let mut inv = Inventory::default();
        inv.add(sword(), 10).unwrap(); // equipped = true
        inv.add(potion(), 10).unwrap(); // equipped = false
        let equipped = inv.equipped();
        assert_eq!(equipped.len(), 1);
        assert_eq!(equipped[0].id.as_str(), "sword_iron");
    }

    #[test]
    fn equipped_empty_when_nothing_equipped() {
        let mut inv = Inventory::default();
        inv.add(potion(), 10).unwrap(); // equipped = false
        assert!(inv.equipped().is_empty());
    }

    // === Inventory: gold ===

    #[test]
    fn default_inventory_has_zero_gold() {
        let inv = Inventory::default();
        assert_eq!(inv.gold, 0);
    }

    #[test]
    fn gold_can_be_set() {
        let mut inv = Inventory::default();
        inv.gold = 100;
        assert_eq!(inv.gold, 100);
    }

    // === Serde round-trip ===

    #[test]
    fn item_json_roundtrip() {
        let item = sword();
        let json = serde_json::to_string(&item).unwrap();
        let back: Item = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id.as_str(), "sword_iron");
        assert_eq!(back.name.as_str(), "Iron Sword");
        assert_eq!(back.equipped, true);
    }

    #[test]
    fn inventory_json_roundtrip() {
        let mut inv = Inventory::default();
        inv.add(sword(), 10).unwrap();
        inv.gold = 200;
        let json = serde_json::to_string(&inv).unwrap();
        let back: Inventory = serde_json::from_str(&json).unwrap();
        assert_eq!(back.item_count(), 1);
        assert_eq!(back.gold, 200);
    }

    #[test]
    fn blank_item_id_rejected_in_json() {
        let json = r#"{"id":"","name":"x","description":"x","category":"weapon","value":0,"weight":0.0,"rarity":"common","narrative_weight":0.0,"tags":[],"equipped":false,"quantity":1}"#;
        let result = serde_json::from_str::<Item>(json);
        assert!(result.is_err(), "blank item id should fail deserialization");
    }
}

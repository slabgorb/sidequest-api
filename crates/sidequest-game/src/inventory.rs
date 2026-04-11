//! Inventory and Item types.
//!
//! ADR-021 Track 3: Items evolve via narrative_weight thresholds.
//! Genre packs define item categories and inventory limits.

use serde::{Deserialize, Serialize};
use sidequest_protocol::NonBlankString;

/// Disposition state of an item in the inventory ledger.
///
/// Items are never removed from inventory — they transition to a non-carried
/// state that records provenance. This enables quest hooks ("recover your
/// stolen sword"), narrative callbacks, and full item history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "detail")]
#[derive(Default)]
pub enum ItemState {
    /// Player is carrying this item (default, active inventory).
    #[default]
    Carried,
    /// Item was consumed (potion drunk, food eaten, ammo spent).
    Consumed,
    /// Item was sold to a merchant.
    Sold {
        /// Name of the merchant or buyer.
        to: String,
    },
    /// Item was given to an NPC or another player.
    Given {
        /// Name of the recipient.
        to: String,
    },
    /// Item was lost (stolen, dropped into a pit, confiscated).
    Lost {
        /// How the item was lost.
        reason: String,
    },
    /// Item was destroyed (broken, burned, disintegrated).
    Destroyed {
        /// How the item was destroyed.
        reason: String,
    },
}


impl ItemState {
    /// Whether the item is currently in the player's active inventory.
    pub fn is_carried(&self) -> bool {
        matches!(self, Self::Carried)
    }
}

impl std::fmt::Display for ItemState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Carried => write!(f, "carried"),
            Self::Consumed => write!(f, "consumed"),
            Self::Sold { to } => write!(f, "sold to {}", to),
            Self::Given { to } => write!(f, "given to {}", to),
            Self::Lost { reason } => write!(f, "lost: {}", reason),
            Self::Destroyed { reason } => write!(f, "destroyed: {}", reason),
        }
    }
}

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
    /// Remaining uses before this item is consumed. `None` means infinite.
    /// Set from genre pack `item_catalog` entries (e.g., `resource_ticks: 6` for a torch).
    #[serde(default)]
    pub uses_remaining: Option<u32>,
    /// Disposition state — items are never deleted, they transition states.
    /// Enables provenance tracking, quest hooks, and narrative callbacks.
    #[serde(default)]
    pub state: ItemState,
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

// Re-export CarryMode from genre crate for convenience.
pub use sidequest_genre::CarryMode;

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
    /// Adding item would exceed weight limit.
    #[error("overweight: {current_weight:.1} + {item_weight:.1} exceeds limit {limit:.1}")]
    Overweight {
        /// Current total weight of carried items.
        current_weight: f64,
        /// Weight of the item being added (weight × quantity).
        item_weight: f64,
        /// Maximum weight limit.
        limit: f64,
    },
}

/// A character's inventory ledger — append-only item history and gold.
///
/// Items are never removed. They transition between states (Carried, Sold,
/// Given, Lost, Destroyed, Consumed). This preserves provenance for quest
/// hooks ("recover your stolen sword") and narrative callbacks.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Inventory {
    /// All items — active (Carried) and historical (other states).
    pub items: Vec<Item>,
    /// Gold currency.
    pub gold: i64,
}

impl Inventory {
    /// Number of items currently carried (active inventory).
    pub fn item_count(&self) -> usize {
        self.items.iter().filter(|i| i.state.is_carried()).count()
    }

    /// All items in the ledger, including non-carried.
    pub fn ledger_size(&self) -> usize {
        self.items.len()
    }

    /// Add an item to the inventory. Returns error if carried items at capacity.
    pub fn add(&mut self, item: Item, carry_limit: usize) -> Result<(), InventoryError> {
        let carried = self.item_count();
        if carried >= carry_limit {
            return Err(InventoryError::Full {
                current: carried,
                limit: carry_limit,
            });
        }
        self.items.push(item);
        Ok(())
    }

    /// Add an item with weight-based limit. Rejects if total weight would exceed limit.
    pub fn add_weighted(&mut self, item: Item, weight_limit: f64) -> Result<(), InventoryError> {
        let current = self.total_weight();
        let item_weight = item.weight * item.quantity as f64;
        if current + item_weight > weight_limit {
            return Err(InventoryError::Overweight {
                current_weight: current,
                item_weight,
                limit: weight_limit,
            });
        }
        self.items.push(item);
        Ok(())
    }

    /// Total weight of all carried items (weight × quantity per item).
    pub fn total_weight(&self) -> f64 {
        self.items
            .iter()
            .filter(|i| i.state.is_carried())
            .map(|i| i.weight * i.quantity as f64)
            .sum()
    }

    /// Whether total carried weight is at or over the given weight limit.
    pub fn is_overencumbered(&self, weight_limit: f64) -> bool {
        self.total_weight() >= weight_limit
    }

    /// Encumbrance multiplier for trope tick stacking.
    /// Returns 1.5 when overencumbered, 1.0 otherwise.
    pub fn encumbrance_multiplier(&self, weight_limit: f64) -> f64 {
        if self.is_overencumbered(weight_limit) {
            1.5
        } else {
            1.0
        }
    }

    /// Transition an item to a new state. Returns the item's previous state,
    /// or `NotFound` if no carried item matches the ID.
    pub fn transition(
        &mut self,
        id: &str,
        new_state: ItemState,
    ) -> Result<ItemState, InventoryError> {
        let item = self
            .items
            .iter_mut()
            .find(|item| item.id.as_str() == id && item.state.is_carried())
            .ok_or_else(|| InventoryError::NotFound(id.to_string()))?;
        let old_state = std::mem::replace(&mut item.state, new_state);
        item.equipped = false;
        Ok(old_state)
    }

    /// Remove an item by ID. Kept for backwards compatibility with merchant
    /// transactions and other code that expects physical removal.
    /// Prefer `transition()` for new code.
    pub fn remove(&mut self, id: &str) -> Result<Item, InventoryError> {
        let pos = self
            .items
            .iter()
            .position(|item| item.id.as_str() == id && item.state.is_carried())
            .ok_or_else(|| InventoryError::NotFound(id.to_string()))?;
        Ok(self.items.remove(pos))
    }

    /// Find a carried item by ID.
    pub fn find(&self, id: &str) -> Option<&Item> {
        self.items
            .iter()
            .find(|item| item.id.as_str() == id && item.state.is_carried())
    }

    /// Find any item by ID regardless of state (for ledger queries).
    pub fn find_any(&self, id: &str) -> Option<&Item> {
        self.items.iter().find(|item| item.id.as_str() == id)
    }

    /// All non-carried items — the history ledger.
    pub fn history(&self) -> Vec<&Item> {
        self.items
            .iter()
            .filter(|i| !i.state.is_carried())
            .collect()
    }

    /// Items lost/stolen/given away — potential quest hooks.
    pub fn recoverable(&self) -> Vec<&Item> {
        self.items
            .iter()
            .filter(|i| matches!(i.state, ItemState::Lost { .. } | ItemState::Given { .. }))
            .collect()
    }

    /// Get all equipped carried items.
    pub fn equipped(&self) -> Vec<&Item> {
        self.items
            .iter()
            .filter(|item| item.equipped && item.state.is_carried())
            .collect()
    }

    /// Iterator over carried items only (active inventory).
    pub fn carried(&self) -> impl Iterator<Item = &self::Item> {
        self.items.iter().filter(|i| i.state.is_carried())
    }

    /// Decrement an item's `uses_remaining` by 1.
    ///
    /// - If `uses_remaining` is `None` (infinite): no-op, returns `None`.
    /// - If `uses_remaining` is `Some(n)` where `n > 1`: decrements to `n - 1`, returns `None`.
    /// - If `uses_remaining` is `Some(1)` or `Some(0)`: transitions to `Consumed`, returns
    ///   a clone with `uses_remaining` set to `Some(0)`.
    /// - If the item is not found (or not carried): returns `None`.
    pub fn consume_use(&mut self, id: &str) -> Option<Item> {
        let pos = self
            .items
            .iter()
            .position(|item| item.id.as_str() == id && item.state.is_carried())?;

        match self.items[pos].uses_remaining {
            None => None, // infinite use
            Some(n) if n <= 1 => {
                self.items[pos].uses_remaining = Some(0);
                self.items[pos].state = ItemState::Consumed;
                self.items[pos].equipped = false;
                Some(self.items[pos].clone())
            }
            Some(n) => {
                self.items[pos].uses_remaining = Some(n - 1);
                None
            }
        }
    }

    /// Deduct gold, clamping at zero. Returns the actual amount deducted.
    ///
    /// If the player has less gold than `amount`, deducts whatever they have
    /// and returns that (smaller) value. Gold never goes negative.
    pub fn spend_gold(&mut self, amount: i64) -> i64 {
        let actual = amount.min(self.gold);
        self.gold -= actual;
        actual
    }

    /// Deplete the first light source on a room transition.
    ///
    /// Finds the first carried item with tag `"light"` and calls [`consume_use`](Self::consume_use).
    /// Returns the consumed item if the light source was exhausted (for GameMessage emission).
    pub fn deplete_light_on_transition(&mut self) -> Option<Item> {
        let light_id = self
            .items
            .iter()
            .find(|item| item.state.is_carried() && item.tags.iter().any(|t| t == "light"))
            .map(|item| item.id.as_str().to_owned())?;

        self.consume_use(&light_id)
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
            uses_remaining: None,
            state: ItemState::Carried,
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
            uses_remaining: None,
            state: ItemState::Carried,
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
            tags: vec![
                "melee".to_string(),
                "blade".to_string(),
                "radiant".to_string(),
            ],
            equipped: false,
            quantity: 1,
            uses_remaining: None,
            state: ItemState::Carried,
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

    // === Consumable depletion (Story 19-5) ===

    fn torch() -> Item {
        Item {
            id: NonBlankString::new("torch_1").unwrap(),
            name: NonBlankString::new("Torch").unwrap(),
            description: NonBlankString::new("A pitch-soaked bundle of rags on a stick").unwrap(),
            category: NonBlankString::new("light").unwrap(),
            value: 1,
            weight: 0.5,
            rarity: NonBlankString::new("common").unwrap(),
            narrative_weight: 0.3,
            tags: vec!["light".to_string(), "consumable".to_string()],
            equipped: false,
            quantity: 1,
            uses_remaining: Some(6),
            state: ItemState::Carried,
        }
    }

    fn lantern_oil() -> Item {
        Item {
            id: NonBlankString::new("lantern_oil_1").unwrap(),
            name: NonBlankString::new("Flask of Lantern Oil").unwrap(),
            description: NonBlankString::new("Enough oil for two hours").unwrap(),
            category: NonBlankString::new("light").unwrap(),
            value: 5,
            weight: 0.5,
            rarity: NonBlankString::new("common").unwrap(),
            narrative_weight: 0.1,
            tags: vec![
                "light".to_string(),
                "consumable".to_string(),
                "fuel".to_string(),
            ],
            equipped: false,
            state: ItemState::Carried,
            quantity: 1,
            uses_remaining: Some(12),
        }
    }

    // --- consume_use: basic behavior ---

    #[test]
    fn consume_use_infinite_item_is_noop() {
        let mut inv = Inventory::default();
        inv.add(sword(), 10).unwrap(); // uses_remaining: None
        let result = inv.consume_use("sword_iron");
        assert!(result.is_none(), "infinite-use item should not be consumed");
        assert_eq!(inv.item_count(), 1, "item should still be in inventory");
    }

    #[test]
    fn consume_use_decrements_uses_remaining() {
        let mut inv = Inventory::default();
        inv.add(torch(), 10).unwrap(); // uses_remaining: Some(6)
        let result = inv.consume_use("torch_1");
        assert!(
            result.is_none(),
            "torch should not be removed after one use"
        );
        let item = inv.find("torch_1").expect("torch should still exist");
        assert_eq!(
            item.uses_remaining,
            Some(5),
            "uses_remaining should decrement from 6 to 5"
        );
    }

    #[test]
    fn consume_use_removes_item_at_last_use() {
        let mut inv = Inventory::default();
        let mut t = torch();
        t.uses_remaining = Some(1); // one use left
        inv.add(t, 10).unwrap();
        let result = inv.consume_use("torch_1");
        assert!(result.is_some(), "last use should return the removed item");
        let removed = result.unwrap();
        assert_eq!(removed.id.as_str(), "torch_1");
        assert_eq!(
            removed.uses_remaining,
            Some(0),
            "removed item should have 0 uses"
        );
        assert_eq!(
            inv.item_count(),
            0,
            "inventory should be empty after removal"
        );
    }

    #[test]
    fn consume_use_removes_item_at_zero() {
        let mut inv = Inventory::default();
        let mut t = torch();
        t.uses_remaining = Some(0); // already at zero
        inv.add(t, 10).unwrap();
        let result = inv.consume_use("torch_1");
        assert!(result.is_some(), "item at 0 uses should be removed");
        assert_eq!(inv.item_count(), 0);
    }

    #[test]
    fn consume_use_nonexistent_item_returns_none() {
        let mut inv = Inventory::default();
        inv.add(torch(), 10).unwrap();
        let result = inv.consume_use("nonexistent");
        assert!(result.is_none(), "nonexistent item should return None");
        assert_eq!(inv.item_count(), 1, "inventory unchanged");
    }

    // --- deplete_light_on_transition ---

    #[test]
    fn deplete_light_decrements_first_light_source() {
        let mut inv = Inventory::default();
        inv.add(torch(), 10).unwrap();
        let result = inv.deplete_light_on_transition();
        assert!(
            result.is_none(),
            "torch with 6 uses should not be removed after 1 transition"
        );
        let item = inv.find("torch_1").expect("torch should still exist");
        assert_eq!(item.uses_remaining, Some(5));
    }

    #[test]
    fn deplete_light_no_light_source_returns_none() {
        let mut inv = Inventory::default();
        inv.add(sword(), 10).unwrap(); // no "light" tag
        let result = inv.deplete_light_on_transition();
        assert!(result.is_none(), "no light source means no depletion");
    }

    #[test]
    fn deplete_light_empty_inventory_returns_none() {
        let mut inv = Inventory::default();
        let result = inv.deplete_light_on_transition();
        assert!(result.is_none());
    }

    #[test]
    fn deplete_light_uses_first_light_not_second() {
        let mut inv = Inventory::default();
        inv.add(torch(), 10).unwrap(); // light, 6 uses
        inv.add(lantern_oil(), 10).unwrap(); // light, 12 uses
        inv.deplete_light_on_transition();
        let t = inv.find("torch_1").expect("torch should exist");
        assert_eq!(t.uses_remaining, Some(5), "torch should be decremented");
        let l = inv.find("lantern_oil_1").expect("lantern oil should exist");
        assert_eq!(
            l.uses_remaining,
            Some(12),
            "lantern oil should be untouched"
        );
    }

    #[test]
    fn deplete_light_infinite_light_source_not_consumed() {
        let mut inv = Inventory::default();
        let mut magic_lamp = sword();
        magic_lamp.tags = vec!["light".to_string()];
        magic_lamp.uses_remaining = None; // infinite
        inv.add(magic_lamp, 10).unwrap();
        let result = inv.deplete_light_on_transition();
        assert!(
            result.is_none(),
            "infinite light source should not be consumed"
        );
        assert_eq!(inv.item_count(), 1);
    }

    // --- AC 5: torch with 6 uses survives 5 transitions, removed on 6th ---

    #[test]
    fn torch_survives_five_transitions_removed_on_sixth() {
        let mut inv = Inventory::default();
        inv.add(torch(), 10).unwrap(); // uses_remaining: Some(6)

        // Transitions 1-5: torch should survive
        for i in 1..=5 {
            let result = inv.deplete_light_on_transition();
            assert!(result.is_none(), "torch should survive transition {i}");
            let t = inv
                .find("torch_1")
                .expect("torch should exist after transition {i}");
            assert_eq!(
                t.uses_remaining,
                Some(6 - i),
                "uses_remaining should be {} after transition {i}",
                6 - i
            );
        }

        // Transition 6: torch should be removed
        let result = inv.deplete_light_on_transition();
        assert!(
            result.is_some(),
            "torch should be removed on 6th transition"
        );
        let removed = result.unwrap();
        assert_eq!(removed.id.as_str(), "torch_1");
        assert_eq!(removed.uses_remaining, Some(0));
        assert_eq!(inv.item_count(), 0, "inventory should be empty");
    }

    #[test]
    fn second_torch_takes_over_after_first_exhausted() {
        let mut inv = Inventory::default();
        let mut torch_1 = torch();
        torch_1.uses_remaining = Some(1); // about to die
        let mut torch_2 = torch();
        torch_2.id = NonBlankString::new("torch_2").unwrap();
        torch_2.uses_remaining = Some(6);
        inv.add(torch_1, 10).unwrap();
        inv.add(torch_2, 10).unwrap();

        // First transition: torch_1 exhausted, removed
        let result = inv.deplete_light_on_transition();
        assert!(result.is_some(), "first torch should be removed");
        assert_eq!(result.unwrap().id.as_str(), "torch_1");
        assert_eq!(inv.item_count(), 1, "only torch_2 remains");

        // Second transition: torch_2 decremented
        let result = inv.deplete_light_on_transition();
        assert!(result.is_none(), "second torch should survive");
        let t2 = inv.find("torch_2").expect("torch_2 should exist");
        assert_eq!(t2.uses_remaining, Some(5));
    }

    // --- Serde: uses_remaining persistence ---

    #[test]
    fn uses_remaining_serializes_when_some() {
        let t = torch();
        let json = serde_json::to_string(&t).unwrap();
        assert!(
            json.contains("\"uses_remaining\":6"),
            "uses_remaining should serialize as 6, got: {json}"
        );
    }

    #[test]
    fn uses_remaining_serializes_when_none() {
        let s = sword();
        let json = serde_json::to_string(&s).unwrap();
        assert!(
            json.contains("\"uses_remaining\":null"),
            "uses_remaining=None should serialize as null, got: {json}"
        );
    }

    #[test]
    fn uses_remaining_round_trips_some() {
        let t = torch();
        let json = serde_json::to_string(&t).unwrap();
        let back: Item = serde_json::from_str(&json).unwrap();
        assert_eq!(
            back.uses_remaining,
            Some(6),
            "uses_remaining should round-trip as Some(6)"
        );
    }

    #[test]
    fn uses_remaining_round_trips_none() {
        let s = sword();
        let json = serde_json::to_string(&s).unwrap();
        let back: Item = serde_json::from_str(&json).unwrap();
        assert_eq!(
            back.uses_remaining, None,
            "uses_remaining should round-trip as None"
        );
    }

    #[test]
    fn uses_remaining_defaults_to_none_when_missing_from_json() {
        // Legacy items without uses_remaining field should deserialize with None
        let json = r#"{"id":"old_sword","name":"Old Sword","description":"A rusty blade","category":"weapon","value":5,"weight":3.0,"rarity":"common","narrative_weight":0.3,"tags":["melee"],"equipped":false,"quantity":1}"#;
        let item: Item = serde_json::from_str(json).unwrap();
        assert_eq!(
            item.uses_remaining, None,
            "missing uses_remaining should default to None (infinite)"
        );
    }

    #[test]
    fn inventory_with_consumables_round_trips() {
        let mut inv = Inventory::default();
        inv.add(torch(), 10).unwrap();
        inv.add(sword(), 10).unwrap();
        inv.gold = 50;
        let json = serde_json::to_string(&inv).unwrap();
        let back: Inventory = serde_json::from_str(&json).unwrap();
        assert_eq!(back.item_count(), 2);
        assert_eq!(back.gold, 50);
        let t = back.find("torch_1").unwrap();
        assert_eq!(t.uses_remaining, Some(6));
        let s = back.find("sword_iron").unwrap();
        assert_eq!(s.uses_remaining, None);
    }

    // === ItemState ledger behavior ===

    #[test]
    fn transition_to_sold_stays_in_ledger() {
        let mut inv = Inventory::default();
        inv.add(sword(), 10).unwrap();
        inv.transition(
            "sword_iron",
            ItemState::Sold {
                to: "Patchwork".into(),
            },
        )
        .unwrap();
        assert_eq!(inv.item_count(), 0, "carried count should be 0");
        assert_eq!(inv.ledger_size(), 1, "item remains in ledger");
        assert!(
            inv.find("sword_iron").is_none(),
            "find only returns carried"
        );
        assert!(
            inv.find_any("sword_iron").is_some(),
            "find_any returns any state"
        );
    }

    #[test]
    fn transition_to_lost_is_recoverable() {
        let mut inv = Inventory::default();
        inv.add(sword(), 10).unwrap();
        inv.transition(
            "sword_iron",
            ItemState::Lost {
                reason: "stolen by Gutter Rats".into(),
            },
        )
        .unwrap();
        let recoverable = inv.recoverable();
        assert_eq!(recoverable.len(), 1);
        assert_eq!(recoverable[0].id.as_str(), "sword_iron");
    }

    #[test]
    fn transition_to_given_is_recoverable() {
        let mut inv = Inventory::default();
        inv.add(sword(), 10).unwrap();
        inv.transition(
            "sword_iron",
            ItemState::Given {
                to: "Shirley".into(),
            },
        )
        .unwrap();
        let recoverable = inv.recoverable();
        assert_eq!(recoverable.len(), 1);
    }

    #[test]
    fn transition_consumed_not_recoverable() {
        let mut inv = Inventory::default();
        inv.add(potion(), 10).unwrap();
        inv.transition("healing_potion", ItemState::Consumed)
            .unwrap();
        assert!(inv.recoverable().is_empty());
    }

    #[test]
    fn transition_unequips_item() {
        let mut inv = Inventory::default();
        inv.add(sword(), 10).unwrap(); // equipped = true
        inv.transition(
            "sword_iron",
            ItemState::Sold {
                to: "merchant".into(),
            },
        )
        .unwrap();
        let item = inv.find_any("sword_iron").unwrap();
        assert!(!item.equipped, "sold items should not remain equipped");
    }

    #[test]
    fn carry_limit_ignores_non_carried() {
        let mut inv = Inventory::default();
        inv.add(sword(), 1).unwrap();
        inv.transition(
            "sword_iron",
            ItemState::Sold {
                to: "merchant".into(),
            },
        )
        .unwrap();
        // Carry limit is 1, but the sword is sold — slot is free
        assert!(inv.add(potion(), 1).is_ok());
    }

    #[test]
    fn history_returns_non_carried() {
        let mut inv = Inventory::default();
        inv.add(sword(), 10).unwrap();
        inv.add(potion(), 10).unwrap();
        inv.transition(
            "sword_iron",
            ItemState::Destroyed {
                reason: "dragon fire".into(),
            },
        )
        .unwrap();
        let history = inv.history();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].id.as_str(), "sword_iron");
    }

    #[test]
    fn consume_use_transitions_to_consumed() {
        let mut inv = Inventory::default();
        let mut p = potion();
        p.uses_remaining = Some(1);
        inv.add(p, 10).unwrap();
        let exhausted = inv.consume_use("healing_potion");
        assert!(exhausted.is_some());
        let item = inv.find_any("healing_potion").unwrap();
        assert_eq!(item.state, ItemState::Consumed);
        assert_eq!(inv.item_count(), 0, "consumed item not in carried count");
        assert_eq!(inv.ledger_size(), 1, "consumed item stays in ledger");
    }

    #[test]
    fn item_state_serde_roundtrip() {
        let mut inv = Inventory::default();
        inv.add(sword(), 10).unwrap();
        inv.transition(
            "sword_iron",
            ItemState::Lost {
                reason: "fell into the void".into(),
            },
        )
        .unwrap();
        let json = serde_json::to_string(&inv).unwrap();
        let back: Inventory = serde_json::from_str(&json).unwrap();
        let item = back.find_any("sword_iron").unwrap();
        assert_eq!(
            item.state,
            ItemState::Lost {
                reason: "fell into the void".into()
            }
        );
    }

    #[test]
    fn default_item_state_is_carried() {
        let item = sword();
        assert_eq!(item.state, ItemState::Carried);
    }

    #[test]
    fn item_state_display() {
        assert_eq!(format!("{}", ItemState::Carried), "carried");
        assert_eq!(
            format!(
                "{}",
                ItemState::Sold {
                    to: "Patchwork".into()
                }
            ),
            "sold to Patchwork"
        );
        assert_eq!(
            format!(
                "{}",
                ItemState::Lost {
                    reason: "stolen".into()
                }
            ),
            "lost: stolen"
        );
    }

    // ── Gold spending tests ──────────────────────────────────────────────

    #[test]
    fn spend_gold_deducts_exact_amount() {
        let mut inv = Inventory {
            items: vec![],
            gold: 50,
        };
        let spent = inv.spend_gold(13);
        assert_eq!(spent, 13);
        assert_eq!(inv.gold, 37);
    }

    #[test]
    fn spend_gold_clamps_at_zero() {
        let mut inv = Inventory {
            items: vec![],
            gold: 10,
        };
        let spent = inv.spend_gold(13);
        assert_eq!(spent, 10, "should only spend what's available");
        assert_eq!(inv.gold, 0, "gold should be 0, not negative");
    }

    #[test]
    fn spend_gold_from_zero_spends_nothing() {
        let mut inv = Inventory {
            items: vec![],
            gold: 0,
        };
        let spent = inv.spend_gold(5);
        assert_eq!(spent, 0);
        assert_eq!(inv.gold, 0);
    }

    #[test]
    fn spend_gold_exact_balance() {
        let mut inv = Inventory {
            items: vec![],
            gold: 10,
        };
        let spent = inv.spend_gold(10);
        assert_eq!(spent, 10);
        assert_eq!(inv.gold, 0);
    }
}

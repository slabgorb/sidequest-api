//! Story 19-5: Consumable item depletion — uses_remaining on items, decrement on room transition
//!
//! Tests that items gain a `uses_remaining: Option<u32>` field, that `Inventory::consume_use()`
//! decrements and removes at 0, and that room transitions in RoomGraph mode auto-deplete
//! light sources.
//!
//! AC coverage:
//! - AC1: Item.uses_remaining field added, serialized/deserialized
//! - AC2: consume_use() decrements and removes at 0
//! - AC3: Room transition decrements active light source
//! - AC4: GameMessage fired on light exhaustion
//! - AC5: Torch with 6 uses survives 5 transitions, removed on 6th
//!
//! Rule coverage (rust-review-checklist):
//! - #6 test-quality: all assertions are meaningful (no vacuous tests)
//! - #8 serde-bypass: uses_remaining round-trips correctly through JSON
//! - #9 public-fields: uses_remaining on Item is public (acceptable — no invariant)

use sidequest_game::inventory::{Inventory, Item};
use sidequest_protocol::NonBlankString;

// ═══════════════════════════════════════════════════════════
// Test helpers
// ═══════════════════════════════════════════════════════════

/// Create a torch with the given `uses_remaining` value.
/// Tags include "light" so the depletion logic targets it.
fn make_torch(uses: u32) -> Item {
    Item {
        id: NonBlankString::new("torch").unwrap(),
        name: NonBlankString::new("Torch").unwrap(),
        description: NonBlankString::new("A flickering wooden torch").unwrap(),
        category: NonBlankString::new("consumable").unwrap(),
        value: 1,
        weight: 1.0,
        rarity: NonBlankString::new("common").unwrap(),
        narrative_weight: 0.1,
        tags: vec!["light".to_string()],
        equipped: true,
        quantity: 1,
        uses_remaining: Some(uses),
        state: sidequest_game::ItemState::Carried,
    }
}

/// Create a lantern with higher uses (oil-fed, longer lasting).
fn make_lantern(uses: u32) -> Item {
    Item {
        id: NonBlankString::new("lantern").unwrap(),
        name: NonBlankString::new("Oil Lantern").unwrap(),
        description: NonBlankString::new("A steady oil lantern").unwrap(),
        category: NonBlankString::new("consumable").unwrap(),
        value: 10,
        weight: 2.0,
        rarity: NonBlankString::new("common").unwrap(),
        narrative_weight: 0.2,
        tags: vec!["light".to_string()],
        equipped: true,
        quantity: 1,
        uses_remaining: Some(uses),
        state: sidequest_game::ItemState::Carried,
    }
}

/// Create a sword (no uses_remaining — infinite use, non-consumable).
fn make_sword() -> Item {
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
        state: sidequest_game::ItemState::Carried,
    }
}

/// Create a rope (has uses_remaining but no "light" tag — not depleted on transition).
fn make_rope(uses: u32) -> Item {
    Item {
        id: NonBlankString::new("rope_50ft").unwrap(),
        name: NonBlankString::new("50ft Rope").unwrap(),
        description: NonBlankString::new("Hempen rope, 50 feet").unwrap(),
        category: NonBlankString::new("tool").unwrap(),
        value: 5,
        weight: 5.0,
        rarity: NonBlankString::new("common").unwrap(),
        narrative_weight: 0.1,
        tags: vec!["utility".to_string()],
        equipped: false,
        quantity: 1,
        uses_remaining: Some(uses),
        state: sidequest_game::ItemState::Carried,
    }
}

// ═══════════════════════════════════════════════════════════
// AC1: Item.uses_remaining field added, serialized/deserialized
// ═══════════════════════════════════════════════════════════

/// Items without uses_remaining default to None (infinite use — backward compat).
#[test]
fn uses_remaining_defaults_to_none_for_non_consumables() {
    let sword = make_sword();
    assert_eq!(
        sword.uses_remaining, None,
        "Non-consumable items should have None uses_remaining"
    );
}

/// Consumable items can have a specific uses_remaining count.
#[test]
fn uses_remaining_set_on_consumable() {
    let torch = make_torch(6);
    assert_eq!(
        torch.uses_remaining,
        Some(6),
        "Torch should have 6 uses remaining"
    );
}

/// Serde round-trip preserves uses_remaining = Some(6).
#[test]
fn uses_remaining_serializes_with_value() {
    let torch = make_torch(6);
    let json = serde_json::to_string(&torch).unwrap();
    let back: Item = serde_json::from_str(&json).unwrap();
    assert_eq!(
        back.uses_remaining,
        Some(6),
        "uses_remaining should survive JSON round-trip"
    );
}

/// Serde round-trip preserves uses_remaining = None.
#[test]
fn uses_remaining_serializes_none() {
    let sword = make_sword();
    let json = serde_json::to_string(&sword).unwrap();
    let back: Item = serde_json::from_str(&json).unwrap();
    assert_eq!(
        back.uses_remaining, None,
        "None uses_remaining should survive JSON round-trip"
    );
}

/// Rule #8 (serde-bypass): JSON without uses_remaining field deserializes as None.
/// This ensures backward compatibility with existing save data that predates this field.
#[test]
fn missing_uses_remaining_in_json_deserializes_as_none() {
    // JSON that omits the uses_remaining field entirely (old save format)
    let json = r#"{
        "id": "sword_iron",
        "name": "Iron Sword",
        "description": "A sturdy iron blade",
        "category": "weapon",
        "value": 50,
        "weight": 3.0,
        "rarity": "common",
        "narrative_weight": 0.3,
        "tags": ["melee", "blade"],
        "equipped": true,
        "quantity": 1
    }"#;
    let item: Item = serde_json::from_str(json).unwrap();
    assert_eq!(
        item.uses_remaining, None,
        "Missing uses_remaining in JSON should deserialize as None (backward compat)"
    );
}

/// Inventory round-trip preserves uses_remaining across all items.
#[test]
fn inventory_roundtrip_preserves_uses_remaining() {
    let mut inv = Inventory::default();
    inv.add(make_torch(6), 10).unwrap();
    inv.add(make_sword(), 10).unwrap();

    let json = serde_json::to_string(&inv).unwrap();
    let back: Inventory = serde_json::from_str(&json).unwrap();

    assert_eq!(back.items[0].uses_remaining, Some(6), "Torch uses preserved");
    assert_eq!(back.items[1].uses_remaining, None, "Sword None preserved");
}

// ═══════════════════════════════════════════════════════════
// AC2: consume_use() decrements and removes at 0
// ═══════════════════════════════════════════════════════════

/// Decrementing an item with uses_remaining > 1 reduces by 1.
#[test]
fn consume_use_decrements_remaining() {
    let mut inv = Inventory::default();
    inv.add(make_torch(6), 10).unwrap();

    let result = inv.consume_use("torch");
    assert!(result.is_none(), "Item should NOT be removed when uses > 0");

    let torch = inv.find("torch").expect("Torch should still be in inventory");
    assert_eq!(
        torch.uses_remaining,
        Some(5),
        "uses_remaining should decrement from 6 to 5"
    );
}

/// Decrementing from 1 → 0 transitions the item to Consumed and returns it.
/// The item remains in the ledger but is no longer carried.
#[test]
fn consume_use_removes_at_zero() {
    let mut inv = Inventory::default();
    inv.add(make_torch(1), 10).unwrap();

    let removed = inv.consume_use("torch");
    assert!(removed.is_some(), "Item should be transitioned when uses hits 0");

    let removed_item = removed.unwrap();
    assert_eq!(removed_item.id.as_str(), "torch");
    assert_eq!(
        removed_item.uses_remaining,
        Some(0),
        "Consumed item should show 0 uses remaining"
    );
    assert_eq!(inv.item_count(), 0, "Torch should no longer be in carried count after depletion");
    assert_eq!(inv.ledger_size(), 1, "Torch stays in ledger as Consumed record");
    assert!(
        inv.find("torch").is_none(),
        "find() must not return consumed torch — only carried items"
    );
}

/// consume_use on an item with None uses_remaining (infinite) does nothing.
#[test]
fn consume_use_infinite_item_no_change() {
    let mut inv = Inventory::default();
    inv.add(make_sword(), 10).unwrap();

    let result = inv.consume_use("sword_iron");
    assert!(
        result.is_none(),
        "Infinite-use item should not be removed"
    );

    let sword = inv.find("sword_iron").expect("Sword should still exist");
    assert_eq!(
        sword.uses_remaining, None,
        "uses_remaining should remain None for infinite items"
    );
}

/// consume_use on a nonexistent item returns None (item not found).
#[test]
fn consume_use_nonexistent_returns_none() {
    let mut inv = Inventory::default();
    let result = inv.consume_use("nonexistent");
    assert!(result.is_none(), "Nonexistent item should return None");
}

/// Multiple decrements track correctly — 6, 5, 4, 3, 2, 1, removed.
#[test]
fn consume_use_progressive_decrement() {
    let mut inv = Inventory::default();
    inv.add(make_torch(3), 10).unwrap();

    // 3 → 2
    assert!(inv.consume_use("torch").is_none());
    assert_eq!(inv.find("torch").unwrap().uses_remaining, Some(2));

    // 2 → 1
    assert!(inv.consume_use("torch").is_none());
    assert_eq!(inv.find("torch").unwrap().uses_remaining, Some(1));

    // 1 → 0: transitions to Consumed
    let removed = inv.consume_use("torch");
    assert!(removed.is_some(), "Should be consumed at 0");
    assert_eq!(inv.item_count(), 0, "No longer carried");
    assert_eq!(inv.ledger_size(), 1, "Still in ledger as Consumed record");
    assert!(inv.find("torch").is_none(), "find() returns only carried items");
}

// ═══════════════════════════════════════════════════════════
// AC3: Room transition decrements active light source
// ═══════════════════════════════════════════════════════════

/// deplete_light_on_transition finds the first item with tag "light" and
/// calls consume_use. Non-light items are untouched.
#[test]
fn room_transition_depletes_first_light_source() {
    let mut inv = Inventory::default();
    inv.add(make_sword(), 10).unwrap();
    inv.add(make_torch(6), 10).unwrap();
    inv.add(make_rope(3), 10).unwrap();

    let depleted = inv.deplete_light_on_transition();

    // Torch should have been decremented
    assert!(depleted.is_none(), "Torch not yet exhausted");
    let torch = inv.find("torch").expect("Torch still in inventory");
    assert_eq!(torch.uses_remaining, Some(5), "Torch decremented to 5");

    // Sword and rope untouched
    let sword = inv.find("sword_iron").expect("Sword untouched");
    assert_eq!(sword.uses_remaining, None);
    let rope = inv.find("rope_50ft").expect("Rope untouched");
    assert_eq!(rope.uses_remaining, Some(3));
}

/// When multiple light sources exist, only the first is decremented.
#[test]
fn only_first_light_source_depleted() {
    let mut inv = Inventory::default();
    inv.add(make_torch(2), 10).unwrap();
    inv.add(make_lantern(10), 10).unwrap();

    let depleted = inv.deplete_light_on_transition();
    assert!(depleted.is_none(), "Torch not yet exhausted");

    let torch = inv.find("torch").expect("Torch exists");
    assert_eq!(torch.uses_remaining, Some(1), "Torch decremented");

    let lantern = inv.find("lantern").expect("Lantern exists");
    assert_eq!(lantern.uses_remaining, Some(10), "Lantern untouched");
}

/// No light sources → no depletion, no error.
#[test]
fn no_light_source_no_depletion() {
    let mut inv = Inventory::default();
    inv.add(make_sword(), 10).unwrap();
    inv.add(make_rope(3), 10).unwrap();

    let depleted = inv.deplete_light_on_transition();
    assert!(depleted.is_none(), "No light source = no depletion");
    assert_eq!(inv.item_count(), 2, "All items remain");
}

/// Empty inventory → no depletion.
#[test]
fn empty_inventory_no_depletion() {
    let mut inv = Inventory::default();
    let depleted = inv.deplete_light_on_transition();
    assert!(depleted.is_none(), "Empty inventory = no depletion");
}

// ═══════════════════════════════════════════════════════════
// AC4: GameMessage fired on light exhaustion
// ═══════════════════════════════════════════════════════════

/// When a light source is exhausted (uses_remaining hits 0),
/// deplete_light_on_transition returns the removed item so the
/// caller can fire a GameMessage. The returned item IS the signal.
#[test]
fn exhausted_light_returns_removed_item() {
    let mut inv = Inventory::default();
    inv.add(make_torch(1), 10).unwrap();

    let depleted = inv.deplete_light_on_transition();
    assert!(depleted.is_some(), "Exhausted light should return the item");

    let item = depleted.unwrap();
    assert_eq!(item.id.as_str(), "torch", "Should be the torch");
    assert_eq!(item.uses_remaining, Some(0), "Should show 0 uses");
}

/// Non-exhaustion transition returns None (no message needed).
#[test]
fn non_exhaustion_returns_none() {
    let mut inv = Inventory::default();
    inv.add(make_torch(6), 10).unwrap();

    let depleted = inv.deplete_light_on_transition();
    assert!(depleted.is_none(), "Non-exhaustion should return None");
}

// ═══════════════════════════════════════════════════════════
// AC5: Torch with 6 uses survives 5 transitions, removed on 6th
// ═══════════════════════════════════════════════════════════

/// The canonical acceptance test: a torch with 6 uses survives exactly
/// 5 room transitions and is removed on the 6th.
#[test]
fn torch_six_uses_survives_five_transitions_removed_on_sixth() {
    let mut inv = Inventory::default();
    inv.add(make_torch(6), 10).unwrap();

    // Transitions 1–5: torch depletes but survives
    for i in 1..=5 {
        let depleted = inv.deplete_light_on_transition();
        assert!(
            depleted.is_none(),
            "Torch should survive transition {i} (uses: {})",
            6 - i
        );
        let torch = inv.find("torch").expect("Torch should still exist");
        assert_eq!(
            torch.uses_remaining,
            Some(6 - i),
            "After transition {i}, uses should be {}",
            6 - i
        );
    }

    // Transition 6: torch exhausted
    let depleted = inv.deplete_light_on_transition();
    assert!(
        depleted.is_some(),
        "Torch should be exhausted on transition 6"
    );
    assert!(
        inv.find("torch").is_none(),
        "Torch should be gone from inventory after exhaustion"
    );
    assert_eq!(
        depleted.unwrap().uses_remaining,
        Some(0),
        "Exhausted torch should report 0 uses"
    );
}

// ═══════════════════════════════════════════════════════════
// Edge cases — the Brute Squad stress tests
// ═══════════════════════════════════════════════════════════

/// After first light source exhausted, the next light source takes over.
#[test]
fn second_light_takes_over_after_first_exhausted() {
    let mut inv = Inventory::default();
    inv.add(make_torch(1), 10).unwrap();
    inv.add(make_lantern(10), 10).unwrap();

    // First transition: torch exhausted
    let depleted = inv.deplete_light_on_transition();
    assert!(depleted.is_some(), "Torch should be exhausted");

    // Second transition: lantern now takes over
    let depleted = inv.deplete_light_on_transition();
    assert!(depleted.is_none(), "Lantern should survive");
    let lantern = inv.find("lantern").expect("Lantern still exists");
    assert_eq!(lantern.uses_remaining, Some(9), "Lantern decremented to 9");
}

/// Item with uses_remaining = 0 should NOT be in inventory in the first place,
/// but if somehow constructed, consume_use should still remove it.
#[test]
fn zero_uses_remaining_removed_immediately() {
    let mut inv = Inventory::default();
    inv.add(make_torch(0), 10).unwrap();

    // Even though torch has 0 uses, calling consume_use should handle gracefully
    let result = inv.consume_use("torch");
    // Whether it removes immediately or was never valid — assert it's gone
    assert!(
        inv.find("torch").is_none() || result.is_some(),
        "Zero-uses item should be removed on next consume_use"
    );
}

/// Depleting a light source doesn't affect item count for non-light items.
#[test]
fn depletion_preserves_other_items() {
    let mut inv = Inventory::default();
    inv.add(make_torch(1), 10).unwrap();
    inv.add(make_sword(), 10).unwrap();
    inv.add(make_rope(5), 10).unwrap();
    assert_eq!(inv.item_count(), 3);

    // Exhaust torch
    let depleted = inv.deplete_light_on_transition();
    assert!(depleted.is_some());
    assert_eq!(inv.item_count(), 2, "Only torch removed; sword + rope remain");
    assert!(inv.find("sword_iron").is_some());
    assert!(inv.find("rope_50ft").is_some());
}

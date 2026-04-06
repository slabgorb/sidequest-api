//! Story 19-7: Weight-based encumbrance — RED phase tests
//!
//! Tests the weight-based inventory system:
//!   AC-1: total_weight() sums all carried item weights × quantities
//!   AC-2: CarryMode::Weight rejects over-limit adds
//!   AC-3: is_overencumbered() returns true when at/over limit
//!   AC-4: Trope multiplier increased when overencumbered (1.5x stacking)
//!   AC-5: Existing count-based carry unaffected
//!   AC-6: Boundary conditions (exact limit, zero weight, empty inventory)
//!
//! Also tests:
//!   - CarryMode enum serde roundtrip
//!   - InventoryError::Overweight variant
//!   - InventoryPhilosophy weight_limit + carry_mode fields

use sidequest_game::inventory::{Inventory, InventoryError, Item, ItemState};
use sidequest_protocol::NonBlankString;

// ============================================================================
// Test helpers
// ============================================================================

fn item_with_weight(id: &str, name: &str, weight: f64) -> Item {
    Item {
        id: NonBlankString::new(id).unwrap(),
        name: NonBlankString::new(name).unwrap(),
        description: NonBlankString::new("test item").unwrap(),
        category: NonBlankString::new("misc").unwrap(),
        value: 10,
        weight,
        rarity: NonBlankString::new("common").unwrap(),
        narrative_weight: 0.3,
        tags: vec![],
        equipped: false,
        quantity: 1,
        uses_remaining: None,
        state: ItemState::Carried,
    }
}

fn heavy_item(id: &str, weight: f64) -> Item {
    item_with_weight(id, &format!("Heavy {}", id), weight)
}

fn stacked_item(id: &str, weight: f64, quantity: u32) -> Item {
    let mut item = item_with_weight(id, &format!("Stack {}", id), weight);
    item.quantity = quantity;
    item
}

// ============================================================================
// AC-1: total_weight() sums carried item weights × quantities
// ============================================================================

#[test]
fn total_weight_empty_inventory_is_zero() {
    let inv = Inventory::default();
    assert!((inv.total_weight() - 0.0).abs() < f64::EPSILON,
        "empty inventory should have zero weight");
}

#[test]
fn total_weight_single_item() {
    let mut inv = Inventory::default();
    inv.add(item_with_weight("sword", "Sword", 3.0), 10).unwrap();
    assert!((inv.total_weight() - 3.0).abs() < f64::EPSILON,
        "single 3.0 weight item should total 3.0");
}

#[test]
fn total_weight_multiple_items() {
    let mut inv = Inventory::default();
    inv.add(item_with_weight("sword", "Sword", 3.0), 10).unwrap();
    inv.add(item_with_weight("shield", "Shield", 5.0), 10).unwrap();
    inv.add(item_with_weight("potion", "Potion", 0.5), 10).unwrap();
    assert!((inv.total_weight() - 8.5).abs() < f64::EPSILON,
        "3.0 + 5.0 + 0.5 = 8.5");
}

#[test]
fn total_weight_respects_quantity() {
    let mut inv = Inventory::default();
    inv.add(stacked_item("arrows", 0.1, 20), 10).unwrap();
    assert!((inv.total_weight() - 2.0).abs() < f64::EPSILON,
        "20 arrows at 0.1 each = 2.0 total weight");
}

#[test]
fn total_weight_ignores_non_carried_items() {
    let mut inv = Inventory::default();
    inv.add(item_with_weight("sword", "Sword", 3.0), 10).unwrap();
    inv.add(item_with_weight("shield", "Shield", 5.0), 10).unwrap();
    inv.transition("sword", ItemState::Sold { to: "merchant".into() }).unwrap();
    assert!((inv.total_weight() - 5.0).abs() < f64::EPSILON,
        "sold sword should not count toward weight");
}

#[test]
fn total_weight_zero_weight_items_contribute_nothing() {
    let mut inv = Inventory::default();
    inv.add(item_with_weight("note", "Note", 0.0), 10).unwrap();
    inv.add(item_with_weight("sword", "Sword", 3.0), 10).unwrap();
    assert!((inv.total_weight() - 3.0).abs() < f64::EPSILON,
        "zero-weight items should not affect total");
}

// ============================================================================
// AC-2: CarryMode::Weight rejects over-limit adds
// ============================================================================

#[test]
fn weight_add_rejects_when_exceeds_limit() {
    let mut inv = Inventory::default();
    inv.add(heavy_item("anvil", 90.0), 100).unwrap();
    // Total weight is 90.0, weight_limit is 100.0, adding 15.0 would exceed
    let result = inv.add_weighted(heavy_item("boulder", 15.0), 100.0);
    assert!(result.is_err(), "adding item that exceeds weight limit should fail");
    match result.unwrap_err() {
        InventoryError::Overweight { current_weight, item_weight, limit } => {
            assert!((current_weight - 90.0).abs() < f64::EPSILON);
            assert!((item_weight - 15.0).abs() < f64::EPSILON);
            assert!((limit - 100.0).abs() < f64::EPSILON);
        }
        other => panic!("expected Overweight error, got: {:?}", other),
    }
}

#[test]
fn weight_add_accepts_when_under_limit() {
    let mut inv = Inventory::default();
    inv.add(heavy_item("sword", 3.0), 100).unwrap();
    let result = inv.add_weighted(heavy_item("shield", 5.0), 100.0);
    assert!(result.is_ok(), "adding item under weight limit should succeed");
    assert_eq!(inv.item_count(), 2);
}

#[test]
fn weight_add_accepts_exactly_at_limit() {
    let mut inv = Inventory::default();
    inv.add(heavy_item("armor", 95.0), 100).unwrap();
    // Adding exactly 5.0 to reach 100.0 — should succeed (at limit, not over)
    let result = inv.add_weighted(heavy_item("helmet", 5.0), 100.0);
    assert!(result.is_ok(), "adding item to reach exactly weight limit should succeed");
}

#[test]
fn weight_add_rejects_single_item_over_limit() {
    let inv = Inventory::default();
    // Empty inventory, but single item weighs more than limit
    let mut inv = inv;
    let result = inv.add_weighted(heavy_item("boulder", 150.0), 100.0);
    assert!(result.is_err(), "single item exceeding limit should be rejected");
}

#[test]
fn weight_add_considers_quantity_for_stacked_items() {
    let mut inv = Inventory::default();
    // 10 arrows at 1.0 each = 10.0 total for this stack
    let result = inv.add_weighted(stacked_item("arrows", 1.0, 10), 8.0);
    assert!(result.is_err(),
        "stacked item with total weight 10.0 should be rejected at limit 8.0");
}

// ============================================================================
// AC-3: is_overencumbered() returns true when at/over limit
// ============================================================================

#[test]
fn is_overencumbered_under_limit() {
    let mut inv = Inventory::default();
    inv.add(heavy_item("sword", 3.0), 100).unwrap();
    assert!(!inv.is_overencumbered(100.0),
        "3.0 / 100.0 should not be overencumbered");
}

#[test]
fn is_overencumbered_at_limit() {
    let mut inv = Inventory::default();
    inv.add(heavy_item("armor", 100.0), 100).unwrap();
    assert!(inv.is_overencumbered(100.0),
        "exactly at weight limit should be overencumbered");
}

#[test]
fn is_overencumbered_over_limit() {
    // Edge case: could happen if items were added via count-based mode
    // then switched to weight mode
    let mut inv = Inventory::default();
    inv.add(heavy_item("anvil", 60.0), 100).unwrap();
    inv.add(heavy_item("boulder", 50.0), 100).unwrap();
    assert!(inv.is_overencumbered(100.0),
        "110.0 / 100.0 should be overencumbered");
}

#[test]
fn is_overencumbered_empty_inventory() {
    let inv = Inventory::default();
    assert!(!inv.is_overencumbered(100.0),
        "empty inventory should not be overencumbered");
}

#[test]
fn is_overencumbered_zero_limit() {
    let inv = Inventory::default();
    // Zero limit means ANY weight would be over — but empty inventory is 0.0
    // which equals the limit, so it IS overencumbered
    assert!(inv.is_overencumbered(0.0),
        "zero limit should be overencumbered even when empty (0.0 >= 0.0)");
}

// ============================================================================
// AC-4: Trope multiplier stacking when overencumbered (1.5x)
// ============================================================================

/// The overencumbered multiplier should be 1.5x, composable with other multipliers.
/// When used in tick_room_transition, the final multiplier is:
///   room.keeper_awareness_modifier * overencumbered_multiplier
#[test]
fn overencumbered_multiplier_is_1_5x() {
    // Inventory.encumbrance_multiplier(weight_limit) returns 1.5 when overencumbered
    let mut inv = Inventory::default();
    inv.add(heavy_item("anvil", 100.0), 100).unwrap();
    let mult = inv.encumbrance_multiplier(100.0);
    assert!((mult - 1.5).abs() < f64::EPSILON,
        "overencumbered multiplier should be 1.5, got {}", mult);
}

#[test]
fn not_overencumbered_multiplier_is_1_0() {
    let mut inv = Inventory::default();
    inv.add(heavy_item("sword", 3.0), 100).unwrap();
    let mult = inv.encumbrance_multiplier(100.0);
    assert!((mult - 1.0).abs() < f64::EPSILON,
        "not overencumbered should have 1.0 multiplier, got {}", mult);
}

#[test]
fn empty_inventory_multiplier_is_1_0() {
    let inv = Inventory::default();
    let mult = inv.encumbrance_multiplier(100.0);
    assert!((mult - 1.0).abs() < f64::EPSILON,
        "empty inventory should have 1.0 multiplier");
}

// ============================================================================
// AC-5: Existing count-based carry unaffected
// ============================================================================

#[test]
fn count_based_add_still_works() {
    // The existing add() with carry_limit (count) should be unchanged
    let mut inv = Inventory::default();
    let result = inv.add(heavy_item("anvil", 100.0), 10);
    assert!(result.is_ok(),
        "count-based add should not check weight — heavy item fits in count limit");
}

#[test]
fn count_based_add_at_capacity_still_rejects() {
    let mut inv = Inventory::default();
    inv.add(heavy_item("item1", 1.0), 1).unwrap();
    let result = inv.add(heavy_item("item2", 1.0), 1);
    assert!(result.is_err(), "count-based capacity should still reject");
    match result.unwrap_err() {
        InventoryError::Full { .. } => {},
        other => panic!("expected Full error for count limit, got: {:?}", other),
    }
}

// ============================================================================
// CarryMode enum
// ============================================================================

#[test]
fn carry_mode_count_serde_roundtrip() {
    use sidequest_game::inventory::CarryMode;
    let mode = CarryMode::Count;
    let json = serde_json::to_string(&mode).unwrap();
    let back: CarryMode = serde_json::from_str(&json).unwrap();
    assert_eq!(back, CarryMode::Count);
}

#[test]
fn carry_mode_weight_serde_roundtrip() {
    use sidequest_game::inventory::CarryMode;
    let mode = CarryMode::Weight;
    let json = serde_json::to_string(&mode).unwrap();
    let back: CarryMode = serde_json::from_str(&json).unwrap();
    assert_eq!(back, CarryMode::Weight);
}

#[test]
fn carry_mode_default_is_count() {
    use sidequest_game::inventory::CarryMode;
    assert_eq!(CarryMode::default(), CarryMode::Count,
        "default carry mode should be Count for backwards compatibility");
}

// ============================================================================
// InventoryError::Overweight variant
// ============================================================================

#[test]
fn overweight_error_display() {
    let err = InventoryError::Overweight {
        current_weight: 90.0,
        item_weight: 15.0,
        limit: 100.0,
    };
    let msg = format!("{}", err);
    assert!(msg.contains("90"), "error message should include current weight");
    assert!(msg.contains("15"), "error message should include item weight");
    assert!(msg.contains("100"), "error message should include limit");
}

// ============================================================================
// Genre config: CarryMode + weight_limit on InventoryPhilosophy
// ============================================================================

#[test]
fn inventory_philosophy_with_weight_mode_deserializes() {
    use sidequest_genre::models::inventory::InventoryPhilosophy;
    use sidequest_game::inventory::CarryMode;
    let yaml = r#"
carry_limit: 20
carry_mode: Weight
weight_limit: 100.0
"#;
    let phil: InventoryPhilosophy = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(phil.carry_limit, Some(20));
    assert_eq!(phil.carry_mode, CarryMode::Weight);
    assert!((phil.weight_limit.unwrap() - 100.0).abs() < f64::EPSILON);
}

#[test]
fn inventory_philosophy_defaults_to_count_mode() {
    use sidequest_genre::models::inventory::InventoryPhilosophy;
    use sidequest_game::inventory::CarryMode;
    let yaml = r#"
carry_limit: 20
"#;
    let phil: InventoryPhilosophy = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(phil.carry_mode, CarryMode::Count,
        "missing carry_mode should default to Count");
    assert!(phil.weight_limit.is_none(),
        "missing weight_limit should be None");
}

// ============================================================================
// Wiring: total_weight is reachable from server crate
// ============================================================================

#[test]
fn total_weight_is_exported() {
    // Compile-time check: total_weight method exists on Inventory
    let _: fn(&Inventory) -> f64 = Inventory::total_weight;
}

#[test]
fn add_weighted_is_exported() {
    // Compile-time check: add_weighted method exists on Inventory
    let _: fn(&mut Inventory, Item, f64) -> Result<(), InventoryError> = Inventory::add_weighted;
}

#[test]
fn is_overencumbered_is_exported() {
    // Compile-time check: is_overencumbered method exists on Inventory
    let _: fn(&Inventory, f64) -> bool = Inventory::is_overencumbered;
}

#[test]
fn encumbrance_multiplier_is_exported() {
    // Compile-time check: encumbrance_multiplier method exists on Inventory
    let _: fn(&Inventory, f64) -> f64 = Inventory::encumbrance_multiplier;
}

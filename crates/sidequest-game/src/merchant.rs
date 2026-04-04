//! Merchant system — disposition-based pricing and buy/sell transactions.
//!
//! NPC merchants price items based on their disposition toward the buyer/seller.
//! Friendly NPCs give discounts; hostile NPCs charge more.

use serde::{Deserialize, Serialize};

use crate::disposition::Disposition;
use crate::inventory::{Inventory, InventoryError};

/// Whether the transaction is a purchase or sale (from the player's perspective).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransactionType {
    /// Player buys from merchant.
    Buy,
    /// Player sells to merchant.
    Sell,
}

/// Record of a completed merchant transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerchantTransaction {
    /// Buy or sell.
    pub transaction_type: TransactionType,
    /// Name of the item transacted.
    pub item_name: String,
    /// Final price in gold.
    pub price: u32,
    /// Who paid gold.
    pub buyer: String,
    /// Who received gold.
    pub seller: String,
}

/// Errors that can occur during merchant transactions.
#[derive(Debug, Clone, thiserror::Error)]
pub enum MerchantError {
    /// The named character was not found.
    #[error("character not found: {0}")]
    CharacterNotFound(String),
    /// Buyer doesn't have enough gold.
    #[error("insufficient gold: have {have}, need {need}")]
    InsufficientGold {
        /// Gold the buyer has.
        have: i64,
        /// Gold required.
        need: u32,
    },
    /// The item was not found in the seller's inventory.
    #[error("item not found in inventory: {0}")]
    ItemNotFound(String),
    /// Inventory operation failed (e.g., buyer inventory full).
    #[error("inventory error: {0}")]
    InventoryFull(#[from] InventoryError),
}

/// A transaction request from the narrator — lightweight, without price.
///
/// The narrator outputs this when a buy/sell occurs. The engine resolves the
/// price mechanically using the merchant NPC's disposition via `calculate_price`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerchantTransactionRequest {
    /// Buy or sell (from the player's perspective).
    pub transaction_type: TransactionType,
    /// Item ID to transact (matches `Item.id`).
    pub item_id: String,
    /// Name of the merchant NPC (matches `Npc.name()`).
    pub merchant_name: String,
}

/// Calculate the transaction price given base value, disposition, and direction.
///
/// Disposition pricing formula:
/// - modifier = clamp(disposition / 100.0, -0.5, 0.5)
/// - Buy price = base_value * (1.0 - modifier) — positive disposition = discount
/// - Sell price = base_value * (0.5 + modifier * 0.5) — base 50% of value
/// - Final price = max(1, round(result))
pub fn calculate_price(base_value: u32, disposition: &Disposition, is_buying: bool) -> u32 {
    let modifier = (disposition.value() as f64 / 100.0).clamp(-0.5, 0.5);
    let raw = if is_buying {
        base_value as f64 * (1.0 - modifier)
    } else {
        base_value as f64 * (0.5 + modifier * 0.5)
    };
    (raw.round() as u32).max(1)
}

/// Execute a buy transaction: player buys an item from the merchant.
///
/// Validation order: (1) gold sufficiency, (2) item exists in merchant inventory.
/// Atomic — either everything succeeds or nothing changes.
pub fn execute_buy(
    buyer_inventory: &mut Inventory,
    seller_inventory: &mut Inventory,
    item_id: &str,
    disposition: &Disposition,
    carry_limit: usize,
) -> Result<MerchantTransaction, MerchantError> {
    // Find the item in the seller's inventory to get its value and name
    let item = seller_inventory
        .find(item_id)
        .ok_or_else(|| MerchantError::ItemNotFound(item_id.to_string()))?;

    let price = calculate_price(item.value as u32, disposition, true);
    let item_name = item.name.as_str().to_string();

    // Check gold sufficiency
    if buyer_inventory.gold < price as i64 {
        return Err(MerchantError::InsufficientGold {
            have: buyer_inventory.gold,
            need: price,
        });
    }

    // Remove item from seller (validates it exists)
    let item = seller_inventory.remove(item_id)?;

    // Try to add to buyer inventory — if this fails, put it back
    if let Err(e) = buyer_inventory.add(item.clone(), carry_limit) {
        // Rollback: put item back in seller inventory (use a large limit since it was just there)
        let _ = seller_inventory.add(item, usize::MAX);
        return Err(MerchantError::InventoryFull(e));
    }

    // Transfer gold
    buyer_inventory.gold -= price as i64;
    seller_inventory.gold += price as i64;

    Ok(MerchantTransaction {
        transaction_type: TransactionType::Buy,
        item_name,
        price,
        buyer: "player".to_string(),
        seller: "merchant".to_string(),
    })
}

/// Execute a sell transaction: player sells an item to the merchant.
///
/// Validation order: (1) item exists in player inventory.
/// No gold check on merchant side — merchants always have enough gold.
pub fn execute_sell(
    seller_inventory: &mut Inventory,
    buyer_inventory: &mut Inventory,
    item_id: &str,
    disposition: &Disposition,
) -> Result<MerchantTransaction, MerchantError> {
    // Find the item in the seller's (player's) inventory
    let item = seller_inventory
        .find(item_id)
        .ok_or_else(|| MerchantError::ItemNotFound(item_id.to_string()))?;

    let price = calculate_price(item.value as u32, disposition, false);
    let item_name = item.name.as_str().to_string();

    // Remove from player
    let item = seller_inventory.remove(item_id)?;

    // Add to merchant (merchants have unlimited carry)
    let _ = buyer_inventory.add(item, usize::MAX);

    // Transfer gold
    seller_inventory.gold += price as i64;
    buyer_inventory.gold -= price as i64;

    Ok(MerchantTransaction {
        transaction_type: TransactionType::Sell,
        item_name,
        price,
        buyer: "merchant".to_string(),
        seller: "player".to_string(),
    })
}

/// Format merchant context for NPC agent prompt injection.
///
/// Produces a text block describing available items and their prices
/// based on the merchant's disposition toward the player.
pub fn format_merchant_context(
    merchant_name: &str,
    inventory: &Inventory,
    disposition: &Disposition,
) -> String {
    if inventory.items.is_empty() {
        return format!("{merchant_name} has nothing for sale.");
    }

    let mut lines = vec![format!("{merchant_name}'s wares:")];
    for item in &inventory.items {
        let buy_price = calculate_price(item.value as u32, disposition, true);
        lines.push(format!("  - {} ({}) — {} gold", item.name, item.category, buy_price));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::Item;
    use sidequest_protocol::NonBlankString;

    fn test_item(id: &str, name: &str, value: i32) -> Item {
        Item {
            id: NonBlankString::new(id).unwrap(),
            name: NonBlankString::new(name).unwrap(),
            description: NonBlankString::new("A test item").unwrap(),
            category: NonBlankString::new("weapon").unwrap(),
            value,
            weight: 1.0,
            rarity: NonBlankString::new("common").unwrap(),
            narrative_weight: 0.3,
            tags: vec![],
            equipped: false,
            quantity: 1,
            uses_remaining: None,
            state: crate::inventory::ItemState::Carried,
        }
    }

    // === Pricing formula ===

    #[test]
    fn neutral_buy_price_equals_base() {
        let d = Disposition::new(0);
        assert_eq!(calculate_price(100, &d, true), 100);
    }

    #[test]
    fn neutral_sell_price_is_half() {
        let d = Disposition::new(0);
        assert_eq!(calculate_price(100, &d, false), 50);
    }

    #[test]
    fn friendly_buy_discount() {
        // disposition 50 → modifier 0.5 → buy price = 100 * (1.0 - 0.5) = 50
        let d = Disposition::new(50);
        assert_eq!(calculate_price(100, &d, true), 50);
    }

    #[test]
    fn friendly_sell_bonus() {
        // disposition 50 → modifier 0.5 → sell price = 100 * (0.5 + 0.25) = 75
        let d = Disposition::new(50);
        assert_eq!(calculate_price(100, &d, false), 75);
    }

    #[test]
    fn hostile_buy_markup() {
        // disposition -50 → modifier -0.5 → buy price = 100 * (1.0 + 0.5) = 150
        let d = Disposition::new(-50);
        assert_eq!(calculate_price(100, &d, true), 150);
    }

    #[test]
    fn hostile_sell_penalty() {
        // disposition -50 → modifier -0.5 → sell price = 100 * (0.5 - 0.25) = 25
        let d = Disposition::new(-50);
        assert_eq!(calculate_price(100, &d, false), 25);
    }

    #[test]
    fn modifier_clamped_at_extremes() {
        // disposition 200 → clamped to 0.5
        let d = Disposition::new(200);
        assert_eq!(calculate_price(100, &d, true), 50); // max discount
        assert_eq!(calculate_price(100, &d, false), 75); // max sell bonus

        // disposition -200 → clamped to -0.5
        let d = Disposition::new(-200);
        assert_eq!(calculate_price(100, &d, true), 150); // max markup
        assert_eq!(calculate_price(100, &d, false), 25); // max sell penalty
    }

    #[test]
    fn minimum_price_is_one() {
        // Even with extreme discount, price floors at 1
        let d = Disposition::new(50);
        assert_eq!(calculate_price(1, &d, true), 1);
    }

    #[test]
    fn price_rounds_correctly() {
        // disposition 15 → modifier 0.15 → buy = 100 * 0.85 = 85
        let d = Disposition::new(15);
        assert_eq!(calculate_price(100, &d, true), 85);

        // sell = 100 * (0.5 + 0.075) ≈ 57.5 (57.499... due to float) → rounds to 57
        assert_eq!(calculate_price(100, &d, false), 57);
    }

    // === Execute buy ===

    #[test]
    fn buy_success() {
        let mut buyer = Inventory::default();
        buyer.gold = 200;
        let mut seller = Inventory::default();
        seller.add(test_item("sword", "Iron Sword", 100), 100).unwrap();

        let d = Disposition::new(0);
        let tx = execute_buy(&mut buyer, &mut seller, "sword", &d, 10).unwrap();

        assert_eq!(tx.transaction_type, TransactionType::Buy);
        assert_eq!(tx.item_name, "Iron Sword");
        assert_eq!(tx.price, 100);
        assert_eq!(buyer.gold, 100);
        assert_eq!(seller.gold, 100);
        assert_eq!(buyer.item_count(), 1);
        assert_eq!(seller.item_count(), 0);
    }

    #[test]
    fn buy_insufficient_gold() {
        let mut buyer = Inventory::default();
        buyer.gold = 10;
        let mut seller = Inventory::default();
        seller.add(test_item("sword", "Iron Sword", 100), 100).unwrap();

        let d = Disposition::new(0);
        let err = execute_buy(&mut buyer, &mut seller, "sword", &d, 10).unwrap_err();

        match err {
            MerchantError::InsufficientGold { have, need } => {
                assert_eq!(have, 10);
                assert_eq!(need, 100);
            }
            _ => panic!("expected InsufficientGold, got: {err}"),
        }

        // Nothing changed
        assert_eq!(buyer.gold, 10);
        assert_eq!(seller.item_count(), 1);
    }

    #[test]
    fn buy_item_not_found() {
        let mut buyer = Inventory::default();
        buyer.gold = 200;
        let mut seller = Inventory::default();

        let d = Disposition::new(0);
        let err = execute_buy(&mut buyer, &mut seller, "nonexistent", &d, 10).unwrap_err();

        assert!(matches!(err, MerchantError::ItemNotFound(_)));
    }

    #[test]
    fn buy_inventory_full() {
        let mut buyer = Inventory::default();
        buyer.gold = 200;
        buyer.add(test_item("existing", "Existing Item", 10), 1).unwrap();

        let mut seller = Inventory::default();
        seller.add(test_item("sword", "Iron Sword", 100), 100).unwrap();

        let d = Disposition::new(0);
        let err = execute_buy(&mut buyer, &mut seller, "sword", &d, 1).unwrap_err();

        assert!(matches!(err, MerchantError::InventoryFull(_)));

        // Atomic: nothing changed
        assert_eq!(buyer.gold, 200);
        assert_eq!(seller.item_count(), 1);
        assert_eq!(buyer.item_count(), 1);
    }

    #[test]
    fn buy_with_friendly_discount() {
        let mut buyer = Inventory::default();
        buyer.gold = 200;
        let mut seller = Inventory::default();
        seller.add(test_item("sword", "Iron Sword", 100), 100).unwrap();

        let d = Disposition::new(50); // max friendly → 50% discount
        let tx = execute_buy(&mut buyer, &mut seller, "sword", &d, 10).unwrap();

        assert_eq!(tx.price, 50);
        assert_eq!(buyer.gold, 150);
    }

    #[test]
    fn buy_with_hostile_markup() {
        let mut buyer = Inventory::default();
        buyer.gold = 200;
        let mut seller = Inventory::default();
        seller.add(test_item("sword", "Iron Sword", 100), 100).unwrap();

        let d = Disposition::new(-50); // max hostile → 50% markup
        let tx = execute_buy(&mut buyer, &mut seller, "sword", &d, 10).unwrap();

        assert_eq!(tx.price, 150);
        assert_eq!(buyer.gold, 50);
    }

    // === Execute sell ===

    #[test]
    fn sell_success() {
        let mut player = Inventory::default();
        player.add(test_item("sword", "Iron Sword", 100), 100).unwrap();

        let mut merchant = Inventory::default();
        merchant.gold = 500;

        let d = Disposition::new(0);
        let tx = execute_sell(&mut player, &mut merchant, "sword", &d).unwrap();

        assert_eq!(tx.transaction_type, TransactionType::Sell);
        assert_eq!(tx.item_name, "Iron Sword");
        assert_eq!(tx.price, 50); // neutral sell = 50%
        assert_eq!(player.gold, 50);
        assert_eq!(merchant.gold, 450);
        assert_eq!(player.item_count(), 0);
        assert_eq!(merchant.item_count(), 1);
    }

    #[test]
    fn sell_item_not_found() {
        let mut player = Inventory::default();
        let mut merchant = Inventory::default();

        let d = Disposition::new(0);
        let err = execute_sell(&mut player, &mut merchant, "nonexistent", &d).unwrap_err();

        assert!(matches!(err, MerchantError::ItemNotFound(_)));
    }

    #[test]
    fn sell_friendly_bonus() {
        let mut player = Inventory::default();
        player.add(test_item("sword", "Iron Sword", 100), 100).unwrap();

        let mut merchant = Inventory::default();
        merchant.gold = 500;

        let d = Disposition::new(50); // max friendly → sell price = 75
        let tx = execute_sell(&mut player, &mut merchant, "sword", &d).unwrap();

        assert_eq!(tx.price, 75);
    }

    // === Format merchant context ===

    #[test]
    fn format_empty_merchant() {
        let inv = Inventory::default();
        let d = Disposition::new(0);
        let ctx = format_merchant_context("Gruk", &inv, &d);
        assert_eq!(ctx, "Gruk has nothing for sale.");
    }

    #[test]
    fn format_merchant_with_items() {
        let mut inv = Inventory::default();
        inv.add(test_item("sword", "Iron Sword", 100), 10).unwrap();
        inv.add(test_item("potion", "Health Potion", 50), 10).unwrap();

        let d = Disposition::new(0);
        let ctx = format_merchant_context("Gruk", &inv, &d);

        assert!(ctx.contains("Gruk's wares:"));
        assert!(ctx.contains("Iron Sword"));
        assert!(ctx.contains("100 gold"));
        assert!(ctx.contains("Health Potion"));
        assert!(ctx.contains("50 gold"));
    }

    #[test]
    fn format_merchant_applies_disposition() {
        let mut inv = Inventory::default();
        inv.add(test_item("sword", "Iron Sword", 100), 10).unwrap();

        let d = Disposition::new(50); // friendly → 50% discount
        let ctx = format_merchant_context("Gruk", &inv, &d);
        assert!(ctx.contains("50 gold"));
    }

    // === Serde round-trip ===

    #[test]
    fn transaction_type_serde() {
        let json = serde_json::to_string(&TransactionType::Buy).unwrap();
        assert_eq!(json, r#""buy""#);
        let back: TransactionType = serde_json::from_str(&json).unwrap();
        assert_eq!(back, TransactionType::Buy);
    }

    #[test]
    fn merchant_transaction_serde() {
        let tx = MerchantTransaction {
            transaction_type: TransactionType::Buy,
            item_name: "Iron Sword".to_string(),
            price: 100,
            buyer: "player".to_string(),
            seller: "merchant".to_string(),
        };
        let json = serde_json::to_string(&tx).unwrap();
        let back: MerchantTransaction = serde_json::from_str(&json).unwrap();
        assert_eq!(back.item_name, "Iron Sword");
        assert_eq!(back.price, 100);
    }
}

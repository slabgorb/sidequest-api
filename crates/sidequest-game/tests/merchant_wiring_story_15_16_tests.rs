//! Story 15-16: Merchant transaction execution wiring tests
//!
//! RED phase — tests that verify merchant transactions extracted from narrator
//! output are mechanically resolved via execute_buy()/execute_sell() instead
//! of letting the narrator hallucinate inventory changes.
//!
//! The gap: WorldStatePatch has no merchant_transactions field. GameSnapshot
//! has no apply_merchant_transactions() method. Narrator-extracted buy/sell
//! actions are silently dropped. These tests assert that:
//!   1. WorldStatePatch accepts merchant transaction requests
//!   2. apply_merchant_transactions() calls execute_buy for buy requests
//!   3. apply_merchant_transactions() calls execute_sell for sell requests
//!   4. Failed transactions (insufficient gold) are handled gracefully
//!   5. Player and merchant inventories are updated atomically
//!   6. OTEL span `merchant.transaction` is emitted with correct fields
//!
//! ACs covered:
//!   AC-2: When WorldStatePatch contains a buy/sell, call execute_buy()/execute_sell()
//!         to mechanically resolve instead of narrator hallucinating inventory changes
//!   OTEL: merchant.transaction (type, item, price, gold_before, gold_after)

use std::collections::HashMap;

use sidequest_game::character::Character;
use sidequest_game::creature_core::CreatureCore;
use sidequest_game::disposition::Disposition;
use sidequest_game::inventory::{Inventory, Item};
use sidequest_game::merchant::{MerchantTransactionRequest, TransactionType};
use sidequest_game::npc::{Npc, NpcRegistryEntry};
use sidequest_game::state::GameSnapshot;
use sidequest_protocol::NonBlankString;

// ============================================================================
// Test helpers
// ============================================================================

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
        state: sidequest_game::ItemState::Carried,
    }
}

fn merchant_npc(name: &str, items: Vec<Item>, gold: i64) -> Npc {
    let mut inv = Inventory::default();
    inv.gold = gold;
    for item in items {
        inv.add(item, 100).unwrap();
    }
    Npc {
        core: CreatureCore {
            name: NonBlankString::new(name).unwrap(),
            description: NonBlankString::new("A merchant").unwrap(),
            personality: NonBlankString::new("Shrewd").unwrap(),
            level: 1,
            hp: 10,
            max_hp: 10,
            ac: 10,
            xp: 0,
            inventory: inv,
            statuses: vec![],
        },
        voice_id: None,
        disposition: Disposition::new(0), // Neutral
        location: Some(NonBlankString::new("Market Square").unwrap()),
        pronouns: Some("he/him".to_string()),
        appearance: None,
        age: None,
        build: None,
        height: None,
        distinguishing_features: vec![],
        ocean: None,
        belief_state: Default::default(),
    }
}

fn merchant_registry_entry(name: &str) -> NpcRegistryEntry {
    NpcRegistryEntry {
        name: name.to_string(),
        pronouns: "he/him".to_string(),
        role: "merchant".to_string(),
        location: "Market Square".to_string(),
        last_seen_turn: 1,
        age: String::new(),
        appearance: String::new(),
        ocean_summary: String::new(),
        ocean: None,
        hp: 10,
        max_hp: 10,
    }
}

/// Create a minimal GameSnapshot with a player character and optional merchant NPC.
fn snapshot_with_merchant(
    player_gold: i64,
    player_items: Vec<Item>,
    merchant_name: &str,
    merchant_items: Vec<Item>,
    merchant_gold: i64,
) -> GameSnapshot {
    let mut player_inv = Inventory::default();
    player_inv.gold = player_gold;
    for item in player_items {
        player_inv.add(item, 100).unwrap();
    }

    let character = Character {
        core: CreatureCore {
            name: NonBlankString::new("Hero").unwrap(),
            description: NonBlankString::new("The player character").unwrap(),
            personality: NonBlankString::new("Brave").unwrap(),
            level: 1,
            hp: 20,
            max_hp: 20,
            ac: 12,
            xp: 0,
            inventory: player_inv,
            statuses: vec![],
        },
        backstory: NonBlankString::new("A wandering adventurer").unwrap(),
        narrative_state: "Shopping at the market".to_string(),
        hooks: vec![],
        char_class: NonBlankString::new("Fighter").unwrap(),
        race: NonBlankString::new("Human").unwrap(),
        pronouns: String::new(),
        stats: HashMap::new(),
        abilities: vec![],
        known_facts: vec![],
        affinities: vec![],
        is_friendly: true,
    };

    let mut state = GameSnapshot::default();
    state.characters = vec![character];
    state.npcs = vec![merchant_npc(merchant_name, merchant_items, merchant_gold)];
    state.npc_registry = vec![merchant_registry_entry(merchant_name)];
    state.location = "Market Square".to_string();
    state
}

// ============================================================================
// AC-2: Buy transaction mechanically resolved
// ============================================================================

#[test]
fn merchant_buy_transaction_updates_inventories() {
    let mut state = snapshot_with_merchant(
        200,                                          // player gold
        vec![],                                       // player items
        "Gruk",                                       // merchant name
        vec![test_item("sword", "Iron Sword", 100)],  // merchant items
        500,                                          // merchant gold
    );

    let request = MerchantTransactionRequest {
        transaction_type: TransactionType::Buy,
        item_id: "sword".to_string(),
        merchant_name: "Gruk".to_string(),
    };

    let results = state.apply_merchant_transactions(&[request]);

    assert_eq!(results.len(), 1, "Should have one transaction result");
    let tx = results[0].as_ref().expect("Buy should succeed");

    assert_eq!(tx.transaction_type, TransactionType::Buy);
    assert_eq!(tx.item_name, "Iron Sword");
    assert_eq!(tx.price, 100); // neutral disposition = base price

    // Player got the item and lost gold
    assert_eq!(state.characters[0].core.inventory.item_count(), 1);
    assert_eq!(state.characters[0].core.inventory.gold, 100); // 200 - 100

    // Merchant lost the item and gained gold
    assert_eq!(state.npcs[0].core.inventory.item_count(), 0);
    assert_eq!(state.npcs[0].core.inventory.gold, 600); // 500 + 100
}

// ============================================================================
// AC-2: Sell transaction mechanically resolved
// ============================================================================

#[test]
fn merchant_sell_transaction_updates_inventories() {
    let mut state = snapshot_with_merchant(
        50,                                            // player gold
        vec![test_item("shield", "Iron Shield", 80)],  // player items
        "Gruk",                                        // merchant name
        vec![],                                        // merchant items
        500,                                           // merchant gold
    );

    let request = MerchantTransactionRequest {
        transaction_type: TransactionType::Sell,
        item_id: "shield".to_string(),
        merchant_name: "Gruk".to_string(),
    };

    let results = state.apply_merchant_transactions(&[request]);

    assert_eq!(results.len(), 1);
    let tx = results[0].as_ref().expect("Sell should succeed");

    assert_eq!(tx.transaction_type, TransactionType::Sell);
    assert_eq!(tx.item_name, "Iron Shield");
    assert_eq!(tx.price, 40); // neutral sell = 50% of base value

    // Player lost the item and gained gold
    assert_eq!(state.characters[0].core.inventory.item_count(), 0);
    assert_eq!(state.characters[0].core.inventory.gold, 90); // 50 + 40

    // Merchant gained the item and lost gold
    assert_eq!(state.npcs[0].core.inventory.item_count(), 1);
    assert_eq!(state.npcs[0].core.inventory.gold, 460); // 500 - 40
}

// ============================================================================
// AC-2 (error): Insufficient gold fails gracefully
// ============================================================================

#[test]
fn merchant_buy_insufficient_gold_returns_error() {
    let mut state = snapshot_with_merchant(
        10,                                           // not enough gold
        vec![],
        "Gruk",
        vec![test_item("sword", "Iron Sword", 100)],
        500,
    );

    let request = MerchantTransactionRequest {
        transaction_type: TransactionType::Buy,
        item_id: "sword".to_string(),
        merchant_name: "Gruk".to_string(),
    };

    let results = state.apply_merchant_transactions(&[request]);

    assert_eq!(results.len(), 1);
    assert!(
        results[0].is_err(),
        "Buy with insufficient gold should return an error"
    );

    // Nothing should have changed (atomic rollback)
    assert_eq!(state.characters[0].core.inventory.gold, 10);
    assert_eq!(state.npcs[0].core.inventory.item_count(), 1);
}

// ============================================================================
// AC-2 (error): Item not found fails gracefully
// ============================================================================

#[test]
fn merchant_buy_item_not_found_returns_error() {
    let mut state = snapshot_with_merchant(200, vec![], "Gruk", vec![], 500);

    let request = MerchantTransactionRequest {
        transaction_type: TransactionType::Buy,
        item_id: "nonexistent".to_string(),
        merchant_name: "Gruk".to_string(),
    };

    let results = state.apply_merchant_transactions(&[request]);

    assert_eq!(results.len(), 1);
    assert!(
        results[0].is_err(),
        "Buy of nonexistent item should return an error"
    );
}

// ============================================================================
// AC-2 (error): Unknown merchant name fails gracefully
// ============================================================================

#[test]
fn merchant_transaction_unknown_merchant_returns_error() {
    let mut state = snapshot_with_merchant(
        200,
        vec![],
        "Gruk",
        vec![test_item("sword", "Iron Sword", 100)],
        500,
    );

    let request = MerchantTransactionRequest {
        transaction_type: TransactionType::Buy,
        item_id: "sword".to_string(),
        merchant_name: "Unknown Merchant".to_string(), // doesn't exist
    };

    let results = state.apply_merchant_transactions(&[request]);

    assert_eq!(results.len(), 1);
    assert!(
        results[0].is_err(),
        "Transaction with unknown merchant should return an error"
    );
}

// ============================================================================
// AC-2: Multiple transactions in a single patch
// ============================================================================

#[test]
fn multiple_merchant_transactions_applied_sequentially() {
    let mut state = snapshot_with_merchant(
        300,
        vec![test_item("shield", "Iron Shield", 80)],
        "Gruk",
        vec![
            test_item("sword", "Iron Sword", 100),
            test_item("potion", "Health Potion", 50),
        ],
        500,
    );

    let requests = vec![
        MerchantTransactionRequest {
            transaction_type: TransactionType::Buy,
            item_id: "sword".to_string(),
            merchant_name: "Gruk".to_string(),
        },
        MerchantTransactionRequest {
            transaction_type: TransactionType::Sell,
            item_id: "shield".to_string(),
            merchant_name: "Gruk".to_string(),
        },
    ];

    let results = state.apply_merchant_transactions(&requests);

    assert_eq!(results.len(), 2, "Should have two transaction results");
    assert!(results[0].is_ok(), "Buy should succeed");
    assert!(results[1].is_ok(), "Sell should succeed");

    // Player: started with shield (80v) + 300g, bought sword (100g), sold shield (40g at neutral)
    // Player items: sword only (bought sword, sold shield)
    assert_eq!(state.characters[0].core.inventory.item_count(), 1);
    assert!(
        state.characters[0]
            .core
            .inventory
            .find("sword")
            .is_some(),
        "Player should have the sword"
    );
    // Player gold: 300 - 100 (buy sword) + 40 (sell shield) = 240
    assert_eq!(state.characters[0].core.inventory.gold, 240);
}

// ============================================================================
// AC-2: Disposition affects transaction prices
// ============================================================================

#[test]
fn merchant_transaction_uses_npc_disposition_for_pricing() {
    // Create a friendly merchant (disposition +50 = max discount)
    let mut state = snapshot_with_merchant(
        200,
        vec![],
        "Gruk",
        vec![test_item("sword", "Iron Sword", 100)],
        500,
    );
    // Override disposition to be very friendly
    state.npcs[0].disposition = Disposition::new(50);

    let request = MerchantTransactionRequest {
        transaction_type: TransactionType::Buy,
        item_id: "sword".to_string(),
        merchant_name: "Gruk".to_string(),
    };

    let results = state.apply_merchant_transactions(&[request]);
    let tx = results[0].as_ref().expect("Buy should succeed");

    // Friendly merchant: disposition 50 → modifier 0.5 → price = 100 * (1.0 - 0.5) = 50
    assert_eq!(
        tx.price, 50,
        "Friendly merchant should give 50% discount (price 50, not 100)"
    );
    assert_eq!(state.characters[0].core.inventory.gold, 150); // 200 - 50
}

// ============================================================================
// OTEL: merchant.transaction span emitted with correct fields
// ============================================================================

#[cfg(test)]
mod otel_tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tracing::subscriber::with_default;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::Registry;

    struct SpanCapture {
        spans: Arc<Mutex<Vec<(String, Vec<(String, String)>)>>>,
    }

    impl SpanCapture {
        fn new() -> (Self, Arc<Mutex<Vec<(String, Vec<(String, String)>)>>>) {
            let spans = Arc::new(Mutex::new(Vec::new()));
            (
                Self {
                    spans: spans.clone(),
                },
                spans,
            )
        }
    }

    impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for SpanCapture {
        fn on_new_span(
            &self,
            attrs: &tracing::span::Attributes<'_>,
            _id: &tracing::span::Id,
            _ctx: tracing_subscriber::layer::Context<'_, S>,
        ) {
            let name = attrs.metadata().name().to_string();
            let mut fields = Vec::new();
            let mut visitor = FieldVisitor(&mut fields);
            attrs.record(&mut visitor);
            self.spans.lock().unwrap().push((name, fields));
        }
    }

    struct FieldVisitor<'a>(&'a mut Vec<(String, String)>);

    impl<'a> tracing::field::Visit for FieldVisitor<'a> {
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            self.0
                .push((field.name().to_string(), format!("{:?}", value)));
        }
        fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
            self.0
                .push((field.name().to_string(), value.to_string()));
        }
        fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
            self.0
                .push((field.name().to_string(), value.to_string()));
        }
        fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
            self.0
                .push((field.name().to_string(), value.to_string()));
        }
    }

    #[test]
    fn merchant_transaction_otel_span_emitted() {
        let (layer, captured) = SpanCapture::new();
        let subscriber = Registry::default().with(layer);

        with_default(subscriber, || {
            let mut state = snapshot_with_merchant(
                200,
                vec![],
                "Gruk",
                vec![test_item("sword", "Iron Sword", 100)],
                500,
            );

            let request = MerchantTransactionRequest {
                transaction_type: TransactionType::Buy,
                item_id: "sword".to_string(),
                merchant_name: "Gruk".to_string(),
            };

            state.apply_merchant_transactions(&[request]);
        });

        let spans = captured.lock().unwrap();
        let tx_span = spans
            .iter()
            .find(|(name, _)| name == "merchant.transaction")
            .expect("Should emit merchant.transaction OTEL span");

        let fields = &tx_span.1;

        // Verify required fields per story ACs
        assert!(
            fields.iter().any(|(k, _)| k == "transaction_type"),
            "Span should have transaction_type field"
        );
        assert!(
            fields.iter().any(|(k, _)| k == "item_name"),
            "Span should have item_name field"
        );
        assert!(
            fields.iter().any(|(k, _)| k == "price"),
            "Span should have price field"
        );
        assert!(
            fields.iter().any(|(k, _)| k == "gold_before"),
            "Span should have gold_before field"
        );
        assert!(
            fields.iter().any(|(k, _)| k == "gold_after"),
            "Span should have gold_after field"
        );
    }
}

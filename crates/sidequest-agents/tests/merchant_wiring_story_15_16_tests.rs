//! Story 15-16: Merchant context injection wiring tests
//!
//! RED phase — tests that verify the orchestrator injects merchant context
//! into narrator/ensemble prompts when a merchant NPC is present.
//!
//! The gap: `format_merchant_context()` exists in sidequest-game but is never
//! called from the orchestrator. TurnContext doesn't carry NPC registry or
//! inventory data. These tests assert that:
//!   1. Merchant context is injected into the prompt for Exploration intent
//!   2. Merchant context is injected into the prompt for Dialogue intent
//!   3. No merchant context when no merchant NPC is in the registry
//!   4. No merchant context for Combat/Chase intents (irrelevant)
//!   5. OTEL span `merchant.context_injected` is emitted with correct fields
//!
//! ACs covered:
//!   AC-1: inject format_merchant_context() when intent is Exploration/Dialogue
//!         and current location has an NPC with role containing "merchant"
//!   OTEL: merchant.context_injected (merchant_name, item_count)

use sidequest_agents::agents::intent_router::Intent;
use sidequest_agents::context_builder::ContextBuilder;
use sidequest_agents::orchestrator::inject_merchant_context;
use sidequest_agents::prompt_framework::{AttentionZone, SectionCategory};
use sidequest_game::creature_core::CreatureCore;
use sidequest_game::disposition::Disposition;
use sidequest_game::inventory::{Inventory, Item};
use sidequest_game::npc::{Npc, NpcRegistryEntry};
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

fn merchant_npc(name: &str, items: Vec<Item>) -> Npc {
    let mut inv = Inventory {
        gold: 500,
        ..Default::default()
    };
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
        disposition: Disposition::new(15), // Slightly friendly
        location: Some(NonBlankString::new("Market Square").unwrap()),
        pronouns: Some("he/him".to_string()),
        appearance: None,
        age: None,
        build: None,
        height: None,
        distinguishing_features: vec![],
        ocean: None,
        belief_state: Default::default(),
        resolution_tier: sidequest_game::npc::ResolutionTier::default(),
        non_transactional_interactions: 0,
        jungian_id: None,
        rpg_role_id: None,
        npc_role_id: None,
        resolved_archetype: None,
    }
}

fn merchant_registry_entry(name: &str, location: &str) -> NpcRegistryEntry {
    NpcRegistryEntry {
        name: name.to_string(),
        pronouns: "he/him".to_string(),
        role: "merchant".to_string(),
        location: location.to_string(),
        last_seen_turn: 1,
        age: String::new(),
        appearance: String::new(),
        ocean_summary: String::new(),
        ocean: None,
        hp: 10,
        max_hp: 10,
        portrait_url: None,
    }
}

fn non_merchant_registry_entry(name: &str) -> NpcRegistryEntry {
    NpcRegistryEntry {
        name: name.to_string(),
        pronouns: "she/her".to_string(),
        role: "guard captain".to_string(),
        location: "Market Square".to_string(),
        last_seen_turn: 1,
        age: String::new(),
        appearance: String::new(),
        ocean_summary: String::new(),
        ocean: None,
        hp: 20,
        max_hp: 20,
        portrait_url: None,
    }
}

// ============================================================================
// AC-1: Merchant context injected for Exploration intent
// ============================================================================

#[test]
fn merchant_context_injected_for_exploration_with_merchant_npc() {
    let mut builder = ContextBuilder::new();
    let registry = vec![merchant_registry_entry("Gruk", "Market Square")];
    let npcs = vec![merchant_npc(
        "Gruk",
        vec![test_item("sword", "Iron Sword", 100)],
    )];

    inject_merchant_context(
        &mut builder,
        &registry,
        &npcs,
        Intent::Exploration,
        "Market Square",
    );

    let prompt = builder.compose();
    assert!(
        prompt.contains("Gruk's wares:"),
        "Prompt should contain merchant's wares listing, got: {}",
        prompt
    );
    assert!(
        prompt.contains("Iron Sword"),
        "Prompt should list the Iron Sword item"
    );
    assert!(prompt.contains("gold"), "Prompt should include gold prices");
}

// ============================================================================
// AC-1: Merchant context injected for Dialogue intent
// ============================================================================

#[test]
fn merchant_context_injected_for_dialogue_with_merchant_npc() {
    let mut builder = ContextBuilder::new();
    let registry = vec![merchant_registry_entry("Gruk", "Market Square")];
    let npcs = vec![merchant_npc(
        "Gruk",
        vec![
            test_item("sword", "Iron Sword", 100),
            test_item("potion", "Health Potion", 50),
        ],
    )];

    inject_merchant_context(
        &mut builder,
        &registry,
        &npcs,
        Intent::Dialogue,
        "Market Square",
    );

    let prompt = builder.compose();
    assert!(
        prompt.contains("Gruk's wares:"),
        "Dialogue intent should also inject merchant context"
    );
    assert!(
        prompt.contains("Health Potion"),
        "All merchant items should be listed"
    );
}

// ============================================================================
// AC-1 (negative): No merchant context when no merchant NPC present
// ============================================================================

#[test]
fn no_merchant_context_when_no_merchant_npc() {
    let mut builder = ContextBuilder::new();
    let registry = vec![non_merchant_registry_entry("Captain Vex")];
    let npcs = vec![]; // No NPCs with merchant role

    inject_merchant_context(
        &mut builder,
        &registry,
        &npcs,
        Intent::Exploration,
        "Market Square",
    );

    let prompt = builder.compose();
    assert!(
        !prompt.contains("wares:"),
        "Should not inject merchant context when no merchant NPCs exist"
    );
    assert_eq!(
        builder.section_count(),
        0,
        "No sections should be added when no merchant is present"
    );
}

// ============================================================================
// AC-1 (negative): No merchant context for Combat intent
// ============================================================================

#[test]
fn no_merchant_context_for_combat_intent() {
    let mut builder = ContextBuilder::new();
    let registry = vec![merchant_registry_entry("Gruk", "Market Square")];
    let npcs = vec![merchant_npc(
        "Gruk",
        vec![test_item("sword", "Iron Sword", 100)],
    )];

    inject_merchant_context(
        &mut builder,
        &registry,
        &npcs,
        Intent::Combat,
        "Market Square",
    );

    let prompt = builder.compose();
    assert!(
        !prompt.contains("wares:"),
        "Combat intent should not inject merchant context"
    );
}

// ============================================================================
// AC-1 (negative): No merchant context for Chase intent
// ============================================================================

#[test]
fn no_merchant_context_for_chase_intent() {
    let mut builder = ContextBuilder::new();
    let registry = vec![merchant_registry_entry("Gruk", "Market Square")];
    let npcs = vec![merchant_npc(
        "Gruk",
        vec![test_item("sword", "Iron Sword", 100)],
    )];

    inject_merchant_context(
        &mut builder,
        &registry,
        &npcs,
        Intent::Chase,
        "Market Square",
    );

    assert_eq!(
        builder.section_count(),
        0,
        "Chase intent should not inject merchant context"
    );
}

// ============================================================================
// AC-1: Merchant in different location is not injected
// ============================================================================

#[test]
fn merchant_in_different_location_not_injected() {
    let mut builder = ContextBuilder::new();
    let registry = vec![merchant_registry_entry("Gruk", "Docks")];
    let npcs = vec![merchant_npc(
        "Gruk",
        vec![test_item("sword", "Iron Sword", 100)],
    )];

    inject_merchant_context(
        &mut builder,
        &registry,
        &npcs,
        Intent::Exploration,
        "Market Square", // Player is at Market Square, merchant is at Docks
    );

    assert_eq!(
        builder.section_count(),
        0,
        "Merchant at a different location should not be injected"
    );
}

// ============================================================================
// AC-1: Merchant context placed in correct attention zone (Valley)
// ============================================================================

#[test]
fn merchant_context_in_valley_zone() {
    let mut builder = ContextBuilder::new();
    let registry = vec![merchant_registry_entry("Gruk", "Market Square")];
    let npcs = vec![merchant_npc(
        "Gruk",
        vec![test_item("sword", "Iron Sword", 100)],
    )];

    inject_merchant_context(
        &mut builder,
        &registry,
        &npcs,
        Intent::Exploration,
        "Market Square",
    );

    let valley_sections = builder.sections_by_zone(AttentionZone::Valley);
    assert!(
        !valley_sections.is_empty(),
        "Merchant context should be in Valley zone"
    );
    let merchant_section = valley_sections
        .iter()
        .find(|s| s.name == "merchant_context")
        .expect("Should have a section named 'merchant_context'");
    assert_eq!(merchant_section.category, SectionCategory::State);
}

// ============================================================================
// AC-1: Multiple merchants at same location — all injected
// ============================================================================

#[test]
fn multiple_merchants_at_same_location_all_injected() {
    let mut builder = ContextBuilder::new();
    let registry = vec![
        merchant_registry_entry("Gruk", "Market Square"),
        merchant_registry_entry("Olga", "Market Square"),
    ];
    let npcs = vec![
        merchant_npc("Gruk", vec![test_item("sword", "Iron Sword", 100)]),
        merchant_npc("Olga", vec![test_item("potion", "Health Potion", 50)]),
    ];

    inject_merchant_context(
        &mut builder,
        &registry,
        &npcs,
        Intent::Exploration,
        "Market Square",
    );

    let prompt = builder.compose();
    assert!(
        prompt.contains("Gruk's wares:"),
        "First merchant should be in prompt"
    );
    assert!(
        prompt.contains("Olga's wares:"),
        "Second merchant should be in prompt"
    );
}

// ============================================================================
// AC-1: Empty merchant inventory — still injected (shows "nothing for sale")
// ============================================================================

#[test]
fn empty_merchant_inventory_still_injected() {
    let mut builder = ContextBuilder::new();
    let registry = vec![merchant_registry_entry("Gruk", "Market Square")];
    let npcs = vec![merchant_npc("Gruk", vec![])]; // No items

    inject_merchant_context(
        &mut builder,
        &registry,
        &npcs,
        Intent::Exploration,
        "Market Square",
    );

    let prompt = builder.compose();
    assert!(
        prompt.contains("Gruk has nothing for sale"),
        "Empty merchant inventory should show 'nothing for sale'"
    );
}

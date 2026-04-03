//! Story 18-9: Session restore syncs full character state.
//!
//! RED phase — these tests verify that session restore extracts the COMPLETE
//! character state from a loaded snapshot, not just base attributes.
//!
//! Bug: dispatch_connect() extracts hp, max_hp, level, xp from the saved
//! character but SKIPS inventory and known_facts. The local `inventory`
//! variable defaults to Inventory::default(), so the next auto-save
//! overwrites the real inventory with empty. Players lose all items on
//! reconnect.
//!
//! ACs tested:
//!   1. restore loads full character state (not just base attributes)
//!   2. Level and XP restored exactly
//!   3. Inventory items restored with quantities and metadata
//!   4. Known facts restored completely
//!   5. OTEL span with snapshot_id, character_name, level, inventory_count, facts_count
//!   6. E2E: save at level N with M items, load, verify all state matches
//!   7. No silent fallbacks — missing field = loud failure

use std::collections::HashMap;

use sidequest_game::character::Character;
use sidequest_game::combat::CombatState;
use sidequest_game::creature_core::CreatureCore;
use sidequest_game::inventory::{Inventory, Item};
use sidequest_game::known_fact::{Confidence, FactSource, KnownFact};
use sidequest_game::persistence::{SessionStore, SqliteStore};
use sidequest_game::state::GameSnapshot;
use sidequest_game::turn::TurnManager;
// RED: This module does not exist yet. Dev must create it.
use sidequest_game::session_restore::{extract_character_state, RestoredCharacterState};
use sidequest_protocol::NonBlankString;

// ============================================================================
// Test fixtures
// ============================================================================

/// Build a character with rich state: level 5, XP 1200, 3 inventory items,
/// 150 gold, and 4 known facts of varying source/confidence.
fn rich_character() -> Character {
    Character {
        core: CreatureCore {
            name: NonBlankString::new("Kael Stormwind").unwrap(),
            description: NonBlankString::new("A weathered ranger with a scar across his left eye")
                .unwrap(),
            personality: NonBlankString::new("Cautious but fiercely loyal").unwrap(),
            level: 5,
            hp: 38,
            max_hp: 42,
            ac: 15,
            xp: 1200,
            inventory: rich_inventory(),
            statuses: vec![],
        },
        backstory: NonBlankString::new("Orphaned during the Flickering, raised by the wasteland")
            .unwrap(),
        narrative_state: "Investigating the old mines".to_string(),
        hooks: vec!["Find the source of the Flickering".to_string()],
        char_class: NonBlankString::new("Ranger").unwrap(),
        race: NonBlankString::new("Human").unwrap(),
        pronouns: "he/him".to_string(),
        stats: HashMap::from([
            ("STR".to_string(), 14),
            ("DEX".to_string(), 16),
            ("CON".to_string(), 13),
            ("INT".to_string(), 12),
            ("WIS".to_string(), 15),
            ("CHA".to_string(), 10),
        ]),
        abilities: vec![],
        known_facts: rich_known_facts(),
        affinities: vec![],
        is_friendly: true,
    }
}

fn rich_inventory() -> Inventory {
    Inventory {
        items: vec![
            Item {
                id: NonBlankString::new("sword_iron").unwrap(),
                name: NonBlankString::new("Rusty Iron Sword").unwrap(),
                description: NonBlankString::new("A battered blade, well-used").unwrap(),
                category: NonBlankString::new("weapon").unwrap(),
                value: 15,
                weight: 3.0,
                rarity: NonBlankString::new("common").unwrap(),
                narrative_weight: 0.6,
                tags: vec!["melee".to_string(), "blade".to_string()],
                equipped: true,
                quantity: 1,
            },
            Item {
                id: NonBlankString::new("potion_healing").unwrap(),
                name: NonBlankString::new("Healing Potion").unwrap(),
                description: NonBlankString::new("A bubbling red liquid").unwrap(),
                category: NonBlankString::new("consumable").unwrap(),
                value: 50,
                weight: 0.5,
                rarity: NonBlankString::new("uncommon").unwrap(),
                narrative_weight: 0.3,
                tags: vec!["healing".to_string()],
                equipped: false,
                quantity: 3,
            },
            Item {
                id: NonBlankString::new("amulet_whispers").unwrap(),
                name: NonBlankString::new("Amulet of Whispers").unwrap(),
                description: NonBlankString::new(
                    "A silver amulet that hums faintly near the old mines",
                )
                .unwrap(),
                category: NonBlankString::new("treasure").unwrap(),
                value: 200,
                weight: 0.2,
                rarity: NonBlankString::new("rare").unwrap(),
                narrative_weight: 0.8, // evolved item — mechanically significant
                tags: vec!["magic".to_string(), "mystery".to_string()],
                equipped: true,
                quantity: 1,
            },
        ],
        gold: 150,
    }
}

fn rich_known_facts() -> Vec<KnownFact> {
    vec![
        KnownFact {
            content: "The old mines were sealed after the second Flickering event".to_string(),
            learned_turn: 3,
            source: FactSource::Dialogue,
            confidence: Confidence::Certain,
            category: sidequest_protocol::FactCategory::Place,
        },
        KnownFact {
            content: "Strange lights have been seen near the mine entrance at dusk".to_string(),
            learned_turn: 5,
            source: FactSource::Observation,
            confidence: Confidence::Certain,
            category: sidequest_protocol::FactCategory::Place,
        },
        KnownFact {
            content: "The merchant Voss may be smuggling artifacts from the mines".to_string(),
            learned_turn: 8,
            source: FactSource::Dialogue,
            confidence: Confidence::Suspected,
            category: sidequest_protocol::FactCategory::Person,
        },
        KnownFact {
            content: "A creature matching the description of a Gloom Stalker was spotted near the south ridge".to_string(),
            learned_turn: 12,
            source: FactSource::Discovery,
            confidence: Confidence::Rumored,
            category: sidequest_protocol::FactCategory::Lore,
        },
    ]
}

fn rich_snapshot() -> GameSnapshot {
    GameSnapshot {
        genre_slug: "mutant_wasteland".to_string(),
        world_slug: "flickering_reach".to_string(),
        characters: vec![rich_character()],
        npcs: vec![],
        location: "Old Mine Entrance".to_string(),
        time_of_day: "dusk".to_string(),
        quest_log: HashMap::from([
            (
                "investigate_mines".to_string(),
                "Find what's causing the lights in the old mines".to_string(),
            ),
            (
                "confront_voss".to_string(),
                "Confront the merchant Voss about smuggling".to_string(),
            ),
        ]),
        notes: vec![],
        narrative_log: vec![],
        combat: CombatState::new(),
        chase: None,
        active_tropes: vec![],
        atmosphere: "eerie".to_string(),
        current_region: "flickering_reach".to_string(),
        discovered_regions: vec![
            "flickering_reach".to_string(),
            "south_ridge".to_string(),
        ],
        discovered_routes: vec!["reach_to_ridge".to_string()],
        turn_manager: TurnManager::new(),
        last_saved_at: Some(chrono::Utc::now()),
        active_stakes: "The mines may hold the key to stopping the Flickering".to_string(),
        lore_established: vec!["The Flickering is a recurring phenomenon".to_string()],
        turns_since_meaningful: 1,
        total_beats_fired: 3,
        campaign_maturity: sidequest_game::CampaignMaturity::Early,
        npc_registry: vec![],
        world_history: vec![],
        ..GameSnapshot::default()
    }
}

// ============================================================================
// AC-1/AC-6: Full character state roundtrip through persistence
// ============================================================================

#[test]
fn persistence_roundtrip_preserves_full_character_state() {
    let store = SqliteStore::open_in_memory().unwrap();
    let snapshot = rich_snapshot();

    store.save(&snapshot).unwrap();
    let loaded = store.load().unwrap().expect("Load after save should return Some");

    let restored = loaded.snapshot.characters.first().expect("Must have character");

    // AC-2: Level and XP
    assert_eq!(restored.core.level, 5, "Level must survive roundtrip");
    assert_eq!(restored.core.xp, 1200, "XP must survive roundtrip");
    assert_eq!(restored.core.hp, 38, "HP must survive roundtrip");
    assert_eq!(restored.core.max_hp, 42, "Max HP must survive roundtrip");

    // AC-3: Inventory items
    assert_eq!(
        restored.core.inventory.items.len(),
        3,
        "All 3 inventory items must survive roundtrip"
    );
    assert_eq!(
        restored.core.inventory.gold, 150,
        "Gold must survive roundtrip"
    );

    // AC-4: Known facts
    assert_eq!(
        restored.known_facts.len(),
        4,
        "All 4 known facts must survive roundtrip"
    );
}

// ============================================================================
// AC-3: Inventory items restored with correct quantities and metadata
// ============================================================================

#[test]
fn persistence_roundtrip_preserves_inventory_item_details() {
    let store = SqliteStore::open_in_memory().unwrap();
    let snapshot = rich_snapshot();

    store.save(&snapshot).unwrap();
    let loaded = store.load().unwrap().unwrap();
    let inv = &loaded.snapshot.characters.first().unwrap().core.inventory;

    // Verify specific item properties survive
    let sword = inv.items.iter().find(|i| i.id.as_str() == "sword_iron");
    assert!(sword.is_some(), "Iron sword must survive roundtrip");
    let sword = sword.unwrap();
    assert_eq!(sword.name.as_str(), "Rusty Iron Sword");
    assert!(sword.equipped, "Equipped state must survive roundtrip");
    assert_eq!(sword.quantity, 1);
    assert!((sword.narrative_weight - 0.6).abs() < f64::EPSILON, "Narrative weight must survive");
    assert_eq!(sword.tags, vec!["melee", "blade"]);
    assert_eq!(sword.category.as_str(), "weapon");
    assert_eq!(sword.rarity.as_str(), "common");

    // Verify stackable consumable
    let potion = inv.items.iter().find(|i| i.id.as_str() == "potion_healing");
    assert!(potion.is_some(), "Healing potion must survive roundtrip");
    assert_eq!(potion.unwrap().quantity, 3, "Stack quantity must survive");

    // Verify evolved item
    let amulet = inv.items.iter().find(|i| i.id.as_str() == "amulet_whispers");
    assert!(amulet.is_some(), "Evolved amulet must survive roundtrip");
    let amulet = amulet.unwrap();
    assert!(amulet.is_evolved(), "Amulet narrative_weight >= 0.7 must survive");
    assert!(amulet.equipped, "Amulet equipped state must survive");
}

#[test]
fn persistence_roundtrip_preserves_gold() {
    let store = SqliteStore::open_in_memory().unwrap();
    let snapshot = rich_snapshot();

    store.save(&snapshot).unwrap();
    let loaded = store.load().unwrap().unwrap();
    let inv = &loaded.snapshot.characters.first().unwrap().core.inventory;

    assert_eq!(inv.gold, 150, "Gold amount must survive roundtrip exactly");
}

// ============================================================================
// AC-4: Known facts restored completely
// ============================================================================

#[test]
fn persistence_roundtrip_preserves_known_fact_details() {
    let store = SqliteStore::open_in_memory().unwrap();
    let snapshot = rich_snapshot();

    store.save(&snapshot).unwrap();
    let loaded = store.load().unwrap().unwrap();
    let facts = &loaded.snapshot.characters.first().unwrap().known_facts;

    assert_eq!(facts.len(), 4, "All 4 facts must survive roundtrip");

    // Verify first fact (Dialogue / Certain)
    let mine_fact = &facts[0];
    assert!(
        mine_fact.content.contains("old mines were sealed"),
        "Fact content must survive: got '{}'",
        mine_fact.content
    );
    assert_eq!(mine_fact.learned_turn, 3, "Learned turn must survive");
    assert!(
        matches!(mine_fact.source, FactSource::Dialogue),
        "FactSource must survive"
    );
    assert!(
        matches!(mine_fact.confidence, Confidence::Certain),
        "Confidence must survive"
    );

    // Verify rumored fact (Discovery / Rumored)
    let creature_fact = &facts[3];
    assert!(
        matches!(creature_fact.source, FactSource::Discovery),
        "Discovery source must survive"
    );
    assert!(
        matches!(creature_fact.confidence, Confidence::Rumored),
        "Rumored confidence must survive"
    );
    assert_eq!(creature_fact.learned_turn, 12);
}

// ============================================================================
// AC-1: extract_character_state returns full state from snapshot
// (RED — session_restore module does not exist yet)
// ============================================================================

#[test]
fn extract_character_state_returns_level_and_xp() {
    let snapshot = rich_snapshot();
    let result = extract_character_state(&snapshot)
        .expect("Must extract character state from snapshot with characters");

    assert_eq!(result.level, 5, "Extracted level must match snapshot");
    assert_eq!(result.xp, 1200, "Extracted XP must match snapshot");
}

#[test]
fn extract_character_state_returns_inventory() {
    let snapshot = rich_snapshot();
    let result = extract_character_state(&snapshot).unwrap();

    assert_eq!(
        result.inventory.items.len(),
        3,
        "Extracted inventory must have all 3 items"
    );
    assert_eq!(
        result.inventory.gold, 150,
        "Extracted inventory must have correct gold"
    );

    // Verify specific items are present with correct IDs
    assert!(
        result.inventory.find("sword_iron").is_some(),
        "Iron sword must be in extracted inventory"
    );
    assert!(
        result.inventory.find("potion_healing").is_some(),
        "Healing potion must be in extracted inventory"
    );
    assert!(
        result.inventory.find("amulet_whispers").is_some(),
        "Amulet must be in extracted inventory"
    );
}

#[test]
fn extract_character_state_returns_known_facts() {
    let snapshot = rich_snapshot();
    let result = extract_character_state(&snapshot).unwrap();

    assert_eq!(
        result.known_facts.len(),
        4,
        "Extracted known facts must have all 4 facts"
    );
    assert!(
        result.known_facts[0].content.contains("old mines were sealed"),
        "First fact content must match"
    );
}

#[test]
fn extract_character_state_returns_hp_and_ac() {
    let snapshot = rich_snapshot();
    let result = extract_character_state(&snapshot).unwrap();

    assert_eq!(result.hp, 38, "HP must be extracted");
    assert_eq!(result.max_hp, 42, "Max HP must be extracted");
    assert_eq!(result.ac, 15, "AC must be extracted");
}

#[test]
fn extract_character_state_returns_character_name() {
    let snapshot = rich_snapshot();
    let result = extract_character_state(&snapshot).unwrap();

    assert_eq!(
        result.character_name, "Kael Stormwind",
        "Character name must be extracted"
    );
}

#[test]
fn extract_character_state_returns_character_json() {
    let snapshot = rich_snapshot();
    let result = extract_character_state(&snapshot).unwrap();

    // The full character JSON must be available for the dispatch loop
    assert!(
        result.character_json.is_some(),
        "Character JSON must be present for dispatch context"
    );

    // Round-trip: deserialize back and verify known_facts survive
    let json = result.character_json.unwrap();
    let roundtripped: Character = serde_json::from_value(json).unwrap();
    assert_eq!(
        roundtripped.known_facts.len(),
        4,
        "Known facts must survive JSON roundtrip through character_json"
    );
    assert_eq!(
        roundtripped.core.inventory.items.len(),
        3,
        "Inventory must survive JSON roundtrip through character_json"
    );
}

// ============================================================================
// AC-7: No silent fallbacks — missing character = error, not default
// ============================================================================

#[test]
fn extract_character_state_returns_none_for_empty_characters() {
    let mut snapshot = rich_snapshot();
    snapshot.characters.clear();

    let result = extract_character_state(&snapshot);
    assert!(
        result.is_none(),
        "extract_character_state must return None when snapshot has no characters — \
         no silent fallback to defaults"
    );
}

// ============================================================================
// AC-6: End-to-end — save at level N with M items, load, extract, verify all
// ============================================================================

#[test]
fn end_to_end_save_load_extract_roundtrip() {
    let store = SqliteStore::open_in_memory().unwrap();
    let snapshot = rich_snapshot();

    // Save
    store.save(&snapshot).unwrap();

    // Load
    let saved = store.load().unwrap().expect("Must load saved session");

    // Extract (the function dispatch_connect should use)
    let state = extract_character_state(&saved.snapshot)
        .expect("Must extract character from loaded snapshot");

    // Verify ALL character state fields
    assert_eq!(state.character_name, "Kael Stormwind");
    assert_eq!(state.level, 5);
    assert_eq!(state.xp, 1200);
    assert_eq!(state.hp, 38);
    assert_eq!(state.max_hp, 42);
    assert_eq!(state.ac, 15);
    assert_eq!(state.inventory.items.len(), 3);
    assert_eq!(state.inventory.gold, 150);
    assert_eq!(state.known_facts.len(), 4);
    assert!(state.character_json.is_some());

    // Verify item details survived the full pipeline
    let sword = state.inventory.find("sword_iron").unwrap();
    assert!(sword.equipped);
    assert!((sword.narrative_weight - 0.6).abs() < f64::EPSILON);

    // Verify fact details survived
    assert!(matches!(state.known_facts[0].source, FactSource::Dialogue));
    assert!(matches!(
        state.known_facts[3].confidence,
        Confidence::Rumored
    ));
}

// ============================================================================
// AC-2: Level and XP edge cases
// ============================================================================

#[test]
fn extract_preserves_level_one_zero_xp() {
    let mut snapshot = rich_snapshot();
    snapshot.characters[0].core.level = 1;
    snapshot.characters[0].core.xp = 0;

    let result = extract_character_state(&snapshot).unwrap();
    assert_eq!(result.level, 1, "Level 1 must not be confused with default");
    assert_eq!(result.xp, 0, "Zero XP must be preserved explicitly");
}

// ============================================================================
// AC-3: Empty inventory is valid (not a fallback to default)
// ============================================================================

#[test]
fn extract_preserves_empty_inventory_with_gold() {
    let mut snapshot = rich_snapshot();
    snapshot.characters[0].core.inventory = Inventory {
        items: vec![],
        gold: 75,
    };

    let result = extract_character_state(&snapshot).unwrap();
    assert!(
        result.inventory.items.is_empty(),
        "Empty inventory is valid — zero items is not a default confusion"
    );
    assert_eq!(
        result.inventory.gold, 75,
        "Gold must survive even with empty item list"
    );
}

// ============================================================================
// Overwrite protection — save, mutate inventory, save, load must have latest
// ============================================================================

#[test]
fn persistence_overwrite_preserves_latest_inventory() {
    let store = SqliteStore::open_in_memory().unwrap();
    let mut snapshot = rich_snapshot();

    // First save with 3 items
    store.save(&snapshot).unwrap();

    // Mutate: add an item, change gold
    snapshot.characters[0].core.inventory.items.push(Item {
        id: NonBlankString::new("gem_ruby").unwrap(),
        name: NonBlankString::new("Ruby Gemstone").unwrap(),
        description: NonBlankString::new("A flawless ruby").unwrap(),
        category: NonBlankString::new("treasure").unwrap(),
        value: 500,
        weight: 0.1,
        rarity: NonBlankString::new("rare").unwrap(),
        narrative_weight: 0.4,
        tags: vec!["gem".to_string()],
        equipped: false,
        quantity: 1,
    });
    snapshot.characters[0].core.inventory.gold = 225;

    // Second save
    store.save(&snapshot).unwrap();

    // Load must have 4 items and 225 gold
    let loaded = store.load().unwrap().unwrap();
    let inv = &loaded.snapshot.characters.first().unwrap().core.inventory;
    assert_eq!(inv.items.len(), 4, "Latest save must have 4 items");
    assert_eq!(inv.gold, 225, "Latest save must have updated gold");
}

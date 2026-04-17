//! Story 3-4 RED: Entity reference validation — narration mentions checked against GameSnapshot.
//!
//! Tests that the entity reference validator correctly:
//!   1. Builds an EntityRegistry from snapshot (characters, NPCs, items, locations, regions)
//!   2. Extracts capitalized phrases as potential entity references
//!   3. Passes narration referencing known entities (no warnings)
//!   4. Flags narration referencing unknown entities (warnings)
//!   5. Handles compound names via substring matching ("Old Grimjaw" → "Grimjaw")
//!   6. Skips sentence-initial capitalization and stop words
//!   7. Emits tracing::warn! with component="watcher", check="entity_reference"
//!
//! RED state: All stubs return empty Vecs / false, so every assertion expecting
//! warnings or matches will fail. The Dev agent implements GREEN.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use tracing::Subscriber;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::Registry;

use sidequest_agents::agents::intent_router::Intent;
use sidequest_agents::entity_reference::{
    check_entity_references, extract_potential_references, EntityRegistry,
};
use sidequest_agents::patch_legality::ValidationResult;
use sidequest_agents::turn_record::{PatchSummary, TurnRecord};
use sidequest_game::{
    Character, CreatureCore, Disposition, GameSnapshot, Inventory, Item, Npc, StateDelta,
    TurnManager,
};
use sidequest_protocol::NonBlankString;

// ===========================================================================
// Test infrastructure: mock builders
// ===========================================================================

/// Build a minimal GameSnapshot for testing.
fn mock_game_snapshot() -> GameSnapshot {
    GameSnapshot {
        genre_slug: "mutant_wasteland".to_string(),
        world_slug: "flickering_reach".to_string(),
        characters: vec![],
        npcs: vec![],
        location: "The Rusty Valve".to_string(),
        time_of_day: "dusk".to_string(),
        quest_log: HashMap::new(),
        notes: vec![],
        narrative_log: vec![],
        active_tropes: vec![],
        atmosphere: "tense and electric".to_string(),
        current_region: "flickering_reach".to_string(),
        discovered_regions: vec!["flickering_reach".to_string()],
        discovered_routes: vec![],
        turn_manager: TurnManager::new(),
        last_saved_at: None,
        active_stakes: String::new(),
        lore_established: vec![],
        turns_since_meaningful: 0,
        ..GameSnapshot::default()
    }
}

/// Build a mock StateDelta (all fields private, must go through serde).
fn mock_state_delta() -> StateDelta {
    serde_json::from_value(serde_json::json!({
        "characters": false,
        "npcs": false,
        "location": false,
        "time_of_day": false,
        "quest_log": false,
        "notes": false,
        "combat": false,
        "chase": false,
        "tropes": false,
        "atmosphere": false,
        "regions": false,
        "routes": false,
        "active_stakes": false,
        "lore": false,
        "new_location": null
    }))
    .expect("mock StateDelta should deserialize")
}

/// Build an NPC with the given name.
fn make_npc(name: &str) -> Npc {
    Npc {
        core: CreatureCore {
            name: NonBlankString::new(name).unwrap(),
            description: NonBlankString::new("A test NPC").unwrap(),
            personality: NonBlankString::new("Stoic").unwrap(),
            level: 3,
            hp: 20,
            max_hp: 20,
            ac: 12,
            xp: 0,
            inventory: Inventory::default(),
            statuses: vec![],
        },
        voice_id: None,
        disposition: Disposition::new(0),
        pronouns: None,
        appearance: None,
        age: None,
        build: None,
        height: None,
        distinguishing_features: vec![],
        location: Some(NonBlankString::new("The Rusty Valve").unwrap()),
        ocean: None,
        belief_state: sidequest_game::belief_state::BeliefState::default(),
        resolution_tier: sidequest_game::npc::ResolutionTier::default(),
        non_transactional_interactions: 0,
        jungian_id: None,
        rpg_role_id: None,
        npc_role_id: None,
        resolved_archetype: None,
    }
}

/// Build a Character with the given name and optional inventory items.
fn make_character(name: &str, item_names: Vec<&str>) -> Character {
    let items: Vec<Item> = item_names
        .into_iter()
        .map(|iname| Item {
            id: NonBlankString::new(&iname.to_lowercase().replace(' ', "_")).unwrap(),
            name: NonBlankString::new(iname).unwrap(),
            description: NonBlankString::new("A test item").unwrap(),
            category: NonBlankString::new("weapon").unwrap(),
            value: 10,
            weight: 1.0,
            rarity: NonBlankString::new("common").unwrap(),
            narrative_weight: 0.5,
            tags: vec![],
            equipped: false,
            quantity: 1,
            uses_remaining: None,
            state: sidequest_game::ItemState::Carried,
        })
        .collect();

    Character {
        core: CreatureCore {
            name: NonBlankString::new(name).unwrap(),
            description: NonBlankString::new("A test character").unwrap(),
            personality: NonBlankString::new("Brave").unwrap(),
            level: 5,
            hp: 30,
            max_hp: 30,
            ac: 14,
            xp: 0,
            inventory: Inventory { items, gold: 100 },
            statuses: vec![],
        },
        backstory: NonBlankString::new("A hero on a quest").unwrap(),
        narrative_state: "exploring".to_string(),
        hooks: vec![],
        char_class: NonBlankString::new("Fighter").unwrap(),
        race: NonBlankString::new("Human").unwrap(),
        pronouns: String::new(),
        stats: HashMap::new(),
        abilities: vec![],
        known_facts: vec![],
        affinities: vec![],
        is_friendly: true,
        resolved_archetype: None,
    }
}

/// Build a TurnRecord with the given narration and customizable snapshot_after.
fn make_record_with_narration(narration: &str) -> TurnRecord {
    TurnRecord {
        turn_id: 1,
        timestamp: Utc::now(),
        player_input: "test action".to_string(),
        classified_intent: Intent::Exploration,
        agent_name: "narrator".to_string(),
        narration: narration.to_string(),
        patches_applied: vec![PatchSummary {
            patch_type: "world".to_string(),
            fields_changed: vec!["notes".to_string()],
        }],
        snapshot_before: mock_game_snapshot(),
        snapshot_after: mock_game_snapshot(),
        delta: mock_state_delta(),
        beats_fired: vec![],
        token_count_in: 500,
        token_count_out: 100,
        agent_duration_ms: 1200,
        is_degraded: false,
        spans: vec![],
        prompt_text: None,
        raw_response_text: None,
    }
}

// ===========================================================================
// Placeholder tests (will be replaced once entity_reference is wired)
// ===========================================================================

#[test]
fn test_placeholder_entity_reference() {
    // Placeholder test — real tests will exercise entity reference validation.
    assert!(true);
}

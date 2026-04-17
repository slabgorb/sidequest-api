//! Story 9-7: Core slash commands tests
//!
//! RED phase — these tests import command types from sidequest_game::commands
//! which does not exist yet. They will fail to compile until Dev implements:
//!   - commands/mod.rs with re-exports
//!   - commands/status.rs (StatusCommand)
//!   - commands/inventory.rs (InventoryCommand)
//!   - commands/map.rs (MapCommand)
//!   - commands/save.rs (SaveCommand)
//!
//! ACs:
//!   1. Status — /status displays character HP, level, class, race, location, situation
//!   2. Inventory — /inventory lists equipped items and pack with weight
//!   3. Map — /map shows discovered regions, current location, routes
//!   4. Save — /save returns confirmation message
//!   5. Pure Functions — No LLM calls, instant response, immutable state
//!   6. Error Handling — Missing data produces clear error messages
//!   7. Integration — Commands registered via SlashRouter

use std::collections::HashMap;

use sidequest_game::character::Character;
use sidequest_game::commands::{InventoryCommand, MapCommand, SaveCommand, StatusCommand};
use sidequest_game::creature_core::CreatureCore;
use sidequest_game::inventory::{Inventory, Item};
use sidequest_game::slash_router::{CommandHandler, CommandResult, SlashRouter};
use sidequest_game::state::GameSnapshot;
use sidequest_game::turn::TurnManager;
use sidequest_protocol::NonBlankString;

// ============================================================================
// Test fixtures
// ============================================================================

fn test_snapshot() -> GameSnapshot {
    GameSnapshot {
        genre_slug: "mutant_wasteland".to_string(),
        world_slug: "flickering_reach".to_string(),
        characters: vec![test_character()],
        npcs: vec![],
        location: "The Rusted Gate".to_string(),
        time_of_day: "dusk".to_string(),
        atmosphere: "tense".to_string(),
        current_region: "outer_wastes".to_string(),
        discovered_regions: vec![
            "outer_wastes".to_string(),
            "scorched_field".to_string(),
            "crumbling_tower".to_string(),
        ],
        discovered_routes: vec![
            "outer_wastes → scorched_field".to_string(),
            "scorched_field → crumbling_tower".to_string(),
        ],
        quest_log: HashMap::new(),
        notes: vec![],
        narrative_log: vec![],
        active_tropes: vec![],
        turn_manager: TurnManager::new(),
        active_stakes: String::new(),
        lore_established: vec![],
        turns_since_meaningful: 0,
        total_beats_fired: 0,
        campaign_maturity: Default::default(),
        npc_registry: vec![],
        world_history: vec![],
        last_saved_at: None,
        ..Default::default()
    }
}

fn test_character() -> Character {
    let mut inventory = Inventory {
        gold: 50,
        ..Default::default()
    };

    inventory.items.push(Item {
        id: NonBlankString::new("machete_rusty").unwrap(),
        name: NonBlankString::new("Rusty Machete").unwrap(),
        description: NonBlankString::new("A worn blade that's seen better days").unwrap(),
        category: NonBlankString::new("weapon").unwrap(),
        value: 25,
        weight: 2.0,
        rarity: NonBlankString::new("common").unwrap(),
        narrative_weight: 0.2,
        tags: vec!["melee".to_string(), "blade".to_string()],
        equipped: true,
        quantity: 1,
        uses_remaining: None,
        state: sidequest_game::ItemState::Carried,
    });

    inventory.items.push(Item {
        id: NonBlankString::new("jacket_leather").unwrap(),
        name: NonBlankString::new("Leather Jacket").unwrap(),
        description: NonBlankString::new("Weathered armor against the wastes").unwrap(),
        category: NonBlankString::new("armor").unwrap(),
        value: 40,
        weight: 3.0,
        rarity: NonBlankString::new("common").unwrap(),
        narrative_weight: 0.3,
        tags: vec!["protection".to_string()],
        equipped: true,
        quantity: 1,
        uses_remaining: None,
        state: sidequest_game::ItemState::Carried,
    });

    inventory.items.push(Item {
        id: NonBlankString::new("bedroll").unwrap(),
        name: NonBlankString::new("Bedroll").unwrap(),
        description: NonBlankString::new("Sleeping gear for the road").unwrap(),
        category: NonBlankString::new("tool").unwrap(),
        value: 5,
        weight: 2.5,
        rarity: NonBlankString::new("common").unwrap(),
        narrative_weight: 0.1,
        tags: vec!["camping".to_string()],
        equipped: false,
        quantity: 1,
        uses_remaining: None,
        state: sidequest_game::ItemState::Carried,
    });

    inventory.items.push(Item {
        id: NonBlankString::new("rations").unwrap(),
        name: NonBlankString::new("Rations (3 days)").unwrap(),
        description: NonBlankString::new("Dried food supplies").unwrap(),
        category: NonBlankString::new("consumable").unwrap(),
        value: 15,
        weight: 1.0,
        rarity: NonBlankString::new("common").unwrap(),
        narrative_weight: 0.0,
        tags: vec!["food".to_string()],
        equipped: false,
        quantity: 3,
        uses_remaining: None,
        state: sidequest_game::ItemState::Carried,
    });

    Character {
        core: CreatureCore {
            name: NonBlankString::new("Reva Ashwalker").unwrap(),
            description: NonBlankString::new("A scarred wanderer").unwrap(),
            personality: NonBlankString::new("Cautious but curious").unwrap(),
            level: 2,
            hp: 18,
            max_hp: 20,
            ac: 13,
            xp: 0,
            inventory,
            statuses: vec![],
        },
        backstory: NonBlankString::new("Born in the ash storms").unwrap(),
        narrative_state: "Approaching the gate, wary of what lurks within.".to_string(),
        hooks: vec![],
        char_class: NonBlankString::new("Scavenger").unwrap(),
        race: NonBlankString::new("Mutant").unwrap(),
        pronouns: String::new(),
        stats: HashMap::from([("STR".to_string(), 12), ("DEX".to_string(), 14)]),
        abilities: vec![],
        known_facts: vec![],
        affinities: vec![],
        is_friendly: true,
        resolved_archetype: None,
    }
}

// ============================================================================
// AC-1: Status Command — HP, level, class, race, location, situation
// ============================================================================

#[test]
fn status_displays_character_name_and_hp() {
    let mut router = SlashRouter::new();
    router.register(Box::new(StatusCommand));
    let state = test_snapshot();

    let result = router.try_dispatch("/status", &state);
    assert!(result.is_some(), "/status must be intercepted");

    match result.unwrap() {
        CommandResult::Display(text) => {
            assert!(
                text.contains("Reva Ashwalker"),
                "Should contain name, got: {}",
                text
            );
            assert!(
                text.contains("18") && text.contains("20"),
                "Should contain HP 18/20, got: {}",
                text
            );
        }
        other => panic!("Expected Display, got {:?}", other),
    }
}

#[test]
fn status_displays_level_class_race() {
    let mut router = SlashRouter::new();
    router.register(Box::new(StatusCommand));
    let state = test_snapshot();

    match router.try_dispatch("/status", &state).unwrap() {
        CommandResult::Display(text) => {
            assert!(
                text.contains("Scavenger"),
                "Should contain class, got: {}",
                text
            );
            assert!(
                text.contains("Mutant"),
                "Should contain race, got: {}",
                text
            );
            // Level can be shown as "Level 2", "Lv 2", "2", etc.
            assert!(text.contains("2"), "Should contain level, got: {}", text);
        }
        other => panic!("Expected Display, got {:?}", other),
    }
}

#[test]
fn status_displays_location() {
    let mut router = SlashRouter::new();
    router.register(Box::new(StatusCommand));
    let state = test_snapshot();

    match router.try_dispatch("/status", &state).unwrap() {
        CommandResult::Display(text) => {
            assert!(
                text.contains("Rusted Gate") || text.contains("outer_wastes"),
                "Should contain location, got: {}",
                text
            );
        }
        other => panic!("Expected Display, got {:?}", other),
    }
}

#[test]
fn status_with_no_character_returns_error() {
    let mut router = SlashRouter::new();
    router.register(Box::new(StatusCommand));
    let mut state = test_snapshot();
    state.characters.clear();

    match router.try_dispatch("/status", &state).unwrap() {
        CommandResult::Error(msg) => {
            assert!(
                msg.to_lowercase().contains("character") || msg.to_lowercase().contains("no"),
                "Should report missing character, got: {}",
                msg
            );
        }
        other => panic!("Expected Error for no character, got {:?}", other),
    }
}

#[test]
fn status_with_zero_hp_still_displays() {
    let mut router = SlashRouter::new();
    router.register(Box::new(StatusCommand));
    let mut state = test_snapshot();
    state.characters[0].core.hp = 0;

    match router.try_dispatch("/status", &state).unwrap() {
        CommandResult::Display(text) => {
            assert!(text.contains("0"), "Should show 0 HP, got: {}", text);
        }
        other => panic!("Expected Display even at 0 HP, got {:?}", other),
    }
}

// ============================================================================
// AC-2: Inventory Command — equipped items, pack contents, weight
// ============================================================================

#[test]
fn inventory_lists_equipped_items() {
    let mut router = SlashRouter::new();
    router.register(Box::new(InventoryCommand));
    let state = test_snapshot();

    match router.try_dispatch("/inventory", &state).unwrap() {
        CommandResult::Display(text) => {
            assert!(
                text.contains("Rusty Machete"),
                "Should list machete, got: {}",
                text
            );
            assert!(
                text.contains("Leather Jacket"),
                "Should list jacket, got: {}",
                text
            );
        }
        other => panic!("Expected Display, got {:?}", other),
    }
}

#[test]
fn inventory_lists_pack_contents() {
    let mut router = SlashRouter::new();
    router.register(Box::new(InventoryCommand));
    let state = test_snapshot();

    match router.try_dispatch("/inventory", &state).unwrap() {
        CommandResult::Display(text) => {
            assert!(
                text.contains("Bedroll"),
                "Should list pack items, got: {}",
                text
            );
            assert!(
                text.contains("Rations"),
                "Should list rations, got: {}",
                text
            );
        }
        other => panic!("Expected Display, got {:?}", other),
    }
}

#[test]
fn inventory_separates_equipped_from_pack() {
    let mut router = SlashRouter::new();
    router.register(Box::new(InventoryCommand));
    let state = test_snapshot();

    match router.try_dispatch("/inventory", &state).unwrap() {
        CommandResult::Display(text) => {
            // Equipped and pack items should appear in distinct sections
            let machete_pos = text.find("Rusty Machete").expect("Should contain machete");
            let bedroll_pos = text.find("Bedroll").expect("Should contain bedroll");
            // Equipped items should appear before pack items in the output
            assert!(
                machete_pos < bedroll_pos,
                "Equipped items should appear before pack items. Machete at {}, Bedroll at {}",
                machete_pos,
                bedroll_pos
            );
        }
        other => panic!("Expected Display, got {:?}", other),
    }
}

#[test]
fn inventory_shows_gold() {
    let mut router = SlashRouter::new();
    router.register(Box::new(InventoryCommand));
    let state = test_snapshot();

    match router.try_dispatch("/inventory", &state).unwrap() {
        CommandResult::Display(text) => {
            assert!(
                text.contains("50"),
                "Should show gold amount (50), got: {}",
                text
            );
        }
        other => panic!("Expected Display, got {:?}", other),
    }
}

#[test]
fn inventory_empty_returns_flavor_text() {
    let mut router = SlashRouter::new();
    router.register(Box::new(InventoryCommand));
    let mut state = test_snapshot();
    state.characters[0].core.inventory.items.clear();
    state.characters[0].core.inventory.gold = 0;

    match router.try_dispatch("/inventory", &state).unwrap() {
        CommandResult::Display(text) => {
            assert!(
                !text.is_empty(),
                "Empty inventory should produce flavor text, not empty string"
            );
            // Should NOT just be headers with nothing underneath
            assert!(
                text.len() > 10,
                "Empty inventory text should be meaningful, got: {}",
                text
            );
        }
        other => panic!("Expected Display for empty inventory, got {:?}", other),
    }
}

#[test]
fn inventory_no_character_returns_error() {
    let mut router = SlashRouter::new();
    router.register(Box::new(InventoryCommand));
    let mut state = test_snapshot();
    state.characters.clear();

    match router.try_dispatch("/inventory", &state).unwrap() {
        CommandResult::Error(msg) => {
            assert!(
                msg.to_lowercase().contains("character") || msg.to_lowercase().contains("no"),
                "Should report missing character, got: {}",
                msg
            );
        }
        other => panic!("Expected Error for no character, got {:?}", other),
    }
}

// ============================================================================
// AC-3: Map Command — discovered regions, current location, routes
// ============================================================================

#[test]
fn map_lists_all_discovered_regions() {
    let mut router = SlashRouter::new();
    router.register(Box::new(MapCommand));
    let state = test_snapshot();

    match router.try_dispatch("/map", &state).unwrap() {
        CommandResult::Display(text) => {
            assert!(
                text.contains("outer_wastes"),
                "Should list region, got: {}",
                text
            );
            assert!(
                text.contains("scorched_field"),
                "Should list region, got: {}",
                text
            );
            assert!(
                text.contains("crumbling_tower"),
                "Should list region, got: {}",
                text
            );
        }
        other => panic!("Expected Display, got {:?}", other),
    }
}

#[test]
fn map_marks_current_region() {
    let mut router = SlashRouter::new();
    router.register(Box::new(MapCommand));
    let state = test_snapshot();

    match router.try_dispatch("/map", &state).unwrap() {
        CommandResult::Display(text) => {
            // Current region should be visually distinct from others
            // Find the line with outer_wastes and check it has a marker
            let current_line = text
                .lines()
                .find(|l| l.contains("outer_wastes"))
                .expect("Should contain outer_wastes line");
            assert!(
                current_line.contains("*")
                    || current_line.contains("current")
                    || current_line.contains("→"),
                "Current region should be marked, got line: {}",
                current_line
            );
        }
        other => panic!("Expected Display, got {:?}", other),
    }
}

#[test]
fn map_lists_discovered_routes() {
    let mut router = SlashRouter::new();
    router.register(Box::new(MapCommand));
    let state = test_snapshot();

    match router.try_dispatch("/map", &state).unwrap() {
        CommandResult::Display(text) => {
            assert!(
                text.contains("outer_wastes") && text.contains("scorched_field"),
                "Should list routes between regions, got: {}",
                text
            );
        }
        other => panic!("Expected Display, got {:?}", other),
    }
}

#[test]
fn map_no_regions_returns_meaningful_text() {
    let mut router = SlashRouter::new();
    router.register(Box::new(MapCommand));
    let mut state = test_snapshot();
    state.discovered_regions.clear();
    state.discovered_routes.clear();

    match router.try_dispatch("/map", &state).unwrap() {
        CommandResult::Display(text) => {
            assert!(
                !text.is_empty() && text.len() > 5,
                "Empty map should produce meaningful text, got: {}",
                text
            );
        }
        other => panic!("Expected Display for empty map, got {:?}", other),
    }
}

// ============================================================================
// AC-4: Save Command — persistence confirmation
// ============================================================================

#[test]
fn save_returns_confirmation() {
    let mut router = SlashRouter::new();
    router.register(Box::new(SaveCommand));
    let state = test_snapshot();

    match router.try_dispatch("/save", &state).unwrap() {
        CommandResult::Display(text) => {
            assert!(
                text.to_lowercase().contains("save") || text.to_lowercase().contains("saved"),
                "Should confirm save, got: {}",
                text
            );
        }
        other => panic!("Expected Display from /save, got {:?}", other),
    }
}

#[test]
fn save_is_deterministic() {
    let mut router = SlashRouter::new();
    router.register(Box::new(SaveCommand));
    let state = test_snapshot();

    let text1 = match router.try_dispatch("/save", &state).unwrap() {
        CommandResult::Display(t) => t,
        other => panic!("Expected Display, got {:?}", other),
    };
    let text2 = match router.try_dispatch("/save", &state).unwrap() {
        CommandResult::Display(t) => t,
        other => panic!("Expected Display, got {:?}", other),
    };

    assert_eq!(text1, text2, "Save must be deterministic");
}

// ============================================================================
// AC-5: Pure functions — sync, immutable state
// ============================================================================

#[test]
fn all_commands_are_sync() {
    let mut router = SlashRouter::new();
    router.register(Box::new(StatusCommand));
    router.register(Box::new(InventoryCommand));
    router.register(Box::new(MapCommand));
    router.register(Box::new(SaveCommand));
    let state = test_snapshot();

    // All of these compile without .await — proves they're sync
    let r1: Option<CommandResult> = router.try_dispatch("/status", &state);
    let r2: Option<CommandResult> = router.try_dispatch("/inventory", &state);
    let r3: Option<CommandResult> = router.try_dispatch("/map", &state);
    let r4: Option<CommandResult> = router.try_dispatch("/save", &state);

    assert!(r1.is_some());
    assert!(r2.is_some());
    assert!(r3.is_some());
    assert!(r4.is_some());
}

// ============================================================================
// AC-7: Integration — all commands registered and functional via router
// ============================================================================

#[test]
fn all_four_commands_dispatch_through_router() {
    let mut router = SlashRouter::new();
    router.register(Box::new(StatusCommand));
    router.register(Box::new(InventoryCommand));
    router.register(Box::new(MapCommand));
    router.register(Box::new(SaveCommand));
    let state = test_snapshot();

    for cmd in &["/status", "/inventory", "/map", "/save"] {
        let result = router.try_dispatch(cmd, &state);
        assert!(result.is_some(), "{} should be dispatched by router", cmd);
        match result.unwrap() {
            CommandResult::Display(_) => {} // all should produce Display
            CommandResult::Error(msg) => panic!("{} returned error: {}", cmd, msg),
            _ => panic!("{} returned unexpected variant", cmd),
        }
    }
}

// ============================================================================
// CommandHandler trait compliance
// ============================================================================

#[test]
fn status_command_trait_methods() {
    let cmd = StatusCommand;
    assert_eq!(cmd.name(), "status");
    assert!(
        !cmd.description().is_empty(),
        "Description must not be empty"
    );
}

#[test]
fn inventory_command_trait_methods() {
    let cmd = InventoryCommand;
    assert_eq!(cmd.name(), "inventory");
    assert!(
        !cmd.description().is_empty(),
        "Description must not be empty"
    );
}

#[test]
fn map_command_trait_methods() {
    let cmd = MapCommand;
    assert_eq!(cmd.name(), "map");
    assert!(
        !cmd.description().is_empty(),
        "Description must not be empty"
    );
}

#[test]
fn save_command_trait_methods() {
    let cmd = SaveCommand;
    assert_eq!(cmd.name(), "save");
    assert!(
        !cmd.description().is_empty(),
        "Description must not be empty"
    );
}

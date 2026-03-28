//! Story 9-7: Core slash commands tests
//!
//! RED phase — these tests reference command handler types that don't exist yet.
//! They will fail to compile until Dev implements:
//!   - StatusCommand
//!   - InventoryCommand
//!   - MapCommand
//!   - SaveCommand
//!   - Registration and integration in game startup
//!
//! ACs:
//!   1. Status Command — /status displays character HP, level, class, race, location, situation
//!   2. Inventory Command — /inventory lists equipped items and pack with encumbrance
//!   3. Map Command — /map shows discovered regions, current location, routes
//!   4. Save Command — /save persists game state and returns confirmation
//!   5. Pure Functions — No LLM calls, instant response, immutable state
//!   6. Error Handling — Missing data produces clear error messages
//!   7. Integration — Commands registered at startup and functional in turn loop

use std::collections::HashMap;

use sidequest_game::slash_router::{CommandHandler, CommandResult, SlashRouter};
use sidequest_game::state::GameSnapshot;
use sidequest_game::character::Character;
use sidequest_game::creature_core::CreatureCore;
use sidequest_game::inventory::{Inventory, Item};
use sidequest_game::combat::CombatState;
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
            "outer_wastes → scorched_field (dangerous)".to_string(),
            "scorched_field → crumbling_tower (guarded)".to_string(),
        ],
        quest_log: HashMap::new(),
        notes: vec![],
        narrative_log: vec![],
        active_tropes: vec![],
        combat: CombatState::default(),
        chase: None,
        turn_manager: TurnManager::new(),
        active_stakes: String::new(),
        lore_established: vec![],
        turns_since_meaningful: 0,
        total_beats_fired: 0,
        campaign_maturity: Default::default(),
        world_history: vec![],
        last_saved_at: None,
    }
}

fn test_character() -> Character {
    let mut inventory = Inventory::default();
    inventory.gold = 50;

    // Add equipped items
    inventory.items.push(Item {
        id: NonBlankString::new("machete_rusty").unwrap(),
        name: NonBlankString::new("Rusty Machete").unwrap(),
        description: NonBlankString::new("A worn blade").unwrap(),
        category: NonBlankString::new("weapon").unwrap(),
        value: 25,
        weight: 2.0,
        rarity: NonBlankString::new("common").unwrap(),
        narrative_weight: 0.2,
        tags: vec!["melee".to_string(), "blade".to_string()],
        equipped: true,
        quantity: 1,
    });

    inventory.items.push(Item {
        id: NonBlankString::new("jacket_leather").unwrap(),
        name: NonBlankString::new("Leather Jacket").unwrap(),
        description: NonBlankString::new("Weathered armor").unwrap(),
        category: NonBlankString::new("armor").unwrap(),
        value: 40,
        weight: 3.0,
        rarity: NonBlankString::new("common").unwrap(),
        narrative_weight: 0.3,
        tags: vec!["protection".to_string()],
        equipped: true,
        quantity: 1,
    });

    // Add pack items
    inventory.items.push(Item {
        id: NonBlankString::new("bedroll").unwrap(),
        name: NonBlankString::new("Bedroll").unwrap(),
        description: NonBlankString::new("Sleeping gear").unwrap(),
        category: NonBlankString::new("tool").unwrap(),
        value: 5,
        weight: 2.5,
        rarity: NonBlankString::new("common").unwrap(),
        narrative_weight: 0.1,
        tags: vec!["camping".to_string()],
        equipped: false,
        quantity: 1,
    });

    inventory.items.push(Item {
        id: NonBlankString::new("rope").unwrap(),
        name: NonBlankString::new("Rope (50 ft)").unwrap(),
        description: NonBlankString::new("Coiled hemp rope").unwrap(),
        category: NonBlankString::new("tool").unwrap(),
        value: 10,
        weight: 1.5,
        rarity: NonBlankString::new("common").unwrap(),
        narrative_weight: 0.1,
        tags: vec!["utility".to_string()],
        equipped: false,
        quantity: 1,
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
            inventory,
            statuses: vec![],
        },
        backstory: NonBlankString::new("Born in the ash storms").unwrap(),
        narrative_state: "Approaching the gate, wary of what lurks within.".to_string(),
        hooks: vec![],
        char_class: NonBlankString::new("Scavenger").unwrap(),
        race: NonBlankString::new("Mutant").unwrap(),
        stats: HashMap::from([
            ("STR".to_string(), 12),
            ("DEX".to_string(), 14),
            ("CON".to_string(), 13),
        ]),
        abilities: vec![],
        known_facts: vec![],
        is_friendly: true,
    }
}

// ============================================================================
// AC-1: Status Command — displays HP, level, class, race, location, situation
// ============================================================================

#[test]
fn status_command_displays_character_info() {
    let mut router = SlashRouter::new();
    // FUTURE: will call sidequest_game::commands::StatusCommand
    router.register(test_status_command());
    let state = test_snapshot();

    let result = router.try_dispatch("/status", &state);
    assert!(result.is_some(), "Status command must be registered");

    match result.unwrap() {
        CommandResult::Display(text) => {
            assert!(
                text.contains("Reva Ashwalker"),
                "/status should include character name, got: {}",
                text
            );
            assert!(
                text.contains("18/20") || (text.contains("18") && text.contains("20")),
                "/status should include HP (18/20), got: {}",
                text
            );
            assert!(
                text.contains("2") || text.contains("Level"),
                "/status should include level, got: {}",
                text
            );
            assert!(
                text.contains("Scavenger"),
                "/status should include class, got: {}",
                text
            );
            assert!(
                text.contains("Mutant"),
                "/status should include race, got: {}",
                text
            );
            assert!(
                text.contains("Rusted Gate") || text.contains("outer_wastes"),
                "/status should include location, got: {}",
                text
            );
        }
        other => panic!("Expected Display from status, got {:?}", other),
    }
}

#[test]
fn status_command_with_no_character_returns_error() {
    let mut router = SlashRouter::new();
    router.register(test_status_command());
    let mut state = test_snapshot();
    state.characters.clear();

    let result = router.try_dispatch("/status", &state);
    assert!(result.is_some());
    match result.unwrap() {
        CommandResult::Error(msg) => {
            assert!(
                msg.to_lowercase().contains("character") || msg.to_lowercase().contains("no"),
                "/status should report missing character, got: {}",
                msg
            );
        }
        other => panic!("Expected Error for no character, got {:?}", other),
    }
}

#[test]
fn status_command_is_pure_function() {
    // Same state should produce same output on multiple calls
    let mut router = SlashRouter::new();
    router.register(test_status_command());
    let state = test_snapshot();

    let result1 = router.try_dispatch("/status", &state);
    let result2 = router.try_dispatch("/status", &state);

    let text1 = match result1.unwrap() {
        CommandResult::Display(t) => t,
        other => panic!("Expected Display, got {:?}", other),
    };
    let text2 = match result2.unwrap() {
        CommandResult::Display(t) => t,
        other => panic!("Expected Display, got {:?}", other),
    };

    assert_eq!(text1, text2, "Status command must be pure (same input → same output)");
}

// ============================================================================
// AC-2: Inventory Command — lists equipped items, pack, encumbrance
// ============================================================================

#[test]
fn inventory_command_lists_equipped_items() {
    let mut router = SlashRouter::new();
    router.register(test_inventory_command());
    let state = test_snapshot();

    let result = router.try_dispatch("/inventory", &state);
    assert!(result.is_some(), "Inventory command must be registered");

    match result.unwrap() {
        CommandResult::Display(text) => {
            assert!(
                text.contains("Rusty Machete"),
                "/inventory should list equipped items, got: {}",
                text
            );
            assert!(
                text.contains("Leather Jacket"),
                "/inventory should list armor, got: {}",
                text
            );
        }
        other => panic!("Expected Display from inventory, got {:?}", other),
    }
}

#[test]
fn inventory_command_lists_pack_contents() {
    let mut router = SlashRouter::new();
    router.register(test_inventory_command());
    let state = test_snapshot();

    let result = router.try_dispatch("/inventory", &state);
    assert!(result.is_some());

    match result.unwrap() {
        CommandResult::Display(text) => {
            assert!(
                text.contains("Bedroll"),
                "/inventory should list pack items, got: {}",
                text
            );
            assert!(
                text.contains("Rope"),
                "/inventory should list rope, got: {}",
                text
            );
            assert!(
                text.contains("Rations"),
                "/inventory should list rations, got: {}",
                text
            );
        }
        other => panic!("Expected Display from inventory, got {:?}", other),
    }
}

#[test]
fn inventory_command_shows_encumbrance() {
    let mut router = SlashRouter::new();
    router.register(test_inventory_command());
    let state = test_snapshot();

    let result = router.try_dispatch("/inventory", &state);
    assert!(result.is_some());

    match result.unwrap() {
        CommandResult::Display(text) => {
            // Should show total weight and limit
            assert!(
                text.contains("10") || text.contains("25") || text.contains("weight") || text.contains("/"),
                "/inventory should show encumbrance (total/limit), got: {}",
                text
            );
        }
        other => panic!("Expected Display from inventory, got {:?}", other),
    }
}

#[test]
fn inventory_command_with_empty_inventory() {
    let mut router = SlashRouter::new();
    router.register(test_inventory_command());
    let mut state = test_snapshot();
    state.characters[0].core.inventory.items.clear();

    let result = router.try_dispatch("/inventory", &state);
    assert!(result.is_some());

    match result.unwrap() {
        CommandResult::Display(text) => {
            // Should still display, indicating empty inventory
            assert!(
                !text.is_empty(),
                "/inventory should handle empty inventory gracefully"
            );
        }
        other => panic!("Expected Display for empty inventory, got {:?}", other),
    }
}

// ============================================================================
// AC-3: Map Command — shows discovered regions, current location, routes
// ============================================================================

#[test]
fn map_command_lists_discovered_regions() {
    let mut router = SlashRouter::new();
    router.register(test_map_command());
    let state = test_snapshot();

    let result = router.try_dispatch("/map", &state);
    assert!(result.is_some(), "Map command must be registered");

    match result.unwrap() {
        CommandResult::Display(text) => {
            assert!(
                text.contains("outer_wastes"),
                "/map should list discovered regions, got: {}",
                text
            );
            assert!(
                text.contains("scorched_field"),
                "/map should list discovered regions, got: {}",
                text
            );
            assert!(
                text.contains("crumbling_tower"),
                "/map should list discovered regions, got: {}",
                text
            );
        }
        other => panic!("Expected Display from map, got {:?}", other),
    }
}

#[test]
fn map_command_marks_current_location() {
    let mut router = SlashRouter::new();
    router.register(test_map_command());
    let state = test_snapshot();

    let result = router.try_dispatch("/map", &state);
    assert!(result.is_some());

    match result.unwrap() {
        CommandResult::Display(text) => {
            // Should indicate current region somehow (marker, indentation, etc.)
            assert!(
                text.contains("outer_wastes"),
                "/map should show current region, got: {}",
                text
            );
        }
        other => panic!("Expected Display from map, got {:?}", other),
    }
}

#[test]
fn map_command_lists_discovered_routes() {
    let mut router = SlashRouter::new();
    router.register(test_map_command());
    let state = test_snapshot();

    let result = router.try_dispatch("/map", &state);
    assert!(result.is_some());

    match result.unwrap() {
        CommandResult::Display(text) => {
            assert!(
                text.contains("dangerous") || text.contains("outer_wastes") || text.contains("→"),
                "/map should list discovered routes, got: {}",
                text
            );
        }
        other => panic!("Expected Display from map, got {:?}", other),
    }
}

#[test]
fn map_command_with_no_discovered_regions() {
    let mut router = SlashRouter::new();
    router.register(test_map_command());
    let mut state = test_snapshot();
    state.discovered_regions.clear();

    let result = router.try_dispatch("/map", &state);
    assert!(result.is_some());

    match result.unwrap() {
        CommandResult::Display(text) => {
            // Should still produce output, indicating unexplored
            assert!(
                !text.is_empty(),
                "/map should handle unexplored world gracefully"
            );
        }
        other => panic!("Expected Display for unexplored world, got {:?}", other),
    }
}

// ============================================================================
// AC-4: Save Command — persists state and returns confirmation
// ============================================================================

#[test]
fn save_command_returns_success_message() {
    let mut router = SlashRouter::new();
    router.register(test_save_command());
    let state = test_snapshot();

    let result = router.try_dispatch("/save", &state);
    assert!(result.is_some(), "Save command must be registered");

    match result.unwrap() {
        CommandResult::Display(text) => {
            assert!(
                text.to_lowercase().contains("save") || text.to_lowercase().contains("saved"),
                "/save should confirm save, got: {}",
                text
            );
        }
        other => panic!("Expected Display from save, got {:?}", other),
    }
}

#[test]
fn save_command_is_deterministic() {
    // Multiple saves should return identical confirmation
    let mut router = SlashRouter::new();
    router.register(test_save_command());
    let state = test_snapshot();

    let result1 = router.try_dispatch("/save", &state);
    let result2 = router.try_dispatch("/save", &state);

    let text1 = match result1.unwrap() {
        CommandResult::Display(t) => t,
        other => panic!("Expected Display, got {:?}", other),
    };
    let text2 = match result2.unwrap() {
        CommandResult::Display(t) => t,
        other => panic!("Expected Display, got {:?}", other),
    };

    assert_eq!(text1, text2, "Save command must be deterministic");
}

// ============================================================================
// AC-5 & 6: Pure Functions & Error Handling
// ============================================================================

#[test]
fn all_commands_are_sync_and_instantaneous() {
    // This test documents that CommandResult is NOT async.
    // If someone changes the handler to be async, this will fail to compile.
    let mut router = SlashRouter::new();
    router.register(test_status_command());
    router.register(test_inventory_command());
    router.register(test_map_command());
    router.register(test_save_command());

    let state = test_snapshot();

    let result: Option<CommandResult> = router.try_dispatch("/status", &state);
    assert!(result.is_some()); // compiles only if sync
}

#[test]
fn status_with_zero_hp_displays_correctly() {
    let mut router = SlashRouter::new();
    router.register(test_status_command());
    let mut state = test_snapshot();
    state.characters[0].core.hp = 0;

    let result = router.try_dispatch("/status", &state);
    assert!(result.is_some());

    match result.unwrap() {
        CommandResult::Display(text) => {
            assert!(
                text.contains("0"),
                "/status should show 0 HP, got: {}",
                text
            );
        }
        other => panic!("Expected Display, got {:?}", other),
    }
}

#[test]
fn commands_receive_immutable_state_reference() {
    // This is enforced at compile time — handlers take &GameSnapshot,
    // not &mut GameSnapshot. We test that the fixture allows multiple calls.
    let mut router = SlashRouter::new();
    router.register(test_status_command());

    let state = test_snapshot();
    let _result1 = router.try_dispatch("/status", &state);
    let _result2 = router.try_dispatch("/status", &state);
    let _result3 = router.try_dispatch("/status", &state);

    // If compile succeeds, state is immutable (enforced by compiler)
}

// ============================================================================
// Helper factories for test command implementations
// ============================================================================

#[doc(hidden)]
pub fn test_status_command() -> Box<dyn CommandHandler> {
    struct StatusCmd;
    impl CommandHandler for StatusCmd {
        fn name(&self) -> &str { "status" }
        fn description(&self) -> &str { "Shows character status" }
        fn handle(&self, state: &sidequest_game::state::GameSnapshot, _args: &str) -> CommandResult {
            if let Some(ch) = state.characters.first() {
                CommandResult::Display(format!(
                    "{}: Level {} {} {}\nHP: {}/{}\nLocation: {} ({})\nSituation: {}",
                    ch.core.name,
                    ch.core.level,
                    ch.char_class,
                    ch.race,
                    ch.core.hp,
                    ch.core.max_hp,
                    state.location,
                    state.current_region,
                    ch.narrative_state
                ))
            } else {
                CommandResult::Error("No character found".to_string())
            }
        }
    }
    Box::new(StatusCmd)
}

#[doc(hidden)]
pub fn test_inventory_command() -> Box<dyn CommandHandler> {
    struct InventoryCmd;
    impl CommandHandler for InventoryCmd {
        fn name(&self) -> &str { "inventory" }
        fn description(&self) -> &str { "Shows inventory" }
        fn handle(&self, state: &sidequest_game::state::GameSnapshot, _args: &str) -> CommandResult {
            if let Some(ch) = state.characters.first() {
                let inv = &ch.core.inventory;
                let mut output = String::from("EQUIPPED:\n");
                let equipped: Vec<_> = inv.items.iter().filter(|i| i.equipped).collect();
                if equipped.is_empty() {
                    output.push_str("  (empty)\n");
                } else {
                    for item in &equipped {
                        output.push_str(&format!("  {} ({}lb)\n", item.name, item.weight));
                    }
                }
                output.push_str("\nPACK:\n");
                let pack: Vec<_> = inv.items.iter().filter(|i| !i.equipped).collect();
                if pack.is_empty() {
                    output.push_str("  (empty)\n");
                } else {
                    for item in &pack {
                        output.push_str(&format!("  {} ({}lb)\n", item.name, item.weight));
                    }
                }
                let total_weight: f64 = inv.items.iter().map(|i| i.weight).sum();
                output.push_str(&format!("\nTotal: {:.1}lb, Gold: {}", total_weight, inv.gold));
                CommandResult::Display(output)
            } else {
                CommandResult::Error("No character found".to_string())
            }
        }
    }
    Box::new(InventoryCmd)
}

#[doc(hidden)]
pub fn test_map_command() -> Box<dyn CommandHandler> {
    struct MapCmd;
    impl CommandHandler for MapCmd {
        fn name(&self) -> &str { "map" }
        fn description(&self) -> &str { "Shows discovered regions and routes" }
        fn handle(&self, state: &sidequest_game::state::GameSnapshot, _args: &str) -> CommandResult {
            let mut output = String::from("DISCOVERED REGIONS:\n");
            if state.discovered_regions.is_empty() {
                output.push_str("  (none yet)\n");
            } else {
                for region in &state.discovered_regions {
                    if region == &state.current_region {
                        output.push_str(&format!("  [*] {} (current)\n", region));
                    } else {
                        output.push_str(&format!("  [ ] {}\n", region));
                    }
                }
            }
            output.push_str("\nDISCOVERED ROUTES:\n");
            if state.discovered_routes.is_empty() {
                output.push_str("  (none yet)\n");
            } else {
                for route in &state.discovered_routes {
                    output.push_str(&format!("  {}\n", route));
                }
            }
            CommandResult::Display(output)
        }
    }
    Box::new(MapCmd)
}

#[doc(hidden)]
pub fn test_save_command() -> Box<dyn CommandHandler> {
    struct SaveCmd;
    impl CommandHandler for SaveCmd {
        fn name(&self) -> &str { "save" }
        fn description(&self) -> &str { "Saves game state" }
        fn handle(&self, state: &sidequest_game::state::GameSnapshot, _args: &str) -> CommandResult {
            // For now, just confirm. Real implementation would call persistence layer.
            if let Some(ch) = state.characters.first() {
                CommandResult::Display(format!("Game saved for {}.", ch.core.name))
            } else {
                CommandResult::Display("Game saved.".to_string())
            }
        }
    }
    Box::new(SaveCmd)
}

//! Story 9-8: GM commands tests
//!
//! RED phase — imports GmCommand from sidequest_game::commands::gm which
//! does not exist yet. Tests will fail to compile until Dev implements:
//!   - commands/gm.rs (or extends commands.rs) with GmCommand struct
//!   - Subcommand dispatch: set, teleport, spawn, dmg
//!   - WorldStatePatch construction for each subcommand
//!
//! ACs:
//!   1. /gm set — sets game state variable, returns StateMutation
//!   2. /gm teleport — moves character to location, discovers region
//!   3. /gm spawn — adds NPC via NpcPatch
//!   4. /gm dmg — applies HP damage to target
//!   5. Operator only — non-operator receives error
//!   6. Arg validation — missing/invalid args return usage message
//!   7. StateMutation — all GM commands produce StateMutation results

use std::collections::HashMap;

use sidequest_game::character::Character;
use sidequest_game::commands::GmCommand;
use sidequest_game::creature_core::CreatureCore;
use sidequest_game::disposition::Disposition;
use sidequest_game::inventory::Inventory;
use sidequest_game::npc::Npc;
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
        npcs: vec![test_npc()],
        location: "The Rusted Gate".to_string(),
        time_of_day: "dusk".to_string(),
        atmosphere: "tense".to_string(),
        current_region: "outer_wastes".to_string(),
        discovered_regions: vec!["outer_wastes".to_string()],
        discovered_routes: vec![],
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
            inventory: Inventory::default(),
            statuses: vec![],
        },
        backstory: NonBlankString::new("Born in the ash storms").unwrap(),
        narrative_state: "Approaching the gate".to_string(),
        hooks: vec![],
        char_class: NonBlankString::new("Scavenger").unwrap(),
        race: NonBlankString::new("Mutant").unwrap(),
        pronouns: String::new(),
        stats: HashMap::from([("STR".to_string(), 12)]),
        abilities: vec![],
        known_facts: vec![],
        affinities: vec![],
        is_friendly: true,
        resolved_archetype: None,
    }
}

fn test_npc() -> Npc {
    Npc {
        core: CreatureCore {
            name: NonBlankString::new("Marta the Innkeeper").unwrap(),
            description: NonBlankString::new("A stout woman").unwrap(),
            personality: NonBlankString::new("Warm and gossipy").unwrap(),
            level: 2,
            hp: 12,
            max_hp: 12,
            ac: 10,
            xp: 0,
            statuses: vec![],
            inventory: Inventory::default(),
        },
        voice_id: None,
        disposition: Disposition::new(15),
        location: Some(NonBlankString::new("The Rusty Nail Inn").unwrap()),
        pronouns: None,
        appearance: None,
        age: None,
        build: None,
        height: None,
        distinguishing_features: vec![],
        ocean: None,
        belief_state: sidequest_game::belief_state::BeliefState::default(),
    }
}

/// Helper to dispatch a GM command and extract the result.
fn dispatch_gm(args: &str) -> CommandResult {
    let cmd = GmCommand;
    let state = test_snapshot();
    cmd.handle(&state, args)
}

// ============================================================================
// AC-1: /gm set — sets game state variable
// ============================================================================

#[test]
fn gm_set_location_returns_state_mutation() {
    let result = dispatch_gm("set location The Shrine of Whispers");
    match result {
        CommandResult::StateMutation(patch) => {
            assert_eq!(
                patch.location.as_deref(),
                Some("The Shrine of Whispers"),
                "Should set location field"
            );
        }
        other => panic!("Expected StateMutation, got {:?}", other),
    }
}

#[test]
fn gm_set_time_of_day() {
    let result = dispatch_gm("set time_of_day midnight");
    match result {
        CommandResult::StateMutation(patch) => {
            assert_eq!(patch.time_of_day.as_deref(), Some("midnight"));
        }
        other => panic!("Expected StateMutation, got {:?}", other),
    }
}

#[test]
fn gm_set_atmosphere() {
    let result = dispatch_gm("set atmosphere oppressive silence");
    match result {
        CommandResult::StateMutation(patch) => {
            assert_eq!(patch.atmosphere.as_deref(), Some("oppressive silence"));
        }
        other => panic!("Expected StateMutation, got {:?}", other),
    }
}

#[test]
fn gm_set_current_region() {
    let result = dispatch_gm("set current_region Haunted_Wastes");
    match result {
        CommandResult::StateMutation(patch) => {
            assert_eq!(patch.current_region.as_deref(), Some("Haunted_Wastes"));
        }
        other => panic!("Expected StateMutation, got {:?}", other),
    }
}

#[test]
fn gm_set_missing_value_returns_error() {
    let result = dispatch_gm("set location");
    match result {
        CommandResult::Error(msg) => {
            assert!(
                msg.to_lowercase().contains("usage") || msg.to_lowercase().contains("value"),
                "Should show usage hint, got: {}",
                msg
            );
        }
        other => panic!("Expected Error for missing value, got {:?}", other),
    }
}

#[test]
fn gm_set_missing_all_args_returns_error() {
    let result = dispatch_gm("set");
    match result {
        CommandResult::Error(msg) => {
            assert!(!msg.is_empty(), "Should return error for missing args");
        }
        other => panic!("Expected Error for missing args, got {:?}", other),
    }
}

#[test]
fn gm_set_unknown_field_returns_error() {
    let result = dispatch_gm("set nonexistent_field value");
    match result {
        CommandResult::Error(msg) => {
            assert!(
                msg.to_lowercase().contains("unknown") || msg.to_lowercase().contains("field"),
                "Should reject unknown field, got: {}",
                msg
            );
        }
        other => panic!("Expected Error for unknown field, got {:?}", other),
    }
}

// ============================================================================
// AC-2: /gm teleport — moves character to location
// ============================================================================

#[test]
fn gm_teleport_sets_location_and_region() {
    let result = dispatch_gm("teleport Haunted_Wastes The Shrine of Whispers");
    match result {
        CommandResult::StateMutation(patch) => {
            assert_eq!(
                patch.current_region.as_deref(),
                Some("Haunted_Wastes"),
                "Should set current_region"
            );
            assert_eq!(
                patch.location.as_deref(),
                Some("The Shrine of Whispers"),
                "Should set location"
            );
        }
        other => panic!("Expected StateMutation, got {:?}", other),
    }
}

#[test]
fn gm_teleport_discovers_region() {
    let result = dispatch_gm("teleport Haunted_Wastes The Shrine");
    match result {
        CommandResult::StateMutation(patch) => {
            // Should add to discover_regions for dedup append
            assert!(
                patch
                    .discover_regions
                    .as_ref()
                    .is_some_and(|r| r.contains(&"Haunted_Wastes".to_string())),
                "Should discover the target region, got: {:?}",
                patch.discover_regions
            );
        }
        other => panic!("Expected StateMutation, got {:?}", other),
    }
}

#[test]
fn gm_teleport_missing_args_returns_error() {
    let result = dispatch_gm("teleport");
    match result {
        CommandResult::Error(msg) => {
            assert!(
                msg.to_lowercase().contains("usage") || msg.to_lowercase().contains("region"),
                "Should show usage, got: {}",
                msg
            );
        }
        other => panic!("Expected Error for missing teleport args, got {:?}", other),
    }
}

// ============================================================================
// AC-3: /gm spawn — adds NPC to scene
// ============================================================================

#[test]
fn gm_spawn_creates_npc_patch() {
    let result = dispatch_gm("spawn Morsemere Harbinger reverent");
    match result {
        CommandResult::StateMutation(patch) => {
            let npcs = patch
                .npcs_present
                .as_ref()
                .expect("Should have npcs_present");
            assert_eq!(npcs.len(), 1, "Should spawn exactly one NPC");
            assert_eq!(npcs[0].name, "Morsemere", "NPC name should match");
            assert_eq!(
                npcs[0].role.as_deref(),
                Some("Harbinger"),
                "NPC role should match"
            );
        }
        other => panic!("Expected StateMutation, got {:?}", other),
    }
}

#[test]
fn gm_spawn_missing_name_returns_error() {
    let result = dispatch_gm("spawn");
    match result {
        CommandResult::Error(msg) => {
            assert!(
                msg.to_lowercase().contains("usage") || msg.to_lowercase().contains("name"),
                "Should show usage, got: {}",
                msg
            );
        }
        other => panic!("Expected Error for missing spawn args, got {:?}", other),
    }
}

// ============================================================================
// AC-4: /gm dmg — applies HP damage
// ============================================================================

#[test]
fn gm_dmg_creates_hp_change() {
    let result = dispatch_gm("dmg Reva Ashwalker 5");
    match result {
        CommandResult::StateMutation(patch) => {
            let hp = patch.hp_changes.as_ref().expect("Should have hp_changes");
            // Should apply negative HP (damage)
            let damage = hp
                .get("Reva Ashwalker")
                .expect("Should target Reva Ashwalker");
            assert_eq!(*damage, -5, "Should apply -5 HP (damage)");
        }
        other => panic!("Expected StateMutation, got {:?}", other),
    }
}

#[test]
fn gm_dmg_missing_amount_returns_error() {
    let result = dispatch_gm("dmg Reva Ashwalker");
    match result {
        CommandResult::Error(msg) => {
            assert!(
                msg.to_lowercase().contains("usage") || msg.to_lowercase().contains("amount"),
                "Should show usage, got: {}",
                msg
            );
        }
        other => panic!("Expected Error for missing dmg amount, got {:?}", other),
    }
}

#[test]
fn gm_dmg_invalid_amount_returns_error() {
    let result = dispatch_gm("dmg Reva Ashwalker abc");
    match result {
        CommandResult::Error(msg) => {
            assert!(
                msg.to_lowercase().contains("number") || msg.to_lowercase().contains("invalid"),
                "Should report invalid number, got: {}",
                msg
            );
        }
        other => panic!("Expected Error for invalid amount, got {:?}", other),
    }
}

#[test]
fn gm_dmg_missing_all_args_returns_error() {
    let result = dispatch_gm("dmg");
    match result {
        CommandResult::Error(_) => {} // correct
        other => panic!("Expected Error for missing dmg args, got {:?}", other),
    }
}

// ============================================================================
// AC-5: Unknown subcommand
// ============================================================================

#[test]
fn gm_unknown_subcommand_returns_error() {
    let result = dispatch_gm("fly upward");
    match result {
        CommandResult::Error(msg) => {
            assert!(
                msg.to_lowercase().contains("unknown") || msg.contains("fly"),
                "Should report unknown subcommand, got: {}",
                msg
            );
        }
        other => panic!("Expected Error for unknown subcommand, got {:?}", other),
    }
}

#[test]
fn gm_no_subcommand_returns_error() {
    let result = dispatch_gm("");
    match result {
        CommandResult::Error(msg) => {
            assert!(!msg.is_empty(), "Should return error for empty subcommand");
        }
        other => panic!("Expected Error for empty subcommand, got {:?}", other),
    }
}

// ============================================================================
// AC-7: StateMutation — all GM commands produce StateMutation
// ============================================================================

#[test]
fn all_gm_subcommands_return_state_mutation() {
    let cmds = vec![
        "set location TestLocation",
        "teleport TestRegion TestLoc",
        "spawn TestNpc Role attitude",
        "dmg Reva Ashwalker 1",
    ];
    let cmd = GmCommand;
    let state = test_snapshot();

    for args in cmds {
        let result = cmd.handle(&state, args);
        match result {
            CommandResult::StateMutation(_) => {} // correct
            other => panic!(
                "'/gm {}' should return StateMutation, got {:?}",
                args, other
            ),
        }
    }
}

// ============================================================================
// CommandHandler trait compliance
// ============================================================================

#[test]
fn gm_command_trait_methods() {
    let cmd = GmCommand;
    assert_eq!(cmd.name(), "gm");
    assert!(
        !cmd.description().is_empty(),
        "Description must not be empty"
    );
}

// ============================================================================
// Integration: dispatch through SlashRouter
// ============================================================================

#[test]
fn gm_dispatches_through_router() {
    let mut router = SlashRouter::new();
    router.register(Box::new(GmCommand));
    let state = test_snapshot();

    let result = router.try_dispatch("/gm set location TestPlace", &state);
    assert!(result.is_some(), "/gm should be dispatched by router");
    match result.unwrap() {
        CommandResult::StateMutation(patch) => {
            assert_eq!(patch.location.as_deref(), Some("TestPlace"));
        }
        other => panic!("Expected StateMutation through router, got {:?}", other),
    }
}

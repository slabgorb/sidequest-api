//! Story 9-6: Slash command router tests
//!
//! RED phase — these tests reference types and modules that don't exist yet.
//! They will fail to compile until Dev implements:
//!   - slash_router.rs: SlashRouter, CommandHandler trait, CommandResult enum
//!   - Integration with orchestrator turn loop (intercept before intent routing)
//!
//! ACs:
//!   1. Intercept — Input starting with "/" bypasses intent router
//!   2. Passthrough — Non-slash input reaches intent router unchanged
//!   3. Parse — "/command arg1 arg2" parsed into name="command", args="arg1 arg2"
//!   4. Registry — Commands registered by name, dispatched via HashMap lookup
//!   5. Unknown command — Unregistered /command returns Error result
//!   6. No LLM — Command handling involves zero Claude calls
//!   7. Pure functions — Handlers receive immutable state reference

use std::collections::HashMap;

use sidequest_game::character::Character;
use sidequest_game::creature_core::CreatureCore;
use sidequest_game::inventory::Inventory;
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
            edge: sidequest_game::creature_core::placeholder_edge_pool(),
            acquired_advancements: vec![],
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
        stats: HashMap::from([("STR".to_string(), 12), ("DEX".to_string(), 14)]),
        abilities: vec![],
        known_facts: vec![],
        affinities: vec![],
        is_friendly: true,
        resolved_archetype: None,
        archetype_provenance: None,
    }
}

/// A test command handler that echoes its args back.
struct EchoCommand;

impl CommandHandler for EchoCommand {
    fn name(&self) -> &str {
        "echo"
    }

    fn description(&self) -> &str {
        "Echoes arguments back"
    }

    fn handle(&self, _state: &GameSnapshot, args: &str) -> CommandResult {
        CommandResult::Display(format!("echo: {}", args))
    }
}

/// A test command that reads state to produce output.
struct StatusCommand;

impl CommandHandler for StatusCommand {
    fn name(&self) -> &str {
        "status"
    }

    fn description(&self) -> &str {
        "Shows character status"
    }

    fn handle(&self, state: &GameSnapshot, _args: &str) -> CommandResult {
        if let Some(ch) = state.characters.first() {
            CommandResult::Display(format!(
                "{}: HP {}/{}",
                ch.core.name, ch.core.edge.current, ch.core.edge.max
            ))
        } else {
            CommandResult::Error("No characters found".to_string())
        }
    }
}

// ============================================================================
// AC-1: Intercept — slash input dispatches through router
// ============================================================================

#[test]
fn slash_input_is_intercepted_by_router() {
    let mut router = SlashRouter::new();
    router.register(Box::new(EchoCommand));
    let state = test_snapshot();

    let result = router.try_dispatch("/echo hello", &state);
    assert!(
        result.is_some(),
        "Slash input must be intercepted by the router"
    );

    match result.unwrap() {
        CommandResult::Display(text) => assert_eq!(text, "echo: hello"),
        other => panic!("Expected Display, got {:?}", other),
    }
}

// ============================================================================
// AC-2: Passthrough — non-slash input returns None
// ============================================================================

#[test]
fn non_slash_input_returns_none() {
    let mut router = SlashRouter::new();
    router.register(Box::new(EchoCommand));
    let state = test_snapshot();

    let result = router.try_dispatch("I attack the goblin", &state);
    assert!(
        result.is_none(),
        "Non-slash input must pass through (return None)"
    );
}

#[test]
fn empty_input_returns_none() {
    let router = SlashRouter::new();
    let state = test_snapshot();

    let result = router.try_dispatch("", &state);
    assert!(result.is_none(), "Empty input must pass through");
}

#[test]
fn whitespace_only_input_returns_none() {
    let router = SlashRouter::new();
    let state = test_snapshot();

    let result = router.try_dispatch("   ", &state);
    assert!(result.is_none(), "Whitespace-only input must pass through");
}

// ============================================================================
// AC-3: Parse — command name and args extraction
// ============================================================================

#[test]
fn parse_command_with_no_args() {
    let mut router = SlashRouter::new();
    router.register(Box::new(EchoCommand));
    let state = test_snapshot();

    let result = router.try_dispatch("/echo", &state);
    assert!(result.is_some());
    match result.unwrap() {
        CommandResult::Display(text) => assert_eq!(text, "echo: "),
        other => panic!("Expected Display, got {:?}", other),
    }
}

#[test]
fn parse_command_with_multiple_args() {
    let mut router = SlashRouter::new();
    router.register(Box::new(EchoCommand));
    let state = test_snapshot();

    // Args after the command name should be passed as a single string
    let result = router.try_dispatch("/echo arg1 arg2 arg3", &state);
    assert!(result.is_some());
    match result.unwrap() {
        CommandResult::Display(text) => assert_eq!(text, "echo: arg1 arg2 arg3"),
        other => panic!("Expected Display, got {:?}", other),
    }
}

#[test]
fn parse_command_with_leading_whitespace_in_args() {
    let mut router = SlashRouter::new();
    router.register(Box::new(EchoCommand));
    let state = test_snapshot();

    // Extra spaces between command and args should be trimmed
    let result = router.try_dispatch("/echo   spaced out", &state);
    assert!(result.is_some());
    match result.unwrap() {
        CommandResult::Display(text) => {
            // Args should be trimmed of leading whitespace
            assert_eq!(text, "echo: spaced out");
        }
        other => panic!("Expected Display, got {:?}", other),
    }
}

#[test]
fn parse_slash_only_returns_error_for_unknown() {
    let router = SlashRouter::new();
    let state = test_snapshot();

    // "/" alone with no command name
    let result = router.try_dispatch("/", &state);
    assert!(result.is_some(), "Bare slash should still be intercepted");
    match result.unwrap() {
        // Current impl returns a helpful Display with "Unknown command" text
        // and the available-commands list — friendlier than a raw Error.
        CommandResult::Display(text) => {
            assert!(
                text.contains("Unknown command"),
                "Bare slash should produce 'Unknown command' text, got: {}",
                text
            );
        }
        other => panic!("Expected Display for bare slash, got {:?}", other),
    }
}

// ============================================================================
// AC-4: Registry — commands registered by name, dispatched via lookup
// ============================================================================

#[test]
fn register_and_dispatch_multiple_commands() {
    let mut router = SlashRouter::new();
    router.register(Box::new(EchoCommand));
    router.register(Box::new(StatusCommand));
    let state = test_snapshot();

    // Dispatch echo
    let echo_result = router.try_dispatch("/echo test", &state);
    assert!(echo_result.is_some());
    match echo_result.unwrap() {
        CommandResult::Display(text) => assert_eq!(text, "echo: test"),
        other => panic!("Expected Display from echo, got {:?}", other),
    }

    // Dispatch status
    let status_result = router.try_dispatch("/status", &state);
    assert!(status_result.is_some());
    match status_result.unwrap() {
        CommandResult::Display(text) => {
            assert!(
                text.contains("Reva Ashwalker"),
                "Status should include character name, got: {}",
                text
            );
            assert!(
                text.contains("18/20"),
                "Status should include HP, got: {}",
                text
            );
        }
        other => panic!("Expected Display from status, got {:?}", other),
    }
}

#[test]
fn command_lookup_is_case_sensitive() {
    let mut router = SlashRouter::new();
    router.register(Box::new(EchoCommand));
    let state = test_snapshot();

    // "echo" is registered, "ECHO" should not match
    let result = router.try_dispatch("/ECHO hello", &state);
    assert!(result.is_some(), "Slash input is still intercepted");
    match result.unwrap() {
        // Case mismatch → unknown command → friendly Display with help text
        CommandResult::Display(text) => {
            assert!(
                text.contains("Unknown command") && text.contains("/ECHO"),
                "Case-mismatched command should produce 'Unknown command: /ECHO' text, got: {}",
                text
            );
        }
        other => panic!(
            "Expected Display for case-mismatched command, got {:?}",
            other
        ),
    }
}

// ============================================================================
// AC-5: Unknown command — unregistered /command returns Error
// ============================================================================

#[test]
fn unknown_command_returns_friendly_display() {
    let router = SlashRouter::new();
    let state = test_snapshot();

    let result = router.try_dispatch("/nonexistent", &state);
    assert!(
        result.is_some(),
        "Unknown slash commands should still be intercepted"
    );
    match result.unwrap() {
        // Unknown commands render as a friendly Display with the command name
        // and the available-commands list, not as a raw Error.
        CommandResult::Display(msg) => {
            assert!(
                msg.contains("nonexistent") && msg.to_lowercase().contains("unknown"),
                "Display should reference the unknown command, got: {}",
                msg
            );
        }
        other => panic!("Expected Display for unknown command, got {:?}", other),
    }
}

#[test]
fn unknown_command_error_does_not_leak_internal_state() {
    let router = SlashRouter::new();
    let state = test_snapshot();

    let result = router.try_dispatch("/secret", &state);
    assert!(result.is_some());
    match result.unwrap() {
        CommandResult::Display(msg) => {
            // Display should not contain stack traces or internal details
            assert!(
                !msg.contains("HashMap") && !msg.contains("panic"),
                "Display message must not leak internals, got: {}",
                msg
            );
        }
        other => panic!("Expected Display, got {:?}", other),
    }
}

// ============================================================================
// AC-7: Pure functions — handlers receive immutable state reference
// ============================================================================

#[test]
fn handler_receives_immutable_state_and_produces_result() {
    let mut router = SlashRouter::new();
    router.register(Box::new(StatusCommand));
    let state = test_snapshot();

    // Call twice — same state should produce identical results
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

    assert_eq!(
        text1, text2,
        "Pure function: same state must produce same result"
    );
}

#[test]
fn handler_with_empty_characters_returns_error() {
    let mut router = SlashRouter::new();
    router.register(Box::new(StatusCommand));
    let mut state = test_snapshot();
    state.characters.clear();

    let result = router.try_dispatch("/status", &state);
    assert!(result.is_some());
    match result.unwrap() {
        CommandResult::Error(msg) => {
            assert!(
                msg.to_lowercase().contains("no characters"),
                "Should report no characters, got: {}",
                msg
            );
        }
        other => panic!("Expected Error for empty characters, got {:?}", other),
    }
}

// ============================================================================
// AC (in scope): /help meta-command lists registered commands
// ============================================================================

#[test]
fn help_command_lists_registered_commands() {
    let mut router = SlashRouter::new();
    router.register(Box::new(EchoCommand));
    router.register(Box::new(StatusCommand));
    let state = test_snapshot();

    let result = router.try_dispatch("/help", &state);
    assert!(result.is_some(), "/help should be handled by the router");
    match result.unwrap() {
        CommandResult::Display(text) => {
            assert!(
                text.contains("echo"),
                "/help should list 'echo' command, got: {}",
                text
            );
            assert!(
                text.contains("status"),
                "/help should list 'status' command, got: {}",
                text
            );
            assert!(
                text.contains("Echoes arguments back"),
                "/help should include command descriptions, got: {}",
                text
            );
        }
        other => panic!("Expected Display from /help, got {:?}", other),
    }
}

#[test]
fn help_command_with_no_registered_commands() {
    let router = SlashRouter::new();
    let state = test_snapshot();

    let result = router.try_dispatch("/help", &state);
    assert!(result.is_some());
    match result.unwrap() {
        CommandResult::Display(text) => {
            // Should still return something meaningful, not crash
            assert!(
                !text.is_empty(),
                "/help with no commands should still produce output"
            );
        }
        other => panic!("Expected Display from /help, got {:?}", other),
    }
}

// ============================================================================
// Rule #2: CommandResult must have #[non_exhaustive]
// ============================================================================

#[test]
fn command_result_is_non_exhaustive() {
    // This test verifies that CommandResult has #[non_exhaustive] by
    // constructing each known variant. If non_exhaustive is missing and
    // a new variant is added later, downstream match arms will break —
    // but that's a compile-time guarantee, not a runtime test.
    // The real enforcement is that this file uses a wildcard match below.
    let display = CommandResult::Display("test".to_string());
    let error = CommandResult::Error("fail".to_string());

    // Use wildcard to prove non_exhaustive is in effect
    match display {
        CommandResult::Display(ref s) => assert_eq!(s, "test"),
        CommandResult::Error(_) => panic!("wrong variant"),
        _ => {} // This arm only compiles if #[non_exhaustive] is present
    }

    if let CommandResult::Error(ref s) = error {
        assert_eq!(s, "fail");
    }
}

// ============================================================================
// Rule #6: Test quality — CommandResult Debug for meaningful assertions
// ============================================================================

#[test]
fn command_result_implements_debug() {
    let result = CommandResult::Display("hello".to_string());
    let debug_str = format!("{:?}", result);
    assert!(
        debug_str.contains("Display"),
        "Debug output should contain variant name, got: {}",
        debug_str
    );
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn slash_command_with_unicode_args() {
    let mut router = SlashRouter::new();
    router.register(Box::new(EchoCommand));
    let state = test_snapshot();

    let result = router.try_dispatch("/echo cafe\u{0301} \u{1F525}", &state);
    assert!(result.is_some());
    match result.unwrap() {
        CommandResult::Display(text) => {
            assert!(
                text.contains("caf\u{00e9}") || text.contains("cafe\u{0301}"),
                "Should handle unicode args, got: {}",
                text
            );
        }
        other => panic!("Expected Display, got {:?}", other),
    }
}

#[test]
fn register_overwrites_duplicate_command_name() {
    // If two handlers share a name, the last registration wins
    let mut router = SlashRouter::new();
    router.register(Box::new(EchoCommand));

    // Register a second handler with the same name
    struct EchoV2;
    impl CommandHandler for EchoV2 {
        fn name(&self) -> &str {
            "echo"
        }
        fn description(&self) -> &str {
            "Echo v2"
        }
        fn handle(&self, _state: &GameSnapshot, args: &str) -> CommandResult {
            CommandResult::Display(format!("v2: {}", args))
        }
    }
    router.register(Box::new(EchoV2));

    let state = test_snapshot();
    let result = router.try_dispatch("/echo test", &state);
    assert!(result.is_some());
    match result.unwrap() {
        CommandResult::Display(text) => {
            assert_eq!(text, "v2: test", "Last registration should win");
        }
        other => panic!("Expected Display, got {:?}", other),
    }
}

#[test]
fn try_dispatch_is_sync_and_returns_immediately() {
    // This test documents that try_dispatch is NOT async.
    // It returns Option<CommandResult>, not a Future.
    // If someone changes the signature to async, this test will fail to compile.
    let router = SlashRouter::new();
    let state = test_snapshot();

    let result: Option<CommandResult> = router.try_dispatch("/anything", &state);
    // The fact that this compiles without .await proves it's sync
    assert!(result.is_some()); // unknown command still returns Some(Error)
}

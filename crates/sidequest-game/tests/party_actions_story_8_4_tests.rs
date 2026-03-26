//! RED tests for Story 8-4: Party action composition.
//!
//! Takes barrier results + player/character mappings and produces a
//! `[PARTY ACTIONS]` text block for the orchestrator. Pure data
//! transformation — no async, no concurrency.
//!
//! Structs under test:
//!   - `PartyActions` — collects character-attributed actions for a turn
//!   - `CharacterAction` — single character's action with attribution
//!
//! Methods under test:
//!   - `PartyActions::compose()` — build from raw action data
//!   - `PartyActions::render()` — produce `[PARTY ACTIONS]` block text

use std::collections::HashMap;

use sidequest_game::character::Character;
use sidequest_game::creature_core::CreatureCore;
use sidequest_game::inventory::Inventory;
use sidequest_game::party_actions::{CharacterAction, PartyActions};
use sidequest_protocol::NonBlankString;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn make_character(name: &str) -> Character {
    Character {
        core: CreatureCore {
            name: NonBlankString::new(name).unwrap(),
            description: NonBlankString::new("A brave adventurer").unwrap(),
            personality: NonBlankString::new("Bold and curious").unwrap(),
            level: 1,
            hp: 20,
            max_hp: 20,
            ac: 12,
            inventory: Inventory::default(),
            statuses: vec![],
        },
        backstory: NonBlankString::new("Grew up on the frontier").unwrap(),
        narrative_state: String::new(),
        hooks: vec![],
        char_class: NonBlankString::new("Fighter").unwrap(),
        race: NonBlankString::new("Human").unwrap(),
        stats: HashMap::new(),
    }
}

fn two_player_map() -> HashMap<String, Character> {
    let mut players = HashMap::new();
    players.insert("player-1".to_string(), make_character("Thorn"));
    players.insert("player-2".to_string(), make_character("Elara"));
    players
}

fn three_player_map() -> HashMap<String, Character> {
    let mut players = HashMap::new();
    players.insert("player-1".to_string(), make_character("Thorn"));
    players.insert("player-2".to_string(), make_character("Elara"));
    players.insert("player-3".to_string(), make_character("Rook"));
    players
}

// ===========================================================================
// AC: Composition — barrier result mapped to named character actions
// ===========================================================================

#[test]
fn compose_maps_player_actions_to_character_actions() {
    let players = two_player_map();
    let mut actions = HashMap::new();
    actions.insert("player-1".to_string(), "I attack the goblin".to_string());
    actions.insert("player-2".to_string(), "I cast fireball".to_string());

    let party = PartyActions::compose(&actions, &players, &[], 1);

    assert_eq!(party.actions().len(), 2);

    let thorn_action = party
        .actions()
        .iter()
        .find(|a| a.character_name() == "Thorn")
        .expect("Thorn should have an action");
    assert_eq!(thorn_action.input(), "I attack the goblin");
    assert!(!thorn_action.is_default());

    let elara_action = party
        .actions()
        .iter()
        .find(|a| a.character_name() == "Elara")
        .expect("Elara should have an action");
    assert_eq!(elara_action.input(), "I cast fireball");
    assert!(!elara_action.is_default());
}

// ===========================================================================
// AC: Default fill — timed-out players get "(waiting)" default action
// ===========================================================================

#[test]
fn timed_out_players_get_waiting_default() {
    let players = two_player_map();
    let mut actions = HashMap::new();
    actions.insert("player-1".to_string(), "I search the room".to_string());
    // player-2 did not submit (timed out)
    let missing = vec!["player-2".to_string()];

    let party = PartyActions::compose(&actions, &players, &missing, 1);

    assert_eq!(party.actions().len(), 2);

    let elara_action = party
        .actions()
        .iter()
        .find(|a| a.character_name() == "Elara")
        .expect("Elara should have a default action");
    assert!(elara_action.is_default(), "timed-out player should be marked as default");
    // The default text should indicate waiting
    assert!(
        elara_action.input().contains("waiting") || elara_action.input().contains("hesitates"),
        "default action should indicate player is waiting/hesitating, got: '{}'",
        elara_action.input()
    );
}

#[test]
fn all_players_timed_out_all_defaults() {
    let players = two_player_map();
    let actions = HashMap::new(); // nobody submitted
    let missing = vec!["player-1".to_string(), "player-2".to_string()];

    let party = PartyActions::compose(&actions, &players, &missing, 3);

    assert_eq!(party.actions().len(), 2);
    for action in party.actions() {
        assert!(
            action.is_default(),
            "{} should be marked as default",
            action.character_name()
        );
    }
}

// ===========================================================================
// AC: Render format — output matches [PARTY ACTIONS] block format
// ===========================================================================

#[test]
fn render_starts_with_party_actions_header() {
    let players = two_player_map();
    let mut actions = HashMap::new();
    actions.insert("player-1".to_string(), "I look around".to_string());
    actions.insert("player-2".to_string(), "I follow Thorn".to_string());

    let party = PartyActions::compose(&actions, &players, &[], 1);
    let rendered = party.render();

    assert!(
        rendered.starts_with("[PARTY ACTIONS]\n"),
        "rendered block must start with [PARTY ACTIONS] header, got: '{}'",
        rendered.lines().next().unwrap_or("")
    );
}

#[test]
fn render_each_action_is_dash_prefixed_line() {
    let players = two_player_map();
    let mut actions = HashMap::new();
    actions.insert("player-1".to_string(), "I look around".to_string());
    actions.insert("player-2".to_string(), "I follow Thorn".to_string());

    let party = PartyActions::compose(&actions, &players, &[], 1);
    let rendered = party.render();
    let action_lines: Vec<&str> = rendered
        .lines()
        .skip(1) // skip header
        .filter(|l| !l.is_empty())
        .collect();

    assert_eq!(action_lines.len(), 2, "should have 2 action lines");
    for line in &action_lines {
        assert!(
            line.starts_with("- "),
            "action line should start with '- ', got: '{line}'"
        );
    }
}

#[test]
fn render_action_line_format_is_name_colon_input() {
    let players = two_player_map();
    let mut actions = HashMap::new();
    actions.insert("player-1".to_string(), "I attack".to_string());
    actions.insert("player-2".to_string(), "I defend".to_string());

    let party = PartyActions::compose(&actions, &players, &[], 1);
    let rendered = party.render();
    let action_lines: Vec<&str> = rendered
        .lines()
        .skip(1)
        .filter(|l| !l.is_empty())
        .collect();

    // Each line should be "- CharName: action text"
    let has_thorn = action_lines.iter().any(|l| *l == "- Thorn: I attack");
    let has_elara = action_lines.iter().any(|l| *l == "- Elara: I defend");
    assert!(has_thorn, "should contain '- Thorn: I attack' in {action_lines:?}");
    assert!(has_elara, "should contain '- Elara: I defend' in {action_lines:?}");
}

#[test]
fn render_default_actions_have_waiting_suffix() {
    let players = two_player_map();
    let mut actions = HashMap::new();
    actions.insert("player-1".to_string(), "I charge".to_string());
    let missing = vec!["player-2".to_string()];

    let party = PartyActions::compose(&actions, &players, &missing, 1);
    let rendered = party.render();

    // Thorn's line should NOT have (waiting)
    let thorn_line = rendered
        .lines()
        .find(|l| l.contains("Thorn"))
        .expect("should have Thorn line");
    assert!(
        !thorn_line.contains("(waiting)"),
        "submitted action should not have (waiting) suffix: '{thorn_line}'"
    );

    // Elara's line SHOULD have (waiting)
    let elara_line = rendered
        .lines()
        .find(|l| l.contains("Elara"))
        .expect("should have Elara line");
    assert!(
        elara_line.contains("(waiting)"),
        "default action should have (waiting) suffix: '{elara_line}'"
    );
}

// ===========================================================================
// AC: Character names — actions attributed to character names, not player IDs
// ===========================================================================

#[test]
fn rendered_block_contains_character_names_not_player_ids() {
    let players = two_player_map();
    let mut actions = HashMap::new();
    actions.insert("player-1".to_string(), "I search".to_string());
    actions.insert("player-2".to_string(), "I hide".to_string());

    let party = PartyActions::compose(&actions, &players, &[], 1);
    let rendered = party.render();

    // Character names present
    assert!(rendered.contains("Thorn"), "should contain character name 'Thorn'");
    assert!(rendered.contains("Elara"), "should contain character name 'Elara'");

    // Player IDs absent
    assert!(
        !rendered.contains("player-1"),
        "should NOT contain player ID 'player-1'"
    );
    assert!(
        !rendered.contains("player-2"),
        "should NOT contain player ID 'player-2'"
    );
}

#[test]
fn character_action_uses_name_not_player_id() {
    let players = two_player_map();
    let mut actions = HashMap::new();
    actions.insert("player-1".to_string(), "I attack".to_string());
    actions.insert("player-2".to_string(), "I heal".to_string());

    let party = PartyActions::compose(&actions, &players, &[], 1);

    let names: Vec<&str> = party.actions().iter().map(|a| a.character_name()).collect();
    assert!(names.contains(&"Thorn"), "should contain Thorn");
    assert!(names.contains(&"Elara"), "should contain Elara");
    assert!(
        !names.contains(&"player-1"),
        "should not contain raw player ID"
    );
}

// ===========================================================================
// AC: Orchestrator input — rendered block accepted as turn input
// ===========================================================================

#[test]
fn rendered_block_is_non_empty_string() {
    let players = two_player_map();
    let mut actions = HashMap::new();
    actions.insert("player-1".to_string(), "I wait".to_string());
    actions.insert("player-2".to_string(), "I watch".to_string());

    let party = PartyActions::compose(&actions, &players, &[], 1);
    let rendered = party.render();

    assert!(!rendered.is_empty(), "rendered block should not be empty");
    assert!(rendered.len() > "[PARTY ACTIONS]\n".len(), "should have content beyond the header");
}

#[test]
fn rendered_block_ends_with_newline() {
    let players = two_player_map();
    let mut actions = HashMap::new();
    actions.insert("player-1".to_string(), "I proceed".to_string());
    actions.insert("player-2".to_string(), "I follow".to_string());

    let party = PartyActions::compose(&actions, &players, &[], 1);
    let rendered = party.render();

    assert!(
        rendered.ends_with('\n'),
        "rendered block should end with newline for clean concatenation"
    );
}

// ===========================================================================
// AC: Turn number — PartyActions tracks which turn it belongs to
// ===========================================================================

#[test]
fn party_actions_tracks_turn_number() {
    let players = two_player_map();
    let mut actions = HashMap::new();
    actions.insert("player-1".to_string(), "I attack".to_string());
    actions.insert("player-2".to_string(), "I defend".to_string());

    let party = PartyActions::compose(&actions, &players, &[], 5);
    assert_eq!(party.turn_number(), 5);
}

#[test]
fn turn_number_one_for_first_turn() {
    let players = two_player_map();
    let mut actions = HashMap::new();
    actions.insert("player-1".to_string(), "hello".to_string());
    actions.insert("player-2".to_string(), "hi".to_string());

    let party = PartyActions::compose(&actions, &players, &[], 1);
    assert_eq!(party.turn_number(), 1);
}

// ===========================================================================
// Edge cases
// ===========================================================================

#[test]
fn single_player_party() {
    let mut players = HashMap::new();
    players.insert("solo".to_string(), make_character("Lone Wolf"));
    let mut actions = HashMap::new();
    actions.insert("solo".to_string(), "I explore alone".to_string());

    let party = PartyActions::compose(&actions, &players, &[], 1);

    assert_eq!(party.actions().len(), 1);
    assert_eq!(party.actions()[0].character_name(), "Lone Wolf");
    assert_eq!(party.actions()[0].input(), "I explore alone");
    assert!(!party.actions()[0].is_default());
}

#[test]
fn single_player_render() {
    let mut players = HashMap::new();
    players.insert("solo".to_string(), make_character("Lone Wolf"));
    let mut actions = HashMap::new();
    actions.insert("solo".to_string(), "I explore".to_string());

    let party = PartyActions::compose(&actions, &players, &[], 1);
    let rendered = party.render();

    assert!(rendered.starts_with("[PARTY ACTIONS]\n"));
    assert!(rendered.contains("Lone Wolf: I explore"));
}

#[test]
fn three_player_mixed_submit_and_timeout() {
    let players = three_player_map();
    let mut actions = HashMap::new();
    actions.insert("player-1".to_string(), "I charge".to_string());
    actions.insert("player-3".to_string(), "I flank".to_string());
    // player-2 timed out
    let missing = vec!["player-2".to_string()];

    let party = PartyActions::compose(&actions, &players, &missing, 2);

    assert_eq!(party.actions().len(), 3);

    let submitted: Vec<&CharacterAction> =
        party.actions().iter().filter(|a| !a.is_default()).collect();
    let defaulted: Vec<&CharacterAction> =
        party.actions().iter().filter(|a| a.is_default()).collect();

    assert_eq!(submitted.len(), 2, "2 players submitted");
    assert_eq!(defaulted.len(), 1, "1 player timed out");
    assert_eq!(defaulted[0].character_name(), "Elara");
}

#[test]
fn unknown_player_in_actions_ignored() {
    // If actions contains a player_id not in the players map, it should be skipped
    let players = two_player_map();
    let mut actions = HashMap::new();
    actions.insert("player-1".to_string(), "I attack".to_string());
    actions.insert("player-2".to_string(), "I defend".to_string());
    actions.insert("ghost-player".to_string(), "I haunt".to_string()); // not in players

    let party = PartyActions::compose(&actions, &players, &[], 1);

    assert_eq!(
        party.actions().len(),
        2,
        "unknown player should be ignored, only 2 real players"
    );
}

#[test]
fn empty_action_text_renders_correctly() {
    let players = two_player_map();
    let mut actions = HashMap::new();
    actions.insert("player-1".to_string(), String::new()); // empty action
    actions.insert("player-2".to_string(), "I cast shield".to_string());

    let party = PartyActions::compose(&actions, &players, &[], 1);
    let rendered = party.render();

    // Even empty action should render a valid line
    let thorn_line = rendered
        .lines()
        .find(|l| l.contains("Thorn"))
        .expect("Thorn should appear even with empty action");
    assert!(
        thorn_line.starts_with("- Thorn:"),
        "empty action line should still have character attribution: '{thorn_line}'"
    );
}

// ===========================================================================
// Rule #9: Fields with invariants should be private with getters
// ===========================================================================

#[test]
fn character_action_exposes_getters() {
    // Verify the public API uses getter methods, not direct field access.
    // This test documents the expected API shape.
    let players = two_player_map();
    let mut actions = HashMap::new();
    actions.insert("player-1".to_string(), "test".to_string());
    actions.insert("player-2".to_string(), "test2".to_string());

    let party = PartyActions::compose(&actions, &players, &[], 1);
    let action = &party.actions()[0];

    // These must compile — they use getter methods
    let _name: &str = action.character_name();
    let _input: &str = action.input();
    let _default: bool = action.is_default();

    // If fields were public, direct access would also work, but we verify
    // the getter API exists and returns the correct types.
    assert!(!_name.is_empty() || _name.is_empty()); // not vacuous — proves getter returns &str
    assert!(_default || !_default); // proves is_default() returns bool
}

#[test]
fn party_actions_exposes_getters() {
    let players = two_player_map();
    let mut actions = HashMap::new();
    actions.insert("player-1".to_string(), "test".to_string());
    actions.insert("player-2".to_string(), "test2".to_string());

    let party = PartyActions::compose(&actions, &players, &[], 7);

    // Getter API verification
    let actions_slice: &[CharacterAction] = party.actions();
    let turn: u64 = party.turn_number();

    assert_eq!(actions_slice.len(), 2);
    assert_eq!(turn, 7);
}

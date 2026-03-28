//! RED tests for Story 8-1: MultiplayerSession
//!
//! MultiplayerSession coordinates multiple players in a single game session
//! with barrier-sync turns, player lifecycle management, and per-player
//! narration delivery. Port of Python `MultiplayerSession`.
//!
//! These tests define the public API through usage — they should fail to
//! compile until the implementation exists.

use std::collections::HashMap;

use sidequest_game::character::Character;
use sidequest_game::creature_core::CreatureCore;
use sidequest_game::inventory::Inventory;
use sidequest_game::multiplayer::{MultiplayerSession, TurnStatus};
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
        abilities: vec![],
        known_facts: vec![],
            is_friendly: true,
    }
}

fn two_player_session() -> MultiplayerSession {
    let mut players = HashMap::new();
    players.insert("player-1".to_string(), make_character("Thorn"));
    players.insert("player-2".to_string(), make_character("Elara"));
    MultiplayerSession::new(players)
}

// ===========================================================================
// 1. Creation and basic properties
// ===========================================================================

#[test]
fn new_session_has_correct_player_count() {
    let session = two_player_session();
    assert_eq!(session.player_count(), 2);
}

#[test]
fn new_session_starts_at_turn_one() {
    let session = two_player_session();
    assert_eq!(session.turn_number(), 1);
}

#[test]
fn new_session_all_players_pending() {
    let session = two_player_session();
    let pending = session.pending_players();
    assert_eq!(pending.len(), 2);
    assert!(pending.contains("player-1"));
    assert!(pending.contains("player-2"));
}

#[test]
fn single_player_session() {
    let mut players = HashMap::new();
    players.insert("solo".to_string(), make_character("Lone Wolf"));
    let session = MultiplayerSession::new(players);
    assert_eq!(session.player_count(), 1);
}

// ===========================================================================
// 2. Player join / leave lifecycle
// ===========================================================================

#[test]
fn add_player_increases_count() {
    let mut session = two_player_session();
    let count = session
        .add_player("player-3".to_string(), make_character("Rook"))
        .unwrap();
    assert_eq!(count, 3);
    assert_eq!(session.player_count(), 3);
}

#[test]
fn add_duplicate_player_is_error() {
    let mut session = two_player_session();
    let result = session.add_player("player-1".to_string(), make_character("Imposter"));
    assert!(result.is_err());
}

#[test]
fn add_player_with_empty_id_is_error() {
    let mut session = two_player_session();
    let result = session.add_player(String::new(), make_character("Nobody"));
    assert!(result.is_err());
}

#[test]
fn remove_player_decreases_count() {
    let mut session = two_player_session();
    let remaining = session.remove_player("player-1").unwrap();
    assert_eq!(remaining, 1);
    assert_eq!(session.player_count(), 1);
}

#[test]
fn remove_unknown_player_is_error() {
    let mut session = two_player_session();
    let result = session.remove_player("ghost");
    assert!(result.is_err());
}

#[test]
fn removed_player_not_in_pending() {
    let mut session = two_player_session();
    session.remove_player("player-1").unwrap();
    let pending = session.pending_players();
    assert!(!pending.contains("player-1"));
    assert!(pending.contains("player-2"));
}

// ===========================================================================
// 3. Action submission and barrier-sync turn resolution
// ===========================================================================

#[test]
fn submit_first_action_returns_waiting() {
    let mut session = two_player_session();
    let result = session.submit_action("player-1", "I search the room");
    assert_eq!(result.status, TurnStatus::Waiting);
    assert!(result.narration.is_none());
}

#[test]
fn submit_all_actions_returns_resolved() {
    let mut session = two_player_session();
    let _ = session.submit_action("player-1", "I search the room");
    let result = session.submit_action("player-2", "I guard the door");
    assert_eq!(result.status, TurnStatus::Resolved);
}

#[test]
fn resolved_turn_has_narration_for_each_player() {
    let mut session = two_player_session();
    let _ = session.submit_action("player-1", "I search the room");
    let result = session.submit_action("player-2", "I guard the door");
    let narration = result
        .narration
        .as_ref()
        .expect("resolved turn should have narration");
    assert!(narration.contains_key("player-1"));
    assert!(narration.contains_key("player-2"));
}

#[test]
fn turn_number_advances_after_resolution() {
    let mut session = two_player_session();
    let _ = session.submit_action("player-1", "attack");
    let _ = session.submit_action("player-2", "defend");
    assert_eq!(session.turn_number(), 2);
}

#[test]
fn pending_resets_after_turn_resolution() {
    let mut session = two_player_session();
    let _ = session.submit_action("player-1", "attack");
    let _ = session.submit_action("player-2", "defend");
    // After resolution, all players should be pending again
    let pending = session.pending_players();
    assert_eq!(pending.len(), 2);
}

#[test]
fn submit_from_unknown_player_is_ignored_or_error() {
    let mut session = two_player_session();
    // Submitting from a player not in the session should not count
    let result = session.submit_action("ghost", "boo");
    assert_eq!(result.status, TurnStatus::Waiting);
    // Original players should still be pending
    assert_eq!(session.pending_players().len(), 2);
}

#[test]
fn duplicate_submit_same_player_idempotent() {
    let mut session = two_player_session();
    let _ = session.submit_action("player-1", "first action");
    // Second submission from same player should not trigger resolution
    let result = session.submit_action("player-1", "second action");
    assert_eq!(result.status, TurnStatus::Waiting);
    // player-2 is still pending
    assert!(session.pending_players().contains("player-2"));
}

// ===========================================================================
// 4. Named actions — player_id to character name mapping
// ===========================================================================

#[test]
fn actions_are_keyed_by_player_id() {
    let mut session = two_player_session();
    session.submit_action("player-1", "I swing my sword");
    let actions = session.current_actions();
    assert_eq!(
        actions.get("player-1").map(String::as_str),
        Some("I swing my sword")
    );
    assert!(actions.get("player-2").is_none());
}

#[test]
fn named_actions_maps_to_character_names() {
    let mut session = two_player_session();
    session.submit_action("player-1", "I swing my sword");
    session.submit_action("player-2", "I cast a spell");
    let named = session.named_actions();
    // Should map character names to their actions
    assert!(named.contains_key("Thorn"));
    assert!(named.contains_key("Elara"));
}

// ===========================================================================
// 5. Snapshot generation for late joiners
// ===========================================================================

#[test]
fn generate_snapshot_for_existing_player() {
    let session = two_player_session();
    let snapshot = session.generate_snapshot("player-1").unwrap();
    assert!(snapshot.contains("Thorn"));
    assert!(snapshot.contains("turn 1"));
}

#[test]
fn generate_snapshot_mentions_other_players() {
    let session = two_player_session();
    let snapshot = session.generate_snapshot("player-1").unwrap();
    // Should mention other party members, not the player themselves
    assert!(snapshot.contains("Elara"));
}

#[test]
fn generate_snapshot_unknown_player_is_error() {
    let session = two_player_session();
    let result = session.generate_snapshot("ghost");
    assert!(result.is_err());
}

// ===========================================================================
// 6. Edge cases — max players, mid-turn join/leave
// ===========================================================================

#[test]
fn player_added_mid_turn_must_submit_to_resolve() {
    let mut session = two_player_session();
    // player-1 submits
    session.submit_action("player-1", "attack");
    // New player joins mid-turn
    session
        .add_player("player-3".to_string(), make_character("Rook"))
        .unwrap();
    // player-2 submits — should NOT resolve because player-3 hasn't submitted
    let result = session.submit_action("player-2", "defend");
    assert_eq!(result.status, TurnStatus::Waiting);
    // Now player-3 submits — should resolve
    let result = session.submit_action("player-3", "flee");
    assert_eq!(result.status, TurnStatus::Resolved);
}

#[test]
fn player_removed_mid_turn_unblocks_barrier() {
    let mut session = two_player_session();
    // player-1 submits
    session.submit_action("player-1", "attack");
    // player-2 disconnects — removing them should resolve the turn
    session.remove_player("player-2").unwrap();
    // With only player-1 remaining and their action submitted, turn should resolve
    // The session should now be on turn 2
    assert_eq!(session.turn_number(), 2);
}

#[test]
fn max_players_enforced() {
    let mut players = HashMap::new();
    for i in 0..MultiplayerSession::MAX_PLAYERS {
        players.insert(format!("player-{i}"), make_character(&format!("Hero {i}")));
    }
    let mut session = MultiplayerSession::new(players);
    // One more should fail
    let result = session.add_player("one-too-many".to_string(), make_character("Overflow"));
    assert!(result.is_err());
}

// ===========================================================================
// 7. Reminders — check which players haven't submitted
// ===========================================================================

#[test]
fn check_reminders_returns_pending_player_ids() {
    let mut session = two_player_session();
    session.submit_action("player-1", "I search the room");
    let reminders = session.check_reminders();
    // Only player-2 should need a reminder
    assert!(reminders.contains_key("player-2"));
    assert!(!reminders.contains_key("player-1"));
}

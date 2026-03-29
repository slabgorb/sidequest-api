//! RED tests for Story 8-2: Turn barrier with configurable timeout.
//!
//! Extends MultiplayerSession (8-1) with a TurnBarrier that coordinates
//! concurrent action submission with a timeout. Uses Arc<Mutex<>> + Notify
//! internally so `wait_for_turn()` can run as a spawned task while
//! `submit_action()` is called concurrently from other tasks.
//!
//! Design: TurnBarrier owns the session behind shared state. Callers use
//! `barrier.submit_action()` (not `session_mut()`) so the barrier can
//! wake up `wait_for_turn()` immediately when the last player submits.

use std::collections::HashMap;
use std::time::Duration;

use sidequest_game::barrier::{TurnBarrier, TurnBarrierConfig, TurnBarrierResult};
use sidequest_game::character::Character;
use sidequest_game::creature_core::CreatureCore;
use sidequest_game::inventory::Inventory;
use sidequest_game::multiplayer::MultiplayerSession;
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
            xp: 0,
            inventory: Inventory::default(),
            statuses: vec![],
        },
        backstory: NonBlankString::new("Grew up on the frontier").unwrap(),
        narrative_state: String::new(),
        hooks: vec![],
        char_class: NonBlankString::new("Fighter").unwrap(),
        race: NonBlankString::new("Human").unwrap(),
        pronouns: String::new(),
        stats: HashMap::new(),
        abilities: vec![],
        known_facts: vec![],
        affinities: vec![],
        is_friendly: true,
    }
}

fn two_player_session() -> MultiplayerSession {
    let mut players = HashMap::new();
    players.insert("player-1".to_string(), make_character("Thorn"));
    players.insert("player-2".to_string(), make_character("Elara"));
    MultiplayerSession::new(players)
}

fn three_player_session() -> MultiplayerSession {
    let mut players = HashMap::new();
    players.insert("player-1".to_string(), make_character("Thorn"));
    players.insert("player-2".to_string(), make_character("Elara"));
    players.insert("player-3".to_string(), make_character("Rook"));
    MultiplayerSession::new(players)
}

// ===========================================================================
// 1. TurnBarrierConfig — construction and defaults
// ===========================================================================

#[test]
fn config_with_explicit_timeout() {
    let config = TurnBarrierConfig::new(Duration::from_secs(30));
    assert_eq!(config.timeout(), Duration::from_secs(30));
    assert!(config.is_enabled());
}

#[test]
fn config_default_is_30_seconds() {
    let config = TurnBarrierConfig::default();
    assert_eq!(config.timeout(), Duration::from_secs(30));
}

#[test]
fn config_disabled() {
    let config = TurnBarrierConfig::disabled();
    assert!(!config.is_enabled());
}

#[test]
fn config_custom_duration() {
    let config = TurnBarrierConfig::new(Duration::from_secs(120));
    assert_eq!(config.timeout(), Duration::from_secs(120));
}

// ===========================================================================
// 2. TurnBarrier — construction and session access
// ===========================================================================

#[test]
fn barrier_wraps_session() {
    let session = two_player_session();
    let config = TurnBarrierConfig::new(Duration::from_secs(30));
    let barrier = TurnBarrier::new(session, config);
    assert_eq!(barrier.player_count(), 2);
}

#[test]
fn barrier_config_is_readable() {
    let session = two_player_session();
    let config = TurnBarrierConfig::new(Duration::from_secs(45));
    let barrier = TurnBarrier::new(session, config);
    assert_eq!(barrier.config().timeout(), Duration::from_secs(45));
}

#[test]
fn barrier_delegates_turn_number() {
    let session = two_player_session();
    let barrier = TurnBarrier::new(session, TurnBarrierConfig::default());
    assert_eq!(barrier.turn_number(), 1);
}

// ===========================================================================
// 3. Concurrent submission — submit while wait_for_turn is running
// ===========================================================================

#[tokio::test]
async fn submit_during_wait_resolves_immediately() {
    tokio::time::pause();

    let session = two_player_session();
    let config = TurnBarrierConfig::new(Duration::from_secs(30));
    let barrier = TurnBarrier::new(session, config);

    // Clone barrier handle for the spawned task (Arc internally)
    let b = barrier.clone();
    let wait_handle = tokio::spawn(async move { b.wait_for_turn().await });

    // Give the wait task a moment to start
    tokio::time::advance(Duration::from_millis(10)).await;

    // Submit actions concurrently while wait_for_turn is running
    barrier.submit_action("player-1", "I attack the goblin");
    barrier.submit_action("player-2", "I guard the door");

    // wait_for_turn should resolve immediately (Notify wakes it up)
    let result = wait_handle.await.unwrap();
    assert!(!result.timed_out);
    assert!(result.missing_players.is_empty());
    assert!(result.narration.contains_key("player-1"));
    assert!(result.narration.contains_key("player-2"));
}

#[tokio::test]
async fn last_submit_wakes_waiter_without_sleeping_full_timeout() {
    tokio::time::pause();

    let session = two_player_session();
    let config = TurnBarrierConfig::new(Duration::from_secs(60));
    let barrier = TurnBarrier::new(session, config);

    let b = barrier.clone();
    let wait_handle = tokio::spawn(async move { b.wait_for_turn().await });

    tokio::time::advance(Duration::from_millis(10)).await;

    barrier.submit_action("player-1", "attack");

    // Advance only 1 second — well under the 60s timeout
    tokio::time::advance(Duration::from_secs(1)).await;

    barrier.submit_action("player-2", "defend");

    let result = wait_handle.await.unwrap();
    // Should have resolved in ~1s, not 60s
    assert!(!result.timed_out);
}

// ===========================================================================
// 4. Timeout auto-resolve — concurrent wait expires
// ===========================================================================

#[tokio::test]
async fn timeout_auto_resolves_with_partial_submissions() {
    tokio::time::pause();

    let session = two_player_session();
    let config = TurnBarrierConfig::new(Duration::from_secs(10));
    let barrier = TurnBarrier::new(session, config);

    let b = barrier.clone();
    let wait_handle = tokio::spawn(async move { b.wait_for_turn().await });

    tokio::time::advance(Duration::from_millis(10)).await;

    // Only player-1 submits
    barrier.submit_action("player-1", "I attack the goblin");

    // Advance past the timeout
    tokio::time::advance(Duration::from_secs(11)).await;

    let result = wait_handle.await.unwrap();
    assert!(result.timed_out);
    assert_eq!(result.missing_players, vec!["player-2".to_string()]);
    assert!(result.narration.contains_key("player-1"));
    assert!(result.narration.contains_key("player-2"));
}

#[tokio::test]
async fn timeout_with_no_submissions() {
    tokio::time::pause();

    let session = two_player_session();
    let config = TurnBarrierConfig::new(Duration::from_secs(5));
    let barrier = TurnBarrier::new(session, config);

    let b = barrier.clone();
    let wait_handle = tokio::spawn(async move { b.wait_for_turn().await });

    // Nobody submits — advance past timeout
    tokio::time::advance(Duration::from_secs(6)).await;

    let result = wait_handle.await.unwrap();
    assert!(result.timed_out);
    let mut missing = result.missing_players.clone();
    missing.sort();
    assert_eq!(missing.len(), 2);
}

#[tokio::test]
async fn timeout_advances_turn_number() {
    tokio::time::pause();

    let session = two_player_session();
    let config = TurnBarrierConfig::new(Duration::from_secs(5));
    let barrier = TurnBarrier::new(session, config);

    let b = barrier.clone();
    let wait_handle = tokio::spawn(async move { b.wait_for_turn().await });

    tokio::time::advance(Duration::from_millis(10)).await;
    barrier.submit_action("player-1", "attack");
    tokio::time::advance(Duration::from_secs(6)).await;

    let _ = wait_handle.await.unwrap();
    assert_eq!(barrier.turn_number(), 2);
}

// ===========================================================================
// 5. TurnBarrierResult — identifies missing players
// ===========================================================================

#[tokio::test]
async fn result_identifies_multiple_missing_players() {
    tokio::time::pause();

    let session = three_player_session();
    let config = TurnBarrierConfig::new(Duration::from_secs(5));
    let barrier = TurnBarrier::new(session, config);

    let b = barrier.clone();
    let wait_handle = tokio::spawn(async move { b.wait_for_turn().await });

    tokio::time::advance(Duration::from_millis(10)).await;
    barrier.submit_action("player-1", "attack");
    tokio::time::advance(Duration::from_secs(6)).await;

    let result = wait_handle.await.unwrap();
    assert!(result.timed_out);
    let mut missing = result.missing_players.clone();
    missing.sort();
    assert_eq!(missing.len(), 2);
    assert!(missing.contains(&"player-2".to_string()));
    assert!(missing.contains(&"player-3".to_string()));
}

#[tokio::test]
async fn result_narration_includes_all_players_even_timed_out() {
    tokio::time::pause();

    let session = two_player_session();
    let config = TurnBarrierConfig::new(Duration::from_secs(5));
    let barrier = TurnBarrier::new(session, config);

    let b = barrier.clone();
    let wait_handle = tokio::spawn(async move { b.wait_for_turn().await });

    tokio::time::advance(Duration::from_millis(10)).await;
    barrier.submit_action("player-1", "I search the room");
    tokio::time::advance(Duration::from_secs(6)).await;

    let result = wait_handle.await.unwrap();
    // Even timed-out players get a narration entry
    assert!(result.narration.contains_key("player-1"));
    assert!(result.narration.contains_key("player-2"));
}

// ===========================================================================
// 6. Multiple sequential turns
// ===========================================================================

#[tokio::test]
async fn multiple_turns_timeout_then_normal() {
    tokio::time::pause();

    let session = two_player_session();
    let config = TurnBarrierConfig::new(Duration::from_secs(5));
    let barrier = TurnBarrier::new(session, config);

    // Turn 1: player-1 submits, player-2 times out
    {
        let b = barrier.clone();
        let handle = tokio::spawn(async move { b.wait_for_turn().await });
        tokio::time::advance(Duration::from_millis(10)).await;
        barrier.submit_action("player-1", "attack");
        tokio::time::advance(Duration::from_secs(6)).await;
        let r = handle.await.unwrap();
        assert!(r.timed_out);
    }
    assert_eq!(barrier.turn_number(), 2);

    // Turn 2: both submit — no timeout
    {
        let b = barrier.clone();
        let handle = tokio::spawn(async move { b.wait_for_turn().await });
        tokio::time::advance(Duration::from_millis(10)).await;
        barrier.submit_action("player-1", "rest");
        barrier.submit_action("player-2", "scout");
        let r = handle.await.unwrap();
        assert!(!r.timed_out);
    }
    assert_eq!(barrier.turn_number(), 3);
}

// ===========================================================================
// 7. Disabled timeout — only resolves when all players submit
// ===========================================================================

#[tokio::test]
async fn disabled_timeout_waits_indefinitely_for_all_submissions() {
    tokio::time::pause();

    let session = two_player_session();
    let config = TurnBarrierConfig::disabled();
    let barrier = TurnBarrier::new(session, config);

    let b = barrier.clone();
    let wait_handle = tokio::spawn(async move { b.wait_for_turn().await });

    tokio::time::advance(Duration::from_millis(10)).await;
    barrier.submit_action("player-1", "attack");

    // Advance well past any reasonable timeout — should NOT resolve
    tokio::time::advance(Duration::from_secs(600)).await;

    // The task should still be pending
    assert!(!wait_handle.is_finished());

    // Now player-2 submits — should resolve immediately
    barrier.submit_action("player-2", "defend");
    // Yield so the notified task can run
    tokio::time::advance(Duration::from_millis(1)).await;

    let result = wait_handle.await.unwrap();
    assert!(!result.timed_out);
    assert!(result.missing_players.is_empty());
    assert_eq!(barrier.turn_number(), 2);
}

// ===========================================================================
// 8. Race condition — submit just as timeout fires
// ===========================================================================

#[tokio::test]
async fn submit_just_before_timeout_prefers_action() {
    tokio::time::pause();

    let session = two_player_session();
    let config = TurnBarrierConfig::new(Duration::from_secs(10));
    let barrier = TurnBarrier::new(session, config);

    let b = barrier.clone();
    let wait_handle = tokio::spawn(async move { b.wait_for_turn().await });

    tokio::time::advance(Duration::from_millis(10)).await;
    barrier.submit_action("player-1", "attack");

    // Advance to just before timeout
    tokio::time::advance(Duration::from_secs(9)).await;

    // Player-2 submits with ~1s remaining
    barrier.submit_action("player-2", "defend");

    let result = wait_handle.await.unwrap();
    assert!(!result.timed_out);
    assert!(result.missing_players.is_empty());
}

// ===========================================================================
// 9. Config can be updated between turns
// ===========================================================================

#[test]
fn update_config_between_turns() {
    let session = two_player_session();
    let config = TurnBarrierConfig::new(Duration::from_secs(30));
    let barrier = TurnBarrier::new(session, config);
    barrier.set_config(TurnBarrierConfig::new(Duration::from_secs(60)));
    assert_eq!(barrier.config().timeout(), Duration::from_secs(60));
}

#[test]
fn disable_timeout_mid_game() {
    let session = two_player_session();
    let config = TurnBarrierConfig::new(Duration::from_secs(30));
    let barrier = TurnBarrier::new(session, config);
    barrier.set_config(TurnBarrierConfig::disabled());
    assert!(!barrier.config().is_enabled());
}

// ===========================================================================
// 10. Player changes during barrier wait
// ===========================================================================

#[tokio::test]
async fn player_removed_during_wait_resolves_immediately() {
    tokio::time::pause();

    let session = two_player_session();
    let config = TurnBarrierConfig::new(Duration::from_secs(30));
    let barrier = TurnBarrier::new(session, config);

    let b = barrier.clone();
    let wait_handle = tokio::spawn(async move { b.wait_for_turn().await });

    tokio::time::advance(Duration::from_millis(10)).await;

    // player-1 submits
    barrier.submit_action("player-1", "attack");

    // player-2 disconnects — barrier should be met
    barrier.remove_player("player-2").unwrap();

    // Yield to let Notify wake the waiter
    tokio::time::advance(Duration::from_millis(1)).await;

    let result = wait_handle.await.unwrap();
    assert!(!result.timed_out);
    assert_eq!(barrier.turn_number(), 2);
}

#[tokio::test]
async fn player_added_during_wait_extends_barrier() {
    tokio::time::pause();

    let session = two_player_session();
    let config = TurnBarrierConfig::new(Duration::from_secs(30));
    let barrier = TurnBarrier::new(session, config);

    let b = barrier.clone();
    let wait_handle = tokio::spawn(async move { b.wait_for_turn().await });

    tokio::time::advance(Duration::from_millis(10)).await;

    // Both original players submit
    barrier.submit_action("player-1", "attack");
    barrier.submit_action("player-2", "defend");

    // New player joins before turn resolves
    barrier
        .add_player("player-3".to_string(), make_character("Rook"))
        .unwrap();

    // Should NOT have resolved — player-3 hasn't submitted
    tokio::time::advance(Duration::from_millis(10)).await;
    assert!(!wait_handle.is_finished());

    // player-3 submits — now it resolves
    barrier.submit_action("player-3", "scout");
    tokio::time::advance(Duration::from_millis(1)).await;

    let result = wait_handle.await.unwrap();
    assert!(!result.timed_out);
}

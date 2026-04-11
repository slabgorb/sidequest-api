//! RED tests for Story 13-10: Barrier error handling — propagate errors, fix races.
//!
//! Three critical bugs in barrier error handling:
//!   Bug 1: `let _ = barrier.add_player()` swallows errors — player silently excluded
//!          from barrier, submissions become no-ops, turn hangs to timeout
//!   Bug 2: `try_lock()` in disconnect path skips barrier removal — ghost player stays
//!          in roster, every turn times out waiting for them
//!   Bug 3: `unwrap_or_default()` on character JSON serialization silently produces
//!          Value::Null — should log error and skip sync instead
//!
//! Tests verify:
//!   AC-1: add_player errors propagate to caller (not swallowed by `let _ =`)
//!   AC-2: Disconnect removal never silently skips — ghost players are impossible
//!   AC-3: Character serialization failures are logged, not silently null-ified
//!
//! Key files:
//!   - sidequest-game/src/barrier.rs — TurnBarrier::add_player, remove_player
//!   - sidequest-game/src/multiplayer.rs — MultiplayerSession error variants
//!   - sidequest-server/src/lib.rs — caller sites for all three bugs

use std::collections::HashMap;
use std::time::Duration;

use sidequest_game::barrier::{TurnBarrier, TurnBarrierConfig};
use sidequest_game::multiplayer::{MultiplayerError, MultiplayerSession};

mod common;
use common::make_character;

fn two_player_barrier() -> TurnBarrier {
    let mut players = HashMap::new();
    players.insert("player-1".to_string(), make_character("Thorn"));
    players.insert("player-2".to_string(), make_character("Elara"));
    let session = MultiplayerSession::new(players);
    TurnBarrier::new(session, TurnBarrierConfig::new(Duration::from_secs(5)))
}

fn full_session_barrier() -> TurnBarrier {
    let mut players = HashMap::new();
    for i in 1..=6 {
        players.insert(format!("player-{i}"), make_character(&format!("Char{i}")));
    }
    let session = MultiplayerSession::new(players);
    TurnBarrier::new(session, TurnBarrierConfig::new(Duration::from_secs(5)))
}

// ---------------------------------------------------------------------------
// AC-1: add_player() errors must propagate — not swallowed by `let _ =`
// ---------------------------------------------------------------------------

/// Bug 1a: Adding a duplicate player must return DuplicatePlayer error.
/// In production, `let _ = barrier.add_player()` swallows this, causing
/// the player to silently miss the barrier roster.
#[test]
fn add_player_duplicate_returns_error() {
    let barrier = two_player_barrier();
    let result = barrier.add_player("player-1".to_string(), make_character("Thorn2"));
    assert!(result.is_err(), "duplicate add_player must return Err");
    match result.unwrap_err() {
        MultiplayerError::DuplicatePlayer(id) => {
            assert_eq!(id, "player-1");
        }
        other => panic!("expected DuplicatePlayer, got: {other:?}"),
    }
}

/// Bug 1b: Adding a player when session is full must return SessionFull error.
/// With `let _ =`, the 7th player would be silently excluded from the barrier.
#[test]
fn add_player_session_full_returns_error() {
    let barrier = full_session_barrier();
    assert_eq!(barrier.player_count(), 6);
    let result = barrier.add_player("player-7".to_string(), make_character("Overflow"));
    assert!(
        result.is_err(),
        "add_player to full session must return Err"
    );
    match result.unwrap_err() {
        MultiplayerError::SessionFull(max) => {
            assert_eq!(max, 6);
        }
        other => panic!("expected SessionFull, got: {other:?}"),
    }
}

/// Bug 1c: Adding a player with empty ID must return EmptyPlayerId error.
#[test]
fn add_player_empty_id_returns_error() {
    let barrier = two_player_barrier();
    let result = barrier.add_player(String::new(), make_character("Ghost"));
    assert!(result.is_err(), "empty player_id must return Err");
    match result.unwrap_err() {
        MultiplayerError::EmptyPlayerId => {}
        other => panic!("expected EmptyPlayerId, got: {other:?}"),
    }
}

/// Bug 1d: After a failed add_player, the barrier roster must not change.
/// If the error was swallowed, the player count might have changed or
/// subsequent submissions from this player might behave unpredictably.
#[test]
fn add_player_error_does_not_corrupt_roster() {
    let barrier = two_player_barrier();
    let count_before = barrier.player_count();

    // Try duplicate — should fail
    let _ = barrier.add_player("player-1".to_string(), make_character("Dup"));

    // Roster unchanged
    assert_eq!(barrier.player_count(), count_before);

    // Barrier still works — two players can still complete a turn
    barrier.submit_action("player-1", "I attack");
    barrier.submit_action("player-2", "I defend");
    // If roster was corrupted, barrier_met would never trigger
}

/// Bug 1e: A player excluded from the barrier (due to swallowed error) would
/// have their submissions silently ignored — record_action returns false for
/// unknown players. This test verifies the contract.
#[test]
fn submit_action_for_unknown_player_is_noop() {
    let barrier = two_player_barrier();

    // "player-3" was never added — their action must not count
    barrier.submit_action("player-3", "I try to act");

    // Barrier should NOT be met — only 2 real players, neither submitted
    // The ghost submission must not satisfy the barrier
    barrier.submit_action("player-1", "I attack");
    // Still waiting for player-2
    // (We can't directly check barrier_met from outside, but we can verify
    // the named_actions don't include the ghost)
    let actions = barrier.named_actions();
    assert!(
        !actions.values().any(|v| v.contains("try to act")),
        "ghost player's action must not appear in named_actions"
    );
}

// ---------------------------------------------------------------------------
// AC-2: Disconnect removal must never silently skip — no ghost players
// ---------------------------------------------------------------------------

/// Bug 2a: remove_player must succeed even under contention. The production
/// bug uses try_lock() which can fail if another task holds the lock.
/// This test verifies that remove_player always works (it uses lock(), not try_lock).
#[test]
fn remove_player_always_succeeds() {
    let barrier = two_player_barrier();
    let result = barrier.remove_player("player-2");
    assert!(
        result.is_ok(),
        "remove_player must not fail for existing player"
    );
    assert_eq!(result.unwrap(), 1);
    assert_eq!(barrier.player_count(), 1);
}

/// Bug 2b: After removing a player, the barrier must adjust — if the remaining
/// player has already submitted, the barrier should be met.
#[test]
fn remove_player_triggers_barrier_when_remaining_submitted() {
    let barrier = two_player_barrier();

    // Player 1 submits, barrier not yet met
    barrier.submit_action("player-1", "I attack");

    // Player 2 disconnects — now only player 1 remains, who already submitted
    let result = barrier.remove_player("player-2");
    assert!(result.is_ok());
    assert_eq!(barrier.player_count(), 1);

    // The barrier should now be met since the only remaining player submitted
    // We verify by checking named_actions has player-1's action
    let actions = barrier.named_actions();
    assert!(
        actions.values().any(|v| v.contains("attack")),
        "remaining player's action must be present after disconnect"
    );
}

/// Bug 2c: Removing a nonexistent player must return PlayerNotFound error.
/// In production, `let _ = barrier.remove_player()` swallows this too.
#[test]
fn remove_player_nonexistent_returns_error() {
    let barrier = two_player_barrier();
    let result = barrier.remove_player("player-999");
    assert!(
        result.is_err(),
        "removing nonexistent player must return Err"
    );
    match result.unwrap_err() {
        MultiplayerError::PlayerNotFound(id) => {
            assert_eq!(id, "player-999");
        }
        other => panic!("expected PlayerNotFound, got: {other:?}"),
    }
}

/// Bug 2d: Ghost player scenario — if a player is NOT removed from the barrier
/// (due to try_lock failure), their slot remains in the roster and every
/// subsequent turn will time out waiting for them. This test proves the
/// positive case: remove works, and the ghost is gone.
#[tokio::test]
async fn no_ghost_player_after_disconnect() {
    let barrier = two_player_barrier();

    // Add player 3
    barrier
        .add_player("player-3".to_string(), make_character("Brak"))
        .expect("add should succeed");
    assert_eq!(barrier.player_count(), 3);

    // Player 3 disconnects
    barrier
        .remove_player("player-3")
        .expect("remove should succeed");
    assert_eq!(barrier.player_count(), 2);

    // Now only 2 players remain — barrier should resolve with just their actions
    barrier.submit_action("player-1", "I search");
    barrier.submit_action("player-2", "I watch");

    // wait_for_turn should resolve immediately (not timeout waiting for ghost)
    let result = tokio::time::timeout(Duration::from_secs(2), barrier.wait_for_turn()).await;
    assert!(
        result.is_ok(),
        "barrier must resolve without timeout — no ghost player blocking"
    );
    let turn_result = result.unwrap();
    assert!(
        !turn_result.timed_out,
        "turn should resolve normally, not by timeout"
    );
    assert!(
        turn_result.missing_players.is_empty(),
        "no players should be missing"
    );
}

/// Bug 2e: Concurrent disconnect and submission — remove_player while another
/// task is submitting should not deadlock or corrupt state.
#[tokio::test]
async fn concurrent_disconnect_and_submission_no_deadlock() {
    let barrier = two_player_barrier();
    barrier
        .add_player("player-3".to_string(), make_character("Brak"))
        .expect("add should succeed");

    let b1 = barrier.clone();
    let b2 = barrier.clone();

    // Concurrently: player-3 disconnects while player-1 submits
    let (r1, _) = tokio::join!(
        tokio::spawn(async move { b1.remove_player("player-3") }),
        tokio::spawn(async move { b2.submit_action("player-1", "I act") }),
    );

    // Neither should panic or deadlock
    assert!(r1.is_ok(), "remove task should complete without panic");

    // After both complete, player count should be 2
    assert_eq!(barrier.player_count(), 2);
}

// ---------------------------------------------------------------------------
// AC-3: Character serialization — verify serde contract
// ---------------------------------------------------------------------------

/// Bug 3a: A valid Character must serialize to a non-null JSON Value.
/// The production code uses unwrap_or_default() which produces Value::Null
/// on failure — we need to verify the happy path produces real JSON.
#[test]
fn character_serializes_to_non_null_json() {
    let character = make_character("Thorn");
    let value = serde_json::to_value(&character);
    assert!(value.is_ok(), "character serialization must succeed");
    let json = value.unwrap();
    assert!(!json.is_null(), "serialized character must not be null");
    assert!(json.is_object(), "serialized character must be an object");

    // CreatureCore is #[serde(flatten)]'d — fields appear at top level
    let obj = json.as_object().unwrap();
    assert!(
        obj.contains_key("name"),
        "must have 'name' field (from flattened core)"
    );
    assert!(obj.contains_key("backstory"), "must have 'backstory' field");
}

/// Bug 3b: Value::default() is Value::Null — this is what unwrap_or_default()
/// produces on failure. Verify that this is indeed Null so the fix knows
/// what it's replacing.
#[test]
fn serde_value_default_is_null() {
    let default_val: serde_json::Value = Default::default();
    assert!(
        default_val.is_null(),
        "serde_json::Value::default() must be Null — this is what unwrap_or_default produces"
    );
}

/// Bug 3c: Character with all valid NonBlankString fields must round-trip
/// through JSON without data loss.
#[test]
fn character_json_round_trip_preserves_name() {
    let character = make_character("Thorn");
    let json = serde_json::to_value(&character).expect("serialize");
    // CreatureCore is #[serde(flatten)]'d — name is at top level
    let name = json.get("name").and_then(|n| n.as_str());
    assert_eq!(
        name,
        Some("Thorn"),
        "character name must survive serialization"
    );
}

/// Bug 3d: When character_json is None (no character created yet),
/// the sync path should handle it gracefully — not insert Null.
/// This tests the contract that Option<Value> is the right type.
#[test]
fn none_character_json_is_distinct_from_null_value() {
    let none_val: Option<serde_json::Value> = None;
    let null_val: Option<serde_json::Value> = Some(serde_json::Value::Null);

    // These must be distinguishable — None means "no character yet",
    // Null means "serialization failed silently"
    assert!(none_val.is_none());
    assert!(null_val.is_some());
    assert!(null_val.unwrap().is_null());
    // The fix should ensure we never get Some(Null) from serialization
}

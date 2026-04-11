//! RED tests for Story 13-9: Barrier timeout handling.
//!
//! Critical bug: result.timed_out is logged but never acted on. The timeout
//! path falls through identically to full-submission.
//!
//! These tests verify:
//!   AC-1: Handler branches on timed_out (different behavior from success)
//!   AC-2: Missing players filled with contextual defaults
//!   AC-3: Auto-resolved info available for notification broadcast
//!   AC-4: Narrator context distinguishes intentional vs auto-resolved actions
//!   AC-5: End-to-end timeout scenario with 3 players, 1 missing

use std::collections::HashMap;
use std::time::Duration;

use sidequest_game::barrier::{TurnBarrier, TurnBarrierConfig};
use sidequest_game::multiplayer::MultiplayerSession;

mod common;
use common::make_character;

fn three_player_session() -> MultiplayerSession {
    let mut players = HashMap::new();
    players.insert("player-1".to_string(), make_character("Thorn"));
    players.insert("player-2".to_string(), make_character("Elara"));
    players.insert("player-3".to_string(), make_character("Brak"));
    MultiplayerSession::new(players)
}

// ---------------------------------------------------------------------------
// AC-1: timed_out flag is actionable — result carries enough info to branch
// ---------------------------------------------------------------------------

#[tokio::test]
async fn timeout_result_has_timed_out_true() {
    let session = three_player_session();
    let barrier = TurnBarrier::new(session, TurnBarrierConfig::new(Duration::from_millis(50)));

    // Only 1 of 3 submits
    barrier.submit_action("player-1", "I search the room");

    let result = barrier.wait_for_turn().await;
    assert!(result.timed_out, "result.timed_out should be true");
}

#[tokio::test]
async fn timeout_result_lists_missing_players() {
    let session = three_player_session();
    let barrier = TurnBarrier::new(session, TurnBarrierConfig::new(Duration::from_millis(50)));

    barrier.submit_action("player-1", "I search the room");

    let result = barrier.wait_for_turn().await;
    assert_eq!(
        result.missing_players.len(),
        2,
        "should have 2 missing players"
    );
    assert!(
        result.missing_players.contains(&"player-2".to_string()),
        "player-2 should be missing"
    );
    assert!(
        result.missing_players.contains(&"player-3".to_string()),
        "player-3 should be missing"
    );
}

// ---------------------------------------------------------------------------
// AC-2: Missing players filled with contextual "hesitates" default
// ---------------------------------------------------------------------------

#[tokio::test]
async fn timeout_narration_contains_all_players_including_missing() {
    let session = three_player_session();
    let barrier = TurnBarrier::new(session, TurnBarrierConfig::new(Duration::from_millis(50)));

    barrier.submit_action("player-1", "I search the room");

    let result = barrier.wait_for_turn().await;

    // All 3 players should have narration entries, even timed-out ones
    assert_eq!(
        result.narration.len(),
        3,
        "all 3 players should have narration"
    );
    assert!(result.narration.contains_key("player-1"));
    assert!(result.narration.contains_key("player-2"));
    assert!(result.narration.contains_key("player-3"));
}

#[tokio::test]
async fn timeout_missing_players_have_hesitates_action() {
    let session = three_player_session();
    let barrier = TurnBarrier::new(session, TurnBarrierConfig::new(Duration::from_millis(50)));

    barrier.submit_action("player-1", "I search the room");

    let result = barrier.wait_for_turn().await;

    let p2 = result
        .narration
        .get("player-2")
        .expect("player-2 narration missing");
    let p3 = result
        .narration
        .get("player-3")
        .expect("player-3 narration missing");

    assert!(
        p2.contains("hesitate"),
        "player-2 should hesitate, got: {}",
        p2
    );
    assert!(
        p3.contains("hesitate"),
        "player-3 should hesitate, got: {}",
        p3
    );
}

#[tokio::test]
async fn timeout_submitter_has_their_actual_action() {
    let session = three_player_session();
    let barrier = TurnBarrier::new(session, TurnBarrierConfig::new(Duration::from_millis(50)));

    barrier.submit_action("player-1", "I search the room");

    let result = barrier.wait_for_turn().await;

    let p1 = result
        .narration
        .get("player-1")
        .expect("player-1 narration missing");
    assert!(
        p1.contains("search") || p1.contains("room"),
        "player-1 should have their submitted action, got: {}",
        p1
    );
}

// ---------------------------------------------------------------------------
// AC-3: format_auto_resolved_context — helper for narrator prompt
// ---------------------------------------------------------------------------

#[tokio::test]
async fn format_auto_resolved_context_includes_missing_names() {
    // The barrier should provide a method to format the auto-resolved info
    // for injection into the narrator prompt.
    let session = three_player_session();
    let barrier = TurnBarrier::new(session, TurnBarrierConfig::new(Duration::from_millis(50)));

    barrier.submit_action("player-1", "I search the room");

    let result = barrier.wait_for_turn().await;

    // New method: formats missing player info for narrator context
    let context = result.format_auto_resolved_context();

    assert!(
        context.contains("auto-resolved")
            || context.contains("timed out")
            || context.contains("did not act"),
        "context should mention auto-resolution, got: {}",
        context
    );
    // Should reference the CHARACTER names, not player IDs
    assert!(
        context.contains("Elara") || context.contains("Brak"),
        "context should reference missing character names, got: {}",
        context
    );
}

// ---------------------------------------------------------------------------
// AC-4: Narrator combined action distinguishes intentional vs auto-resolved
// ---------------------------------------------------------------------------

#[tokio::test]
async fn named_actions_after_timeout_marks_auto_resolved() {
    let session = three_player_session();
    let barrier = TurnBarrier::new(session, TurnBarrierConfig::new(Duration::from_millis(50)));

    barrier.submit_action("player-1", "I search the room");

    let _result = barrier.wait_for_turn().await;

    // After timeout, named_actions should distinguish real vs auto-filled
    let named = barrier.named_actions();

    // Thorn's action should be the real one
    let thorn_action = named.get("Thorn").expect("Thorn should have an action");
    assert!(
        thorn_action.contains("search"),
        "Thorn should have real action, got: {}",
        thorn_action
    );

    // Elara and Brak should have hesitates
    let elara_action = named.get("Elara").expect("Elara should have an action");
    assert!(
        elara_action.contains("hesitate"),
        "Elara should have hesitates, got: {}",
        elara_action
    );
}

// ---------------------------------------------------------------------------
// AC-5: Full submission (no timeout) should NOT have auto-resolved context
// ---------------------------------------------------------------------------

#[tokio::test]
async fn full_submission_has_empty_auto_resolved_context() {
    let session = three_player_session();
    let barrier = TurnBarrier::new(session, TurnBarrierConfig::new(Duration::from_secs(5)));

    barrier.submit_action("player-1", "I search");
    barrier.submit_action("player-2", "I wait");
    barrier.submit_action("player-3", "I hide");

    let result = barrier.wait_for_turn().await;

    assert!(!result.timed_out);
    assert!(result.missing_players.is_empty());

    let context = result.format_auto_resolved_context();
    assert!(
        context.is_empty(),
        "full submission should have empty auto-resolved context, got: {}",
        context
    );
}

// ---------------------------------------------------------------------------
// AC-5: End-to-end 3-player scenario — 2 submit, 1 AFK
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_three_player_one_afk() {
    let session = three_player_session();
    let barrier = TurnBarrier::new(session, TurnBarrierConfig::new(Duration::from_millis(100)));

    // Players 1 and 2 submit, player 3 is AFK
    barrier.submit_action("player-1", "I charge the enemy");
    barrier.submit_action("player-2", "I cast a healing spell");

    let result = barrier.wait_for_turn().await;

    // Verify complete result
    assert!(result.timed_out, "should time out");
    assert_eq!(result.missing_players, vec!["player-3".to_string()]);
    assert_eq!(result.narration.len(), 3, "all 3 should have narration");

    // Verify named actions have all 3
    let named = barrier.named_actions();
    assert_eq!(named.len(), 3, "named actions should have all 3 characters");

    // Verify auto-resolved context mentions Brak
    let context = result.format_auto_resolved_context();
    assert!(
        context.contains("Brak"),
        "auto-resolved context should mention Brak"
    );
}

//! RED tests for Story 13-4: Timeout fallback with player notification.
//!
//! When the adaptive timeout fires before all players submit, auto-fill
//! missing actions with mode-contextual defaults and notify remaining
//! players which characters were auto-resolved.
//!
//! These tests verify:
//!   AC-1: Identify missing players when timeout fires
//!   AC-2: Auto-fill with mode-contextual defaults (structured/cinematic)
//!   AC-3: Extract structured auto_resolved character names for broadcast
//!   AC-4: Narrator context includes mode-specific auto-resolution metadata
//!   AC-6: Mixed complete/incomplete submission scenarios

use std::collections::HashMap;
use std::time::Duration;

use sidequest_game::barrier::{TurnBarrier, TurnBarrierConfig, TurnBarrierResult};
use sidequest_game::multiplayer::MultiplayerSession;
use sidequest_game::turn_mode::TurnMode;

mod common;
use common::make_character;

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
    players.insert("player-3".to_string(), make_character("Brak"));
    MultiplayerSession::new(players)
}

// ---------------------------------------------------------------------------
// AC-2: Mode-contextual auto-fill defaults
// ---------------------------------------------------------------------------
// force_resolve_turn() currently hardcodes "hesitates". Story 13-4 requires
// mode-aware defaults via force_resolve_turn_for_mode().

#[test]
fn structured_mode_default_action_uses_hesitates() {
    let mut session = three_player_session();
    session.record_action("player-1", "I search the room");

    // New method: force_resolve_turn_for_mode takes TurnMode
    let narration = session.force_resolve_turn_for_mode(&TurnMode::Structured);

    let p2 = narration.get("player-2").expect("player-2 should have narration");
    assert!(
        p2.contains("hesitate") || p2.contains("waiting"),
        "structured mode should use 'hesitates' variant, got: {}",
        p2
    );
}

#[test]
fn cinematic_mode_default_action_uses_remains_silent() {
    let mut session = three_player_session();
    session.record_action("player-1", "I search the room");

    let narration =
        session.force_resolve_turn_for_mode(&TurnMode::Cinematic { prompt: None });

    let p2 = narration.get("player-2").expect("player-2 should have narration");
    assert!(
        p2.contains("remains silent") || p2.contains("silent"),
        "cinematic mode should use 'remains silent', got: {}",
        p2
    );
}

#[test]
fn freeplay_mode_default_action_uses_generic_hesitates() {
    // FreePlay shouldn't normally timeout (no barrier), but the default
    // should still be reasonable if force_resolve is called directly.
    let mut session = two_player_session();
    session.record_action("player-1", "I search");

    let narration = session.force_resolve_turn_for_mode(&TurnMode::FreePlay);

    let p2 = narration.get("player-2").expect("player-2 should have narration");
    assert!(
        p2.contains("hesitate"),
        "freeplay mode should fallback to hesitates, got: {}",
        p2
    );
}

#[test]
fn mode_default_preserves_submitted_player_action() {
    let mut session = three_player_session();
    session.record_action("player-1", "I charge the enemy");

    let narration =
        session.force_resolve_turn_for_mode(&TurnMode::Cinematic { prompt: None });

    // Player who submitted should keep their actual action, not get overwritten
    let p1 = narration.get("player-1").expect("player-1 should have narration");
    assert!(
        p1.contains("charge") || p1.contains("enemy"),
        "submitted player should keep their real action, got: {}",
        p1
    );
}

// ---------------------------------------------------------------------------
// AC-3: Structured auto_resolved character names for broadcast payload
// ---------------------------------------------------------------------------
// TurnBarrierResult needs auto_resolved_character_names() returning Vec<String>
// of character names (not player IDs) for ActionRevealPayload.auto_resolved.

#[tokio::test]
async fn auto_resolved_character_names_populated_on_timeout() {
    let session = three_player_session();
    let barrier = TurnBarrier::new(session, TurnBarrierConfig::new(Duration::from_millis(50)));

    // Only player-1 submits; player-2 (Elara) and player-3 (Brak) timeout
    barrier.submit_action("player-1", "I search the room");

    let result = barrier.wait_for_turn().await;
    assert!(result.timed_out, "should have timed out");

    // New method: returns character names, not player IDs
    let names = result.auto_resolved_character_names();
    assert_eq!(names.len(), 2, "should have 2 auto-resolved characters");
    assert!(
        names.contains(&"Elara".to_string()),
        "Elara should be auto-resolved, got: {:?}",
        names
    );
    assert!(
        names.contains(&"Brak".to_string()),
        "Brak should be auto-resolved, got: {:?}",
        names
    );
}

#[tokio::test]
async fn auto_resolved_character_names_empty_on_full_submit() {
    let session = three_player_session();
    let barrier = TurnBarrier::new(session, TurnBarrierConfig::new(Duration::from_secs(5)));

    barrier.submit_action("player-1", "I search");
    barrier.submit_action("player-2", "I wait");
    barrier.submit_action("player-3", "I hide");

    let result = barrier.wait_for_turn().await;
    assert!(!result.timed_out);

    let names = result.auto_resolved_character_names();
    assert!(
        names.is_empty(),
        "full submission should have no auto-resolved characters, got: {:?}",
        names
    );
}

#[tokio::test]
async fn auto_resolved_returns_character_names_not_player_ids() {
    let session = two_player_session();
    let barrier = TurnBarrier::new(session, TurnBarrierConfig::new(Duration::from_millis(50)));

    barrier.submit_action("player-1", "I search");

    let result = barrier.wait_for_turn().await;
    let names = result.auto_resolved_character_names();

    // Must be character name "Elara", never player ID "player-2"
    assert!(
        !names.contains(&"player-2".to_string()),
        "should not contain player IDs"
    );
    assert!(
        names.contains(&"Elara".to_string()),
        "should contain character name 'Elara', got: {:?}",
        names
    );
}

// ---------------------------------------------------------------------------
// AC-4: Narrator context with mode-specific auto-resolution metadata
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mode_aware_barrier_timeout_uses_cinematic_default() {
    // TurnBarrier needs to accept a TurnMode to pass through to
    // force_resolve_turn_for_mode() during timeout resolution.
    let session = three_player_session();
    let barrier = TurnBarrier::new(session, TurnBarrierConfig::new(Duration::from_millis(50)));

    // New method: set the turn mode on the barrier
    barrier.set_turn_mode(TurnMode::Cinematic { prompt: None });

    barrier.submit_action("player-1", "I listen carefully");

    let result = barrier.wait_for_turn().await;
    assert!(result.timed_out);

    // Auto-resolved players should have cinematic-style default, not "hesitates"
    let p2_narration = result.narration.get("player-2").expect("player-2 narration");
    assert!(
        p2_narration.contains("silent") || p2_narration.contains("remains"),
        "cinematic timeout should use 'remains silent', got: {}",
        p2_narration
    );
}

#[tokio::test]
async fn mode_aware_barrier_timeout_uses_structured_default() {
    let session = three_player_session();
    let barrier = TurnBarrier::new(session, TurnBarrierConfig::new(Duration::from_millis(50)));

    barrier.set_turn_mode(TurnMode::Structured);

    barrier.submit_action("player-1", "I attack");

    let result = barrier.wait_for_turn().await;
    assert!(result.timed_out);

    let p2_narration = result.narration.get("player-2").expect("player-2 narration");
    assert!(
        p2_narration.contains("hesitate") || p2_narration.contains("waiting"),
        "structured timeout should use 'hesitates' variant, got: {}",
        p2_narration
    );
}

// ---------------------------------------------------------------------------
// AC-6: Mixed complete/incomplete submission scenarios
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_two_of_three_submit_cinematic_mode() {
    let session = three_player_session();
    let barrier =
        TurnBarrier::new(session, TurnBarrierConfig::new(Duration::from_millis(100)));

    barrier.set_turn_mode(TurnMode::Cinematic { prompt: None });

    // Players 1 and 2 submit, player 3 (Brak) is AFK
    barrier.submit_action("player-1", "I charge the enemy");
    barrier.submit_action("player-2", "I cast a healing spell");

    let result = barrier.wait_for_turn().await;

    // Verify timeout state
    assert!(result.timed_out, "should time out");
    assert_eq!(result.missing_players, vec!["player-3".to_string()]);

    // Verify all 3 have narration
    assert_eq!(result.narration.len(), 3, "all 3 should have narration");

    // Verify auto-resolved uses cinematic default
    let brak = result.narration.get("player-3").expect("player-3 narration");
    assert!(
        brak.contains("silent") || brak.contains("remains"),
        "Brak should have cinematic default in narration, got: {}",
        brak
    );

    // Verify structured character names extraction
    let auto_names = result.auto_resolved_character_names();
    assert_eq!(auto_names, vec!["Brak".to_string()]);
}

#[tokio::test]
async fn e2e_all_players_timeout() {
    let session = two_player_session();
    let barrier = TurnBarrier::new(session, TurnBarrierConfig::new(Duration::from_millis(50)));

    barrier.set_turn_mode(TurnMode::Structured);

    // Nobody submits
    let result = barrier.wait_for_turn().await;

    assert!(result.timed_out, "should time out");
    assert_eq!(result.missing_players.len(), 2, "both players should be missing");

    let auto_names = result.auto_resolved_character_names();
    assert_eq!(auto_names.len(), 2, "both characters should be auto-resolved");
    assert!(auto_names.contains(&"Thorn".to_string()));
    assert!(auto_names.contains(&"Elara".to_string()));

    // All narration entries should have mode-contextual default
    for (_, narration) in &result.narration {
        assert!(
            narration.contains("hesitate") || narration.contains("waiting"),
            "all-timeout structured should use hesitates, got: {}",
            narration
        );
    }
}

#[tokio::test]
async fn e2e_single_player_timeout_returns_one_auto_resolved() {
    let mut players = HashMap::new();
    players.insert("solo".to_string(), make_character("Zara"));
    let session = MultiplayerSession::new(players);
    let barrier = TurnBarrier::new(session, TurnBarrierConfig::new(Duration::from_millis(50)));

    barrier.set_turn_mode(TurnMode::Structured);

    // Solo player doesn't submit
    let result = barrier.wait_for_turn().await;

    assert!(result.timed_out);
    let auto_names = result.auto_resolved_character_names();
    assert_eq!(auto_names, vec!["Zara".to_string()]);
}

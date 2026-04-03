//! Story 2-2: Session Actor — acceptance tests
//!
//! RED-phase tests: the `Session` type has not been implemented yet.
//! These tests are preserved behind a feature gate so they don't break
//! the build. Remove the gate when Session is implemented.
#![cfg(feature = "session_actor")]
//!
//! Tests cover 7 acceptance criteria:
//! 1. Session state machine: AwaitingConnect → Creating → Playing
//! 2. Genre binding via SESSION_EVENT{connect}
//! 3. State-appropriate message dispatch
//! 4. Out-of-phase rejection
//! 5. Session cleanup on disconnect
//! 6. Connected response with has_character flag
//! 7. Multiple independent sessions

use sidequest_server::{test_app_state, AppState, PlayerId};

// =========================================================================
// AC1: Session state machine — Connect → Create → Play transitions
// =========================================================================

/// The server must expose a Session type with three states.
#[test]
fn session_starts_in_awaiting_connect() {
    use sidequest_server::Session;

    let session = Session::new();
    assert!(
        session.is_awaiting_connect(),
        "New session should start in AwaitingConnect state"
    );
}

#[test]
fn session_transitions_to_creating() {
    use sidequest_server::Session;

    let mut session = Session::new();
    // Simulate a connect with genre binding
    let result = session.handle_connect("mutant_wasteland", "flickering_reach", "TestPlayer");
    assert!(result.is_ok(), "Connect should succeed");
    assert!(
        session.is_creating(),
        "After connect, session should be in Creating state"
    );
}

#[test]
fn session_transitions_to_playing() {
    use sidequest_server::Session;

    let mut session = Session::new();
    session
        .handle_connect("mutant_wasteland", "flickering_reach", "TestPlayer")
        .unwrap();
    // Complete character creation (stub — actual creation is story 2-3)
    let result = session.complete_character_creation();
    assert!(
        result.is_ok(),
        "Character creation completion should succeed"
    );
    assert!(
        session.is_playing(),
        "After character creation, session should be in Playing state"
    );
}

#[test]
fn session_cannot_skip_states() {
    use sidequest_server::Session;

    let mut session = Session::new();
    // Cannot complete character creation without connecting first
    let result = session.complete_character_creation();
    assert!(
        result.is_err(),
        "Cannot complete character creation in AwaitingConnect state"
    );
}

// =========================================================================
// AC2: Genre binding — SESSION_EVENT{connect} loads genre pack
// =========================================================================

#[test]
fn session_connect_binds_genre() {
    use sidequest_server::Session;

    let mut session = Session::new();
    session
        .handle_connect("mutant_wasteland", "flickering_reach", "TestPlayer")
        .unwrap();
    assert_eq!(
        session.genre_slug(),
        Some("mutant_wasteland"),
        "Genre slug should be bound after connect"
    );
    assert_eq!(
        session.world_slug(),
        Some("flickering_reach"),
        "World slug should be bound after connect"
    );
}

#[test]
fn session_connect_stores_player_name() {
    use sidequest_server::Session;

    let mut session = Session::new();
    session
        .handle_connect("mutant_wasteland", "flickering_reach", "Grog")
        .unwrap();
    assert_eq!(
        session.player_name(),
        Some("Grog"),
        "Player name should be stored after connect"
    );
}

// =========================================================================
// AC3: State dispatch — messages routed based on current state
// =========================================================================

#[test]
fn session_current_state_name() {
    use sidequest_server::Session;

    let session = Session::new();
    assert_eq!(session.state_name(), "AwaitingConnect");
}

#[test]
fn session_state_name_changes_on_transition() {
    use sidequest_server::Session;

    let mut session = Session::new();
    session
        .handle_connect("mutant_wasteland", "flickering_reach", "TestPlayer")
        .unwrap();
    assert_eq!(session.state_name(), "Creating");
}

// =========================================================================
// AC4: Out-of-phase rejection — wrong message type returns error
// =========================================================================

#[test]
fn session_rejects_player_action_in_awaiting_connect() {
    use sidequest_server::Session;

    let session = Session::new();
    let result = session.can_handle_message_type("PLAYER_ACTION");
    assert!(
        !result,
        "PLAYER_ACTION should be rejected in AwaitingConnect state"
    );
}

#[test]
fn session_allows_session_event_in_awaiting_connect() {
    use sidequest_server::Session;

    let session = Session::new();
    let result = session.can_handle_message_type("SESSION_EVENT");
    assert!(
        result,
        "SESSION_EVENT should be allowed in AwaitingConnect state"
    );
}

#[test]
fn session_rejects_player_action_in_creating() {
    use sidequest_server::Session;

    let mut session = Session::new();
    session
        .handle_connect("mutant_wasteland", "flickering_reach", "TestPlayer")
        .unwrap();
    let result = session.can_handle_message_type("PLAYER_ACTION");
    assert!(
        !result,
        "PLAYER_ACTION should be rejected in Creating state"
    );
}

#[test]
fn session_allows_character_creation_in_creating() {
    use sidequest_server::Session;

    let mut session = Session::new();
    session
        .handle_connect("mutant_wasteland", "flickering_reach", "TestPlayer")
        .unwrap();
    let result = session.can_handle_message_type("CHARACTER_CREATION");
    assert!(
        result,
        "CHARACTER_CREATION should be allowed in Creating state"
    );
}

#[test]
fn session_allows_player_action_in_playing() {
    use sidequest_server::Session;

    let mut session = Session::new();
    session
        .handle_connect("mutant_wasteland", "flickering_reach", "TestPlayer")
        .unwrap();
    session.complete_character_creation().unwrap();
    let result = session.can_handle_message_type("PLAYER_ACTION");
    assert!(result, "PLAYER_ACTION should be allowed in Playing state");
}

// =========================================================================
// AC5: Session cleanup — disconnect cleans up resources
// =========================================================================

#[test]
fn session_cleanup_returns_to_initial_state() {
    use sidequest_server::Session;

    let mut session = Session::new();
    session
        .handle_connect("mutant_wasteland", "flickering_reach", "TestPlayer")
        .unwrap();
    session.cleanup();
    assert!(
        session.is_awaiting_connect(),
        "After cleanup, session should reset to AwaitingConnect"
    );
}

// =========================================================================
// AC6: Connected response — SESSION_EVENT{connected} with has_character
// =========================================================================

#[test]
fn session_connect_produces_connected_response() {
    use sidequest_server::Session;

    let mut session = Session::new();
    let response = session
        .handle_connect("mutant_wasteland", "flickering_reach", "TestPlayer")
        .unwrap();

    // The response should be a SESSION_EVENT with event="connected"
    match response {
        sidequest_protocol::GameMessage::SessionEvent { payload, .. } => {
            assert_eq!(
                payload.event, "connected",
                "Response should have event='connected'"
            );
            assert!(
                payload.has_character.is_some(),
                "Response should include has_character flag"
            );
        }
        other => panic!("Expected SessionEvent, got: {:?}", other),
    }
}

#[test]
fn session_connect_new_player_has_no_character() {
    use sidequest_server::Session;

    let mut session = Session::new();
    let response = session
        .handle_connect("mutant_wasteland", "flickering_reach", "NewPlayer")
        .unwrap();

    match response {
        sidequest_protocol::GameMessage::SessionEvent { payload, .. } => {
            assert_eq!(
                payload.has_character,
                Some(false),
                "New player should have has_character=false"
            );
        }
        other => panic!("Expected SessionEvent, got: {:?}", other),
    }
}

// =========================================================================
// AC7: Multiple sessions — independent state
// =========================================================================

#[test]
fn two_sessions_are_independent() {
    use sidequest_server::Session;

    let mut session_a = Session::new();
    let mut session_b = Session::new();

    session_a
        .handle_connect("mutant_wasteland", "flickering_reach", "PlayerA")
        .unwrap();

    // session_b should still be in AwaitingConnect
    assert!(
        session_b.is_awaiting_connect(),
        "Session B should be independent from Session A"
    );
    assert!(
        session_a.is_creating(),
        "Session A should be in Creating after connect"
    );
}

// =========================================================================
// Rule enforcement: #[non_exhaustive] on Session-related enums
// =========================================================================

#[test]
fn session_state_is_queryable() {
    use sidequest_server::Session;

    // Verify all three state query methods exist and are consistent
    let session = Session::new();
    assert!(session.is_awaiting_connect());
    assert!(!session.is_creating());
    assert!(!session.is_playing());
}

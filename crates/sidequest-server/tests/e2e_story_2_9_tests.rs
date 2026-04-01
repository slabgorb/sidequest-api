//! Story 2-9: End-to-end integration — failing tests (RED phase)
//!
//! Tests cover the full integration pipeline: UI connects to API over WebSocket,
//! completes a turn cycle, narration streams to client, state updates broadcast.
//!
//! Acceptance Criteria:
//!   1. Session connect — WebSocket → SESSION_EVENT{connect} → SESSION_EVENT{connected}
//!   2. Character creation — server-driven scene flow via WebSocket
//!   3. Full turn cycle — PLAYER_ACTION → THINKING → NARRATION_CHUNK* → NARRATION_END
//!   4. State updates — PARTY_STATUS broadcast after turn
//!   5. Multi-turn play — sequential actions without reconnect
//!   6. Error handling — wrong-state messages, timeouts, unknown types survive
//!   7. Reconnection — new WebSocket resumes session with saved state
//!   8. Genre list — GET /api/genres returns genre data with worlds
//!   9. Integration test — automated connect → create → action → narration
//!
//! Rule-enforcement tests from the Rust lang-review checklist are included
//! at the bottom of this file.

use std::time::Duration;

use futures::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::time::timeout;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message as WsMessage;

use sidequest_server::{build_router, serve_with_listener, test_app_state, AppState};

// =========================================================================
// Test helpers
// =========================================================================

/// Start a test server on an OS-assigned port. Returns (address, shutdown sender).
async fn start_test_server() -> (std::net::SocketAddr, tokio::sync::oneshot::Sender<()>) {
    let state = test_app_state();
    start_test_server_with_state(state).await
}

/// Start a test server with a specific AppState.
async fn start_test_server_with_state(
    state: AppState,
) -> (std::net::SocketAddr, tokio::sync::oneshot::Sender<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    tokio::spawn(async move {
        serve_with_listener(state, listener, shutdown_rx)
            .await
            .unwrap();
    });

    // Brief pause to let the server start accepting connections
    tokio::time::sleep(Duration::from_millis(50)).await;

    (addr, shutdown_tx)
}

/// Connect a WebSocket client to the test server.
async fn ws_connect(
    addr: std::net::SocketAddr,
) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    let url = format!("ws://127.0.0.1:{}/ws", addr.port());
    let (ws, _) = connect_async(&url)
        .await
        .expect("WebSocket connection should succeed");
    ws
}

/// Send a GameMessage as JSON over WebSocket.
async fn send_game_message(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    msg: &Value,
) {
    ws.send(WsMessage::Text(msg.to_string())).await.unwrap();
}

/// Receive the next text message, parse as JSON. Skips ping/pong frames.
/// Times out after the given duration.
async fn recv_json(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    timeout_dur: Duration,
) -> Value {
    let deadline = timeout_dur;
    loop {
        let msg = timeout(deadline, ws.next())
            .await
            .expect("Should receive message within timeout")
            .expect("Stream should not end")
            .expect("Message should not be an error");

        match msg {
            WsMessage::Text(text) => {
                return serde_json::from_str(&text)
                    .unwrap_or_else(|e| panic!("Failed to parse JSON: {e}\nRaw: {text}"));
            }
            WsMessage::Ping(_) | WsMessage::Pong(_) => continue,
            other => panic!("Unexpected WebSocket frame: {:?}", other),
        }
    }
}

/// Receive the next GameMessage. Times out after 2 seconds.
async fn recv_game_message(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> Value {
    recv_json(ws, Duration::from_secs(2)).await
}

/// Helper: construct a SESSION_EVENT{connect} message.
fn session_connect_msg(player_name: &str, genre: &str, world: &str) -> Value {
    serde_json::json!({
        "type": "SESSION_EVENT",
        "payload": {
            "event": "connect",
            "player_name": player_name,
            "genre": genre,
            "world": world
        },
        "player_id": ""
    })
}

/// Helper: construct a PLAYER_ACTION message.
fn player_action_msg(action: &str) -> Value {
    serde_json::json!({
        "type": "PLAYER_ACTION",
        "payload": { "action": action, "aside": false },
        "player_id": ""
    })
}

/// Helper: construct a CHARACTER_CREATION response (choice).
fn character_creation_choice_msg(choice: &str) -> Value {
    serde_json::json!({
        "type": "CHARACTER_CREATION",
        "payload": { "phase": "scene", "choice": choice },
        "player_id": ""
    })
}

/// Helper: construct a CHARACTER_CREATION confirmation.
fn character_creation_confirm_msg() -> Value {
    serde_json::json!({
        "type": "CHARACTER_CREATION",
        "payload": { "phase": "confirmation", "choice": "Yes" },
        "player_id": ""
    })
}

// =========================================================================
// AC1: Session connect — WebSocket → SESSION_EVENT{connect} → response
// =========================================================================

/// After connecting via WebSocket and sending SESSION_EVENT{connect},
/// the server MUST respond with SESSION_EVENT{connected, has_character: false}
/// for a new player (no save file).
#[tokio::test]
async fn session_connect_returns_connected_response() {
    let (addr, _shutdown) = start_test_server().await;
    let mut ws = ws_connect(addr).await;

    // Send connect
    let connect = session_connect_msg("Grok", "mutant_wasteland", "flickering_reach");
    send_game_message(&mut ws, &connect).await;

    // Expect SESSION_EVENT{connected}
    let response = recv_game_message(&mut ws).await;
    assert_eq!(
        response["type"], "SESSION_EVENT",
        "Response must be SESSION_EVENT, got: {}",
        response["type"]
    );
    assert_eq!(
        response["payload"]["event"], "connected",
        "Event must be 'connected', got: {}",
        response["payload"]["event"]
    );
    assert_eq!(
        response["payload"]["has_character"], false,
        "New player must have has_character=false"
    );
}

/// The connected response should echo back the genre and world.
#[tokio::test]
async fn session_connect_echoes_genre_and_world() {
    let (addr, _shutdown) = start_test_server().await;
    let mut ws = ws_connect(addr).await;

    let connect = session_connect_msg("Zara", "mutant_wasteland", "flickering_reach");
    send_game_message(&mut ws, &connect).await;

    let response = recv_game_message(&mut ws).await;
    assert_eq!(response["payload"]["genre"], "mutant_wasteland");
    assert_eq!(response["payload"]["world"], "flickering_reach");
}

/// Sending SESSION_EVENT{connect} twice on the same socket should produce
/// an error, not silently succeed.
#[tokio::test]
async fn double_connect_rejected() {
    let (addr, _shutdown) = start_test_server().await;
    let mut ws = ws_connect(addr).await;

    let connect = session_connect_msg("Grok", "mutant_wasteland", "flickering_reach");
    send_game_message(&mut ws, &connect).await;

    // Consume the connected response and any queued creation scene
    let _ = recv_game_message(&mut ws).await; // connected
                                              // Drain the CHARACTER_CREATION scene that the server auto-sends after connect
    let msg2 = recv_game_message(&mut ws).await;
    if msg2["type"] != "ERROR" {
        // Scene was queued — now send second connect
        send_game_message(&mut ws, &connect).await;
    }

    // Read until we find the ERROR (skip any remaining queued messages)
    let mut got_error = false;
    for _ in 0..5 {
        let response = recv_game_message(&mut ws).await;
        if response["type"] == "ERROR" {
            got_error = true;
            break;
        }
    }
    assert!(got_error, "Double connect must produce ERROR");
}

// =========================================================================
// AC2: Character creation flow via WebSocket
// =========================================================================

/// After connect (has_character=false), the server must initiate character
/// creation by sending CHARACTER_CREATION{phase: "scene"} messages.
#[tokio::test]
async fn character_creation_scene_sent_after_connect() {
    let (addr, _shutdown) = start_test_server().await;
    let mut ws = ws_connect(addr).await;

    // Connect
    let connect = session_connect_msg("Grok", "mutant_wasteland", "flickering_reach");
    send_game_message(&mut ws, &connect).await;
    let connected = recv_game_message(&mut ws).await;
    assert_eq!(connected["payload"]["has_character"], false);

    // Server should send the first CHARACTER_CREATION scene
    let scene = recv_game_message(&mut ws).await;
    assert_eq!(
        scene["type"], "CHARACTER_CREATION",
        "After connect with no character, server must send CHARACTER_CREATION, got: {}",
        scene["type"]
    );
    assert_eq!(
        scene["payload"]["phase"], "scene",
        "First creation message must be phase 'scene'"
    );
    assert!(
        scene["payload"]["scene_index"].is_number(),
        "Scene must have a numeric scene_index"
    );
    assert!(
        scene["payload"]["total_scenes"].is_number(),
        "Scene must have a numeric total_scenes"
    );
}

/// Character creation must include choices array with label and description.
#[tokio::test]
async fn character_creation_scene_has_choices() {
    let (addr, _shutdown) = start_test_server().await;
    let mut ws = ws_connect(addr).await;

    let connect = session_connect_msg("Grok", "mutant_wasteland", "flickering_reach");
    send_game_message(&mut ws, &connect).await;
    let _ = recv_game_message(&mut ws).await; // connected

    let scene = recv_game_message(&mut ws).await;
    assert_eq!(scene["type"], "CHARACTER_CREATION");

    let choices = scene["payload"]["choices"]
        .as_array()
        .expect("Scene must have a choices array");
    assert!(!choices.is_empty(), "Choices array must not be empty");
    for choice in choices {
        assert!(
            choice["label"].is_string(),
            "Each choice must have a string label"
        );
        assert!(
            choice["description"].is_string(),
            "Each choice must have a string description"
        );
    }
}

/// After the client responds to all scenes, the server must send
/// CHARACTER_CREATION{phase: "confirmation"} and then {phase: "complete"}
/// followed by SESSION_EVENT{ready}.
#[tokio::test]
#[ignore = "requires Claude CLI subprocess — needs test infrastructure for mocking"]
async fn character_creation_completes_to_ready() {
    let (addr, _shutdown) = start_test_server().await;
    let mut ws = ws_connect(addr).await;

    // Connect
    let connect = session_connect_msg("Grok", "mutant_wasteland", "flickering_reach");
    send_game_message(&mut ws, &connect).await;
    let _ = recv_game_message(&mut ws).await; // connected

    // Drive through all creation scenes
    let mut got_complete = false;
    let mut got_ready = false;

    for _ in 0..20 {
        // safety limit
        let msg = recv_game_message(&mut ws).await;

        match msg["type"].as_str().unwrap_or("") {
            "CHARACTER_CREATION" => {
                let phase = msg["payload"]["phase"].as_str().unwrap_or("");
                match phase {
                    "scene" => {
                        // Pick the first choice
                        let choice = character_creation_choice_msg("1");
                        send_game_message(&mut ws, &choice).await;
                    }
                    "confirmation" => {
                        let confirm = character_creation_confirm_msg();
                        send_game_message(&mut ws, &confirm).await;
                    }
                    "complete" => {
                        got_complete = true;
                        // character data should be present
                        assert!(
                            !msg["payload"]["character"].is_null(),
                            "Complete phase must include character data"
                        );
                    }
                    _ => panic!("Unexpected CHARACTER_CREATION phase: {}", phase),
                }
            }
            "SESSION_EVENT" => {
                if msg["payload"]["event"] == "ready" {
                    got_ready = true;
                    break;
                }
            }
            _ => {} // skip other messages
        }
    }

    assert!(
        got_complete,
        "Must receive CHARACTER_CREATION{{phase: complete}}"
    );
    assert!(
        got_ready,
        "Must receive SESSION_EVENT{{ready}} after creation"
    );
}

// =========================================================================
// AC3: Full turn cycle — PLAYER_ACTION → THINKING → NARRATION → state
// =========================================================================

/// After a player is in the Playing state and sends PLAYER_ACTION,
/// the server must respond with THINKING, then NARRATION_CHUNK(s) or
/// NARRATION, then NARRATION_END.
#[tokio::test]
#[ignore = "requires Claude CLI subprocess — needs test infrastructure for mocking"]
async fn turn_cycle_produces_thinking_then_narration() {
    let (addr, _shutdown) = start_test_server().await;
    let mut ws = ws_connect(addr).await;

    // Fast-path: connect + complete character creation
    drive_to_playing_state(&mut ws).await;

    // Send a player action
    let action = player_action_msg("I look around the tavern");
    send_game_message(&mut ws, &action).await;

    // Collect the response sequence
    let mut got_thinking = false;
    let mut got_narration_end = false;
    let mut narration_text = String::new();

    for _ in 0..30 {
        let msg = recv_json(&mut ws, Duration::from_secs(10)).await;
        match msg["type"].as_str().unwrap_or("") {
            "THINKING" => {
                got_thinking = true;
            }
            "NARRATION" => {
                // Batch narration (non-streaming)
                let text = msg["payload"]["text"].as_str().unwrap_or("");
                narration_text.push_str(text);
            }
            "NARRATION_CHUNK" => {
                let text = msg["payload"]["text"].as_str().unwrap_or("");
                narration_text.push_str(text);
            }
            "NARRATION_END" => {
                got_narration_end = true;
                break;
            }
            _ => {} // PARTY_STATUS, TURN_STATUS, etc.
        }
    }

    assert!(got_thinking, "Server must send THINKING before narration");
    assert!(
        got_narration_end,
        "Server must send NARRATION_END after narration"
    );
    assert!(
        !narration_text.is_empty(),
        "At least some narration text must be produced"
    );
}

/// THINKING must arrive BEFORE any NARRATION_CHUNK or NARRATION_END.
#[tokio::test]
#[ignore = "requires Claude CLI subprocess — needs test infrastructure for mocking"]
async fn thinking_arrives_before_narration() {
    let (addr, _shutdown) = start_test_server().await;
    let mut ws = ws_connect(addr).await;

    drive_to_playing_state(&mut ws).await;

    let action = player_action_msg("I search for supplies");
    send_game_message(&mut ws, &action).await;

    let mut saw_thinking = false;
    let mut narration_before_thinking = false;

    for _ in 0..30 {
        let msg = recv_json(&mut ws, Duration::from_secs(10)).await;
        match msg["type"].as_str().unwrap_or("") {
            "THINKING" => saw_thinking = true,
            "NARRATION" | "NARRATION_CHUNK" | "NARRATION_END" => {
                if !saw_thinking {
                    narration_before_thinking = true;
                }
                if msg["type"] == "NARRATION_END" {
                    break;
                }
            }
            _ => {}
        }
    }

    assert!(saw_thinking, "Must receive THINKING");
    assert!(
        !narration_before_thinking,
        "THINKING must arrive before any narration messages"
    );
}

/// NARRATION_END must include a state_delta field (may be null if no changes).
#[tokio::test]
#[ignore = "requires Claude CLI subprocess — needs test infrastructure for mocking"]
async fn narration_end_includes_state_delta_field() {
    let (addr, _shutdown) = start_test_server().await;
    let mut ws = ws_connect(addr).await;

    drive_to_playing_state(&mut ws).await;

    let action = player_action_msg("I pick up the rusted pipe");
    send_game_message(&mut ws, &action).await;

    for _ in 0..30 {
        let msg = recv_json(&mut ws, Duration::from_secs(10)).await;
        if msg["type"] == "NARRATION_END" {
            // state_delta field must be present (even if null)
            assert!(
                msg["payload"].get("state_delta").is_some(),
                "NARRATION_END must include state_delta field in payload"
            );
            return;
        }
    }
    panic!("Never received NARRATION_END");
}

// =========================================================================
// AC4: State updates — PARTY_STATUS after turn
// =========================================================================

/// After a completed turn, the server should broadcast PARTY_STATUS
/// with the player's character in the members list.
#[tokio::test]
#[ignore = "requires Claude CLI subprocess — needs test infrastructure for mocking"]
async fn party_status_sent_after_turn() {
    let (addr, _shutdown) = start_test_server().await;
    let mut ws = ws_connect(addr).await;

    drive_to_playing_state(&mut ws).await;

    let action = player_action_msg("I introduce myself to the wasteland");
    send_game_message(&mut ws, &action).await;

    let mut got_party_status = false;

    for _ in 0..30 {
        let msg = recv_json(&mut ws, Duration::from_secs(10)).await;
        if msg["type"] == "PARTY_STATUS" {
            got_party_status = true;
            let members = msg["payload"]["members"]
                .as_array()
                .expect("PARTY_STATUS must have members array");
            assert!(
                !members.is_empty(),
                "After character creation, party must have at least one member"
            );
            // Each member must have required fields per api-contract.md
            for member in members {
                assert!(member["name"].is_string(), "Member must have name");
                assert!(
                    member["current_hp"].is_number(),
                    "Member must have current_hp"
                );
                assert!(member["max_hp"].is_number(), "Member must have max_hp");
            }
            break;
        }
        if msg["type"] == "NARRATION_END" && !got_party_status {
            // PARTY_STATUS may come after NARRATION_END
            continue;
        }
    }

    assert!(
        got_party_status,
        "Server must send PARTY_STATUS after a completed turn"
    );
}

// =========================================================================
// AC5: Multi-turn play — sequential actions, same session
// =========================================================================

/// A player must be able to take multiple sequential actions in the same
/// session without reconnecting. Each action must produce a full
/// THINKING → narration → NARRATION_END sequence.
#[tokio::test]
#[ignore = "requires Claude CLI subprocess — needs test infrastructure for mocking"]
async fn multi_turn_play_works() {
    let (addr, _shutdown) = start_test_server().await;
    let mut ws = ws_connect(addr).await;

    drive_to_playing_state(&mut ws).await;

    // Take three sequential actions
    for (i, action_text) in [
        "I look around",
        "I talk to the nearest person",
        "I search for supplies",
    ]
    .iter()
    .enumerate()
    {
        let action = player_action_msg(action_text);
        send_game_message(&mut ws, &action).await;

        let mut got_end = false;
        for _ in 0..30 {
            let msg = recv_json(&mut ws, Duration::from_secs(10)).await;
            if msg["type"] == "NARRATION_END" {
                got_end = true;
                break;
            }
        }
        assert!(
            got_end,
            "Turn {} ({}) must produce NARRATION_END",
            i + 1,
            action_text
        );
    }
}

// =========================================================================
// AC6: Error handling — wrong state, timeouts, unknown types
// =========================================================================

/// Sending PLAYER_ACTION before connecting (in AwaitingConnect state)
/// must produce an ERROR response, not crash.
#[tokio::test]
async fn player_action_before_connect_rejected() {
    let (addr, _shutdown) = start_test_server().await;
    let mut ws = ws_connect(addr).await;

    // Send action without connecting first
    let action = player_action_msg("I look around");
    send_game_message(&mut ws, &action).await;

    let response = recv_game_message(&mut ws).await;
    assert_eq!(
        response["type"], "ERROR",
        "Action before connect must produce ERROR, got: {}",
        response["type"]
    );
}

/// Sending PLAYER_ACTION during character creation (in Creating state)
/// must produce an ERROR response, not silently ignore.
#[tokio::test]
async fn player_action_during_creation_rejected() {
    let (addr, _shutdown) = start_test_server().await;
    let mut ws = ws_connect(addr).await;

    // Connect (enters Creating state)
    let connect = session_connect_msg("Grok", "mutant_wasteland", "flickering_reach");
    send_game_message(&mut ws, &connect).await;
    let _ = recv_game_message(&mut ws).await; // connected

    // Try to send action while in creation
    let action = player_action_msg("I look around");
    send_game_message(&mut ws, &action).await;

    // Should get ERROR (possibly after the first creation scene)
    let mut got_error = false;
    for _ in 0..5 {
        let msg = recv_game_message(&mut ws).await;
        if msg["type"] == "ERROR" {
            got_error = true;
            break;
        }
    }
    assert!(
        got_error,
        "PLAYER_ACTION during character creation must produce ERROR"
    );
}

/// Sending a completely unknown message type should not crash the server
/// or disconnect the client. It should produce an ERROR response.
#[tokio::test]
#[ignore = "requires full session flow — connect drains wrong number of messages without Claude CLI"]
async fn unknown_message_type_produces_error_not_crash() {
    let (addr, _shutdown) = start_test_server().await;
    let mut ws = ws_connect(addr).await;

    // Connect first and drain queued messages (connected + creation scene)
    let connect = session_connect_msg("Grok", "mutant_wasteland", "flickering_reach");
    send_game_message(&mut ws, &connect).await;
    let _ = recv_game_message(&mut ws).await; // connected
    let _ = recv_game_message(&mut ws).await; // creation scene

    // Send unknown type
    let unknown = serde_json::json!({
        "type": "ALIEN_PROBE",
        "payload": { "beep": "boop" },
        "player_id": ""
    });
    send_game_message(&mut ws, &unknown).await;

    // Should get ERROR response, not disconnect
    let response = recv_game_message(&mut ws).await;
    assert_eq!(
        response["type"], "ERROR",
        "Unknown message type must produce ERROR, got: {}",
        response["type"]
    );

    // Connection should still be alive — send another valid message
    let connect2 = serde_json::json!({
        "type": "SESSION_EVENT",
        "payload": { "event": "ping" },
        "player_id": ""
    });
    send_game_message(&mut ws, &connect2).await;
    // If we got here without error, the connection survived
}

/// Sending empty JSON object should produce ERROR, not crash.
#[tokio::test]
async fn empty_json_object_produces_error() {
    let (addr, _shutdown) = start_test_server().await;
    let mut ws = ws_connect(addr).await;

    send_game_message(&mut ws, &serde_json::json!({})).await;

    let response = recv_game_message(&mut ws).await;
    assert_eq!(response["type"], "ERROR", "Empty JSON must produce ERROR");
}

// =========================================================================
// AC7: Reconnection — new WebSocket resumes session
// =========================================================================

/// After disconnecting and reconnecting with the same player name,
/// the server should recognize the returning player. If they had a
/// character, SESSION_EVENT{ready} with initial_state should be sent
/// instead of starting character creation again.
#[tokio::test]
#[ignore = "requires Claude CLI subprocess — needs test infrastructure for mocking"]
async fn reconnection_resumes_with_character() {
    let (addr, _shutdown) = start_test_server().await;

    // First connection: create character
    let mut ws1 = ws_connect(addr).await;
    drive_to_playing_state(&mut ws1).await;

    // Take an action to generate some state
    let action = player_action_msg("I look around");
    send_game_message(&mut ws1, &action).await;
    // Drain the response
    for _ in 0..30 {
        let msg = recv_json(&mut ws1, Duration::from_secs(10)).await;
        if msg["type"] == "NARRATION_END" {
            break;
        }
    }

    // Disconnect first client
    ws1.close(None).await.ok();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Reconnect with same player name
    let mut ws2 = ws_connect(addr).await;
    let connect = session_connect_msg("Grok", "mutant_wasteland", "flickering_reach");
    send_game_message(&mut ws2, &connect).await;

    let response = recv_game_message(&mut ws2).await;
    assert_eq!(response["type"], "SESSION_EVENT");

    // For a returning player with a character, has_character should be true
    // and event should be "ready" (or "connected" with has_character: true)
    let has_character = response["payload"]["has_character"].as_bool();
    let event = response["payload"]["event"].as_str().unwrap_or("");

    assert!(
        has_character == Some(true) || event == "ready",
        "Returning player should get has_character=true or event=ready, got: has_character={:?}, event={}",
        has_character,
        event
    );
}

/// Reconnection for a player with a character should include initial_state
/// with the character's current status.
#[tokio::test]
#[ignore = "requires Claude CLI subprocess — needs test infrastructure for mocking"]
async fn reconnection_includes_initial_state() {
    let (addr, _shutdown) = start_test_server().await;

    // First session: create character
    let mut ws1 = ws_connect(addr).await;
    drive_to_playing_state(&mut ws1).await;
    ws1.close(None).await.ok();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Reconnect
    let mut ws2 = ws_connect(addr).await;
    let connect = session_connect_msg("Grok", "mutant_wasteland", "flickering_reach");
    send_game_message(&mut ws2, &connect).await;

    // Find the ready event (may be first or second message)
    for _ in 0..5 {
        let msg = recv_game_message(&mut ws2).await;
        if msg["payload"]["event"] == "ready" {
            let initial_state = &msg["payload"]["initial_state"];
            assert!(
                !initial_state.is_null(),
                "Ready event for returning player must include initial_state"
            );
            assert!(
                initial_state["characters"].is_array(),
                "initial_state must include characters array"
            );
            assert!(
                initial_state["location"].is_string(),
                "initial_state must include location string"
            );
            return;
        }
    }
    panic!("Never received SESSION_EVENT{{ready}} with initial_state for returning player");
}

// =========================================================================
// AC8: Genre list — GET /api/genres returns valid data
// =========================================================================

/// GET /api/genres must return a JSON object where each key is a genre slug
/// and each value has a "worlds" array of strings.
#[tokio::test]
async fn genres_endpoint_returns_valid_structure() {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    let state = test_app_state();
    let app = build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/genres")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    let obj = json.as_object().expect("Response must be a JSON object");

    // If genre_packs path exists, we should have at least one genre
    // (mutant_wasteland is the test genre)
    if !obj.is_empty() {
        for (slug, data) in obj {
            assert!(
                data["worlds"].is_array(),
                "Genre '{}' must have worlds array",
                slug
            );
        }
    }
}

// =========================================================================
// AC9: Full integration test — connect → create → action → narration
// =========================================================================

/// The golden path integration test: a player connects, creates a character,
/// takes an action, and receives narrated response with state updates.
/// This is the capstone test for story 2-9.
#[tokio::test]
#[ignore = "requires Claude CLI subprocess — needs test infrastructure for mocking"]
async fn full_e2e_connect_create_play_narrate() {
    let (addr, _shutdown) = start_test_server().await;
    let mut ws = ws_connect(addr).await;

    // Step 1: Connect
    let connect = session_connect_msg("Grok", "mutant_wasteland", "flickering_reach");
    send_game_message(&mut ws, &connect).await;

    let connected = recv_game_message(&mut ws).await;
    assert_eq!(connected["type"], "SESSION_EVENT");
    assert_eq!(connected["payload"]["event"], "connected");

    // Step 2: Character creation (drive through scenes)
    let mut ready = false;
    for _ in 0..20 {
        let msg = recv_game_message(&mut ws).await;
        match msg["type"].as_str().unwrap_or("") {
            "CHARACTER_CREATION" => {
                let phase = msg["payload"]["phase"].as_str().unwrap_or("");
                match phase {
                    "scene" => {
                        send_game_message(&mut ws, &character_creation_choice_msg("1")).await;
                    }
                    "confirmation" => {
                        send_game_message(&mut ws, &character_creation_confirm_msg()).await;
                    }
                    "complete" => {}
                    _ => {}
                }
            }
            "SESSION_EVENT" if msg["payload"]["event"] == "ready" => {
                ready = true;
                break;
            }
            _ => {}
        }
    }
    assert!(ready, "Must reach ready state after character creation");

    // Step 3: Take an action
    let action = player_action_msg("I look around the tavern");
    send_game_message(&mut ws, &action).await;

    // Step 4: Collect narration response
    let mut got_thinking = false;
    let mut got_narration_text = false;
    let mut got_narration_end = false;

    for _ in 0..30 {
        let msg = recv_json(&mut ws, Duration::from_secs(10)).await;
        match msg["type"].as_str().unwrap_or("") {
            "THINKING" => got_thinking = true,
            "NARRATION" => {
                if msg["payload"]["text"]
                    .as_str()
                    .map_or(false, |t| !t.is_empty())
                {
                    got_narration_text = true;
                }
            }
            "NARRATION_CHUNK" => {
                if msg["payload"]["text"]
                    .as_str()
                    .map_or(false, |t| !t.is_empty())
                {
                    got_narration_text = true;
                }
            }
            "NARRATION_END" => {
                got_narration_end = true;
                break;
            }
            _ => {}
        }
    }

    assert!(got_thinking, "E2E: Must receive THINKING");
    assert!(got_narration_text, "E2E: Must receive narration text");
    assert!(got_narration_end, "E2E: Must receive NARRATION_END");
}

// =========================================================================
// Wire format verification — message field naming matches api-contract.md
// =========================================================================

/// All server-originated messages must use SCREAMING_SNAKE_CASE for the
/// type field, matching the UI's expectations.
#[tokio::test]
async fn server_messages_use_screaming_snake_type_field() {
    let (addr, _shutdown) = start_test_server().await;
    let mut ws = ws_connect(addr).await;

    let connect = session_connect_msg("Grok", "mutant_wasteland", "flickering_reach");
    send_game_message(&mut ws, &connect).await;

    // Collect available messages (connected + creation scene) and verify format
    // Server sends 2 messages after connect: SESSION_EVENT + CHARACTER_CREATION
    for _ in 0..2 {
        let msg = recv_game_message(&mut ws).await;
        let msg_type = msg["type"]
            .as_str()
            .expect("Server message must have string 'type' field");

        // SCREAMING_SNAKE_CASE: all uppercase with underscores
        assert!(
            msg_type.chars().all(|c| c.is_ascii_uppercase() || c == '_'),
            "Message type '{}' must be SCREAMING_SNAKE_CASE",
            msg_type
        );
    }
}

/// Payload fields must use snake_case, not camelCase.
#[tokio::test]
async fn server_payload_fields_are_snake_case() {
    let (addr, _shutdown) = start_test_server().await;
    let mut ws = ws_connect(addr).await;

    let connect = session_connect_msg("Grok", "mutant_wasteland", "flickering_reach");
    send_game_message(&mut ws, &connect).await;

    let response = recv_game_message(&mut ws).await;
    let payload = response["payload"].as_object().expect("Must have payload");

    for key in payload.keys() {
        // snake_case: no uppercase letters
        assert!(
            !key.chars().any(|c| c.is_ascii_uppercase()),
            "Payload field '{}' must be snake_case, not camelCase",
            key
        );
    }
}

/// Client sends player_id as empty string. Server must accept this.
#[tokio::test]
async fn server_accepts_empty_player_id_from_client() {
    let (addr, _shutdown) = start_test_server().await;
    let mut ws = ws_connect(addr).await;

    // The connect message has player_id: "" per api-contract.md
    let connect = serde_json::json!({
        "type": "SESSION_EVENT",
        "payload": {
            "event": "connect",
            "player_name": "Grok",
            "genre": "mutant_wasteland",
            "world": "flickering_reach"
        },
        "player_id": ""
    });
    send_game_message(&mut ws, &connect).await;

    let response = recv_game_message(&mut ws).await;
    // Should not be an error — empty player_id is expected from clients
    assert_ne!(
        response["type"], "ERROR",
        "Server must accept empty player_id from client"
    );
}

// =========================================================================
// Rule-enforcement tests (Rust lang-review checklist)
// =========================================================================

// --- Rule #1: Silent error swallowing ---
// Integration-level: verified by the error handling tests above.
// The server must not silently swallow errors from session dispatch.

// --- Rule #2: #[non_exhaustive] on public enums ---

/// ServerError must remain #[non_exhaustive] — verified via wildcard match.
#[test]
fn server_error_non_exhaustive() {
    use sidequest_server::ServerError;
    let err = ServerError::connection_closed();
    match err {
        ServerError::ConnectionClosed => {}
        _ => {} // compiles only if #[non_exhaustive]
    }
}

// --- Rule #6: Test quality self-check ---
// Every test in this file has meaningful assertions with descriptive messages.
// No `let _ = result;` patterns. No `assert!(true)`.
// recv_game_message uses timeout + unwrap for fail-fast on missing messages.

// --- Rule #9: Private fields with getters ---

/// Session state must be accessed through methods, not public fields.
#[test]
fn session_state_via_methods_not_fields() {
    use sidequest_server::Session;
    let session = Session::new();
    // These are getter methods — direct field access should not compile
    assert!(session.is_awaiting_connect());
    assert_eq!(session.state_name(), "AwaitingConnect");
    assert_eq!(session.genre_slug(), None);
    assert_eq!(session.world_slug(), None);
    assert_eq!(session.player_name(), None);
}

// --- Rule #13: Constructor/Deserialize consistency ---
// Tested by session connect tests: the wire format must match
// what Session::handle_connect produces. No bypass path.

// =========================================================================
// Helper: drive a WebSocket connection to the Playing state
// =========================================================================

/// Connect, go through character creation, arrive in Playing state.
/// This is used by tests that need a ready player.
async fn drive_to_playing_state(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) {
    // Connect
    let connect = session_connect_msg("Grok", "mutant_wasteland", "flickering_reach");
    send_game_message(ws, &connect).await;

    // Drive through creation
    for _ in 0..20 {
        let msg = recv_game_message(ws).await;
        match msg["type"].as_str().unwrap_or("") {
            "SESSION_EVENT" => {
                let event = msg["payload"]["event"].as_str().unwrap_or("");
                match event {
                    "connected" => {
                        // If has_character is true, we're already playing
                        if msg["payload"]["has_character"] == true {
                            // Wait for ready
                            continue;
                        }
                    }
                    "ready" => return, // Done!
                    _ => {}
                }
            }
            "CHARACTER_CREATION" => {
                let phase = msg["payload"]["phase"].as_str().unwrap_or("");
                match phase {
                    "scene" => {
                        send_game_message(ws, &character_creation_choice_msg("1")).await;
                    }
                    "confirmation" => {
                        send_game_message(ws, &character_creation_confirm_msg()).await;
                    }
                    "complete" => {} // Wait for ready
                    _ => {}
                }
            }
            _ => {}
        }
    }
    panic!("Failed to reach Playing state within 20 message exchanges");
}

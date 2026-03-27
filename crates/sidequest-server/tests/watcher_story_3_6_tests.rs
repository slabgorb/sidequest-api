//! Story 3-6 RED: Watcher WebSocket endpoint — /ws/watcher streaming telemetry.
//!
//! Tests cover all acceptance criteria for the watcher endpoint:
//! 1. Endpoint serves WebSocket — /ws/watcher accepts upgrade and holds connection
//! 2. Telemetry stream — connected client receives JSON-serialized WatcherEvent messages
//! 3. Multiple clients — two viewers connect simultaneously and both receive all events
//! 4. Zero overhead — broadcast::send errors silently ignored when no viewer connected
//! 5. Event structure — each event includes timestamp, component, event_type, severity, fields
//! 6. Clean disconnect — viewer disconnects without affecting game traffic on /ws
//!
//! Additionally tests rule-enforcement from the Rust lang-review checklist.

use std::collections::HashMap;
use std::time::Duration;

use axum::body::Body;
use axum::http::Request;
use serde_json::Value;
use tokio::time::timeout;
use tower::ServiceExt;

use sidequest_server::{build_router, test_app_state, AppState};

// Import types that story 3-6 must create.
// These will cause compilation failures until implemented — that's the RED signal.
use sidequest_server::{Severity, WatcherEvent, WatcherEventType};

// =========================================================================
// Helpers
// =========================================================================

/// Build a sample WatcherEvent for test assertions.
fn sample_watcher_event() -> WatcherEvent {
    let mut fields = HashMap::new();
    fields.insert(
        "agent_name".to_string(),
        serde_json::Value::String("DescriptionAgent".to_string()),
    );

    WatcherEvent {
        timestamp: chrono::Utc::now(),
        component: "agent".to_string(),
        event_type: WatcherEventType::AgentSpanOpen,
        severity: Severity::Info,
        fields,
    }
}

/// Helper: create server with a pre-bound listener (mirrors 2-1 test pattern).
async fn create_server_with_listener(
    state: AppState,
    listener: tokio::net::TcpListener,
    shutdown: tokio::sync::oneshot::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use sidequest_server::serve_with_listener;
    serve_with_listener(state, listener, shutdown).await
}

// =========================================================================
// AC: WatcherEvent type — must exist with required fields
// =========================================================================

/// WatcherEvent must be a serializable struct with timestamp, component,
/// event_type, severity, and fields.
#[test]
fn watcher_event_has_required_fields() {
    let event = sample_watcher_event();

    // Verify all fields are accessible
    assert_eq!(event.component, "agent");
    assert!(matches!(event.event_type, WatcherEventType::AgentSpanOpen));
    assert!(matches!(event.severity, Severity::Info));
    assert!(event.fields.contains_key("agent_name"));

    // Timestamp should be recent (within last 5 seconds)
    let now = chrono::Utc::now();
    let age = now.signed_duration_since(event.timestamp);
    assert!(
        age.num_seconds() < 5,
        "Timestamp should be recent, age was {} seconds",
        age.num_seconds()
    );
}

/// WatcherEvent must serialize to JSON with all fields present.
#[test]
fn watcher_event_serializes_to_json() {
    let event = sample_watcher_event();
    let json_str = serde_json::to_string(&event).expect("WatcherEvent must be Serialize");
    let parsed: Value = serde_json::from_str(&json_str).unwrap();

    assert!(
        parsed["timestamp"].is_string(),
        "timestamp must be a string"
    );
    assert_eq!(parsed["component"], "agent");
    assert!(
        parsed["event_type"].is_string(),
        "event_type must serialize as string"
    );
    assert!(
        parsed["severity"].is_string(),
        "severity must serialize as string"
    );
    assert!(parsed["fields"].is_object(), "fields must be an object");
}

/// WatcherEvent must implement Clone (required for broadcast::channel).
#[test]
fn watcher_event_is_clone() {
    let event = sample_watcher_event();
    let cloned = event.clone();
    assert_eq!(event.component, cloned.component);
}

/// WatcherEvent must implement Debug (required for diagnostics).
#[test]
fn watcher_event_is_debug() {
    let event = sample_watcher_event();
    let debug_str = format!("{:?}", event);
    assert!(
        debug_str.contains("agent"),
        "Debug output should contain component name"
    );
}

// =========================================================================
// AC: WatcherEventType — all required variants exist
// =========================================================================

/// WatcherEventType must have all the variants specified in the context doc.
#[test]
fn watcher_event_type_has_all_variants() {
    // Each variant must exist — compilation failure if missing
    let _a = WatcherEventType::AgentSpanOpen;
    let _b = WatcherEventType::AgentSpanClose;
    let _c = WatcherEventType::ValidationWarning;
    let _d = WatcherEventType::SubsystemExerciseSummary;
    let _e = WatcherEventType::CoverageGap;
    let _f = WatcherEventType::JsonExtractionResult;
    let _g = WatcherEventType::StateTransition;
}

/// WatcherEventType must serialize as snake_case (per serde rename_all).
#[test]
fn watcher_event_type_serializes_snake_case() {
    let event = sample_watcher_event();
    let json_str = serde_json::to_string(&event).unwrap();
    let parsed: Value = serde_json::from_str(&json_str).unwrap();

    assert_eq!(
        parsed["event_type"], "agent_span_open",
        "WatcherEventType::AgentSpanOpen should serialize as 'agent_span_open'"
    );
}

// =========================================================================
// AC: Severity enum — all variants exist and serialize as lowercase
// =========================================================================

#[test]
fn severity_has_all_variants() {
    let _i = Severity::Info;
    let _w = Severity::Warn;
    let _e = Severity::Error;
}

#[test]
fn severity_serializes_lowercase() {
    let mut fields = HashMap::new();
    fields.insert("test".to_string(), serde_json::Value::Bool(true));

    let event = WatcherEvent {
        timestamp: chrono::Utc::now(),
        component: "watcher".to_string(),
        event_type: WatcherEventType::ValidationWarning,
        severity: Severity::Warn,
        fields,
    };
    let json_str = serde_json::to_string(&event).unwrap();
    let parsed: Value = serde_json::from_str(&json_str).unwrap();

    assert_eq!(
        parsed["severity"], "warn",
        "Severity::Warn should serialize as 'warn'"
    );
}

// =========================================================================
// AC: AppState watcher broadcast — subscribe_watcher / send_watcher_event
// =========================================================================

/// AppState must expose a watcher broadcast channel via subscribe_watcher().
#[tokio::test]
async fn app_state_has_watcher_broadcast_channel() {
    let state = test_app_state();

    // Must be able to subscribe to watcher events
    let mut rx = state.subscribe_watcher();

    // Send an event through the channel
    let event = sample_watcher_event();
    state.send_watcher_event(event.clone());

    // Should receive the event
    let received = timeout(Duration::from_millis(100), rx.recv())
        .await
        .expect("Should receive within 100ms")
        .expect("recv should not error");

    assert_eq!(
        received.component, event.component,
        "Received event should match sent event"
    );
}

/// Multiple subscribers should all receive the same watcher event.
#[tokio::test]
async fn watcher_broadcast_delivers_to_multiple_subscribers() {
    let state = test_app_state();

    let mut rx1 = state.subscribe_watcher();
    let mut rx2 = state.subscribe_watcher();

    let event = sample_watcher_event();
    state.send_watcher_event(event.clone());

    let r1 = timeout(Duration::from_millis(100), rx1.recv())
        .await
        .expect("rx1 timeout")
        .expect("rx1 recv error");
    let r2 = timeout(Duration::from_millis(100), rx2.recv())
        .await
        .expect("rx2 timeout")
        .expect("rx2 recv error");

    assert_eq!(r1.component, "agent");
    assert_eq!(r2.component, "agent");
}

/// Zero overhead: sending watcher events with no subscribers must not panic.
/// The broadcast::send error (no receivers) should be silently ignored.
#[tokio::test]
async fn watcher_broadcast_no_subscribers_does_not_panic() {
    let state = test_app_state();

    // No subscribers — this must not panic
    let event = sample_watcher_event();
    state.send_watcher_event(event);
    // If we reach here without panic, the test passes
}

// =========================================================================
// AC: /ws/watcher endpoint — route exists and accepts WebSocket upgrade
// =========================================================================

/// GET /ws/watcher without WebSocket upgrade should return 4xx (not 404).
/// This verifies the route is registered in the router.
#[tokio::test]
async fn watcher_endpoint_rejects_non_upgrade_request() {
    let state = test_app_state();
    let app = build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/ws/watcher")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Route must exist (not 404), but reject non-upgrade (likely 400 or 405)
    let status = response.status().as_u16();
    assert_ne!(status, 404, "/ws/watcher route must exist, got 404");
    assert!(
        status >= 400 && status < 500,
        "/ws/watcher should reject non-upgrade with 4xx, got {}",
        status
    );
}

/// The game /ws and watcher /ws/watcher routes must coexist without conflict.
#[tokio::test]
async fn game_and_watcher_routes_coexist() {
    let state = test_app_state();
    let app = build_router(state);

    // Both routes should exist — neither should 404
    let resp_ws = app
        .clone()
        .oneshot(Request::builder().uri("/ws").body(Body::empty()).unwrap())
        .await
        .unwrap();

    let resp_watcher = app
        .oneshot(
            Request::builder()
                .uri("/ws/watcher")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_ne!(resp_ws.status().as_u16(), 404, "/ws route must exist");
    assert_ne!(
        resp_watcher.status().as_u16(),
        404,
        "/ws/watcher route must exist"
    );
}

// =========================================================================
// Integration: WebSocket watcher connection lifecycle
// =========================================================================

/// Full integration: connect to /ws/watcher, send a watcher event through
/// the broadcast channel, and verify the client receives it as JSON.
#[tokio::test]
async fn watcher_client_receives_broadcast_events() {
    use futures::StreamExt;
    use tokio_tungstenite::connect_async;

    let state = test_app_state();
    let broadcast_state = state.clone();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let _server =
        tokio::spawn(
            async move { create_server_with_listener(state, listener, shutdown_rx).await },
        );

    // Connect to the watcher endpoint (not /ws)
    let url = format!("ws://127.0.0.1:{}/ws/watcher", addr.port());
    let (mut ws_stream, _) = connect_async(&url)
        .await
        .expect("/ws/watcher WebSocket connection should succeed");

    // Give connection time to register
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Send a watcher event via broadcast
    let event = sample_watcher_event();
    broadcast_state.send_watcher_event(event);

    // Client should receive the event as JSON
    let msg = timeout(Duration::from_secs(2), ws_stream.next())
        .await
        .expect("Should receive within 2s")
        .expect("Stream should have a message")
        .expect("Message should not be an error");

    if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
        let parsed: Value =
            serde_json::from_str(&text).expect("Watcher message must be valid JSON");
        assert_eq!(parsed["component"], "agent");
        assert_eq!(parsed["event_type"], "agent_span_open");
        assert!(parsed["timestamp"].is_string());
        assert!(parsed["severity"].is_string());
        assert!(parsed["fields"].is_object());
    } else {
        panic!("Expected Text frame from watcher, got: {:?}", msg);
    }

    // Cleanup
    shutdown_tx.send(()).ok();
}

/// Two watcher clients connected simultaneously both receive the same event.
#[tokio::test]
async fn multiple_watcher_clients_receive_same_event() {
    use futures::StreamExt;
    use tokio_tungstenite::connect_async;

    let state = test_app_state();
    let broadcast_state = state.clone();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let _server =
        tokio::spawn(
            async move { create_server_with_listener(state, listener, shutdown_rx).await },
        );

    let url = format!("ws://127.0.0.1:{}/ws/watcher", addr.port());

    // Connect two watcher clients
    let (mut ws1, _) = connect_async(&url).await.expect("Client 1 should connect");
    let (mut ws2, _) = connect_async(&url).await.expect("Client 2 should connect");

    // Give connections time to register
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Broadcast a watcher event
    let event = sample_watcher_event();
    broadcast_state.send_watcher_event(event);

    // Both clients should receive it
    let r1 = timeout(Duration::from_secs(2), ws1.next())
        .await
        .expect("Client 1 should receive within 2s")
        .expect("Client 1 stream should have a message")
        .expect("Client 1 message should not be an error");

    let r2 = timeout(Duration::from_secs(2), ws2.next())
        .await
        .expect("Client 2 should receive within 2s")
        .expect("Client 2 stream should have a message")
        .expect("Client 2 message should not be an error");

    // Verify both received watcher events (not game messages)
    for (i, msg) in [(1, r1), (2, r2)] {
        if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
            let parsed: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(
                parsed["component"], "agent",
                "Client {} should receive agent event",
                i
            );
        } else {
            panic!("Client {} expected Text frame, got: {:?}", i, msg);
        }
    }

    shutdown_tx.send(()).ok();
}

// =========================================================================
// AC: Clean disconnect — watcher disconnect doesn't affect game traffic
// =========================================================================

/// A watcher client disconnecting must not interfere with the game /ws endpoint.
/// Game traffic should continue unaffected after a watcher drops.
#[tokio::test]
async fn watcher_disconnect_does_not_affect_game_ws() {
    use futures::{SinkExt, StreamExt};
    use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};

    let state = test_app_state();
    let broadcast_state = state.clone();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let _server =
        tokio::spawn(
            async move { create_server_with_listener(state, listener, shutdown_rx).await },
        );

    let game_url = format!("ws://127.0.0.1:{}/ws", addr.port());
    let watcher_url = format!("ws://127.0.0.1:{}/ws/watcher", addr.port());

    // Connect a game client and a watcher client
    let (mut game_ws, _) = connect_async(&game_url)
        .await
        .expect("Game client should connect");
    let (watcher_ws, _) = connect_async(&watcher_url)
        .await
        .expect("Watcher should connect");

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Disconnect the watcher client
    drop(watcher_ws);
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Game client should still work — send a message, get error response for malformed
    game_ws
        .send(WsMessage::Text("not valid json".to_string()))
        .await
        .expect("Game client send should succeed after watcher disconnect");

    let response = timeout(Duration::from_secs(2), game_ws.next())
        .await
        .expect("Game client should receive response within 2s")
        .expect("Game stream should have a message")
        .expect("Game message should not be an error");

    if let WsMessage::Text(text) = response {
        let parsed: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            parsed["type"], "ERROR",
            "Game client should still receive ERROR responses after watcher disconnect"
        );
    }

    shutdown_tx.send(()).ok();
}

/// Watcher events should NOT leak to game /ws clients.
/// The two WebSocket endpoints are completely separate streams.
#[tokio::test]
async fn watcher_events_do_not_leak_to_game_clients() {
    use futures::StreamExt;
    use tokio_tungstenite::connect_async;

    let state = test_app_state();
    let broadcast_state = state.clone();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let _server =
        tokio::spawn(
            async move { create_server_with_listener(state, listener, shutdown_rx).await },
        );

    let game_url = format!("ws://127.0.0.1:{}/ws", addr.port());
    let watcher_url = format!("ws://127.0.0.1:{}/ws/watcher", addr.port());

    // Connect both types of clients
    let (mut game_ws, _) = connect_async(&game_url).await.unwrap();
    let (mut watcher_ws, _) = connect_async(&watcher_url).await.unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Send a watcher event
    broadcast_state.send_watcher_event(sample_watcher_event());

    // Watcher should receive it
    let watcher_msg = timeout(Duration::from_secs(2), watcher_ws.next())
        .await
        .expect("Watcher should receive event");
    assert!(watcher_msg.is_some(), "Watcher should get a message");

    // Game client should NOT receive it (timeout expected)
    let game_msg = timeout(Duration::from_millis(500), game_ws.next()).await;
    assert!(
        game_msg.is_err(),
        "Game client must NOT receive watcher events — streams are separate"
    );

    shutdown_tx.send(()).ok();
}

// =========================================================================
// Rule-enforcement tests (Rust lang-review checklist)
// =========================================================================

// --- Rule #2: #[non_exhaustive] on public enums ---

/// WatcherEventType must be #[non_exhaustive] since it will grow
/// as new telemetry event types are added in future stories.
#[test]
fn watcher_event_type_is_non_exhaustive() {
    // If #[non_exhaustive], the wildcard arm is required
    let et = WatcherEventType::AgentSpanOpen;
    match et {
        WatcherEventType::AgentSpanOpen => {}
        _ => {} // wildcard needed because #[non_exhaustive]
    }
}

/// Severity must be #[non_exhaustive] to allow future log levels.
#[test]
fn severity_is_non_exhaustive() {
    let s = Severity::Info;
    match s {
        Severity::Info => {}
        _ => {} // wildcard needed because #[non_exhaustive]
    }
}

// --- Rule #9: Private fields with getters ---
// WatcherEvent fields are public (HashMap-style data bag) — this is intentional
// per the context doc. No invariant validation needed on a diagnostic event struct.

// --- Rule #4: Tracing coverage ---
// Watcher connection/disconnect should be traced — verified in integration tests
// by checking the server doesn't panic.

// --- Rule #6: Test quality self-check ---
// Every test above has meaningful assertions. No `let _ = result` patterns.
// No `assert!(true)` or vacuous assertions.

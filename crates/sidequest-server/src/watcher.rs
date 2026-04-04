//! Watcher WebSocket handler (Story 3-6).
//!
//! Read-only telemetry stream for GM panel / BikeRack GUI.

use axum::extract::ws::{Message as AxumWsMessage, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use futures::{SinkExt, StreamExt};

use crate::AppState;

/// GET /ws/watcher — WebSocket upgrade for telemetry viewers.
pub(crate) async fn ws_watcher_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    tracing::info!("Watcher WebSocket connection upgrading");
    ws.on_upgrade(move |socket| handle_watcher_connection(socket, state))
}

async fn handle_watcher_connection(socket: WebSocket, state: AppState) {
    tracing::info!("Watcher WebSocket connected");

    let (mut ws_sink, mut ws_stream) = socket.split();
    let mut watcher_rx = state.subscribe_watcher();

    // Send a handshake event so the client knows the connection is live
    // and to confirm the broadcast channel is wired correctly.
    {
        let handshake = crate::WatcherEvent {
            timestamp: chrono::Utc::now(),
            component: "watcher".to_string(),
            event_type: crate::WatcherEventType::AgentSpanOpen,
            severity: crate::Severity::Info,
            fields: {
                let mut m = std::collections::HashMap::new();
                m.insert("event".to_string(), serde_json::json!("watcher_connected"));
                m
            },
        };
        let json = serde_json::to_string(&handshake).unwrap_or_default();
        if ws_sink.send(AxumWsMessage::Text(json.into())).await.is_err() {
            tracing::warn!("Watcher WebSocket closed before handshake sent");
            return;
        }
        tracing::info!("watcher.handshake_sent — connection confirmed live");
    }

    // Writer task: forward watcher broadcast events to this WebSocket client
    let writer_handle = tokio::spawn(async move {
        let mut event_count: u64 = 0;
        while let Ok(event) = watcher_rx.recv().await {
            event_count += 1;
            let json = match serde_json::to_string(&event) {
                Ok(j) => j,
                Err(e) => {
                    tracing::error!(error = %e, "Failed to serialize watcher event");
                    continue;
                }
            };
            if event_count <= 3 || event_count % 50 == 0 {
                tracing::info!(
                    event_count,
                    component = %event.component,
                    event_type = ?event.event_type,
                    "watcher.event_forwarded"
                );
            }
            if ws_sink
                .send(AxumWsMessage::Text(json.into()))
                .await
                .is_err()
            {
                tracing::info!(event_count, "watcher.writer_closed — WebSocket send failed");
                break;
            }
        }
        tracing::info!(event_count, "watcher.writer_exited — broadcast channel closed or lagged");
    });

    // Reader loop: watchers are read-only, but we need to detect disconnect
    while let Some(msg) = ws_stream.next().await {
        match msg {
            Ok(AxumWsMessage::Close(_)) => break,
            Err(_) => break,
            _ => {} // ignore any messages from watcher clients
        }
    }

    writer_handle.abort();
    tracing::info!("Watcher WebSocket disconnected");
}

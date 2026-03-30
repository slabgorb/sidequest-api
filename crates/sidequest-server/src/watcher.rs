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

    // Writer task: forward watcher broadcast events to this WebSocket client
    let writer_handle = tokio::spawn(async move {
        while let Ok(event) = watcher_rx.recv().await {
            let json = match serde_json::to_string(&event) {
                Ok(j) => j,
                Err(e) => {
                    tracing::error!(error = %e, "Failed to serialize watcher event");
                    continue;
                }
            };
            if ws_sink
                .send(AxumWsMessage::Text(json.into()))
                .await
                .is_err()
            {
                break;
            }
        }
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

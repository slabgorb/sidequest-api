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
        if ws_sink.send(AxumWsMessage::Text(json)).await.is_err() {
            tracing::warn!("Watcher WebSocket closed before handshake sent");
            return;
        }
        tracing::info!("watcher.handshake_sent — connection confirmed live");
    }

    // Bug #6: Send initial state replay for all active sessions so a late-connecting
    // GM panel sees current game state instead of "Waiting for first turn..."
    {
        let sessions = state.inner.sessions.lock().unwrap().clone();
        for (key, ss_arc) in &sessions {
            let ss = ss_arc.lock().await;
            let player_data: Vec<serde_json::Value> = ss
                .players
                .iter()
                .map(|(pid, ps)| {
                    serde_json::json!({
                        "player_id": pid,
                        "character_name": ps.character_name,
                    })
                })
                .collect();
            let npc_data: Vec<serde_json::Value> = ss
                .npc_registry
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "name": e.name,
                        "role": e.role,
                        "location": e.location,
                    })
                })
                .collect();
            let active_tropes: Vec<serde_json::Value> = ss
                .trope_states
                .iter()
                .map(|ts| {
                    serde_json::json!({
                        "id": ts.trope_definition_id(),
                        "progression": ts.progression(),
                        "status": format!("{:?}", ts.status()),
                    })
                })
                .collect();
            let snapshot = serde_json::json!({
                "session_key": key,
                "genre": ss.genre_slug,
                "world": ss.world_slug,
                "location": ss.current_location,
                "player_count": ss.player_count(),
                "players": player_data,
                "npc_registry": npc_data,
                "active_tropes": active_tropes,
                "narration_history_len": ss.narration_history.len(),
                "turn_mode": format!("{:?}", ss.turn_mode),
            });
            let event = crate::WatcherEvent {
                timestamp: chrono::Utc::now(),
                component: "game".to_string(),
                event_type: crate::WatcherEventType::GameStateSnapshot,
                severity: crate::Severity::Info,
                fields: {
                    let mut m = std::collections::HashMap::new();
                    m.insert(
                        "event".to_string(),
                        serde_json::json!("initial_state_replay"),
                    );
                    m.insert("snapshot".to_string(), snapshot);
                    m
                },
            };
            let json = serde_json::to_string(&event).unwrap_or_default();
            if ws_sink.send(AxumWsMessage::Text(json)).await.is_err() {
                tracing::warn!("Watcher WebSocket closed during initial state replay");
                return;
            }
        }
        if !sessions.is_empty() {
            tracing::info!(
                session_count = sessions.len(),
                "watcher.initial_state_replay — sent snapshots for active sessions"
            );
        }
    }

    // Replay stored watcher event history so late-connecting GM panels see past turns.
    // This is the fix for OTEL dashboard showing 0 turns after an active session.
    {
        let history = state.get_watcher_history();
        if !history.is_empty() {
            tracing::info!(
                event_count = history.len(),
                "watcher.history_replay — sending stored events to late-connecting client"
            );
            for event in &history {
                let json = serde_json::to_string(event).unwrap_or_default();
                if ws_sink.send(AxumWsMessage::Text(json)).await.is_err() {
                    tracing::warn!("Watcher WebSocket closed during history replay");
                    return;
                }
            }
        }
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
            if ws_sink.send(AxumWsMessage::Text(json)).await.is_err() {
                tracing::info!(event_count, "watcher.writer_closed — WebSocket send failed");
                break;
            }
        }
        tracing::info!(
            event_count,
            "watcher.writer_exited — broadcast channel closed or lagged"
        );
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

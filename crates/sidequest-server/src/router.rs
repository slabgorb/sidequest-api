//! Axum router construction with all routes and middleware.

use std::collections::HashMap;

use axum::extract::ws::{Message as AxumWsMessage, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use futures::{SinkExt, StreamExt};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::services::ServeDir;

use sidequest_protocol::GameMessage;

use crate::ws::handle_ws_connection;
use crate::{AppState, PlayerId};

/// Build the axum Router with all routes and middleware.
///
/// Routes:
/// - `GET /api/genres` — list available genres and their worlds
/// - `GET /ws` — WebSocket upgrade for game sessions
/// - `GET /ws/watcher` — watcher telemetry stream
///
/// Middleware:
/// - CORS for React dev server at localhost:5173
pub fn build_router(state: AppState) -> Router {
    // Spawn image broadcaster — listens for render completions and broadcasts IMAGE messages
    if let Some(queue) = state.render_queue() {
        let mut render_rx = queue.subscribe();
        let broadcast_tx = state.broadcast_tx();
        tokio::spawn(async move {
            while let Ok(result) = render_rx.recv().await {
                if let sidequest_game::RenderJobResult::Success {
                    job_id,
                    image_url,
                    generation_ms,
                    tier,
                    scene_type,
                } = result
                {
                    if image_url.trim().is_empty() {
                        tracing::error!(job_id = %job_id, "render_broadcast_blocked — empty image_url");
                        continue;
                    }
                    let served_url = {
                        let img_path = std::path::Path::new(&image_url);
                        let renders_base = std::env::var("SIDEQUEST_OUTPUT_DIR")
                            .map(std::path::PathBuf::from)
                            .unwrap_or_else(|_| {
                                std::path::PathBuf::from(
                                    std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string()),
                                )
                                .join(".sidequest")
                                .join("renders")
                            });
                        if let Ok(rel) = img_path.strip_prefix(&renders_base) {
                            format!("/api/renders/{}", rel.display())
                        } else if let Some(filename) = img_path.file_name().and_then(|f| f.to_str())
                        {
                            format!("/api/renders/{}", filename)
                        } else {
                            image_url
                        }
                    };
                    tracing::info!(
                        job_id = %job_id, url = %served_url,
                        tier = %tier, scene_type = %scene_type,
                        "render_broadcast — sending IMAGE"
                    );
                    let msg = GameMessage::Image {
                        payload: sidequest_protocol::ImagePayload {
                            url: served_url,
                            description: String::new(),
                            handout: false,
                            render_id: Some(job_id.to_string()),
                            tier: if tier.is_empty() { None } else { Some(tier) },
                            scene_type: if scene_type.is_empty() {
                                None
                            } else {
                                Some(scene_type)
                            },
                            generation_ms: Some(generation_ms),
                        },
                        player_id: String::new(),
                    };
                    let _ = broadcast_tx.send(msg);
                }
            }
        });
    }

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(["http://localhost:5173"
            .parse()
            .unwrap()]))
        .allow_methods([axum::http::Method::GET])
        .allow_headers(tower_http::cors::Any);

    let genre_assets = ServeDir::new(state.genre_packs_path());

    let renders_dir = std::env::var("SIDEQUEST_OUTPUT_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            std::path::PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string()))
                .join(".sidequest")
                .join("renders")
        });
    let renders_assets = ServeDir::new(&renders_dir);

    Router::new()
        .route("/api/genres", get(list_genres))
        .route("/ws", get(ws_handler))
        .route("/ws/watcher", get(ws_watcher_handler))
        .nest_service("/genre", genre_assets)
        .nest_service("/api/renders", renders_assets)
        .layer(cors)
        .with_state(state)
}

/// GET /api/genres — scan genre_packs_path for genre directories with worlds.
#[tracing::instrument(skip(state))]
async fn list_genres(State(state): State<AppState>) -> Json<HashMap<String, serde_json::Value>> {
    let mut genres: HashMap<String, serde_json::Value> = HashMap::new();

    let packs_path = state.genre_packs_path();
    if !packs_path.exists() {
        tracing::warn!(path = %packs_path.display(), "Genre packs path does not exist");
        return Json(genres);
    }

    let entries = match std::fs::read_dir(packs_path) {
        Ok(entries) => entries,
        Err(e) => {
            tracing::error!(error = %e, "Failed to read genre packs directory");
            return Json(genres);
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let genre_slug = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name.to_string(),
            None => continue,
        };

        if !path.join("pack.yaml").exists() {
            continue;
        }

        let mut worlds = Vec::new();
        let worlds_dir = path.join("worlds");
        if worlds_dir.exists() {
            if let Ok(world_entries) = std::fs::read_dir(&worlds_dir) {
                for world_entry in world_entries.flatten() {
                    if world_entry.path().is_dir() {
                        if let Some(name) = world_entry.file_name().to_str() {
                            worlds.push(name.to_string());
                        }
                    }
                }
            }
        }

        genres.insert(genre_slug, serde_json::json!({ "worlds": worlds }));
    }

    Json(genres)
}

/// GET /ws — WebSocket upgrade handler.
async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    let player_id = PlayerId::new();
    tracing::info!(player_id = %player_id, "WebSocket connection upgrading");
    ws.on_upgrade(move |socket| handle_ws_connection(socket, state, player_id))
}

/// GET /ws/watcher — WebSocket upgrade for telemetry viewers.
async fn ws_watcher_handler(
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

    while let Some(msg) = ws_stream.next().await {
        match msg {
            Ok(AxumWsMessage::Close(_)) => break,
            Err(_) => break,
            _ => {}
        }
    }

    writer_handle.abort();
    tracing::info!("Watcher WebSocket disconnected");
}

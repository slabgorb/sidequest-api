//! WebSocket connection handler — reader/writer split, per-connection state, cleanup.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::ws::{Message as AxumWsMessage, WebSocket};
use futures::{SinkExt, StreamExt};
use tokio::sync::{broadcast, mpsc};

use sidequest_protocol::{GameMessage, SessionEventPayload};

use crate::helpers::npc::NpcRegistryEntry;
use crate::shared_session;
use crate::telemetry::{Severity, WatcherEvent, WatcherEventType};
use crate::types::{error_response, PlayerId};
use crate::{dispatch_message, AppState};

pub(crate) async fn handle_ws_connection(socket: WebSocket, state: AppState, player_id: PlayerId) {
    tracing::info!(player_id = %player_id, "WebSocket connected");

    let (mut ws_sink, mut ws_stream) = socket.split();

    let (tx, mut rx) = mpsc::channel::<GameMessage>(32);
    state.add_connection(player_id.clone(), tx.clone());

    let mut broadcast_rx = state.subscribe_broadcast();
    let mut binary_rx = state.subscribe_binary();

    let player_id_str = player_id.to_string();

    let shared_session: Arc<
        tokio::sync::Mutex<Option<Arc<tokio::sync::Mutex<shared_session::SharedGameSession>>>>,
    > = Arc::new(tokio::sync::Mutex::new(None));
    let shared_session_for_writer = shared_session.clone();

    let writer_player_id = player_id_str.clone();
    let writer_handle = tokio::spawn(async move {
        let mut session_rx: Option<broadcast::Receiver<shared_session::TargetedMessage>> = None;

        loop {
            if session_rx.is_none() {
                let guard = shared_session_for_writer.lock().await;
                if let Some(ref ss) = *guard {
                    let ss_lock = ss.lock().await;
                    session_rx = Some(ss_lock.subscribe());
                    tracing::info!(player_id = %writer_player_id, "session_rx.subscribed — writer now receives session broadcasts");
                }
            }

            tokio::select! {
                Some(msg) = rx.recv() => {
                    let json = match serde_json::to_string(&msg) {
                        Ok(j) => j,
                        Err(e) => {
                            tracing::error!(player_id = %writer_player_id, error = %e, "Failed to serialize message");
                            continue;
                        }
                    };
                    if ws_sink.send(AxumWsMessage::Text(json.into())).await.is_err() {
                        break;
                    }
                }
                Ok(msg) = broadcast_rx.recv() => {
                    let json = match serde_json::to_string(&msg) {
                        Ok(j) => j,
                        Err(e) => {
                            tracing::error!(player_id = %writer_player_id, error = %e, "Failed to serialize broadcast message");
                            continue;
                        }
                    };
                    if ws_sink.send(AxumWsMessage::Text(json.into())).await.is_err() {
                        break;
                    }
                }
                result = async { match session_rx.as_mut() { Some(rx) => rx.recv().await, None => std::future::pending().await } } => {
                    if let Ok(targeted) = result {
                        if let Some(ref target) = targeted.target_player_id {
                            if target != &writer_player_id {
                                continue;
                            }
                        }
                        let msg = targeted.msg;
                        if targeted.target_player_id.is_none() {
                            let msg_player_id = match &msg {
                                GameMessage::Narration { player_id, .. }
                                | GameMessage::NarrationEnd { player_id, .. }
                                | GameMessage::ChapterMarker { player_id, .. }
                                | GameMessage::SessionEvent { player_id, .. } => Some(player_id.as_str()),
                                _ => None,
                            };
                            if msg_player_id == Some(writer_player_id.as_str()) {
                                tracing::debug!(player_id = %writer_player_id, msg_type = ?std::mem::discriminant(&msg), "session_rx.self_skip — untargeted broadcast from self");
                                continue;
                            }
                        }
                        tracing::debug!(
                            player_id = %writer_player_id,
                            targeted = targeted.target_player_id.is_some(),
                            msg_type = ?std::mem::discriminant(&msg),
                            "session_rx.delivering message to writer"
                        );
                        let json = match serde_json::to_string(&msg) {
                            Ok(j) => j,
                            Err(e) => {
                                tracing::error!(player_id = %writer_player_id, error = %e, "Failed to serialize session message");
                                continue;
                            }
                        };
                        if ws_sink.send(AxumWsMessage::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                }
                result = binary_rx.recv() => {
                    match result {
                        Ok(bytes) => {
                            if ws_sink.send(AxumWsMessage::Binary(bytes.into())).await.is_err() {
                                break;
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(player_id = %writer_player_id, skipped = n, "Binary broadcast lagged");
                        }
                        Err(_) => break,
                    }
                }
                else => break,
            }
        }
    });

    // Per-connection state
    let mut session = crate::Session::new();
    let mut builder: Option<sidequest_game::builder::CharacterBuilder> = None;
    let mut player_name_for_session: Option<String> = None;
    let mut character_json: Option<serde_json::Value> = None;
    let mut character_name: Option<String> = None;
    let mut character_hp: i32 = 10;
    let mut character_max_hp: i32 = 10;
    let mut character_level: u32 = 1;
    let mut character_xp: u32 = 0;
    let mut current_location: String = String::new();
    let mut inventory = sidequest_game::Inventory::default();
    let mut combat_state = sidequest_game::combat::CombatState::default();
    let mut chase_state: Option<sidequest_game::ChaseState> = None;
    let mut trope_states: Vec<sidequest_game::trope::TropeState> = vec![];
    let mut trope_defs: Vec<sidequest_genre::TropeDefinition> = vec![];
    let mut quest_log: HashMap<String, String> = HashMap::new();
    let mut world_context: String = String::new();
    let mut axes_config: Option<sidequest_genre::AxesConfig> = None;
    let mut axis_values: Vec<sidequest_game::axis::AxisValue> = vec![];
    let mut visual_style: Option<sidequest_genre::VisualStyle> = None;
    let mut music_director: Option<sidequest_game::MusicDirector> = None;
    let mut npc_registry: Vec<NpcRegistryEntry> = vec![];
    let mut discovered_regions: Vec<String> = vec![];
    let mut turn_manager = sidequest_game::TurnManager::new();
    let mut lore_store = sidequest_game::LoreStore::new();
    let mut narration_history: Vec<String> = vec![];
    let mut continuity_corrections = String::new();
    let audio_mixer: Arc<tokio::sync::Mutex<Option<sidequest_game::AudioMixer>>> =
        Arc::new(tokio::sync::Mutex::new(None));
    let prerender_scheduler: Arc<
        tokio::sync::Mutex<Option<sidequest_game::PrerenderScheduler>>,
    > = Arc::new(tokio::sync::Mutex::new(None));

    // Reader loop
    while let Some(msg) = ws_stream.next().await {
        match msg {
            Ok(AxumWsMessage::Text(text)) => match serde_json::from_str::<GameMessage>(&text) {
                Ok(game_msg) => {
                    let responses = dispatch_message(
                        game_msg,
                        &mut session,
                        &mut builder,
                        &mut player_name_for_session,
                        &mut character_json,
                        &mut character_name,
                        &mut character_hp,
                        &mut character_max_hp,
                        &mut character_level,
                        &mut character_xp,
                        &mut current_location,
                        &mut inventory,
                        &mut combat_state,
                        &mut chase_state,
                        &mut trope_states,
                        &mut trope_defs,
                        &mut world_context,
                        &mut axes_config,
                        &mut axis_values,
                        &mut visual_style,
                        &mut music_director,
                        &audio_mixer,
                        &prerender_scheduler,
                        &mut npc_registry,
                        &mut quest_log,
                        &mut narration_history,
                        &mut discovered_regions,
                        &mut turn_manager,
                        &mut lore_store,
                        &shared_session,
                        &state,
                        &player_id_str,
                        &mut continuity_corrections,
                    )
                    .await;
                    for resp in responses {
                        let _ = tx.send(resp).await;
                    }
                }
                Err(e) => {
                    tracing::warn!(player_id = %player_id_str, error = %e, "Invalid message");
                    let err_msg = error_response(&player_id_str, &format!("Invalid JSON: {}", e));
                    let _ = tx.send(err_msg).await;
                }
            },
            Ok(AxumWsMessage::Close(_)) => break,
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(player_id = %player_id_str, error = %e, "WebSocket error");
                break;
            }
        }
    }

    // Cleanup
    if let (Some(genre), Some(world)) = (session.genre_slug(), session.world_slug()) {
        let key = shared_session::game_session_key(genre, world);
        {
            let sessions = state.sessions_lock();
            if let Some(ss_arc) = sessions.get(&key).cloned() {
                drop(sessions);
                if let Ok(mut ss) = ss_arc.try_lock() {
                    let leave_msg = GameMessage::SessionEvent {
                        payload: SessionEventPayload {
                            event: "player_left".to_string(),
                            player_name: player_name_for_session.clone(),
                            genre: None,
                            world: None,
                            has_character: None,
                            initial_state: None,
                            css: None,
                        },
                        player_id: player_id_str.clone(),
                    };
                    ss.broadcast(leave_msg);

                    let remaining_count = ss.player_count().saturating_sub(1);
                    let old_mode = std::mem::take(&mut ss.turn_mode);
                    ss.turn_mode = old_mode.apply(
                        sidequest_game::turn_mode::TurnModeTransition::PlayerLeft {
                            player_count: remaining_count,
                        },
                    );
                    tracing::info!(
                        new_mode = ?ss.turn_mode,
                        remaining_players = remaining_count,
                        "Turn mode transitioned on player leave"
                    );
                    if let Some(ref barrier) = ss.turn_barrier {
                        let _ = barrier.remove_player(&player_id_str);
                    }
                    if !ss.turn_mode.should_use_barrier() {
                        ss.turn_barrier = None;
                    }
                }
            }
        }
        let remaining = state.remove_player_from_session(genre, world, &player_id_str);
        tracing::info!(
            player_id = %player_id_str,
            remaining_players = remaining,
            "Player removed from shared session"
        );
        state.send_watcher_event(WatcherEvent {
            timestamp: chrono::Utc::now(),
            component: "multiplayer".to_string(),
            event_type: WatcherEventType::StateTransition,
            severity: Severity::Info,
            fields: {
                let mut f = HashMap::new();
                f.insert("event".to_string(), serde_json::json!("session_left"));
                f.insert("session_key".to_string(), serde_json::json!(key));
                f.insert(
                    "remaining_players".to_string(),
                    serde_json::json!(remaining),
                );
                f
            },
        });
    }
    state.remove_connection(&player_id);
    writer_handle.abort();
    tracing::info!(player_id = %player_id_str, "WebSocket disconnected");
}

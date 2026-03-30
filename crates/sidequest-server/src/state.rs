//! Shared application state (AppState + AppStateInner).

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tokio::sync::{broadcast, mpsc};

use sidequest_agents::orchestrator::GameService;
use sidequest_protocol::GameMessage;

use crate::shared_session;
use crate::telemetry::WatcherEvent;
use crate::types::PlayerId;

/// Shared application state, injected into axum handlers via `State<AppState>`.
///
/// Must be `Clone + Send + Sync` for axum. The inner data lives behind `Arc`.
#[derive(Clone, Debug)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

struct AppStateInner {
    game_service: Box<dyn GameService>,
    genre_packs_path: PathBuf,
    connections: Mutex<HashMap<PlayerId, mpsc::Sender<GameMessage>>>,
    processing: Mutex<HashSet<PlayerId>>,
    broadcast_tx: broadcast::Sender<GameMessage>,
    watcher_tx: broadcast::Sender<WatcherEvent>,
    persistence: sidequest_game::PersistenceHandle,
    render_queue: Option<sidequest_game::RenderQueue>,
    subject_extractor: sidequest_game::SubjectExtractor,
    beat_filter: tokio::sync::Mutex<sidequest_game::BeatFilter>,
    binary_broadcast_tx: broadcast::Sender<Vec<u8>>,
    /// Shared multiplayer sessions keyed by "genre:world".
    sessions: Mutex<HashMap<String, Arc<tokio::sync::Mutex<shared_session::SharedGameSession>>>>,
    /// When true, skip TTS synthesis entirely (text narration still sent).
    tts_disabled: bool,
}

impl fmt::Debug for AppStateInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppStateInner")
            .field("genre_packs_path", &self.genre_packs_path)
            .finish_non_exhaustive()
    }
}

impl AppState {
    /// Create AppState with a GameService and a save directory for persistence.
    pub fn new_with_game_service(
        game_service: Box<dyn GameService>,
        genre_packs_path: PathBuf,
        save_dir: PathBuf,
    ) -> Self {
        let (broadcast_tx, _) = broadcast::channel(256);
        let (watcher_tx, _) = broadcast::channel(256);
        let (binary_broadcast_tx, _) = broadcast::channel(64);

        // Render pipeline — daemon client connects lazily on first render
        let render_queue = sidequest_game::RenderQueue::spawn(
            sidequest_game::RenderQueueConfig::default(),
            |prompt, art_style, tier, negative_prompt: String| async move {
                tracing::info!(
                    prompt_len = prompt.len(),
                    prompt_preview = %&prompt[..prompt.len().min(120)],
                    art_style = %art_style,
                    tier = %tier,
                    "render_pipeline_start — connecting to daemon"
                );
                let config = sidequest_daemon_client::DaemonConfig::default();
                match sidequest_daemon_client::DaemonClient::connect(config).await {
                    Ok(mut client) => {
                        tracing::info!(tier = %tier, "render_daemon_connected");
                        let positive_prompt = if art_style.is_empty() {
                            prompt.clone()
                        } else {
                            format!("{}, {}", prompt, art_style)
                        };
                        match client
                            .render(sidequest_daemon_client::RenderParams {
                                prompt: prompt.clone(),
                                art_style: art_style.clone(),
                                tier: tier.clone(),
                                positive_prompt,
                                negative_prompt: negative_prompt.clone(),
                            })
                            .await
                        {
                            Ok(result) => {
                                let raw_path = &result.image_url;
                                let servable_url = if raw_path.starts_with('/')
                                    || raw_path.starts_with("C:\\")
                                {
                                    let src = std::path::Path::new(raw_path);
                                    if let Some(filename) = src.file_name() {
                                        let renders_dir = std::env::var("SIDEQUEST_OUTPUT_DIR")
                                            .map(std::path::PathBuf::from)
                                            .unwrap_or_else(|_| {
                                                std::path::PathBuf::from(
                                                    std::env::var("HOME")
                                                        .unwrap_or_else(|_| "/tmp".to_string()),
                                                )
                                                .join(".sidequest")
                                                .join("renders")
                                            });
                                        let _ = std::fs::create_dir_all(&renders_dir);
                                        let dest = renders_dir.join(filename);
                                        if src.exists() {
                                            if let Err(e) = std::fs::copy(src, &dest) {
                                                tracing::error!(error = %e, src = %raw_path, "render_file_copy_failed — image won't be servable");
                                            }
                                        } else {
                                            tracing::error!(src = %raw_path, "render_file_missing — daemon returned path that doesn't exist on disk");
                                        }
                                        format!("/api/renders/{}", filename.to_string_lossy())
                                    } else {
                                        raw_path.clone()
                                    }
                                } else if raw_path.starts_with("http://")
                                    || raw_path.starts_with("https://")
                                    || raw_path.starts_with("/api/")
                                {
                                    raw_path.clone()
                                } else {
                                    format!("/api/renders/{}", raw_path)
                                };
                                tracing::info!(
                                    raw_path = %raw_path,
                                    servable_url = %servable_url,
                                    generation_ms = result.generation_ms,
                                    tier = %tier,
                                    "render_pipeline_complete"
                                );
                                Ok((servable_url, result.generation_ms))
                            }
                            Err(e) => {
                                tracing::error!(
                                    error = %e,
                                    prompt_preview = %&prompt[..prompt.len().min(80)],
                                    tier = %tier,
                                    "render_pipeline_failed — daemon returned error or deserialization failed"
                                );
                                Err(format!("render failed: {e}"))
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!(error = %e, tier = %tier, "render_daemon_connect_failed — is the renderer running?");
                        Err(format!("daemon unavailable: {e}"))
                    }
                }
            },
        );

        let persistence = sidequest_game::PersistenceWorker::spawn(save_dir);

        Self {
            inner: Arc::new(AppStateInner {
                game_service,
                genre_packs_path,
                connections: Mutex::new(HashMap::new()),
                processing: Mutex::new(HashSet::new()),
                broadcast_tx,
                watcher_tx,
                persistence,
                render_queue: Some(render_queue),
                subject_extractor: sidequest_game::SubjectExtractor::new(),
                beat_filter: tokio::sync::Mutex::new(sidequest_game::BeatFilter::new(
                    sidequest_game::BeatFilterConfig::default(),
                )),
                binary_broadcast_tx,
                sessions: Mutex::new(HashMap::new()),
                tts_disabled: false,
            }),
        }
    }

    /// Disable TTS voice synthesis (builder-style).
    pub fn with_tts_disabled(mut self, disabled: bool) -> Self {
        Arc::get_mut(&mut self.inner)
            .expect("with_tts_disabled must be called before cloning")
            .tts_disabled = disabled;
        self
    }

    /// Whether TTS is disabled.
    pub fn tts_disabled(&self) -> bool {
        self.inner.tts_disabled
    }

    /// Get the persistence handle for save/load operations.
    pub fn persistence(&self) -> &sidequest_game::PersistenceHandle {
        &self.inner.persistence
    }

    /// Get the game service reference.
    pub(crate) fn game_service(&self) -> &dyn GameService {
        &*self.inner.game_service
    }

    /// Path to genre packs directory.
    pub fn genre_packs_path(&self) -> &Path {
        &self.inner.genre_packs_path
    }

    /// Number of active connections.
    pub fn connection_count(&self) -> usize {
        self.inner.connections.lock().unwrap().len()
    }

    /// Register a connection for a player.
    pub fn add_connection(&self, player_id: PlayerId, tx: mpsc::Sender<GameMessage>) {
        self.inner.connections.lock().unwrap().insert(player_id, tx);
    }

    /// Remove a connection by player id.
    pub fn remove_connection(&self, player_id: &PlayerId) {
        self.inner.connections.lock().unwrap().remove(player_id);
    }

    /// Get or create a shared multiplayer session for a genre:world pair.
    pub fn get_or_create_session(
        &self,
        genre: &str,
        world: &str,
    ) -> Arc<tokio::sync::Mutex<shared_session::SharedGameSession>> {
        let key = shared_session::game_session_key(genre, world);
        let mut sessions = self.inner.sessions.lock().unwrap();
        sessions
            .entry(key)
            .or_insert_with(|| {
                Arc::new(tokio::sync::Mutex::new(
                    shared_session::SharedGameSession::new(genre.to_string(), world.to_string()),
                ))
            })
            .clone()
    }

    /// Remove a player from a shared session. If the session is empty
    /// afterward, remove it from the registry entirely. Returns the
    /// remaining player count (0 means session was removed).
    pub fn remove_player_from_session(&self, genre: &str, world: &str, player_id: &str) -> usize {
        let key = shared_session::game_session_key(genre, world);
        let mut sessions = self.inner.sessions.lock().unwrap();
        let remaining = if let Some(session_arc) = sessions.get(&key).cloned() {
            if let Ok(mut session) = session_arc.try_lock() {
                session.players.remove(player_id);
                if let Some(ref barrier) = session.turn_barrier {
                    let _ = barrier.remove_player(player_id);
                }
                let remaining = session.players.len();
                let old_mode = std::mem::take(&mut session.turn_mode);
                session.turn_mode = old_mode.apply(
                    sidequest_game::turn_mode::TurnModeTransition::PlayerLeft {
                        player_count: remaining,
                    },
                );
                if !session.turn_mode.should_use_barrier() {
                    session.turn_barrier = None;
                }
                remaining
            } else {
                return 1;
            }
        } else {
            return 0;
        };
        if remaining == 0 {
            sessions.remove(&key);
        }
        remaining
    }

    /// Subscribe to the broadcast channel.
    pub fn subscribe_broadcast(&self) -> broadcast::Receiver<GameMessage> {
        self.inner.broadcast_tx.subscribe()
    }

    /// Send a message to all broadcast subscribers.
    pub fn broadcast(
        &self,
        msg: GameMessage,
    ) -> Result<usize, broadcast::error::SendError<GameMessage>> {
        self.inner.broadcast_tx.send(msg)
    }

    /// Subscribe to the watcher telemetry broadcast channel.
    pub fn subscribe_watcher(&self) -> broadcast::Receiver<WatcherEvent> {
        self.inner.watcher_tx.subscribe()
    }

    /// Send a telemetry event to all connected watcher clients.
    pub fn send_watcher_event(&self, event: WatcherEvent) {
        let _ = self.inner.watcher_tx.send(event);
    }

    /// Broadcast binary data to all connected WebSocket clients.
    pub(crate) fn broadcast_binary(&self, data: Vec<u8>) {
        let _ = self.inner.binary_broadcast_tx.send(data);
    }

    /// Subscribe to binary broadcast frames (e.g. TTS audio).
    pub(crate) fn subscribe_binary(&self) -> broadcast::Receiver<Vec<u8>> {
        self.inner.binary_broadcast_tx.subscribe()
    }

    /// Try to mark a player as processing. Returns false if already processing.
    pub(crate) fn try_start_processing(&self, player_id: &PlayerId) -> bool {
        self.inner
            .processing
            .lock()
            .unwrap()
            .insert(player_id.clone())
    }

    /// Remove a player from the processing set.
    pub(crate) fn stop_processing(&self, player_id: &PlayerId) {
        self.inner.processing.lock().unwrap().remove(player_id);
    }

    // -- pub(crate) accessors for fields used by other modules --

    /// Access the render queue (used by router and dispatch).
    pub(crate) fn render_queue(&self) -> Option<&sidequest_game::RenderQueue> {
        self.inner.render_queue.as_ref()
    }

    /// Clone the broadcast sender (used by router for image broadcaster).
    pub(crate) fn broadcast_tx(&self) -> broadcast::Sender<GameMessage> {
        self.inner.broadcast_tx.clone()
    }

    /// Access the subject extractor (used by dispatch).
    pub(crate) fn subject_extractor(&self) -> &sidequest_game::SubjectExtractor {
        &self.inner.subject_extractor
    }

    /// Access the beat filter (used by dispatch).
    pub(crate) fn beat_filter(&self) -> &tokio::sync::Mutex<sidequest_game::BeatFilter> {
        &self.inner.beat_filter
    }

    /// Access the sessions map (used by ws connection cleanup).
    pub(crate) fn sessions_lock(
        &self,
    ) -> std::sync::MutexGuard<'_, HashMap<String, Arc<tokio::sync::Mutex<shared_session::SharedGameSession>>>> {
        self.inner.sessions.lock().unwrap()
    }
}

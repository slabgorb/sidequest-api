//! SideQuest Server — axum HTTP/WebSocket server library.
//!
//! Exposes `build_router()`, `AppState`, and server lifecycle functions for the binary and tests.
//! The server depends on the `GameService` trait facade — never on game internals.

mod dispatch;
mod extraction;
mod npc_context;
pub mod render_integration;
pub mod shared_session;
pub mod tracing_setup;
mod watcher;

pub use tracing_setup::{build_subscriber_with_filter, init_tracing, tracing_subscriber_for_test};

use npc_context::build_name_bank_context;

/// NPC registry entry — re-exported from sidequest-game for persistence.
pub(crate) type NpcRegistryEntry = sidequest_game::NpcRegistryEntry;

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::extract::ws::{Message as AxumWsMessage, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use clap::Parser;
use futures::{SinkExt, StreamExt};
use tokio::sync::{broadcast, mpsc, oneshot};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::services::ServeDir;

// tracing_subscriber imports moved to tracing_setup module

use sidequest_agents::orchestrator::GameService;
use sidequest_game::builder::CharacterBuilder;
use sidequest_genre::{GenreCode, GenreLoader};
use sidequest_protocol::{
    ChapterMarkerPayload, CharacterCreationPayload, CharacterSheetPayload,
    CharacterState, ErrorPayload, GameMessage, InitialState,
    NarrationEndPayload, NarrationPayload, PartyMember, PartyStatusPayload,
    SessionEventPayload, TurnStatusPayload,
};

// ---------------------------------------------------------------------------
// Watcher Telemetry Types (Story 3-6)
// ---------------------------------------------------------------------------

/// A telemetry event streamed to `/ws/watcher` clients.
///
/// This is a diagnostic data bag — no invariants to enforce, so fields are public.
/// Serializes to JSON for the WebSocket stream.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WatcherEvent {
    /// When the event occurred.
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Which subsystem emitted this event (e.g. "agent", "validation", "game").
    pub component: String,
    /// The kind of telemetry event.
    pub event_type: WatcherEventType,
    /// Log severity.
    pub severity: Severity,
    /// Arbitrary key-value fields for event-specific data.
    pub fields: HashMap<String, serde_json::Value>,
}

/// Kinds of telemetry events streamed to watchers.
///
/// Will grow as new observability features land — hence `#[non_exhaustive]`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum WatcherEventType {
    /// An agent span has opened (work started).
    AgentSpanOpen,
    /// An agent span has closed (work finished).
    AgentSpanClose,
    /// A validation rule fired a warning.
    ValidationWarning,
    /// Summary of which subsystems were exercised in a turn.
    SubsystemExerciseSummary,
    /// A gap in expected coverage was detected.
    CoverageGap,
    /// Result of a JSON extraction from LLM output.
    JsonExtractionResult,
    /// A game state machine transition occurred.
    StateTransition,
    /// A complete turn has been processed (all subsystems ran).
    TurnComplete,
}

/// Severity levels for watcher telemetry events.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Severity {
    /// Informational.
    Info,
    /// Warning.
    Warn,
    /// Error.
    Error,
}

// Tracing functions moved to tracing_setup module.

// ---------------------------------------------------------------------------
// CLI Args
// ---------------------------------------------------------------------------

/// Command-line arguments for the SideQuest server.
#[derive(Parser, Debug)]
#[command(name = "sidequest-server")]
pub struct Args {
    /// Port to bind the server to.
    #[arg(long, default_value = "8765")]
    port: u16,

    /// Path to the genre packs directory.
    #[arg(long)]
    genre_packs_path: PathBuf,

    /// Directory for save files. Defaults to ~/.sidequest/saves.
    #[arg(long)]
    save_dir: Option<PathBuf>,

    /// Disable TTS voice synthesis (narration text is still sent, just no audio).
    #[arg(long, default_value = "false")]
    no_tts: bool,

    /// Write a Chrome trace file (trace-<pid>.json) for flame-chart visualization in Perfetto.
    #[arg(long, default_value = "false")]
    trace: bool,

    /// Headless playtest mode — no daemon, no TTS, no rendering.
    /// Game loop and narration run normally. OTEL spans still fire for media hooks.
    #[arg(long, default_value = "false")]
    headless: bool,
}

impl Args {
    /// The port to bind to.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Path to genre packs directory.
    pub fn genre_packs_path(&self) -> &Path {
        &self.genre_packs_path
    }

    /// Optional save directory.
    pub fn save_dir(&self) -> Option<&Path> {
        self.save_dir.as_deref()
    }

    /// Whether TTS is disabled.
    pub fn no_tts(&self) -> bool {
        self.no_tts
    }

    /// Whether chrome tracing is enabled.
    pub fn trace(&self) -> bool {
        self.trace || self.headless
    }

    /// Whether headless playtest mode is enabled.
    pub fn headless(&self) -> bool {
        self.headless
    }
}

// ---------------------------------------------------------------------------
// PlayerId
// ---------------------------------------------------------------------------

/// A player identifier backed by UUID v4.
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct PlayerId(uuid::Uuid);

impl PlayerId {
    /// Generate a new random PlayerId.
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl fmt::Display for PlayerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ---------------------------------------------------------------------------
// ServerError
// ---------------------------------------------------------------------------

/// Server-specific errors.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ServerError {
    /// The WebSocket connection was closed.
    #[error("connection closed")]
    ConnectionClosed,

    /// Failed to deserialize a message.
    #[error("deserialization error: {0}")]
    Deserialization(String),

    /// Broadcast send failed.
    #[error("broadcast error: {0}")]
    Broadcast(String),
}

impl ServerError {
    /// Create a ConnectionClosed error.
    pub fn connection_closed() -> Self {
        Self::ConnectionClosed
    }
}

// ---------------------------------------------------------------------------
// AppState
// ---------------------------------------------------------------------------

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
        Self::new_with_options(game_service, genre_packs_path, save_dir, false)
    }

    /// Create AppState with explicit headless mode control.
    pub fn new_with_options(
        game_service: Box<dyn GameService>,
        genre_packs_path: PathBuf,
        save_dir: PathBuf,
        headless: bool,
    ) -> Self {
        let (broadcast_tx, _) = broadcast::channel(256);
        let (watcher_tx, _) = broadcast::channel(256);
        let (binary_broadcast_tx, _) = broadcast::channel(64);

        // Render pipeline — headless mode skips daemon, emits tracing spans only
        let render_queue = if headless {
            sidequest_game::RenderQueue::spawn(
                sidequest_game::RenderQueueConfig::default(),
                |prompt, art_style, tier, _negative_prompt: String| async move {
                    tracing::info!(
                        prompt_len = prompt.len(),
                        prompt_preview = %&prompt[..prompt.len().min(120)],
                        art_style = %art_style,
                        tier = %tier,
                        headless = true,
                        "render_pipeline_headless_skip"
                    );
                    Ok(("/api/renders/headless-placeholder.svg".to_string(), 0u64))
                },
            )
        } else {
            sidequest_game::RenderQueue::spawn(
            sidequest_game::RenderQueueConfig::default(),
            |prompt, art_style, tier, negative_prompt: String| async move {
                // ── OTel: render pipeline start ──────────────────────────
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
                        // The art_style field carries the full composed style string
                        // (positive_suffix + location tag overrides), built at the
                        // enqueue call site. Combine with the raw prompt fragment to
                        // produce the positive_prompt the daemon expects.
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
                                // The daemon returns an absolute file path (e.g. /tmp/sq-flux-xxx/render_abc.png).
                                // Convert to a servable URL via /api/renders/{filename}.
                                // First, copy the file to the renders directory so the static server can serve it.
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
                                            // File doesn't exist at the path daemon told us — loud error
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
                                    // Bare filename — assume it's in the renders dir
                                    format!("/api/renders/{}", raw_path)
                                };
                                // ── OTel: render pipeline success ────────────────────
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
                                // ── OTel: render pipeline failure ────────────────────
                                // Error-level, not warn. A failed render is not a
                                // recoverable situation — the player sees a broken image.
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
                        // ── OTel: daemon connection failure ──────────────────
                        tracing::error!(error = %e, tier = %tier, "render_daemon_connect_failed — is the renderer running?");
                        Err(format!("daemon unavailable: {e}"))
                    }
                }
            },
        )
        }; // end if headless / else

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
    fn game_service(&self) -> &dyn GameService {
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
    ///
    /// Uses `.lock().await` on the session mutex to guarantee cleanup
    /// completes even under contention (fixes ghost player bug from try_lock).
    pub async fn remove_player_from_session(&self, genre: &str, world: &str, player_id: &str) -> usize {
        let key = shared_session::game_session_key(genre, world);
        // Clone the Arc and drop the sessions guard before awaiting the session lock.
        let session_arc = {
            let sessions = self.inner.sessions.lock().unwrap();
            match sessions.get(&key).cloned() {
                Some(arc) => arc,
                None => return 0,
            }
        };
        let remaining = {
            let mut session = session_arc.lock().await;
            session.players.remove(player_id);
            // Remove player from barrier roster if active
            if let Some(ref barrier) = session.turn_barrier {
                if let Err(e) = barrier.remove_player(player_id) {
                    tracing::warn!(
                        player_id = %player_id,
                        error = %e,
                        "Failed to remove player from barrier during session cleanup"
                    );
                }
            }
            let remaining = session.players.len();
            // Transition TurnMode when dropping back to solo
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
        };
        if remaining == 0 {
            let mut sessions = self.inner.sessions.lock().unwrap();
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
    /// Silently ignores the error when no subscribers are connected (zero overhead).
    pub fn send_watcher_event(&self, event: WatcherEvent) {
        let _ = self.inner.watcher_tx.send(event);
    }

    /// Broadcast binary data to all connected WebSocket clients.
    fn broadcast_binary(&self, data: Vec<u8>) {
        let _ = self.inner.binary_broadcast_tx.send(data);
    }

    /// Subscribe to binary broadcast frames (e.g. TTS audio).
    fn subscribe_binary(&self) -> broadcast::Receiver<Vec<u8>> {
        self.inner.binary_broadcast_tx.subscribe()
    }

    /// Try to mark a player as processing. Returns false if already processing.
    fn try_start_processing(&self, player_id: &PlayerId) -> bool {
        self.inner
            .processing
            .lock()
            .unwrap()
            .insert(player_id.clone())
    }

    /// Remove a player from the processing set.
    fn stop_processing(&self, player_id: &PlayerId) {
        self.inner.processing.lock().unwrap().remove(player_id);
    }
}

// ---------------------------------------------------------------------------
// ProcessingGuard
// ---------------------------------------------------------------------------

/// RAII guard that prevents concurrent actions from the same player.
/// When dropped, the player is removed from the processing set.
pub struct ProcessingGuard {
    state: AppState,
    player_id: PlayerId,
}

impl ProcessingGuard {
    /// Try to acquire a processing guard for a player.
    /// Returns `None` if the player already has an action in progress.
    pub fn acquire(state: &AppState, player_id: &PlayerId) -> Option<Self> {
        if state.try_start_processing(player_id) {
            Some(Self {
                state: state.clone(),
                player_id: player_id.clone(),
            })
        } else {
            None
        }
    }
}

impl Drop for ProcessingGuard {
    fn drop(&mut self) {
        self.state.stop_processing(&self.player_id);
    }
}

// ---------------------------------------------------------------------------
// Session
// ---------------------------------------------------------------------------

/// Per-connection session state machine.
///
/// Each WebSocket connection owns a Session that tracks the player's progress:
/// `AwaitingConnect` → `Creating` → `Playing`.
///
/// Messages are dispatched based on current state — out-of-phase messages
/// are rejected with an error, not a crash.
pub struct Session {
    state: SessionState,
}

enum SessionState {
    AwaitingConnect,
    Creating {
        genre_slug: String,
        world_slug: String,
        player_name: String,
    },
    Playing {
        genre_slug: String,
        world_slug: String,
        player_name: String,
    },
}

impl Session {
    /// Create a new session in the AwaitingConnect state.
    pub fn new() -> Self {
        Self {
            state: SessionState::AwaitingConnect,
        }
    }

    /// Handle a SESSION_EVENT{connect} — bind genre/world and transition to Creating.
    /// Returns a SESSION_EVENT{connected} response message.
    pub fn handle_connect(
        &mut self,
        genre: &str,
        world: &str,
        player_name: &str,
    ) -> Result<GameMessage, ServerError> {
        match &self.state {
            SessionState::AwaitingConnect => {
                self.state = SessionState::Creating {
                    genre_slug: genre.to_string(),
                    world_slug: world.to_string(),
                    player_name: player_name.to_string(),
                };

                // For now, new players always have has_character=false.
                // Save file checking is deferred to story 2-4.
                Ok(GameMessage::SessionEvent {
                    payload: sidequest_protocol::SessionEventPayload {
                        event: "connected".to_string(),
                        player_name: Some(player_name.to_string()),
                        genre: Some(genre.to_string()),
                        world: Some(world.to_string()),
                        has_character: Some(false),
                        initial_state: None,
                        css: None,
                    },
                    player_id: String::new(),
                })
            }
            _ => Err(ServerError::Deserialization(
                "Cannot connect: session already connected".to_string(),
            )),
        }
    }

    /// Complete character creation and transition to Playing.
    /// Actual character creation logic is story 2-3 — this is the state transition stub.
    pub fn complete_character_creation(&mut self) -> Result<(), ServerError> {
        match &self.state {
            SessionState::Creating {
                genre_slug,
                world_slug,
                player_name,
            } => {
                let genre_slug = genre_slug.clone();
                let world_slug = world_slug.clone();
                let player_name = player_name.clone();
                self.state = SessionState::Playing {
                    genre_slug,
                    world_slug,
                    player_name,
                };
                Ok(())
            }
            _ => Err(ServerError::Deserialization(
                "Cannot complete character creation: not in Creating state".to_string(),
            )),
        }
    }

    /// Check if the session is in AwaitingConnect state.
    pub fn is_awaiting_connect(&self) -> bool {
        matches!(self.state, SessionState::AwaitingConnect)
    }

    /// Check if the session is in Creating state.
    pub fn is_creating(&self) -> bool {
        matches!(self.state, SessionState::Creating { .. })
    }

    /// Check if the session is in Playing state.
    pub fn is_playing(&self) -> bool {
        matches!(self.state, SessionState::Playing { .. })
    }

    /// Get the current state name as a string.
    pub fn state_name(&self) -> &str {
        match &self.state {
            SessionState::AwaitingConnect => "AwaitingConnect",
            SessionState::Creating { .. } => "Creating",
            SessionState::Playing { .. } => "Playing",
        }
    }

    /// Check if a message type is valid for the current session state.
    pub fn can_handle_message_type(&self, msg_type: &str) -> bool {
        match &self.state {
            SessionState::AwaitingConnect => matches!(msg_type, "SESSION_EVENT"),
            SessionState::Creating { .. } => {
                matches!(msg_type, "CHARACTER_CREATION" | "SESSION_EVENT")
            }
            SessionState::Playing { .. } => {
                matches!(msg_type, "PLAYER_ACTION" | "SESSION_EVENT")
            }
        }
    }

    /// Reset the session to AwaitingConnect state. Used on disconnect.
    pub fn cleanup(&mut self) {
        self.state = SessionState::AwaitingConnect;
    }

    /// Get the bound genre slug, if connected.
    pub fn genre_slug(&self) -> Option<&str> {
        match &self.state {
            SessionState::Creating { genre_slug, .. }
            | SessionState::Playing { genre_slug, .. } => Some(genre_slug),
            _ => None,
        }
    }

    /// Get the bound world slug, if connected.
    pub fn world_slug(&self) -> Option<&str> {
        match &self.state {
            SessionState::Creating { world_slug, .. }
            | SessionState::Playing { world_slug, .. } => Some(world_slug),
            _ => None,
        }
    }

    /// Get the player name, if connected.
    pub fn player_name(&self) -> Option<&str> {
        match &self.state {
            SessionState::Creating { player_name, .. }
            | SessionState::Playing { player_name, .. } => Some(player_name),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Construct a GameMessage::Error from a player id and error description.
pub fn error_response(player_id: &str, message: &str) -> GameMessage {
    GameMessage::Error {
        payload: ErrorPayload {
            message: message.to_string(),
            reconnect_required: None,
        },
        player_id: player_id.to_string(),
    }
}

/// Construct a GameMessage::Error that tells the client to re-send its
/// SESSION_EVENT{connect} handshake before retrying.
pub fn reconnect_required_response(player_id: &str, message: &str) -> GameMessage {
    GameMessage::Error {
        payload: ErrorPayload {
            message: message.to_string(),
            reconnect_required: Some(true),
        },
        player_id: player_id.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Build the axum Router with all routes and middleware.
///
/// Routes:
/// - `GET /api/genres` — list available genres and their worlds
/// - `GET /ws` — WebSocket upgrade for game sessions
///
/// Middleware:
/// - CORS for React dev server at localhost:5173
pub fn build_router(state: AppState) -> Router {
    // Spawn image broadcaster — listens for render completions and broadcasts IMAGE messages
    if let Some(ref queue) = state.inner.render_queue {
        let mut render_rx = queue.subscribe();
        let broadcast_tx = state.inner.broadcast_tx.clone();
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
                    // Rewrite absolute file paths to served URLs.
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

    // Serve genre pack static assets (fonts, images, audio) at /genre/{slug}/...
    let genre_assets = ServeDir::new(state.genre_packs_path());

    // Serve rendered images at /api/renders/...
    // Use SIDEQUEST_OUTPUT_DIR (same dir the daemon writes to) or fall back to ~/.sidequest/renders
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
        .route("/ws/watcher", get(watcher::ws_watcher_handler))
        .nest_service("/genre", genre_assets)
        .nest_service("/api/renders", renders_assets)
        .layer(cors)
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /api/genres — scan genre_packs_path for genre directories with worlds.
///
/// Returns: `{ "genre_slug": { "worlds": ["world1", "world2"] } }`
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

        // Check for pack.yaml to confirm this is a genre pack
        if !path.join("pack.yaml").exists() {
            continue;
        }

        // Scan worlds/ subdirectory
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

async fn handle_ws_connection(socket: WebSocket, state: AppState, player_id: PlayerId) {
    tracing::info!(player_id = %player_id, "WebSocket connected");

    let (mut ws_sink, mut ws_stream) = socket.split();

    // Create an mpsc channel for sending messages to this client
    let (tx, mut rx) = mpsc::channel::<GameMessage>(32);
    state.add_connection(player_id.clone(), tx.clone());

    // Subscribe to broadcast
    let mut broadcast_rx = state.subscribe_broadcast();
    let mut binary_rx = state.subscribe_binary();

    let player_id_str = player_id.to_string();

    // Shared session — populated after dispatch_connect identifies genre/world.
    // Wrapped in Arc so the writer task can also receive session broadcasts.
    let shared_session: Arc<
        tokio::sync::Mutex<Option<Arc<tokio::sync::Mutex<shared_session::SharedGameSession>>>>,
    > = Arc::new(tokio::sync::Mutex::new(None));
    let shared_session_for_writer = shared_session.clone();

    // Writer task: reads from mpsc channel + broadcast + binary + session, sends to WS
    let writer_player_id = player_id_str.clone();
    let writer_handle = tokio::spawn(async move {
        // Session broadcast receiver — lazily initialized when shared session is set.
        let mut session_rx: Option<broadcast::Receiver<crate::shared_session::TargetedMessage>> = None;

        loop {
            // Lazily subscribe to session broadcast if we don't have a receiver yet.
            // Uses lock().await (not try_lock) to guarantee subscription succeeds.
            // Without this, session messages (NARRATION, NARRATION_END for observers)
            // are silently dropped and narration accumulates without separators.
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
                // Session-scoped broadcast: narration from other players in the same session.
                // Skip messages originating from this player to avoid duplicate rendering.
                // Also filters targeted messages — only delivers to the intended recipient.
                result = async { match session_rx.as_mut() { Some(rx) => rx.recv().await, None => std::future::pending().await } } => {
                    if let Ok(targeted) = result {
                        // Target filter: if message is targeted to a specific player, skip if not us
                        if let Some(ref target) = targeted.target_player_id {
                            if target != &writer_player_id {
                                continue;
                            }
                        }
                        let msg = targeted.msg;
                        // Self-skip: only applies to UNTARGETED broadcasts (target_player_id is None).
                        // Targeted messages already passed the target filter above — the player_id
                        // field on targeted messages is the RECIPIENT's ID, not the sender's.
                        // Skipping on player_id match would drop every targeted message.
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
    let mut session = Session::new();
    let mut builder: Option<CharacterBuilder> = None;
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
    let mut quest_log: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut world_context: String = String::new();
    let mut axes_config: Option<sidequest_genre::AxesConfig> = None;
    let mut axis_values: Vec<sidequest_game::axis::AxisValue> = vec![];
    let mut visual_style: Option<sidequest_genre::VisualStyle> = None;
    let mut music_director: Option<sidequest_game::MusicDirector> = None;
    let mut npc_registry: Vec<NpcRegistryEntry> = vec![];
    let mut genie_wishes: Vec<sidequest_game::GenieWish> = vec![];
    let mut discovered_regions: Vec<String> = vec![];
    let mut turn_manager = sidequest_game::TurnManager::new();
    let mut lore_store = sidequest_game::LoreStore::new();
    // Bug 17: In-memory narration history for context accumulation across turns.
    // Each entry is "Player: <action>\nNarrator: <response>" for the last N turns.
    let mut narration_history: Vec<String> = vec![];
    // Continuity validator: corrections from the previous turn, injected into the next narrator prompt.
    let mut continuity_corrections = String::new();
    let audio_mixer: std::sync::Arc<tokio::sync::Mutex<Option<sidequest_game::AudioMixer>>> =
        std::sync::Arc::new(tokio::sync::Mutex::new(None));
    let prerender_scheduler: std::sync::Arc<
        tokio::sync::Mutex<Option<sidequest_game::PrerenderScheduler>>,
    > = std::sync::Arc::new(tokio::sync::Mutex::new(None));

    // Reader loop: read messages, deserialize, dispatch through session
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
                        &mut genie_wishes,
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
            Ok(_) => {} // ping/pong/binary handled by axum
            Err(e) => {
                tracing::warn!(player_id = %player_id_str, error = %e, "WebSocket error");
                break;
            }
        }
    }

    // Cleanup — remove from shared session if joined, broadcast updated PARTY_STATUS
    if let (Some(genre), Some(world)) = (session.genre_slug(), session.world_slug()) {
        let key = shared_session::game_session_key(genre, world);
        // Broadcast leave before removing, so the broadcast channel still exists
        // Scope the std Mutex guard so it's dropped before the tokio .await
        let ss_arc = {
            let sessions = state.inner.sessions.lock().unwrap();
            sessions.get(&key).cloned()
        };
        if let Some(ss_arc) = ss_arc {
            // Use .lock().await (not try_lock) to guarantee cleanup completes.
            let mut ss = ss_arc.lock().await;
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

                // Transition turn mode when player leaves
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
                // Remove player from barrier roster and tear down if back to FreePlay
                if let Some(ref barrier) = ss.turn_barrier {
                    if let Err(e) = barrier.remove_player(&player_id_str) {
                        tracing::warn!(
                            player_id = %player_id_str,
                            error = %e,
                            "Failed to remove player from barrier during disconnect cleanup"
                        );
                    }
                }
                if !ss.turn_mode.should_use_barrier() {
                    ss.turn_barrier = None;
                }
        }
        let remaining = state.remove_player_from_session(genre, world, &player_id_str).await;
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

// ---------------------------------------------------------------------------
// Message dispatch
// ---------------------------------------------------------------------------

/// Dispatch a deserialized GameMessage through the session state machine.
/// Returns a list of response messages to send back to the client.
#[allow(clippy::too_many_arguments)]
async fn dispatch_message(
    msg: GameMessage,
    session: &mut Session,
    builder: &mut Option<CharacterBuilder>,
    player_name_store: &mut Option<String>,
    character_json: &mut Option<serde_json::Value>,
    character_name: &mut Option<String>,
    character_hp: &mut i32,
    character_max_hp: &mut i32,
    character_level: &mut u32,
    character_xp: &mut u32,
    current_location: &mut String,
    inventory: &mut sidequest_game::Inventory,
    combat_state: &mut sidequest_game::combat::CombatState,
    chase_state: &mut Option<sidequest_game::ChaseState>,
    trope_states: &mut Vec<sidequest_game::trope::TropeState>,
    trope_defs: &mut Vec<sidequest_genre::TropeDefinition>,
    world_context: &mut String,
    axes_config: &mut Option<sidequest_genre::AxesConfig>,
    axis_values: &mut Vec<sidequest_game::axis::AxisValue>,
    visual_style: &mut Option<sidequest_genre::VisualStyle>,
    music_director: &mut Option<sidequest_game::MusicDirector>,
    audio_mixer: &std::sync::Arc<tokio::sync::Mutex<Option<sidequest_game::AudioMixer>>>,
    prerender_scheduler: &std::sync::Arc<
        tokio::sync::Mutex<Option<sidequest_game::PrerenderScheduler>>,
    >,
    npc_registry: &mut Vec<NpcRegistryEntry>,
    quest_log: &mut std::collections::HashMap<String, String>,
    narration_history: &mut Vec<String>,
    discovered_regions: &mut Vec<String>,
    turn_manager: &mut sidequest_game::TurnManager,
    lore_store: &mut sidequest_game::LoreStore,
    shared_session_holder: &Arc<
        tokio::sync::Mutex<Option<Arc<tokio::sync::Mutex<shared_session::SharedGameSession>>>>,
    >,
    state: &AppState,
    player_id: &str,
    continuity_corrections: &mut String,
    genie_wishes: &mut Vec<sidequest_game::GenieWish>,
) -> Vec<GameMessage> {
    match &msg {
        GameMessage::SessionEvent { payload, .. } if payload.event == "connect" => {
            let mut responses = dispatch_connect(
                payload,
                session,
                builder,
                player_name_store,
                character_json,
                character_name,
                character_hp,
                character_max_hp,
                character_level,
                character_xp,
                current_location,
                discovered_regions,
                trope_defs,
                trope_states,
                world_context,
                axes_config,
                axis_values,
                visual_style,
                music_director,
                audio_mixer,
                prerender_scheduler,
                turn_manager,
                npc_registry,
                quest_log,
                lore_store,
                state,
                player_id,
                continuity_corrections,
                genie_wishes,
                combat_state,
                chase_state,
            )
            .await;
            // After connect identifies genre/world, join/create the shared session
            if let (Some(genre), Some(world)) = (session.genre_slug(), session.world_slug()) {
                let ss = state.get_or_create_session(genre, world);
                *shared_session_holder.lock().await = Some(ss.clone());

                // Load cartography regions if not already loaded
                {
                    let mut ss_guard = ss.lock().await;
                    if ss_guard.region_names.is_empty() {
                        if let Ok(genre_code) = GenreCode::new(genre) {
                            let loader =
                                GenreLoader::new(vec![state.genre_packs_path().to_path_buf()]);
                            if let Ok(pack) = loader.load(&genre_code) {
                                if let Some(w) = pack.worlds.get(world) {
                                    ss_guard.load_cartography(&w.cartography.regions);
                                }
                            }
                        }
                    }
                }

                // If this is a returning player (already Playing), add them to
                // the shared session now. New players get added after character
                // creation completes in dispatch_character_creation.
                if session.is_playing() {
                    let mut ss_guard = ss.lock().await;
                    let pname = player_name_store
                        .clone()
                        .unwrap_or_else(|| "Player".to_string());
                    let mut ps = shared_session::PlayerState::new(pname);
                    // Populate character data from locals (set by dispatch_connect)
                    ps.character_name = character_name.clone();
                    ps.character_hp = *character_hp;
                    ps.character_max_hp = *character_max_hp;
                    ps.display_location = current_location.clone();
                    ps.region_id = ss_guard
                        .resolve_region(current_location)
                        .unwrap_or_default();
                    // Extract level/class from character_json since dispatch_connect
                    // doesn't restore them to the scalar locals.
                    if let Some(ref cj) = *character_json {
                        ps.character_level = cj
                            .get("core")
                            .and_then(|c| c.get("level"))
                            .and_then(|l| l.as_u64())
                            .unwrap_or(1) as u32;
                        ps.character_class = cj
                            .get("char_class")
                            .and_then(|c| c.as_str())
                            .unwrap_or("")
                            .to_string();
                        // Also fix the scalar locals so dispatch_player_action has them
                        *character_level = ps.character_level;
                    }
                    ss_guard.players.insert(player_id.to_string(), ps);

                    // Transition turn mode (PlayerJoined)
                    let pc = ss_guard.player_count();
                    let old_mode = std::mem::take(&mut ss_guard.turn_mode);
                    ss_guard.turn_mode = old_mode.apply(
                        sidequest_game::turn_mode::TurnModeTransition::PlayerJoined {
                            player_count: pc,
                        },
                    );
                    tracing::info!(
                        new_mode = ?ss_guard.turn_mode,
                        player_count = pc,
                        "Turn mode transitioned on reconnecting player join"
                    );

                    // Initialize barrier if transitioning to structured mode
                    if ss_guard.turn_mode.should_use_barrier()
                        && ss_guard.turn_barrier.is_none()
                    {
                        let mp_session =
                            sidequest_game::multiplayer::MultiplayerSession::with_player_ids(
                                ss_guard.players.keys().cloned(),
                            );
                        let adaptive =
                            sidequest_game::barrier::AdaptiveTimeout::default();
                        ss_guard.turn_barrier =
                            Some(sidequest_game::barrier::TurnBarrier::with_adaptive(
                                mp_session, adaptive,
                            ));
                        tracing::info!(
                            player_count = pc,
                            "Initialized turn barrier for reconnecting player"
                        );
                    }

                    // Broadcast targeted PARTY_STATUS to all session members
                    let members: Vec<PartyMember> = ss_guard
                        .players
                        .iter()
                        .map(|(pid, ps)| PartyMember {
                            player_id: pid.clone(),
                            name: ps.player_name.clone(),
                            character_name: ps
                                .character_name
                                .clone()
                                .unwrap_or_else(|| ps.player_name.clone()),
                            current_hp: ps.character_hp,
                            max_hp: ps.character_max_hp,
                            statuses: vec![],
                            class: ps.character_class.clone(),
                            level: ps.character_level,
                            portrait_url: None,
                        })
                        .collect();
                    if !members.is_empty() {
                        let pids: Vec<String> =
                            ss_guard.players.keys().cloned().collect();
                        for target_pid in &pids {
                            let party_msg = GameMessage::PartyStatus {
                                payload: PartyStatusPayload {
                                    members: members.clone(),
                                },
                                player_id: target_pid.clone(),
                            };
                            ss_guard
                                .send_to_player(party_msg, target_pid.clone());
                        }
                    }

                    tracing::info!(
                        player_id = %player_id,
                        player_count = pc,
                        "Reconnecting player joined shared session"
                    );

                    // Send full multiplayer PARTY_STATUS directly to the reconnecting
                    // player (via direct tx, not session channel which may not be subscribed).
                    let all_members: Vec<PartyMember> = ss_guard
                        .players
                        .iter()
                        .map(|(pid, ps)| PartyMember {
                            player_id: pid.clone(),
                            name: ps.player_name.clone(),
                            character_name: ps
                                .character_name
                                .clone()
                                .unwrap_or_else(|| ps.player_name.clone()),
                            current_hp: ps.character_hp,
                            max_hp: ps.character_max_hp,
                            statuses: vec![],
                            class: ps.character_class.clone(),
                            level: ps.character_level,
                            portrait_url: None,
                        })
                        .collect();
                    let member_count = all_members.len();
                    responses.push(GameMessage::PartyStatus {
                        payload: PartyStatusPayload { members: all_members },
                        player_id: player_id.to_string(),
                    });
                    tracing::info!(
                        player_id = %player_id,
                        member_count,
                        "reconnect.party_status — sent full party via direct tx"
                    );

                    // Send TURN_STATUS "resolved" so the reconnecting player's input
                    // is enabled. If it's someone else's turn, the next action will
                    // send a proper TURN_STATUS "active" via global broadcast.
                    if pc > 1 {
                        responses.push(GameMessage::TurnStatus {
                            payload: TurnStatusPayload {
                                player_name: player_name_store
                                    .clone()
                                    .unwrap_or_else(|| "Player".to_string()),
                                status: "resolved".into(),
                                state_delta: None,
                            },
                            player_id: player_id.to_string(),
                        });
                        tracing::info!(player_id = %player_id, pc, "reconnect.turn_status_resolved — sent via direct tx");
                    } else {
                        tracing::info!(player_id = %player_id, pc, "reconnect.solo — no TURN_STATUS needed (single player)");
                    }
                }
            }
            responses
        }
        GameMessage::CharacterCreation { payload, .. } => {
            if !session.is_creating() {
                return vec![error_response(player_id, "Not in character creation state")];
            }
            dispatch_character_creation(
                payload,
                session,
                builder,
                player_name_store,
                character_json,
                character_name,
                character_hp,
                character_max_hp,
                character_level,
                character_xp,
                current_location,
                inventory,
                combat_state,
                chase_state,
                trope_states,
                trope_defs,
                world_context,
                axes_config,
                axis_values,
                visual_style,
                npc_registry,
                quest_log,
                narration_history,
                discovered_regions,
                turn_manager,
                lore_store,
                shared_session_holder,
                music_director,
                audio_mixer,
                prerender_scheduler,
                state,
                player_id,
                continuity_corrections,
                genie_wishes,
            )
            .await
        }
        GameMessage::PlayerAction { payload, .. } => {
            if !session.is_playing() {
                let err = if session.is_awaiting_connect() {
                    reconnect_required_response(
                        player_id,
                        "Session not established. Please reconnect.",
                    )
                } else {
                    error_response(
                        player_id,
                        &format!("Cannot process action in {} state", session.state_name()),
                    )
                };
                return vec![err];
            }
            dispatch::dispatch_player_action(&mut dispatch::DispatchContext {
                action: &payload.action,
                char_name: character_name.as_deref().unwrap_or("Unknown"),
                hp: character_hp,
                max_hp: character_max_hp,
                level: character_level,
                xp: character_xp,
                current_location,
                inventory,
                character_json,
                combat_state,
                chase_state,
                trope_states,
                trope_defs,
                world_context,
                axes_config,
                axis_values,
                visual_style,
                npc_registry,
                quest_log,
                narration_history,
                discovered_regions,
                turn_manager,
                lore_store,
                shared_session_holder,
                music_director,
                audio_mixer,
                prerender_scheduler,
                state,
                player_id,
                genre_slug: session.genre_slug().unwrap_or(""),
                world_slug: session.world_slug().unwrap_or(""),
                player_name_for_save: player_name_store.as_deref().unwrap_or("Player"),
                continuity_corrections,
                genie_wishes,
            })
            .await
        }
        // All other valid message types in wrong state
        _ => {
            if session.is_awaiting_connect() {
                vec![reconnect_required_response(
                    player_id,
                    "Session not established. Please reconnect.",
                )]
            } else {
                vec![error_response(
                    player_id,
                    &format!("Unexpected message in {} state", session.state_name()),
                )]
            }
        }
    }
}

/// Handle SESSION_EVENT{connect}.
#[allow(clippy::too_many_arguments)]
async fn dispatch_connect(
    payload: &SessionEventPayload,
    session: &mut Session,
    builder: &mut Option<CharacterBuilder>,
    player_name_store: &mut Option<String>,
    character_json_store: &mut Option<serde_json::Value>,
    character_name_store: &mut Option<String>,
    character_hp: &mut i32,
    character_max_hp: &mut i32,
    character_level: &mut u32,
    character_xp: &mut u32,
    current_location: &mut String,
    discovered_regions: &mut Vec<String>,
    trope_defs: &mut Vec<sidequest_genre::TropeDefinition>,
    trope_states: &mut Vec<sidequest_game::trope::TropeState>,
    world_context: &mut String,
    axes_config: &mut Option<sidequest_genre::AxesConfig>,
    axis_values: &mut Vec<sidequest_game::axis::AxisValue>,
    visual_style: &mut Option<sidequest_genre::VisualStyle>,
    music_director: &mut Option<sidequest_game::MusicDirector>,
    audio_mixer: &std::sync::Arc<tokio::sync::Mutex<Option<sidequest_game::AudioMixer>>>,
    prerender_scheduler: &std::sync::Arc<
        tokio::sync::Mutex<Option<sidequest_game::PrerenderScheduler>>,
    >,
    turn_manager: &mut sidequest_game::TurnManager,
    npc_registry: &mut Vec<NpcRegistryEntry>,
    quest_log: &mut std::collections::HashMap<String, String>,
    lore_store: &mut sidequest_game::LoreStore,
    state: &AppState,
    player_id: &str,
    _continuity_corrections: &mut String,
    genie_wishes: &mut Vec<sidequest_game::GenieWish>,
    combat_state: &mut sidequest_game::combat::CombatState,
    chase_state: &mut Option<sidequest_game::ChaseState>,
) -> Vec<GameMessage> {
    let genre = payload.genre.as_deref().unwrap_or("");
    let world = payload.world.as_deref().unwrap_or("");
    let pname = payload.player_name.as_deref().unwrap_or("Player");

    // Check for returning player — load from SQLite (now keyed by player name)
    let returning = state.persistence().exists(genre, world, pname).await;

    match session.handle_connect(genre, world, pname) {
        Ok(mut connected_msg) => {
            let mut responses = Vec::new();
            *player_name_store = Some(pname.to_string());

            if returning {
                // Returning player — load snapshot from SQLite (keyed by player name)
                match state.persistence().load(genre, world, pname).await {
                    Ok(Some(saved)) => {
                        if let GameMessage::SessionEvent {
                            ref mut payload, ..
                        } = connected_msg
                        {
                            payload.has_character = Some(true);
                        }
                        responses.push(connected_msg);

                        // Extract character data from saved snapshot
                        if let Some(character) = saved.snapshot.characters.first() {
                            match serde_json::to_value(character) {
                                Ok(json) => {
                                    *character_json_store = Some(json);
                                }
                                Err(e) => {
                                    tracing::error!(
                                        error = %e,
                                        "Failed to serialize character from saved snapshot — skipping character_json sync"
                                    );
                                }
                            }
                            *character_name_store = Some(character.core.name.as_str().to_string());
                            *character_hp = character.core.hp;
                            *character_max_hp = character.core.max_hp;
                            *character_level = character.core.level;
                            *character_xp = character.core.xp;
                        }
                        // Restore location, regions, turn state, and NPC registry from snapshot
                        *current_location = saved.snapshot.location.clone();
                        *discovered_regions = saved.snapshot.discovered_regions.clone();
                        *turn_manager = saved.snapshot.turn_manager.clone();
                        *npc_registry = saved.snapshot.npc_registry.clone();
                        *genie_wishes = saved.snapshot.genie_wishes.clone();
                        *axis_values = saved.snapshot.axis_values.clone();
                        *trope_states = saved.snapshot.active_tropes.clone();
                        *quest_log = saved.snapshot.quest_log.clone();
                        *combat_state = saved.snapshot.combat.clone();
                        *chase_state = saved.snapshot.chase.clone();
                        tracing::info!(
                            trope_count = trope_states.len(),
                            quest_count = quest_log.len(),
                            in_combat = combat_state.in_combat(),
                            combat_round = combat_state.round(),
                            "reconnect.state_restored — tropes, quests, combat, chase loaded from save"
                        );

                        // Transition session to Playing
                        let _ = session.complete_character_creation();

                        let ready = GameMessage::SessionEvent {
                            payload: SessionEventPayload {
                                event: "ready".to_string(),
                                player_name: None,
                                genre: None,
                                world: None,
                                has_character: None,
                                initial_state: Some(InitialState {
                                    characters: saved
                                        .snapshot
                                        .characters
                                        .iter()
                                        .map(|c| CharacterState {
                                            name: c.core.name.as_str().to_string(),
                                            hp: c.core.hp,
                                            max_hp: c.core.max_hp,
                                            level: c.core.level,
                                            class: c.char_class.as_str().to_string(),
                                            statuses: c.core.statuses.clone(),
                                            inventory: c
                                                .core
                                                .inventory
                                                .items
                                                .iter()
                                                .map(|i| i.name.as_str().to_string())
                                                .collect(),
                                        })
                                        .collect(),
                                    location: saved.snapshot.location.clone(),
                                    quests: saved.snapshot.quest_log.clone(),
                                    turn_count: saved.snapshot.turn_manager.interaction().saturating_sub(1) as u32,
                                }),
                                css: None,
                            },
                            player_id: player_id.to_string(),
                        };
                        responses.push(ready);

                        // Replay essential state for reconnecting client
                        // CHARACTER_SHEET
                        if let Some(character) = saved.snapshot.characters.first() {
                            responses.push(GameMessage::CharacterSheet {
                                payload: CharacterSheetPayload {
                                    name: character.core.name.as_str().to_string(),
                                    class: character.char_class.as_str().to_string(),
                                    race: character.race.as_str().to_string(),
                                    level: character.core.level as u32,
                                    stats: character
                                        .stats
                                        .iter()
                                        .map(|(k, v)| (k.clone(), *v))
                                        .collect(),
                                    abilities: character.hooks.clone(),
                                    backstory: character.backstory.as_str().to_string(),
                                    personality: character.core.personality.as_str().to_string(),
                                    pronouns: character.pronouns.clone(),
                                    equipment: character.core.inventory.items.iter().map(|i| {
                                        if i.equipped {
                                            format!("{} [equipped]", i.name)
                                        } else {
                                            i.name.as_str().to_string()
                                        }
                                    }).collect(),
                                    portrait_url: None,
                                },
                                player_id: player_id.to_string(),
                            });
                        }

                        // CHAPTER_MARKER for current location
                        if !saved.snapshot.location.is_empty() {
                            responses.push(GameMessage::ChapterMarker {
                                payload: ChapterMarkerPayload {
                                    title: Some(saved.snapshot.location.clone()),
                                    location: Some(saved.snapshot.location.clone()),
                                },
                                player_id: player_id.to_string(),
                            });
                        }

                        // Last NARRATION — recap or last narrative log entry
                        let recap_text = saved.recap.clone().or_else(|| {
                            saved
                                .snapshot
                                .narrative_log
                                .last()
                                .map(|e| e.content.clone())
                        });
                        if let Some(text) = recap_text {
                            responses.push(GameMessage::Narration {
                                payload: NarrationPayload {
                                    text,
                                    state_delta: None,
                                    footnotes: vec![],
                                },
                                player_id: player_id.to_string(),
                            });
                            responses.push(GameMessage::NarrationEnd {
                                payload: NarrationEndPayload { state_delta: None },
                                player_id: player_id.to_string(),
                            });
                        }

                        // PARTY_STATUS
                        {
                            let members: Vec<PartyMember> = saved
                                .snapshot
                                .characters
                                .iter()
                                .map(|c| PartyMember {
                                    player_id: player_id.to_string(),
                                    name: player_name_store.as_deref().unwrap_or("Player").to_string(),
                                    character_name: c.core.name.as_str().to_string(),
                                    current_hp: c.core.hp,
                                    max_hp: c.core.max_hp,
                                    statuses: c.core.statuses.clone(),
                                    class: c.char_class.as_str().to_string(),
                                    level: c.core.level as u32,
                                    portrait_url: None,
                                })
                                .collect();
                            responses.push(GameMessage::PartyStatus {
                                payload: PartyStatusPayload { members },
                                player_id: player_id.to_string(),
                            });
                        }

                        // Initialize audio subsystems for returning player
                        if let Ok(genre_code) = GenreCode::new(genre) {
                            let loader =
                                GenreLoader::new(vec![state.genre_packs_path().to_path_buf()]);
                            if let Ok(pack) = loader.load(&genre_code) {
                                *visual_style = Some(pack.visual_style.clone());
                                *axes_config = Some(pack.axes.clone());
                                *music_director =
                                    Some(sidequest_game::MusicDirector::new(&pack.audio));
                                *audio_mixer.lock().await = Some(sidequest_game::AudioMixer::new(
                                    sidequest_game::DuckConfig::default(),
                                ));
                                *prerender_scheduler.lock().await =
                                    Some(sidequest_game::PrerenderScheduler::new(
                                        sidequest_game::PrerenderConfig::default(),
                                    ));
                                // Load trope definitions for returning player (same logic as start_character_creation)
                                let mut all_tropes = pack.tropes.clone();
                                if let Some(w) = pack.worlds.get(world) {
                                    all_tropes.extend(w.tropes.clone());
                                }
                                for trope in &mut all_tropes {
                                    if trope.id.is_none() {
                                        let slug = trope.name.as_str().to_lowercase().replace(' ', "-")
                                            .replace(|c: char| !c.is_alphanumeric() && c != '-', "");
                                        trope.id = Some(slug);
                                    }
                                }
                                all_tropes.retain(|t| !t.is_abstract);
                                *trope_defs = all_tropes;
                                tracing::info!(count = trope_defs.len(), genre = %genre, "Loaded trope definitions for returning player");

                                tracing::info!(genre = %genre, "Audio subsystems initialized for returning player");

                                // Seed lore store from genre pack (story 11-4)
                                let lore_count =
                                    sidequest_game::seed_lore_from_genre_pack(lore_store, &pack);
                                tracing::info!(
                                    count = lore_count,
                                    genre = %genre,
                                    "rag.lore_store_seeded"
                                );

                                // Inject name bank context for returning player
                                let cultures = pack
                                    .worlds
                                    .get(world)
                                    .filter(|w| !w.cultures.is_empty())
                                    .map(|w| w.cultures.as_slice())
                                    .unwrap_or(&pack.cultures);
                                let name_bank = build_name_bank_context(cultures);
                                if !name_bank.is_empty() {
                                    world_context.push_str(&name_bank);
                                }
                            }
                        }

                        tracing::info!(
                            player = %pname,
                            genre = %genre,
                            world = %world,
                            "Player reconnected from saved session"
                        );
                    }
                    Ok(None) => {
                        // Save file exists but no game state — treat as new player
                        tracing::warn!(genre = %genre, world = %world, "Save file exists but empty");
                        responses.push(connected_msg);
                        if let Some(scene_msg) = start_character_creation(
                            builder,
                            trope_defs,
                            world_context,
                            visual_style,
                            axes_config,
                            music_director,
                            audio_mixer,
                            prerender_scheduler,
                            lore_store,
                            genre,
                            world,
                            state,
                            player_id,
                        )
                        .await
                        {
                            responses.push(scene_msg);
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to load saved session, starting fresh");
                        responses.push(connected_msg);
                        if let Some(scene_msg) = start_character_creation(
                            builder,
                            trope_defs,
                            world_context,
                            visual_style,
                            axes_config,
                            music_director,
                            audio_mixer,
                            prerender_scheduler,
                            lore_store,
                            genre,
                            world,
                            state,
                            player_id,
                        )
                        .await
                        {
                            responses.push(scene_msg);
                        }
                    }
                }
            } else {
                // New player — send connected, then start character creation
                responses.push(connected_msg);
                if let Some(scene_msg) = start_character_creation(
                    builder,
                    trope_defs,
                    world_context,
                    visual_style,
                    axes_config,
                    music_director,
                    audio_mixer,
                    prerender_scheduler,
                    lore_store,
                    genre,
                    world,
                    state,
                    player_id,
                )
                .await
                {
                    responses.push(scene_msg);
                }
            }

            // Generate theme CSS from theme.yaml + optional client_theme.css overrides
            if let Ok(genre_code) = GenreCode::new(genre) {
                let loader =
                    GenreLoader::new(vec![state.genre_packs_path().to_path_buf()]);
                if let Ok(pack) = loader.load(&genre_code) {
                    let mut css = pack.theme.generate_css();

                    // Append client_theme.css overrides if present
                    let css_path = state
                        .genre_packs_path()
                        .join(genre)
                        .join("client_theme.css");
                    if let Ok(override_css) = tokio::fs::read_to_string(&css_path).await {
                        css.push('\n');
                        css.push_str(&override_css);
                    }

                    responses.push(GameMessage::SessionEvent {
                        payload: SessionEventPayload {
                            event: "theme_css".to_string(),
                            player_name: None,
                            genre: None,
                            world: None,
                            has_character: None,
                            initial_state: None,
                            css: Some(css),
                        },
                        player_id: player_id.to_string(),
                    });
                }
            }

            responses
        }
        Err(e) => {
            vec![error_response(player_id, &e.to_string())]
        }
    }
}

/// Load genre pack, create CharacterBuilder, return first scene message + trope defs + world context.
async fn start_character_creation(
    builder: &mut Option<CharacterBuilder>,
    trope_defs_out: &mut Vec<sidequest_genre::TropeDefinition>,
    world_context_out: &mut String,
    visual_style_out: &mut Option<sidequest_genre::VisualStyle>,
    axes_config_out: &mut Option<sidequest_genre::AxesConfig>,
    music_director_out: &mut Option<sidequest_game::MusicDirector>,
    audio_mixer_lock: &std::sync::Arc<tokio::sync::Mutex<Option<sidequest_game::AudioMixer>>>,
    prerender_lock: &std::sync::Arc<tokio::sync::Mutex<Option<sidequest_game::PrerenderScheduler>>>,
    lore_store: &mut sidequest_game::LoreStore,
    genre: &str,
    world_slug: &str,
    state: &AppState,
    player_id: &str,
) -> Option<GameMessage> {
    let genre_code = match GenreCode::new(genre) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(genre = %genre, error = %e, "Invalid genre code");
            return None;
        }
    };

    let loader = GenreLoader::new(vec![state.genre_packs_path().to_path_buf()]);
    let pack = match loader.load(&genre_code) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(genre = %genre, error = %e, "Failed to load genre pack");
            return None;
        }
    };

    *visual_style_out = Some(pack.visual_style.clone());
    *axes_config_out = Some(pack.axes.clone());

    // Initialize audio subsystems from genre pack
    *music_director_out = Some(sidequest_game::MusicDirector::new(&pack.audio));
    *audio_mixer_lock.lock().await = Some(sidequest_game::AudioMixer::new(
        sidequest_game::DuckConfig::default(),
    ));
    *prerender_lock.lock().await = Some(sidequest_game::PrerenderScheduler::new(
        sidequest_game::PrerenderConfig::default(),
    ));
    tracing::info!(genre = %genre, "Audio subsystems initialized from genre pack");

    // Seed lore store from genre pack (story 11-4)
    let lore_count = sidequest_game::seed_lore_from_genre_pack(lore_store, &pack);
    tracing::info!(count = lore_count, genre = %genre, "rag.lore_store_seeded");

    // Extract trope definitions from the genre pack for per-session use.
    // Collect from genre-level tropes and all world tropes.
    // Auto-generate IDs from names for tropes that don't have explicit IDs,
    // and filter out abstract archetypes (they need world-level specialization).
    let mut all_tropes = pack.tropes.clone();
    for world in pack.worlds.values() {
        all_tropes.extend(world.tropes.clone());
    }
    // Backfill missing IDs from name slugs so seeding/tick can match them
    for trope in &mut all_tropes {
        if trope.id.is_none() {
            let slug = trope
                .name
                .as_str()
                .to_lowercase()
                .replace(' ', "-")
                .replace(|c: char| !c.is_alphanumeric() && c != '-', "");
            trope.id = Some(slug);
        }
    }
    // Filter out abstract archetypes — they are templates, not activatable tropes
    all_tropes.retain(|t| !t.is_abstract);
    *trope_defs_out = all_tropes;
    tracing::info!(count = trope_defs_out.len(), genre = %genre, "Loaded trope definitions (abstract filtered, IDs backfilled)");

    // Extract world description for narrator prompt context
    if let Some(world) = pack.worlds.get(world_slug) {
        let mut ctx = format!("World: {}", world.config.name);
        ctx.push_str(&format!("\n{}", world.config.description));
        if let Some(ref history) = world.lore.history {
            ctx.push_str(&format!(
                "\nHistory: {}",
                history.chars().take(200).collect::<String>()
            ));
        }
        if let Some(ref geography) = world.lore.geography {
            ctx.push_str(&format!(
                "\nGeography: {}",
                geography.chars().take(200).collect::<String>()
            ));
        }
        *world_context_out = ctx;
        tracing::info!(world = %world_slug, context_len = world_context_out.len(), "Loaded world context");
    }

    // Inject name bank context from cultures (prefer world-specific, fall back to genre-level)
    let cultures = pack
        .worlds
        .get(world_slug)
        .filter(|w| !w.cultures.is_empty())
        .map(|w| w.cultures.as_slice())
        .unwrap_or(&pack.cultures);
    let name_bank = build_name_bank_context(cultures);
    if !name_bank.is_empty() {
        world_context_out.push_str(&name_bank);
    }

    // Filter scenes to those with non-empty choices
    let scenes: Vec<_> = pack
        .char_creation
        .into_iter()
        .filter(|s| !s.choices.is_empty())
        .collect();

    if scenes.is_empty() {
        tracing::warn!(genre = %genre, "No character creation scenes with choices");
        return None;
    }

    let b = match CharacterBuilder::try_new(scenes, &pack.rules) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(error = ?e, "Failed to create CharacterBuilder");
            return None;
        }
    };

    let scene_msg = b.to_scene_message(player_id);
    *builder = Some(b);
    Some(scene_msg)
}

/// Handle CHARACTER_CREATION messages (client choices).
#[allow(clippy::too_many_arguments)]
async fn dispatch_character_creation(
    payload: &CharacterCreationPayload,
    session: &mut Session,
    builder: &mut Option<CharacterBuilder>,
    player_name_store: &mut Option<String>,
    character_json_store: &mut Option<serde_json::Value>,
    character_name_store: &mut Option<String>,
    character_hp: &mut i32,
    character_max_hp: &mut i32,
    character_level: &mut u32,
    character_xp: &mut u32,
    current_location: &mut String,
    inventory: &mut sidequest_game::Inventory,
    combat_state: &mut sidequest_game::combat::CombatState,
    chase_state: &mut Option<sidequest_game::ChaseState>,
    trope_states: &mut Vec<sidequest_game::trope::TropeState>,
    trope_defs: &mut Vec<sidequest_genre::TropeDefinition>,
    world_context: &str,
    axes_config: &Option<sidequest_genre::AxesConfig>,
    axis_values: &mut Vec<sidequest_game::axis::AxisValue>,
    visual_style: &Option<sidequest_genre::VisualStyle>,
    npc_registry: &mut Vec<NpcRegistryEntry>,
    quest_log: &mut std::collections::HashMap<String, String>,
    narration_history: &mut Vec<String>,
    discovered_regions: &mut Vec<String>,
    turn_manager: &mut sidequest_game::TurnManager,
    lore_store: &mut sidequest_game::LoreStore,
    shared_session_holder: &Arc<
        tokio::sync::Mutex<Option<Arc<tokio::sync::Mutex<shared_session::SharedGameSession>>>>,
    >,
    music_director: &mut Option<sidequest_game::MusicDirector>,
    audio_mixer: &std::sync::Arc<tokio::sync::Mutex<Option<sidequest_game::AudioMixer>>>,
    prerender_scheduler: &std::sync::Arc<
        tokio::sync::Mutex<Option<sidequest_game::PrerenderScheduler>>,
    >,
    state: &AppState,
    player_id: &str,
    continuity_corrections: &mut String,
    genie_wishes: &mut Vec<sidequest_game::GenieWish>,
) -> Vec<GameMessage> {
    let b = match builder.as_mut() {
        Some(b) => b,
        None => return vec![error_response(player_id, "No character builder active")],
    };

    let phase = payload.phase.as_str();
    tracing::info!(phase = %phase, player_id = %player_id, "Character creation phase");

    match phase {
        "scene" => {
            // Parse choice (1-based string → 0-based index)
            let choice_str = payload.choice.as_deref().unwrap_or("1");
            let index = choice_str.parse::<usize>().unwrap_or(1).saturating_sub(1);

            state.send_watcher_event(WatcherEvent {
                timestamp: chrono::Utc::now(),
                component: "character_creation".to_string(),
                event_type: WatcherEventType::StateTransition,
                severity: Severity::Info,
                fields: {
                    let mut f = HashMap::new();
                    f.insert(
                        "phase".to_string(),
                        serde_json::Value::String(phase.to_string()),
                    );
                    f.insert("choice_index".to_string(), serde_json::json!(index));
                    f.insert(
                        "player_id".to_string(),
                        serde_json::Value::String(player_id.to_string()),
                    );
                    f
                },
            });

            if let Err(e) = b.apply_choice(index) {
                return vec![error_response(
                    player_id,
                    &format!("Invalid choice: {:?}", e),
                )];
            }

            // Send the next scene or confirmation
            vec![b.to_scene_message(player_id)]
        }
        "confirmation" => {
            // Build the character
            let pname = player_name_store.as_deref().unwrap_or("Player");
            match b.build(pname) {
                Ok(character) => {
                    let char_json = match serde_json::to_value(&character) {
                        Ok(json) => json,
                        Err(e) => {
                            tracing::error!(
                                error = %e,
                                char_name = %character.core.name,
                                "Failed to serialize built character — character_json will not be set"
                            );
                            // Return error to player rather than silently producing Null
                            return vec![error_response(
                                player_id,
                                "Character created but state sync failed — please reconnect",
                            )];
                        }
                    };

                    state.send_watcher_event(WatcherEvent {
                        timestamp: chrono::Utc::now(),
                        component: "character_creation".to_string(),
                        event_type: WatcherEventType::StateTransition,
                        severity: Severity::Info,
                        fields: {
                            let mut f = HashMap::new();
                            f.insert(
                                "event".to_string(),
                                serde_json::Value::String("character_built".to_string()),
                            );
                            f.insert(
                                "name".to_string(),
                                serde_json::Value::String(character.core.name.as_str().to_string()),
                            );
                            f.insert(
                                "class".to_string(),
                                serde_json::Value::String(
                                    character.char_class.as_str().to_string(),
                                ),
                            );
                            f.insert(
                                "race".to_string(),
                                serde_json::Value::String(character.race.as_str().to_string()),
                            );
                            f.insert("hp".to_string(), serde_json::json!(character.core.hp));
                            f
                        },
                    });

                    // Store character data — sync ALL mutable fields from the built character
                    *character_name_store = Some(character.core.name.as_str().to_string());
                    *character_hp = character.core.hp;
                    *character_max_hp = character.core.max_hp;
                    *inventory = character.core.inventory.clone();
                    *character_json_store = Some(char_json.clone());
                    tracing::info!(
                        char_name = %character.core.name,
                        hp = character.core.hp,
                        items = character.core.inventory.items.len(),
                        pronouns = %character.pronouns,
                        "chargen.complete — character built, inventory synced"
                    );

                    // Save to SQLite for reconnection across restarts (keyed by player)
                    let genre = session.genre_slug().unwrap_or("").to_string();
                    let world = session.world_slug().unwrap_or("").to_string();
                    let pname_for_save =
                        player_name_store.as_deref().unwrap_or("Player").to_string();
                    let snapshot = sidequest_game::GameSnapshot {
                        genre_slug: genre.clone(),
                        world_slug: world.clone(),
                        characters: vec![character.clone()],
                        location: "Starting area".to_string(),
                        ..Default::default()
                    };
                    if let Err(e) = state
                        .persistence()
                        .save(&genre, &world, &pname_for_save, &snapshot)
                        .await
                    {
                        tracing::warn!(error = %e, genre = %genre, world = %world, player = %pname_for_save, "Failed to persist initial session");
                    }

                    // Transition session to Playing
                    let _ = session.complete_character_creation();
                    *builder = None;

                    let complete = GameMessage::CharacterCreation {
                        payload: CharacterCreationPayload {
                            phase: "complete".to_string(),
                            scene_index: None,
                            total_scenes: None,
                            prompt: None,
                            summary: None,
                            message: None,
                            choices: None,
                            allows_freeform: None,
                            input_type: None,
                            character_preview: None,
                            choice: None,
                            character: Some(char_json),
                        },
                        player_id: player_id.to_string(),
                    };

                    let ready = GameMessage::SessionEvent {
                        payload: SessionEventPayload {
                            event: "ready".to_string(),
                            player_name: None,
                            genre: None,
                            world: None,
                            has_character: None,
                            initial_state: None,
                            css: None,
                        },
                        player_id: player_id.to_string(),
                    };

                    // Auto-trigger an introductory narration so the game view isn't empty
                    let intro_messages = dispatch::dispatch_player_action(&mut dispatch::DispatchContext {
                        action: "I look around and take in my surroundings.",
                        char_name: character.core.name.as_str(),
                        hp: character_hp,
                        max_hp: character_max_hp,
                        level: character_level,
                        xp: character_xp,
                        current_location,
                        inventory,
                        character_json: character_json_store,
                        combat_state,
                        chase_state,
                        trope_states,
                        trope_defs,
                        world_context,
                        axes_config,
                        axis_values,
                        visual_style,
                        npc_registry,
                        quest_log,
                        narration_history,
                        discovered_regions,
                        turn_manager,
                        lore_store,
                        shared_session_holder,
                        music_director,
                        audio_mixer,
                        prerender_scheduler,
                        state,
                        player_id,
                        genre_slug: &genre,
                        world_slug: &world,
                        player_name_for_save: &pname_for_save,
                        continuity_corrections,
                        genie_wishes,
                    })
                    .await;

                    // Emit CHARACTER_SHEET for the UI overlay
                    let char_sheet = GameMessage::CharacterSheet {
                        payload: CharacterSheetPayload {
                            name: character.core.name.as_str().to_string(),
                            class: character.char_class.as_str().to_string(),
                            race: character.race.as_str().to_string(),
                            level: character.core.level as u32,
                            stats: character
                                .stats
                                .iter()
                                .map(|(k, v)| (k.clone(), *v))
                                .collect(),
                            abilities: character.hooks.clone(),
                            backstory: character.backstory.as_str().to_string(),
                            personality: character.core.personality.as_str().to_string(),
                            pronouns: character.pronouns.clone(),
                            equipment: character.core.inventory.items.iter().map(|i| {
                                if i.equipped {
                                    format!("{} [equipped]", i.name)
                                } else {
                                    i.name.as_str().to_string()
                                }
                            }).collect(),
                            portrait_url: None,
                        },
                        player_id: player_id.to_string(),
                    };

                    // Emit the character's backstory as a prose narration so
                    // it appears in the narrative view — not just in the overlay.
                    let backstory_narration = GameMessage::Narration {
                        payload: NarrationPayload {
                            text: character.backstory.as_str().to_string(),
                            state_delta: None,
                            footnotes: vec![],
                        },
                        player_id: player_id.to_string(),
                    };
                    let backstory_end = GameMessage::NarrationEnd {
                        payload: NarrationEndPayload { state_delta: None },
                        player_id: player_id.to_string(),
                    };

                    // Add player to shared session and broadcast PARTY_STATUS
                    {
                        let holder = shared_session_holder.lock().await;
                        if let Some(ref ss_arc) = *holder {
                            let mut ss = ss_arc.lock().await;
                            let ps = shared_session::PlayerState::new(
                                player_name_store
                                    .clone()
                                    .unwrap_or_else(|| "Player".to_string()),
                            );
                            ss.players.insert(player_id.to_string(), ps);
                            // Populate character data on the PlayerState
                            if let Some(p) = ss.players.get_mut(player_id) {
                                p.character_name = Some(character.core.name.as_str().to_string());
                                p.character_hp = character.core.hp;
                                p.character_max_hp = character.core.max_hp;
                                p.character_level = character.core.level as u32;
                                p.character_class = character.char_class.as_str().to_string();
                            }
                            // Notify existing players that a new character has arrived
                            let arrival_text = format!(
                                "{} has entered the scene.",
                                character.core.name.as_str()
                            );
                            let existing_pids: Vec<String> = ss
                                .players
                                .keys()
                                .filter(|pid| pid.as_str() != player_id)
                                .cloned()
                                .collect();
                            for target_pid in &existing_pids {
                                ss.send_to_player(
                                    GameMessage::Narration {
                                        payload: NarrationPayload {
                                            text: arrival_text.clone(),
                                            state_delta: None,
                                            footnotes: vec![],
                                        },
                                        player_id: target_pid.clone(),
                                    },
                                    target_pid.clone(),
                                );
                                ss.send_to_player(
                                    GameMessage::NarrationEnd {
                                        payload: NarrationEndPayload { state_delta: None },
                                        player_id: target_pid.clone(),
                                    },
                                    target_pid.clone(),
                                );
                            }
                            // Build and send targeted PARTY_STATUS to each session member
                            // Each player gets their own player_id so the client HUD
                            // shows the correct identity.
                            let members: Vec<PartyMember> = ss
                                .players
                                .iter()
                                .map(|(pid, ps)| {
                                    if pid == player_id {
                                        // Current player — use local character data
                                        PartyMember {
                                            player_id: pid.clone(),
                                            name: ps.player_name.clone(),
                                            character_name: character.core.name.as_str().to_string(),
                                            current_hp: character.core.hp,
                                            max_hp: character.core.max_hp,
                                            statuses: character.core.statuses.clone(),
                                            class: character.char_class.as_str().to_string(),
                                            level: character.core.level as u32,
                                            portrait_url: None,
                                        }
                                    } else {
                                        // Other player — use PlayerState fields
                                        PartyMember {
                                            player_id: pid.clone(),
                                            name: ps.player_name.clone(),
                                            character_name: ps.character_name.clone().unwrap_or_else(|| ps.player_name.clone()),
                                            current_hp: ps.character_hp,
                                            max_hp: ps.character_max_hp,
                                            statuses: vec![],
                                            class: ps.character_class.clone(),
                                            level: ps.character_level,
                                            portrait_url: None,
                                        }
                                    }
                                })
                                .collect();
                            if !members.is_empty() {
                                let player_ids: Vec<String> = ss.players.keys().cloned().collect();
                                for target_pid in &player_ids {
                                    let party_msg = GameMessage::PartyStatus {
                                        payload: PartyStatusPayload { members: members.clone() },
                                        player_id: target_pid.clone(),
                                    };
                                    ss.send_to_player(party_msg, target_pid.clone());
                                }
                            }
                            let pc = ss.player_count();
                            tracing::info!(
                                player_id = %player_id,
                                player_count = pc,
                                "Player joined shared session"
                            );
                            state.send_watcher_event(WatcherEvent {
                                timestamp: chrono::Utc::now(),
                                component: "multiplayer".to_string(),
                                event_type: WatcherEventType::StateTransition,
                                severity: Severity::Info,
                                fields: {
                                    let mut f = HashMap::new();
                                    f.insert(
                                        "event".to_string(),
                                        serde_json::json!("session_joined"),
                                    );
                                    f.insert(
                                        "session_key".to_string(),
                                        serde_json::json!(format!("{}:{}", genre, world)),
                                    );
                                    f.insert("player_count".to_string(), serde_json::json!(pc));
                                    f
                                },
                            });

                            // Transition turn mode when a player joins
                            let old_mode = std::mem::take(&mut ss.turn_mode);
                            ss.turn_mode = old_mode.apply(
                                sidequest_game::turn_mode::TurnModeTransition::PlayerJoined {
                                    player_count: pc,
                                },
                            );
                            tracing::info!(
                                new_mode = ?ss.turn_mode,
                                player_count = pc,
                                "Turn mode transitioned on player join"
                            );
                            // Initialize or expand barrier if in structured mode
                            if ss.turn_mode.should_use_barrier() {
                                if let Some(ref barrier) = ss.turn_barrier {
                                    // Add player to existing barrier roster
                                    let placeholder_char = {
                                        use sidequest_game::character::Character;
                                        use sidequest_game::creature_core::CreatureCore;
                                        use sidequest_game::inventory::Inventory;
                                        use sidequest_protocol::NonBlankString;
                                        Character {
                                            core: CreatureCore {
                                                name: NonBlankString::new(player_id).unwrap(),
                                                description: NonBlankString::new("barrier placeholder").unwrap(),
                                                personality: NonBlankString::new("n/a").unwrap(),
                                                level: 1, hp: 1, max_hp: 1, ac: 10, xp: 0,
                                                statuses: vec![],
                                                inventory: Inventory::default(),
                                            },
                                            backstory: NonBlankString::new("n/a").unwrap(),
                                            narrative_state: String::new(),
                                            hooks: vec![],
                                            char_class: NonBlankString::new("barrier").unwrap(),
                                            race: NonBlankString::new("barrier").unwrap(),
                                            pronouns: String::new(),
                                            stats: HashMap::new(),
                                            abilities: vec![],
                                            known_facts: vec![],
                                            affinities: vec![],
                                            is_friendly: true,
                                        }
                                    };
                                    match barrier.add_player(player_id.to_string(), placeholder_char) {
                                        Ok(count) => {
                                            tracing::info!(
                                                player_id = %player_id,
                                                barrier_count = count,
                                                "Added player to existing barrier"
                                            );
                                        }
                                        Err(e) => {
                                            tracing::error!(
                                                player_id = %player_id,
                                                error = %e,
                                                "Failed to add player to barrier — player will not participate in turn collection"
                                            );
                                            // Send error to player so they know their actions won't count
                                            ss.send_to_player(
                                                error_response(player_id, &format!("Failed to join turn barrier: {e}")),
                                                player_id.to_string(),
                                            );
                                        }
                                    }
                                } else {
                                    let mp_session = sidequest_game::multiplayer::MultiplayerSession::with_player_ids(
                                        ss.players.keys().cloned(),
                                    );
                                    let adaptive = sidequest_game::barrier::AdaptiveTimeout::default();
                                    ss.turn_barrier = Some(sidequest_game::barrier::TurnBarrier::with_adaptive(
                                        mp_session, adaptive,
                                    ));
                                    tracing::info!(player_count = pc, "Initialized turn barrier for multiplayer");
                                }
                            }
                        }
                    }

                    let mut msgs = vec![
                        complete,
                        char_sheet,
                        backstory_narration,
                        backstory_end,
                        ready,
                    ];
                    msgs.extend(intro_messages);
                    msgs
                }
                Err(e) => vec![error_response(
                    player_id,
                    &format!("Failed to build character: {:?}", e),
                )],
            }
        }
        _ => vec![error_response(
            player_id,
            &format!("Unexpected creation phase: {}", phase),
        )],
    }
}

/// DaemonSynthesizer — implements TtsSynthesizer for the real daemon client.
pub(crate) struct DaemonSynthesizer {
    client: tokio::sync::Mutex<sidequest_daemon_client::DaemonClient>,
}

impl sidequest_game::tts_stream::TtsSynthesizer for DaemonSynthesizer {
    fn synthesize(
        &self,
        text: &str,
        _speaker: &str,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<Vec<u8>, sidequest_game::tts_stream::TtsError>>
                + Send
                + '_,
        >,
    > {
        let text = text.to_string();
        Box::pin(async move {
            let params = sidequest_daemon_client::TtsParams {
                text,
                model: "kokoro".to_string(),
                voice_id: "en_male_deep".to_string(),
                speed: 0.95,
                ..Default::default()
            };
            let mut client = self.client.lock().await;
            match client.synthesize(params).await {
                Ok(result) => Ok(result.audio_bytes),
                Err(e) => Err(sidequest_game::tts_stream::TtsError::SynthesisFailed(
                    e.to_string(),
                )),
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Server lifecycle
// ---------------------------------------------------------------------------

/// Create and run the server on a given port with a shutdown signal.
pub async fn create_server(
    state: AppState,
    port: u16,
    shutdown: oneshot::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port)).await?;
    tracing::info!(port = %listener.local_addr()?, "SideQuest Server listening");
    serve_with_listener(state, listener, shutdown).await
}

/// Run the server with a pre-bound listener and shutdown signal.
pub async fn serve_with_listener(
    state: AppState,
    listener: tokio::net::TcpListener,
    shutdown: oneshot::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let app = build_router(state);
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            shutdown.await.ok();
            tracing::info!("Shutdown signal received");
        })
        .await?;
    tracing::info!("Server shut down cleanly");
    Ok(())
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Create an AppState suitable for testing.
///
/// Uses a default Orchestrator and a temp path for genre packs.
pub fn test_app_state() -> AppState {
    use sidequest_agents::orchestrator::Orchestrator;
    use sidequest_agents::turn_record::{TurnRecord, WATCHER_CHANNEL_CAPACITY};

    // Use the real genre_packs path if available, otherwise a temp path
    let genre_packs_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent() // crates/
        .and_then(|p| p.parent()) // sidequest-api/
        .and_then(|p| p.parent()) // oq-1/ (orchestrator root)
        .map(|p| p.join("genre_packs"))
        .unwrap_or_else(|| PathBuf::from("/tmp/test-genre-packs"));

    let (watcher_tx, _watcher_rx) =
        tokio::sync::mpsc::channel::<TurnRecord>(WATCHER_CHANNEL_CAPACITY);
    // Use a unique temp directory per test_app_state call to avoid cross-test contamination
    let save_dir = std::env::temp_dir()
        .join("sidequest-test-saves")
        .join(format!("{}-{}", std::process::id(), uuid::Uuid::new_v4()));
    AppState::new_with_game_service(
        Box::new(Orchestrator::new(watcher_tx)),
        genre_packs_path,
        save_dir,
    )
}

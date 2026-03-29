//! SideQuest Server — axum HTTP/WebSocket server library.
//!
//! Exposes `build_router()`, `AppState`, and server lifecycle functions for the binary and tests.
//! The server depends on the `GameService` trait facade — never on game internals.

pub mod render_integration;
pub mod shared_session;

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

use tracing_subscriber::prelude::*;
use tracing_subscriber::{EnvFilter, Registry};

use sidequest_agents::orchestrator::{GameService, TurnContext};
use sidequest_game::builder::CharacterBuilder;
use sidequest_genre::{GenreCache, GenreCode, GenreLoader};
use sidequest_protocol::{
    AudioCuePayload, ChapterMarkerPayload, CharacterCreationPayload, CharacterSheetPayload,
    CharacterState, CombatEventPayload, ErrorPayload, GameMessage, InitialState, InventoryPayload,
    MapUpdatePayload, NarrationEndPayload, NarrationPayload, PartyMember, PartyStatusPayload,
    SessionEventPayload, ThinkingPayload, TurnStatusPayload,
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

// ---------------------------------------------------------------------------
// Tracing / Telemetry (Story 3-1)
// ---------------------------------------------------------------------------

/// Initialize the composable tracing subscriber stack.
///
/// Uses Registry + layers instead of the bare `tracing_subscriber::fmt::init()`.
/// Layers:
/// - EnvFilter: respects RUST_LOG (default: `sidequest=debug,tower_http=info`)
/// - JSON layer: structured output for production (always active)
/// - Pretty layer: human-readable output in debug builds only
pub fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("sidequest=debug,tower_http=info"));

    let json_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_target(true)
        .with_current_span(true);

    let pretty_layer = if cfg!(debug_assertions) {
        Some(tracing_subscriber::fmt::layer().pretty())
    } else {
        None
    };

    Registry::default()
        .with(env_filter)
        .with(json_layer)
        .with(pretty_layer)
        .init();
}

/// Wrapper for writing to a shared buffer (used in tests).
struct SharedWriter(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for SharedWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Build a tracing subscriber that writes JSON to a shared buffer (for tests).
pub fn tracing_subscriber_for_test(
    writer: Arc<Mutex<Vec<u8>>>,
) -> Box<dyn tracing::Subscriber + Send + Sync> {
    let json_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_target(true)
        .with_current_span(true)
        .with_writer(move || SharedWriter(writer.clone()));

    Box::new(Registry::default().with(json_layer))
}

/// Build a subscriber with a custom EnvFilter string.
/// Returns `Some(subscriber)` if the filter parses, `None` otherwise.
pub fn build_subscriber_with_filter(
    filter: &str,
) -> Option<Box<dyn tracing::Subscriber + Send + Sync>> {
    let env_filter = EnvFilter::try_new(filter).ok()?;
    let json_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_target(true)
        .with_current_span(true);

    Some(Box::new(
        Registry::default().with(env_filter).with(json_layer),
    ))
}

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
// Session key helper
// ---------------------------------------------------------------------------

fn session_key(player_name: &str, genre: &str, world: &str) -> String {
    format!("{}:{}:{}", player_name, genre, world)
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
        let (broadcast_tx, _) = broadcast::channel(256);
        let (watcher_tx, _) = broadcast::channel(256);
        let (binary_broadcast_tx, _) = broadcast::channel(64);

        // Render pipeline — daemon client connects lazily on first render
        let render_queue = sidequest_game::RenderQueue::spawn(
            sidequest_game::RenderQueueConfig::default(),
            |prompt, art_style, tier| async move {
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
                                ..Default::default()
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
    pub fn remove_player_from_session(&self, genre: &str, world: &str, player_id: &str) -> usize {
        let key = shared_session::game_session_key(genre, world);
        let mut sessions = self.inner.sessions.lock().unwrap();
        let remaining = if let Some(session_arc) = sessions.get(&key).cloned() {
            if let Ok(mut session) = session_arc.try_lock() {
                session.players.remove(player_id);
                // Remove player from barrier roster if active
                if let Some(ref barrier) = session.turn_barrier {
                    let _ = barrier.remove_player(player_id);
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
            } else {
                return 1; // Couldn't lock — conservatively report not empty
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

    /// Check if a player is currently processing an action.
    fn is_processing(&self, player_id: &PlayerId) -> bool {
        self.inner.processing.lock().unwrap().contains(player_id)
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
        .route("/ws/watcher", get(ws_watcher_handler))
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
        {
            let sessions = state.inner.sessions.lock().unwrap();
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
                world_context,
                axes_config,
                axis_values,
                visual_style,
                music_director,
                audio_mixer,
                prerender_scheduler,
                turn_manager,
                npc_registry,
                trope_states,
                quest_log,
                lore_store,
                state,
                player_id,
                continuity_corrections,
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
            dispatch_player_action(
                &payload.action,
                character_name.as_deref().unwrap_or("Unknown"),
                character_hp,
                character_max_hp,
                character_level,
                character_xp,
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
                session.genre_slug().unwrap_or(""),
                session.world_slug().unwrap_or(""),
                player_name_store.as_deref().unwrap_or("Player"),
                continuity_corrections,
            )
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
    trope_states: &mut Vec<sidequest_game::trope::TropeState>,
    quest_log: &mut std::collections::HashMap<String, String>,
    lore_store: &mut sidequest_game::LoreStore,
    state: &AppState,
    player_id: &str,
    continuity_corrections: &mut String,
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
                            *character_json_store =
                                Some(serde_json::to_value(character).unwrap_or_default());
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
                        *axis_values = saved.snapshot.axis_values.clone();
                        // Restore trope progression state from snapshot
                        *trope_states = saved.snapshot.active_tropes.clone();
                        // Restore quest log from snapshot
                        *quest_log = saved.snapshot.quest_log.clone();
                        tracing::info!(
                            trope_count = trope_states.len(),
                            quest_count = quest_log.len(),
                            "reconnect.state_restored — tropes and quests loaded from save"
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
                                    turn_count: saved.snapshot.turn_manager.round(),
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

            // Send theme_css SESSION_EVENT if the genre pack has a client_theme.css
            let css_path = state
                .genre_packs_path()
                .join(genre)
                .join("client_theme.css");
            if let Ok(css) = tokio::fs::read_to_string(&css_path).await {
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
                    let char_json = serde_json::to_value(&character).unwrap_or_default();

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
                    let intro_messages = dispatch_player_action(
                        "I look around and take in my surroundings.",
                        character.core.name.as_str(),
                        character_hp,
                        character_max_hp,
                        character_level,
                        character_xp,
                        current_location,
                        inventory,
                        character_json_store,
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
                        &genre,
                        &world,
                        &pname_for_save,
                        continuity_corrections,
                    )
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
                                    let _ = barrier.add_player(player_id.to_string(), placeholder_char);
                                    tracing::info!(player_id = %player_id, "Added player to existing barrier");
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

/// Handle PLAYER_ACTION — send THINKING, narration, NARRATION_END, PARTY_STATUS.
#[allow(clippy::too_many_arguments)]
async fn dispatch_player_action(
    action: &str,
    char_name: &str,
    hp: &mut i32,
    max_hp: &mut i32,
    level: &mut u32,
    xp: &mut u32,
    current_location: &mut String,
    inventory: &mut sidequest_game::Inventory,
    character_json: &mut Option<serde_json::Value>,
    combat_state: &mut sidequest_game::combat::CombatState,
    chase_state: &mut Option<sidequest_game::ChaseState>,
    trope_states: &mut Vec<sidequest_game::trope::TropeState>,
    trope_defs: &[sidequest_genre::TropeDefinition],
    world_context: &str,
    axes_config: &Option<sidequest_genre::AxesConfig>,
    axis_values: &mut Vec<sidequest_game::axis::AxisValue>,
    visual_style: &Option<sidequest_genre::VisualStyle>,
    npc_registry: &mut Vec<NpcRegistryEntry>,
    quest_log: &mut std::collections::HashMap<String, String>,
    narration_history: &mut Vec<String>,
    discovered_regions: &mut Vec<String>,
    turn_manager: &mut sidequest_game::TurnManager,
    lore_store: &sidequest_game::LoreStore,
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
    genre_slug: &str,
    world_slug: &str,
    player_name_for_save: &str,
    continuity_corrections: &mut String,
) -> Vec<GameMessage> {
    // Sync world-level state from shared session (if multiplayer)
    {
        let holder = shared_session_holder.lock().await;
        if let Some(ref ss_arc) = *holder {
            let ss = ss_arc.lock().await;
            ss.sync_to_locals(
                current_location,
                npc_registry,
                narration_history,
                discovered_regions,
                trope_states,
            );
            // Sync per-player state from barrier modifications (HP, inventory, combat, etc.)
            ss.sync_player_to_locals(
                player_id,
                hp,
                max_hp,
                level,
                xp,
                inventory,
                combat_state,
                chase_state,
                character_json,
            );
            let pc = ss.player_count();
            if pc > 1 {
                state.send_watcher_event(WatcherEvent {
                    timestamp: chrono::Utc::now(),
                    component: "multiplayer".to_string(),
                    event_type: WatcherEventType::AgentSpanOpen,
                    severity: Severity::Info,
                    fields: {
                        let mut f = HashMap::new();
                        f.insert("event".to_string(), serde_json::json!("multiplayer_action"));
                        f.insert(
                            "session_key".to_string(),
                            serde_json::json!(format!("{}:{}", genre_slug, world_slug)),
                        );
                        f.insert("player_id".to_string(), serde_json::json!(player_id));
                        f.insert("party_size".to_string(), serde_json::json!(pc));
                        f
                    },
                });
            }
        }
    }

    // Watcher: action received
    let turn_number = turn_manager.interaction();
    state.send_watcher_event(WatcherEvent {
        timestamp: chrono::Utc::now(),
        component: "game".to_string(),
        event_type: WatcherEventType::AgentSpanOpen,
        severity: Severity::Info,
        fields: {
            let mut f = HashMap::new();
            f.insert(
                "action".to_string(),
                serde_json::Value::String(action.to_string()),
            );
            f.insert(
                "player".to_string(),
                serde_json::Value::String(char_name.to_string()),
            );
            f.insert("turn_number".to_string(), serde_json::json!(turn_number));
            f
        },
    });

    // TURN_STATUS "active" — tell all players whose turn it is BEFORE the LLM call.
    // Sent via GLOBAL broadcast (not session channel) because the session channel
    // subscriber may not be initialized yet — broadcast::channel drops messages
    // sent before subscription. Global broadcast reaches all connected clients.
    {
        let holder = shared_session_holder.lock().await;
        if let Some(ref ss_arc) = *holder {
            let ss = ss_arc.lock().await;
            if ss.players.len() > 1 {
                let turn_active = GameMessage::TurnStatus {
                    payload: TurnStatusPayload {
                        player_name: player_name_for_save.to_string(),
                        status: "active".into(),
                        state_delta: None,
                    },
                    player_id: player_id.to_string(),
                };
                let _ = state.broadcast(turn_active);
                tracing::info!(player_id = %player_id, player_name = %player_name_for_save, "turn_status.active broadcast to all clients");
            }
        }
    }

    // THINKING indicator — send eagerly BEFORE LLM call so UI shows it immediately.
    // Send only to the acting player via session channel (not global broadcast)
    // so that other players' input is not blocked by the "narrator thinking" lock.
    let thinking = GameMessage::Thinking {
        payload: ThinkingPayload {},
        player_id: player_id.to_string(),
    };
    tracing::info!(player_id = %player_id, "thinking.sent");
    {
        let holder = shared_session_holder.lock().await;
        if let Some(ref ss_arc) = *holder {
            let ss = ss_arc.lock().await;
            ss.send_to_player(thinking.clone(), player_id.to_string());
        } else {
            // Single-player fallback: use global broadcast
            let _ = state.broadcast(thinking.clone());
        }
    }

    // Slash command interception — route /commands to mechanical handlers, not the LLM.
    if action.starts_with('/') {
        use sidequest_game::commands::{
            GmCommand, InventoryCommand, MapCommand, QuestsCommand, SaveCommand, StatusCommand,
        };
        use sidequest_game::slash_router::SlashRouter;
        use sidequest_game::state::GameSnapshot;

        let mut router = SlashRouter::new();
        router.register(Box::new(StatusCommand));
        router.register(Box::new(InventoryCommand));
        router.register(Box::new(MapCommand));
        router.register(Box::new(QuestsCommand));
        router.register(Box::new(SaveCommand));
        router.register(Box::new(GmCommand));
        if let Some(ref ac) = axes_config {
            router.register(Box::new(sidequest_game::ToneCommand::new(ac.clone())));
        }

        // Build a minimal GameSnapshot from the local session state.
        let snapshot = {
            let mut snap = GameSnapshot {
                genre_slug: genre_slug.to_string(),
                world_slug: world_slug.to_string(),
                location: current_location.clone(),
                combat: combat_state.clone(),
                chase: chase_state.clone(),
                axis_values: axis_values.clone(),
                active_tropes: trope_states.clone(),
                quest_log: quest_log.clone(),
                ..GameSnapshot::default()
            };
            // Reconstruct a minimal Character from loose variables.
            if let Some(ref cj) = character_json {
                if let Ok(mut ch) = serde_json::from_value::<sidequest_game::Character>(cj.clone())
                {
                    // Sync mutable fields that may have diverged from the JSON snapshot.
                    ch.core.hp = *hp;
                    ch.core.max_hp = *max_hp;
                    ch.core.level = *level;
                    ch.core.inventory = inventory.clone();
                    snap.characters.push(ch);
                }
            }
            snap
        };

        if let Some(cmd_result) = router.try_dispatch(action, &snapshot) {
            tracing::info!(command = %action, result_type = ?std::mem::discriminant(&cmd_result), "slash_command.dispatched");
            let text = match &cmd_result {
                sidequest_game::slash_router::CommandResult::Display(t) => t.clone(),
                sidequest_game::slash_router::CommandResult::Error(e) => e.clone(),
                sidequest_game::slash_router::CommandResult::StateMutation(patch) => {
                    // Apply location/region changes from /gm commands.
                    if let Some(ref loc) = patch.location {
                        *current_location = loc.clone();
                    }
                    if let Some(ref hp_changes) = patch.hp_changes {
                        for (_target, delta) in hp_changes {
                            *hp = (*hp + delta).max(0);
                        }
                    }
                    format!("GM command applied.")
                }
                sidequest_game::slash_router::CommandResult::ToneChange(new_values) => {
                    *axis_values = new_values.clone();
                    format!("Tone updated.")
                }
                _ => "Command executed.".to_string(),
            };

            // Watcher: slash command handled
            state.send_watcher_event(WatcherEvent {
                timestamp: chrono::Utc::now(),
                component: "game".to_string(),
                event_type: WatcherEventType::AgentSpanClose,
                severity: Severity::Info,
                fields: {
                    let mut f = HashMap::new();
                    f.insert(
                        "slash_command".to_string(),
                        serde_json::Value::String(action.to_string()),
                    );
                    f.insert("result_len".to_string(), serde_json::json!(text.len()));
                    f
                },
            });

            return vec![
                GameMessage::Narration {
                    payload: NarrationPayload {
                        text,
                        state_delta: None,
                        footnotes: vec![],
                    },
                    player_id: player_id.to_string(),
                },
                GameMessage::NarrationEnd {
                    payload: NarrationEndPayload { state_delta: None },
                    player_id: player_id.to_string(),
                },
            ];
        }
    }

    // Seed starter tropes if none are active yet (first turn)
    if trope_states.is_empty() && !trope_defs.is_empty() {
        // Prefer tropes with passive_progression so tick() can advance them.
        // Fall back to any trope if none have passive_progression.
        let mut seedable: Vec<&sidequest_genre::TropeDefinition> = trope_defs
            .iter()
            .filter(|d| d.passive_progression.is_some() && d.id.is_some())
            .collect();
        if seedable.is_empty() {
            seedable = trope_defs.iter().filter(|d| d.id.is_some()).collect();
        }
        let seed_count = seedable.len().min(3);
        tracing::info!(
            total_defs = trope_defs.len(),
            with_progression = trope_defs
                .iter()
                .filter(|d| d.passive_progression.is_some())
                .count(),
            seedable = seedable.len(),
            seed_count = seed_count,
            "Trope seeding — selecting starter tropes"
        );
        for def in &seedable[..seed_count] {
            if let Some(id) = &def.id {
                sidequest_game::trope::TropeEngine::activate(trope_states, id);
                tracing::info!(
                    trope_id = %id,
                    name = %def.name,
                    has_progression = def.passive_progression.is_some(),
                    "Seeded starter trope"
                );
                state.send_watcher_event(WatcherEvent {
                    timestamp: chrono::Utc::now(),
                    component: "trope".to_string(),
                    event_type: WatcherEventType::StateTransition,
                    severity: Severity::Info,
                    fields: {
                        let mut f = HashMap::new();
                        f.insert(
                            "event".to_string(),
                            serde_json::Value::String("trope_activated".to_string()),
                        );
                        f.insert(
                            "trope_id".to_string(),
                            serde_json::Value::String(id.clone()),
                        );
                        f
                    },
                });
            }
        }
    }

    // Build active trope context for the narrator prompt
    let trope_context = if trope_states.is_empty() {
        String::new()
    } else {
        let mut lines = vec!["Active narrative arcs:".to_string()];
        for ts in trope_states.iter() {
            if let Some(def) = trope_defs
                .iter()
                .find(|d| d.id.as_deref() == Some(ts.trope_definition_id()))
            {
                lines.push(format!(
                    "- {} ({}% progressed): {}",
                    def.name,
                    (ts.progression() * 100.0) as u32,
                    def.description
                        .as_deref()
                        .unwrap_or("")
                        .chars()
                        .take(120)
                        .collect::<String>(),
                ));
                // Include the next unfired escalation beat as a hint
                for beat in &def.escalation {
                    if beat.at > ts.progression() {
                        lines.push(format!(
                            "  → Next beat at {}%: {}",
                            (beat.at * 100.0) as u32,
                            beat.event.chars().take(80).collect::<String>()
                        ));
                        break;
                    }
                }
            }
        }
        lines.join("\n")
    };

    // Build state summary for grounding narration (Bug 1: include location + entities)
    let mut state_summary = format!(
        "Character: {} (HP {}/{}, Level {}, XP {})\nGenre: {}",
        char_name, *hp, *max_hp, *level, *xp, genre_slug,
    );

    // Inject party roster so the narrator knows which characters are player-controlled
    // and never puppets them (gives them dialogue, actions, or internal state).
    {
        let holder = shared_session_holder.lock().await;
        if let Some(ref ss_arc) = *holder {
            let ss = ss_arc.lock().await;
            let other_pcs: Vec<String> = ss
                .players
                .iter()
                .filter(|(pid, _)| pid.as_str() != player_id)
                .filter_map(|(_, ps)| ps.character_name.clone())
                .collect();
            // Check co-location: which other PCs are in the same region?
            let co_located_names: Vec<String> = ss
                .co_located_players(player_id)
                .iter()
                .filter_map(|pid| ss.players.get(pid.as_str()).and_then(|ps| ps.character_name.clone()))
                .collect();

            if !other_pcs.is_empty() {
                state_summary.push_str("\n\nPLAYER-CONTROLLED CHARACTERS IN THE PARTY:\n");
                state_summary.push_str("The following characters are controlled by OTHER human players:\n");
                for name in &other_pcs {
                    state_summary.push_str(&format!("- {}\n", name));
                }
                if !co_located_names.is_empty() {
                    state_summary.push_str(&format!(
                        "\nCO-LOCATION — HARD RULE: The following party members are RIGHT HERE with the acting player: {}. \
                         They are physically present at the SAME location. The narrator MUST acknowledge their presence \
                         in the scene. Do NOT narrate them as being elsewhere or arriving from somewhere else. \
                         They are already here.\n",
                        co_located_names.join(", ")
                    ));
                }
                state_summary.push_str(concat!(
                    "\n\nPLAYER AGENCY — ABSOLUTE RULE (violations break the game):\n",
                    "You MUST NOT write dialogue, actions, thoughts, feelings, gestures, or internal ",
                    "state for ANY player character — including the acting player beyond their stated action.\n",
                    "FORBIDDEN examples:\n",
                    "- \"Laverne holds up their power glove. 'I've got the strong hand covered.'\" (writing dialogue FOR a player)\n",
                    "- \"Shirley nudges Laverne with an elbow\" (scripting PC-to-PC physical interaction)\n",
                    "- \"Kael's heart races as he...\" (writing internal state for a player)\n",
                    "ALLOWED examples:\n",
                    "- \"Laverne is nearby, power glove faintly humming.\" (describing presence without action)\n",
                    "- \"The other party members are within earshot.\" (acknowledging presence)\n",
                    "Players control their OWN characters. You control the WORLD, NPCs, and narration only.",
                ));
                state_summary.push_str(
                    "\n\nPERSPECTIVE MODE: Third-person omniscient. \
                     You are narrating for multiple players simultaneously. \
                     Do NOT use 'you' for any character — including the acting player. \
                     All characters are named explicitly in third-person. \
                     Correct: 'Mira surveys the gantry. Kael moves to cover.' \
                     Wrong: 'You survey the gantry.'"
                );
            }
        }
    }

    // Location constraint — prevent narrator from teleporting between scenes
    if !current_location.is_empty() {
        // Dialogue context: if the player interacted with an NPC in the last 2 turns,
        // any location mention in the action is likely dialogue (describing a place to
        // the NPC), not a travel intent. Strengthen the stay-put constraint.
        let turn_approx = turn_manager.interaction() as u32;
        let recent_npc_interaction = npc_registry
            .iter()
            .any(|e| turn_approx.saturating_sub(e.last_seen_turn) <= 2);
        let extra_dialogue_guard = if recent_npc_interaction {
            " IMPORTANT: The player is currently in dialogue with an NPC. If the player's \
             action mentions a location or place name, they are TALKING ABOUT that place, \
             NOT traveling there. Keep the scene at the current location. Only move if the \
             player explicitly ends the conversation and states they are leaving."
        } else {
            ""
        };
        state_summary.push_str(&format!(
            "\n\nLOCATION CONSTRAINT — THIS IS A HARD RULE:\nThe player is at: {}\nYou MUST continue the scene at this location. Do NOT introduce a new setting, move to a different area, or describe the player arriving somewhere else UNLESS the player explicitly says they want to travel or leave. If the player's action implies staying here, describe what happens HERE. Only change location when the player takes a deliberate travel action (e.g., 'I go to...', 'I leave...', 'I head north').{}",
            current_location, extra_dialogue_guard
        ));
    }

    // Inventory constraint — the narrator must respect the character sheet
    let equipped_count = inventory.items.iter().filter(|i| i.equipped).count();
    tracing::debug!(
        items = inventory.items.len(),
        equipped = equipped_count,
        gold = inventory.gold,
        "narrator_prompt.inventory_constraint — injecting character sheet"
    );
    state_summary.push_str("\n\nCHARACTER SHEET — INVENTORY (canonical, overrides narration):");
    if !inventory.items.is_empty() {
        state_summary.push_str("\nThe player currently possesses EXACTLY these items:");
        for item in &inventory.items {
            let equipped_tag = if item.equipped { " [EQUIPPED]" } else { "" };
            let qty_tag = if item.quantity > 1 {
                format!(" (x{})", item.quantity)
            } else {
                String::new()
            };
            state_summary.push_str(&format!(
                "\n- {}{}{} — {} ({})",
                item.name, equipped_tag, qty_tag, item.description, item.category
            ));
        }
        state_summary.push_str(&format!("\nGold: {}", inventory.gold));
        state_summary.push_str(concat!(
            "\n\nINVENTORY RULES (HARD CONSTRAINTS — violations break the game):",
            "\n1. If the player uses an item on this list, it WORKS. The item is real and present.",
            "\n2. If the player uses an item NOT on this list, it FAILS — they don't have it.",
            "\n3. NEVER narrate an item being lost, stolen, broken, or missing unless the game",
            "\n   engine explicitly removes it. The inventory list above is the TRUTH.",
            "\n4. [EQUIPPED] items are currently in hand/worn — the player does not need to 'find'",
            "\n   or 'reach for' them. They are ready to use immediately.",
        ));
    } else {
        state_summary.push_str("\nThe player has NO items. If the player claims to use any item, the narrator MUST reject it — they have nothing in their possession yet.");
    }

    // Quest log — inject active quests so narrator can reference them
    if !quest_log.is_empty() {
        state_summary.push_str("\n\nACTIVE QUESTS:\n");
        for (quest_name, status) in quest_log.iter() {
            state_summary.push_str(&format!("- {}: {}\n", quest_name, status));
        }
        state_summary.push_str("Reference active quests when narratively relevant. Update quest status in quest_updates when objectives change.\n");
    }

    // Bug 6: Include chase state if active
    if let Some(ref cs) = chase_state {
        state_summary.push_str(&format!(
            "\nACTIVE CHASE: {:?} (round {}, separation {})",
            cs.chase_type(),
            cs.round(),
            cs.separation()
        ));
    }

    // Include character abilities and mutations so the narrator knows what
    // the character can and cannot do (prevents hallucinated abilities).
    if let Some(ref cj) = character_json {
        // Extract hooks (narrative abilities, mutations, etc.)
        if let Some(hooks) = cj.get("hooks").and_then(|h| h.as_array()) {
            let hook_strs: Vec<&str> = hooks.iter().filter_map(|v| v.as_str()).collect();
            if !hook_strs.is_empty() {
                state_summary.push_str("\n\nABILITY CONSTRAINTS — THIS IS A HARD RULE:\n");
                state_summary.push_str("The character can ONLY use the following abilities. Any action that requires a power, mutation, or supernatural ability NOT on this list MUST fail or be reinterpreted as a mundane attempt. Do NOT grant the character abilities they do not have.\n");
                state_summary.push_str("Allowed abilities:\n");
                for h in &hook_strs {
                    state_summary.push_str(&format!("- {}\n", h));
                }
                state_summary.push_str("If the player attempts to use an ability NOT listed above, describe the attempt failing or reframe it as a non-supernatural action.\n");
                state_summary.push_str("PROACTIVE MUTATION NARRATION: When the scene naturally creates an opportunity for the character's abilities/mutations to be relevant (sensory input, danger, social situations), weave them into the narration subtly. A psychic character might catch stray thoughts; a bioluminescent character's skin might flicker in darkness. Don't force it every turn, but don't ignore mutations either — they define who the character IS.\n");
            }
        }
        // Extract backstory
        if let Some(backstory) = cj.get("backstory").and_then(|b| b.as_str()) {
            state_summary.push_str(&format!("\nBackstory: {}", backstory));
        }
        // Extract class and race for narrator awareness
        if let Some(class) = cj.get("char_class").and_then(|c| c.as_str()) {
            state_summary.push_str(&format!("\nClass: {}", class));
        }
        if let Some(race) = cj.get("race").and_then(|r| r.as_str()) {
            state_summary.push_str(&format!("\nRace/Origin: {}", race));
        }
        if let Some(pronouns) = cj.get("pronouns").and_then(|p| p.as_str()) {
            if !pronouns.is_empty() {
                state_summary.push_str(&format!("\nPronouns: {} — ALWAYS use these pronouns for this character.", pronouns));
                tracing::debug!(pronouns = %pronouns, "narrator_prompt.pronouns — injected into state_summary");
            }
        }
    }

    if !world_context.is_empty() {
        state_summary.push('\n');
        state_summary.push_str(world_context);
    }

    // Inject known locations so the narrator uses canonical place names
    if !discovered_regions.is_empty() {
        state_summary.push_str("\n\nKNOWN LOCATIONS IN THIS WORLD:\n");
        state_summary.push_str("Use ONLY these location names when referring to places the party has visited or heard about. Do NOT invent new settlement names.\n");
        for region in discovered_regions.iter() {
            state_summary.push_str(&format!("- {}\n", region));
        }
    }
    // Also inject cartography region names from the shared session (if available)
    {
        let holder = shared_session_holder.lock().await;
        if let Some(ref ss_arc) = *holder {
            let ss = ss_arc.lock().await;
            if !ss.region_names.is_empty() {
                if discovered_regions.is_empty() {
                    state_summary.push_str("\n\nWORLD LOCATIONS (from cartography):\n");
                    state_summary.push_str("Use these canonical location names. Do NOT invent new ones.\n");
                } else {
                    state_summary.push_str("Additional world locations (not yet visited):\n");
                }
                for (region_id, _display_name) in &ss.region_names {
                    if !discovered_regions.iter().any(|r| r.to_lowercase() == *region_id) {
                        state_summary.push_str(&format!("- {}\n", region_id));
                    }
                }
            }
        }
    }

    if !trope_context.is_empty() {
        state_summary.push('\n');
        state_summary.push_str(&trope_context);
    }

    // Inject tone context from narrative axes (story F2/F10)
    if let Some(ref ac) = axes_config {
        let tone_text = sidequest_game::format_tone_context(ac, axis_values);
        if !tone_text.is_empty() {
            state_summary.push_str(&tone_text);
        }
    }

    // Bug 17: Include recent narration history so the narrator maintains continuity
    if !narration_history.is_empty() {
        state_summary.push_str("\n\nRECENT CONVERSATION HISTORY (multiple players, most recent last):\nEntries are tagged with [CharacterName]. Only narrate for the ACTING player — do not continue another player's scene:\n");
        // Include at most the last 10 turns to stay within context limits
        let start = narration_history.len().saturating_sub(10);
        for entry in &narration_history[start..] {
            state_summary.push_str(entry);
            state_summary.push('\n');
        }
    }

    // Inject NPC registry so the narrator maintains identity consistency
    let npc_context = build_npc_registry_context(npc_registry);
    if !npc_context.is_empty() {
        state_summary.push_str(&npc_context);
    }

    // Inject lore context from genre pack — budget-aware selection (story 11-4)
    {
        let context_hint = if !current_location.is_empty() {
            Some(current_location.as_str())
        } else {
            None
        };
        let lore_budget = 500; // ~500 tokens for lore context
        let selected =
            sidequest_game::select_lore_for_prompt(lore_store, lore_budget, context_hint);
        if !selected.is_empty() {
            let lore_text = sidequest_game::format_lore_context(&selected);
            tracing::info!(
                fragments = selected.len(),
                tokens = selected.iter().map(|f| f.token_estimate()).sum::<usize>(),
                hint = ?context_hint,
                "rag.lore_injected_to_prompt"
            );
            state_summary.push_str("\n\n");
            state_summary.push_str(&lore_text);
        }
    }

    // F9: Wish Consequence Engine — detect power-grab actions and inject consequence context
    {
        let mut engine = sidequest_game::WishConsequenceEngine::new();
        if let Some(wish) = engine.evaluate(char_name, action) {
            let wish_context = sidequest_game::WishConsequenceEngine::build_prompt_context(&wish);
            tracing::info!(
                wisher = %wish.wisher_name,
                category = ?wish.consequence_category,
                "wish_consequence.power_grab_detected"
            );
            state_summary.push_str(&wish_context);
        }
    }

    // Inject continuity corrections from the previous turn (if any)
    if !continuity_corrections.is_empty() {
        state_summary.push_str("\n\n");
        state_summary.push_str(continuity_corrections);
        tracing::info!(
            corrections_len = continuity_corrections.len(),
            "continuity.corrections_injected_to_prompt"
        );
        // Clear after injection — corrections are one-shot
        continuity_corrections.clear();
    }

    // Check if barrier mode is active (Structured/Cinematic turn mode).
    // If active, submit action to barrier, send "waiting" to this player via session
    // channel, then await barrier resolution inline. After resolution, override the
    // action with the combined party context and fall through to the normal pipeline.
    // This ensures ALL post-narration systems (HP, combat, tropes, quests, inventory,
    // persistence, music, render, etc.) run for barrier turns — not just narration.
    let barrier_combined_action: Option<String> = {
        let holder = shared_session_holder.lock().await;
        if let Some(ref ss_arc) = *holder {
            let ss = ss_arc.lock().await;
            tracing::debug!(
                turn_mode = ?ss.turn_mode,
                player_count = ss.players.len(),
                has_barrier = ss.turn_barrier.is_some(),
                "turn_mode.check — evaluating barrier vs freeplay"
            );
            if ss.turn_mode.should_use_barrier() {
                if let Some(ref barrier) = ss.turn_barrier {
                    // Submit action to barrier (doesn't trigger narration yet)
                    tracing::info!(player_id = %player_id, "barrier.submit — action submitted, waiting for other players");
                    barrier.submit_action(player_id, action);

                    // Broadcast TURN_STATUS "active" so other players' UIs know this player submitted
                    let turn_submitted = GameMessage::TurnStatus {
                        payload: TurnStatusPayload {
                            player_name: player_name_for_save.to_string(),
                            status: "active".into(),
                            state_delta: None,
                        },
                        player_id: player_id.to_string(),
                    };
                    let _ = state.broadcast(turn_submitted);
                    tracing::info!(player_name = %player_name_for_save, "barrier.turn_status.active — broadcast submission notification");
                    let barrier_clone = barrier.clone();

                    // Send "waiting" to this player via session channel (writer task delivers it)
                    ss.send_to_player(
                        GameMessage::SessionEvent {
                            payload: SessionEventPayload {
                                event: "waiting".to_string(),
                                player_name: None,
                                genre: None,
                                world: None,
                                has_character: None,
                                initial_state: None,
                                css: None,
                            },
                            player_id: player_id.to_string(),
                        },
                        player_id.to_string(),
                    );

                    drop(ss);
                    drop(holder);

                    // Await barrier resolution inline — player is waiting, handler blocked is fine
                    let result = barrier_clone.wait_for_turn().await;
                    tracing::info!(
                        timed_out = result.timed_out,
                        missing = ?result.missing_players,
                        genre = %genre_slug,
                        world = %world_slug,
                        "Turn barrier resolved"
                    );

                    // Build combined action context from all players' submissions
                    let named_actions = {
                        let holder = shared_session_holder.lock().await;
                        if let Some(ref ss_arc) = *holder {
                            let ss = ss_arc.lock().await;
                            ss.multiplayer.named_actions()
                        } else {
                            HashMap::new()
                        }
                    };
                    let combined = named_actions
                        .iter()
                        .map(|(name, act)| format!("{}: {}", name, act))
                        .collect::<Vec<_>>()
                        .join("\n");

                    // Prepend combined actions + perspective instruction to state_summary
                    state_summary = format!(
                        "Combined party actions:\n{}\n\nPERSPECTIVE: Write in third-person omniscient. Do NOT use 'you' for any character. Name all characters explicitly.\n\n{}",
                        combined, state_summary
                    );

                    Some(combined)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    };

    // Use combined action for barrier turns, original action for FreePlay
    let effective_action: std::borrow::Cow<str> = match &barrier_combined_action {
        Some(combined) => std::borrow::Cow::Borrowed(combined.as_str()),
        None => std::borrow::Cow::Borrowed(action),
    };

    // Preprocess raw player input — STT cleanup + three-perspective rewrite.
    // Uses haiku-tier LLM with 15s timeout; falls back to mechanical rewrite on failure.
    let preprocessed = sidequest_agents::preprocessor::preprocess_action(&effective_action, char_name);
    tracing::info!(
        raw = %action,
        you = %preprocessed.you,
        named = %preprocessed.named,
        intent = %preprocessed.intent,
        "Action preprocessed"
    );

    // Process the action through GameService (FreePlay mode — immediate resolution)
    let context = TurnContext {
        state_summary: Some(state_summary),
        in_combat: combat_state.in_combat(),
        in_chase: chase_state.is_some(),
    };
    let result = state
        .game_service()
        .process_action(&preprocessed.you, &context);

    // Watcher: narration generated (with intent classification and agent routing)
    state.send_watcher_event(WatcherEvent {
        timestamp: chrono::Utc::now(),
        component: "game".to_string(),
        event_type: WatcherEventType::AgentSpanClose,
        severity: Severity::Info,
        fields: {
            let mut f = HashMap::new();
            f.insert(
                "narration_len".to_string(),
                serde_json::json!(result.narration.len()),
            );
            f.insert(
                "is_degraded".to_string(),
                serde_json::json!(result.is_degraded),
            );
            f.insert("turn_number".to_string(), serde_json::json!(turn_number));
            if let Some(ref intent) = result.classified_intent {
                f.insert("classified_intent".to_string(), serde_json::json!(intent));
            }
            if let Some(ref agent) = result.agent_name {
                f.insert("agent_routed_to".to_string(), serde_json::json!(agent));
            }
            f
        },
    });

    let mut messages = vec![];

    // Extract location header from narration (format: **Location Name**\n\n...)
    // Bug 1: Update current_location so subsequent turns maintain continuity
    let narration_text = &result.narration;
    if let Some(location) = extract_location_header(narration_text) {
        let is_new = !discovered_regions.iter().any(|r| r == &location);
        *current_location = location.clone();
        if is_new {
            discovered_regions.push(location.clone());
        }
        tracing::info!(
            location = %location,
            is_new,
            total_discovered = discovered_regions.len(),
            "location.changed"
        );
        state.send_watcher_event(WatcherEvent {
            timestamp: chrono::Utc::now(),
            component: "game".to_string(),
            event_type: WatcherEventType::StateTransition,
            severity: Severity::Info,
            fields: {
                let mut f = HashMap::new();
                f.insert(
                    "event".to_string(),
                    serde_json::Value::String("location_changed".to_string()),
                );
                f.insert(
                    "location".to_string(),
                    serde_json::Value::String(location.clone()),
                );
                f.insert("turn_number".to_string(), serde_json::json!(turn_number));
                f
            },
        });
        messages.push(GameMessage::ChapterMarker {
            payload: ChapterMarkerPayload {
                title: Some(location.clone()),
                location: Some(location.clone()),
            },
            player_id: player_id.to_string(),
        });
        // Build explored locations from discovered_regions
        let explored_locs: Vec<sidequest_protocol::ExploredLocation> = discovered_regions
            .iter()
            .map(|name| sidequest_protocol::ExploredLocation {
                name: name.clone(),
                x: 0,
                y: 0,
                location_type: String::new(),
                connections: vec![],
            })
            .collect();
        messages.push(GameMessage::MapUpdate {
            payload: MapUpdatePayload {
                current_location: location,
                region: current_location.clone(),
                explored: explored_locs,
                fog_bounds: None,
            },
            player_id: player_id.to_string(),
        });
        // Location change = meaningful narrative beat → advance display round
        turn_manager.advance_round();
        tracing::info!(
            new_round = turn_manager.round(),
            interaction = turn_manager.interaction(),
            "turn_manager.advance_round — location change"
        );
    }

    // Strip the location header from narration text if present
    let clean_narration = strip_location_header(narration_text);

    // Bug 17: Accumulate narration history for context on subsequent turns.
    // Truncate narrator response to ~300 chars to keep context bounded.
    let truncated_narration: String = clean_narration.chars().take(300).collect();
    narration_history.push(format!(
        "[{}] Action: {}\nNarrator: {}",
        char_name, effective_action, truncated_narration
    ));
    // Cap the buffer at 20 entries to prevent unbounded growth
    if narration_history.len() > 20 {
        narration_history.drain(..narration_history.len() - 20);
    }

    // Update NPC registry from structured narrator output (preferred) + regex fallback.
    // Structured extraction produces clean data; regex catches NPCs the narrator forgot to list.
    let turn_approx = turn_manager.interaction() as u32;
    if !result.npcs_present.is_empty() {
        tracing::info!(count = result.npcs_present.len(), "npc_registry.structured — updating from narrator JSON");
        for npc in &result.npcs_present {
            if npc.name.is_empty() { continue; }
            let name_lower = npc.name.to_lowercase();
            if let Some(entry) = npc_registry.iter_mut().find(|e| {
                e.name.to_lowercase() == name_lower
                    || e.name.to_lowercase().contains(&name_lower)
                    || name_lower.contains(&e.name.to_lowercase())
            }) {
                // Update existing — preserve identity, update last_seen
                entry.last_seen_turn = turn_approx;
                if !current_location.is_empty() {
                    entry.location = current_location.to_string();
                }
                // Upgrade name if structured version is more specific
                if npc.name.len() > entry.name.len() {
                    entry.name = npc.name.clone();
                }
                // Fill in missing fields from structured data
                if entry.pronouns.is_empty() && !npc.pronouns.is_empty() {
                    entry.pronouns = npc.pronouns.clone();
                }
                if entry.role.is_empty() && !npc.role.is_empty() {
                    entry.role = npc.role.clone();
                }
                if entry.appearance.is_empty() && !npc.appearance.is_empty() {
                    entry.appearance = npc.appearance.clone();
                }
            } else if npc.is_new {
                // New NPC — create entry
                npc_registry.push(NpcRegistryEntry {
                    name: npc.name.clone(),
                    pronouns: npc.pronouns.clone(),
                    role: npc.role.clone(),
                    age: String::new(),
                    appearance: npc.appearance.clone(),
                    location: current_location.to_string(),
                    last_seen_turn: turn_approx,
                });
                tracing::info!(name = %npc.name, pronouns = %npc.pronouns, role = %npc.role, "npc_registry.new — created from structured data");
            }
        }
    }
    // Regex fallback — catches NPCs the narrator forgot to list in the JSON block.
    // Include both discovered regions AND all cartography region names so that
    // location-derived words (e.g., "Tood" from "Tood's Dome") are never registered as NPCs.
    let mut all_location_names: Vec<String> = discovered_regions.clone();
    {
        let holder = shared_session_holder.lock().await;
        if let Some(ref ss_arc) = *holder {
            let ss = ss_arc.lock().await;
            for (region_id, _name_lower) in &ss.region_names {
                if !all_location_names.iter().any(|r| r == region_id) {
                    all_location_names.push(region_id.clone());
                }
            }
        }
    }
    let region_refs: Vec<&str> = all_location_names.iter().map(|s| s.as_str()).collect();
    update_npc_registry(
        npc_registry,
        &clean_narration,
        current_location,
        turn_approx,
        &region_refs,
    );
    tracing::debug!(
        npc_count = npc_registry.len(),
        "NPC registry updated from narration"
    );

    // Continuity validation — check narrator output against game state.
    // Build a minimal snapshot from the local session variables for the validator.
    {
        let mut validation_snapshot = sidequest_game::GameSnapshot {
            location: current_location.clone(),
            ..sidequest_game::GameSnapshot::default()
        };
        // Reconstruct character with inventory for the validator
        if let Some(ref cj) = character_json {
            if let Ok(mut ch) = serde_json::from_value::<sidequest_game::Character>(cj.clone()) {
                ch.core.hp = *hp;
                ch.core.inventory = inventory.clone();
                validation_snapshot.characters.push(ch);
            }
        }
        let validation_result =
            sidequest_game::validate_continuity(&clean_narration, &validation_snapshot);
        if !validation_result.is_clean() {
            let corrections = validation_result.format_corrections();
            tracing::warn!(
                contradictions = validation_result.contradictions.len(),
                "continuity.contradictions_detected"
            );
            for c in &validation_result.contradictions {
                tracing::warn!(
                    category = ?c.category,
                    detail = %c.detail,
                    expected = %c.expected,
                    "continuity.contradiction"
                );
            }
            // Store for injection into the NEXT turn's narrator prompt
            *continuity_corrections = corrections;
        }
    }

    // Combat HP changes — apply typed CombatPatch from creature_smith (replaces keyword heuristic)
    if let Some(ref combat_patch) = result.combat_patch {
        // Apply HP changes from the patch
        if let Some(ref hp_changes) = combat_patch.hp_changes {
            // Find changes that apply to the acting player's character
            let char_name_lower = player_name_for_save.to_lowercase();
            for (target, delta) in hp_changes {
                let target_lower = target.to_lowercase();
                if target_lower == char_name_lower
                    || character_json.as_ref().and_then(|cj| cj.get("name")).and_then(|n| n.as_str()).map(|n| n.to_lowercase() == target_lower).unwrap_or(false)
                {
                    *hp = sidequest_game::clamp_hp(*hp, *delta, *max_hp);
                    tracing::info!(target = %target, delta = delta, new_hp = *hp, "combat.patch.hp_applied — player HP changed");
                }
            }
        }
        // Apply combat state transitions
        if let Some(in_combat) = combat_patch.in_combat {
            if in_combat && !combat_state.in_combat() {
                combat_state.set_in_combat(true);
                tracing::info!("combat.patch.started — CombatPatch set in_combat=true");
            } else if !in_combat && combat_state.in_combat() {
                combat_state.set_in_combat(false);
                tracing::info!("combat.patch.ended — CombatPatch set in_combat=false");
            }
        }
        if let Some(dw) = combat_patch.drama_weight {
            combat_state.set_drama_weight(dw);
        }
        if combat_patch.advance_round {
            combat_state.advance_round();
        }
        tracing::info!(
            in_combat = ?combat_patch.in_combat,
            hp_changes = ?combat_patch.hp_changes,
            drama_weight = ?combat_patch.drama_weight,
            "combat.patch.applied"
        );
    }

    // Bug 3: XP award based on action type
    {
        let xp_award = if combat_state.in_combat() {
            25 // combat actions give more XP
        } else {
            10 // exploration/dialogue gives base XP
        };
        *xp += xp_award;
        tracing::info!(
            xp_award = xp_award,
            total_xp = *xp,
            level = *level,
            "XP awarded"
        );

        // Check for level up
        let threshold = sidequest_game::xp_for_level(*level + 1);
        if *xp >= threshold {
            *level += 1;
            let new_max_hp = sidequest_game::level_to_hp(10, *level);
            let hp_gain = new_max_hp - *max_hp;
            *max_hp = new_max_hp;
            *hp = sidequest_game::clamp_hp(*hp + hp_gain, 0, *max_hp);
            tracing::info!(
                new_level = *level,
                new_max_hp = *max_hp,
                hp_gain = hp_gain,
                "Level up!"
            );
        }
    }

    // Affinity progression (Story F8) — check thresholds after XP/level-up.
    // Loads genre pack affinities via state to avoid adding another parameter.
    if let Some(ref cj) = character_json {
        if let Ok(mut ch) = serde_json::from_value::<sidequest_game::Character>(cj.clone()) {
            // Sync mutable fields
            ch.core.hp = *hp;
            ch.core.max_hp = *max_hp;
            ch.core.level = *level;
            ch.core.inventory = inventory.clone();

            // Increment affinity progress for any matching action triggers.
            let genre_code = sidequest_genre::GenreCode::new(genre_slug);
            if let Ok(code) = genre_code {
                let loader = GenreLoader::new(vec![state.genre_packs_path().to_path_buf()]);
                if let Ok(pack) = loader.load(&code) {
                    let genre_affinities = &pack.progression.affinities;

                    // Increment progress for affinities whose triggers match the action
                    for aff_def in genre_affinities {
                        let action_lower = effective_action.to_lowercase();
                        let matches_trigger = aff_def
                            .triggers
                            .iter()
                            .any(|t| action_lower.contains(&t.to_lowercase()));
                        if matches_trigger {
                            sidequest_game::increment_affinity_progress(
                                &mut ch.affinities,
                                &aff_def.name,
                                1,
                            );
                            tracing::info!(
                                affinity = %aff_def.name,
                                progress = ch.affinities.iter().find(|a| a.name == aff_def.name).map(|a| a.progress).unwrap_or(0),
                                "Affinity progress incremented"
                            );
                        }
                    }

                    // Check thresholds for tier-ups
                    let thresholds_for = |name: &str| -> Option<Vec<u32>> {
                        genre_affinities
                            .iter()
                            .find(|a| a.name == name)
                            .map(|a| a.tier_thresholds.clone())
                    };
                    let narration_hint_for = |name: &str, tier: u8| -> Option<String> {
                        genre_affinities
                            .iter()
                            .find(|a| a.name == name)
                            .and_then(|a| {
                                a.unlocks.as_ref().and_then(|u| {
                                    let tier_data = match tier {
                                        1 => u.tier_1.as_ref(),
                                        2 => u.tier_2.as_ref(),
                                        3 => u.tier_3.as_ref(),
                                        _ => None,
                                    };
                                    tier_data.map(|t| t.description.clone())
                                })
                            })
                    };

                    let tier_events = sidequest_game::check_affinity_thresholds(
                        &mut ch.affinities,
                        char_name,
                        &thresholds_for,
                        &narration_hint_for,
                    );

                    for event in &tier_events {
                        tracing::info!(
                            affinity = %event.affinity_name,
                            old_tier = event.old_tier,
                            new_tier = event.new_tier,
                            character = %event.character_name,
                            "Affinity tier up!"
                        );
                    }
                }
            } // if let Ok(code)

            // Write updated character back to character_json
            if let Ok(updated_json) = serde_json::to_value(&ch) {
                *character_json = Some(updated_json);
            }
        }
    }

    // Item acquisition — driven by structured extraction from the LLM response.
    // The narrator emits items_gained in its JSON block when the player
    // actually acquires something.
    const VALID_ITEM_CATEGORIES: &[&str] = &[
        "weapon", "armor", "tool", "consumable", "quest", "treasure", "misc",
    ];
    for item_def in &result.items_gained {
        // Reject prose fragments: item names should be short noun phrases,
        // not sentences or long descriptions.
        let name_trimmed = item_def.name.trim();
        let word_count = name_trimmed.split_whitespace().count();
        if name_trimmed.len() > 60 || word_count > 8 {
            tracing::warn!(
                item_name = %item_def.name,
                len = name_trimmed.len(),
                words = word_count,
                "Rejected item: name too long (likely prose fragment)"
            );
            continue;
        }
        // Reject names that look like sentences (contain common verbs)
        let lower = name_trimmed.to_lowercase();
        if lower.starts_with("the ") && word_count > 5 {
            tracing::warn!(item_name = %item_def.name, "Rejected item: sentence-like name");
            continue;
        }
        // Validate category
        let category = item_def.category.trim().to_lowercase();
        let valid_cat = if VALID_ITEM_CATEGORIES.contains(&category.as_str()) {
            category
        } else {
            "misc".to_string()
        };
        let item_id = name_trimmed
            .to_lowercase()
            .replace(' ', "_")
            .replace(|c: char| !c.is_alphanumeric() && c != '_', "");
        if inventory.find(&item_id).is_some() {
            continue;
        }
        if let (Ok(id), Ok(name), Ok(desc), Ok(cat), Ok(rarity)) = (
            sidequest_protocol::NonBlankString::new(&item_id),
            sidequest_protocol::NonBlankString::new(name_trimmed),
            sidequest_protocol::NonBlankString::new(&item_def.description),
            sidequest_protocol::NonBlankString::new(&valid_cat),
            sidequest_protocol::NonBlankString::new("common"),
        ) {
            let item = sidequest_game::Item {
                id,
                name,
                description: desc,
                category: cat,
                value: 0,
                weight: 1.0,
                rarity,
                narrative_weight: 0.3,
                tags: vec![],
                equipped: false,
                quantity: 1,
            };
            let _ = inventory.add(item, 50);
            tracing::info!(item_name = %item_def.name, "Item added to inventory from LLM extraction");
        }
    }

    // Legacy regex-based extraction disabled — replaced by LLM structured extraction above.
    if false {
        let items_found = extract_items_from_narration(&clean_narration);
        for (item_name, item_type) in &items_found {
            let item_id = item_name
                .to_lowercase()
                .replace(' ', "_")
                .replace(|c: char| !c.is_alphanumeric() && c != '_', "");
            // Skip if already in inventory
            if inventory.find(&item_id).is_some() {
                continue;
            }
            if let (Ok(id), Ok(name), Ok(desc), Ok(cat), Ok(rarity)) = (
                sidequest_protocol::NonBlankString::new(&item_id),
                sidequest_protocol::NonBlankString::new(item_name),
                sidequest_protocol::NonBlankString::new(&format!(
                    "A {} found during adventure",
                    item_type
                )),
                sidequest_protocol::NonBlankString::new(item_type),
                sidequest_protocol::NonBlankString::new("common"),
            ) {
                let item = sidequest_game::Item {
                    id,
                    name,
                    description: desc,
                    category: cat,
                    value: 0,
                    weight: 1.0,
                    rarity,
                    narrative_weight: 0.3,
                    tags: vec![],
                    equipped: false,
                    quantity: 1,
                };
                let _ = inventory.add(item, 50);
                tracing::info!(item_name = %item_name, "Item added to inventory from narration");
                state.send_watcher_event(WatcherEvent {
                    timestamp: chrono::Utc::now(),
                    component: "inventory".to_string(),
                    event_type: WatcherEventType::StateTransition,
                    severity: Severity::Info,
                    fields: {
                        let mut f = HashMap::new();
                        f.insert("event".to_string(), serde_json::json!("item_gained"));
                        f.insert("item".to_string(), serde_json::json!(item_name));
                        f.insert(
                            "turn_number".to_string(),
                            serde_json::json!(turn_manager.interaction()),
                        );
                        f
                    },
                });
            }
        }

        // Extract item losses from narration (trades, gifts, drops)
        let items_lost = extract_item_losses(&clean_narration);
        for lost_name in &items_lost {
            let item_id = lost_name
                .to_lowercase()
                .replace(' ', "_")
                .replace(|c: char| !c.is_alphanumeric() && c != '_', "");
            if inventory.find(&item_id).is_some() {
                let _ = inventory.remove(&item_id);
                tracing::info!(item_name = %lost_name, "Item removed from inventory from narration");
                state.send_watcher_event(WatcherEvent {
                    timestamp: chrono::Utc::now(),
                    component: "inventory".to_string(),
                    event_type: WatcherEventType::StateTransition,
                    severity: Severity::Info,
                    fields: {
                        let mut f = HashMap::new();
                        f.insert("event".to_string(), serde_json::json!("item_lost"));
                        f.insert("item".to_string(), serde_json::json!(lost_name));
                        f.insert(
                            "turn_number".to_string(),
                            serde_json::json!(turn_manager.interaction()),
                        );
                        f
                    },
                });
            }
        }
    }

    // Quest log updates — merge narrator-extracted quest changes into quest_log
    if !result.quest_updates.is_empty() {
        for (quest_name, status) in &result.quest_updates {
            quest_log.insert(quest_name.clone(), status.clone());
            tracing::info!(quest = %quest_name, status = %status, "quest.updated");
        }
    }

    // Narration — include character state so the UI state mirror picks it up
    let inventory_names: Vec<String> = inventory
        .items
        .iter()
        .map(|i| i.name.as_str().to_string())
        .collect();
    let char_class_name = character_json
        .as_ref()
        .and_then(|cj| cj.get("char_class"))
        .and_then(|c| c.as_str())
        .unwrap_or("Adventurer");
    messages.push(GameMessage::Narration {
        payload: NarrationPayload {
            text: clean_narration.clone(),
            state_delta: Some(sidequest_protocol::StateDelta {
                location: extract_location_header(narration_text),
                characters: Some(vec![sidequest_protocol::CharacterState {
                    name: char_name.to_string(),
                    hp: *hp,
                    max_hp: *max_hp,
                    level: *level,
                    class: char_class_name.to_string(),
                    statuses: vec![],
                    inventory: inventory_names.clone(),
                }]),
                quests: None,
                items_gained: if result.items_gained.is_empty() {
                    None
                } else {
                    Some(result.items_gained.clone())
                },
            }),
            footnotes: result.footnotes.clone(),
        },
        player_id: player_id.to_string(),
    });

    // RAG pipeline: convert new footnotes to discovered facts (story 9-11)
    if !result.footnotes.is_empty() {
        let discovered = sidequest_agents::footnotes::footnotes_to_discovered_facts(
            &result.footnotes,
            char_name,
            turn_manager.interaction(),
        );
        if !discovered.is_empty() {
            tracing::info!(
                count = discovered.len(),
                character = %char_name,
                interaction = turn_manager.interaction(),
                "rag.footnotes_to_discovered_facts"
            );
            // Apply discovered facts to snapshot via WorldStatePatch path
            // (This feeds into the persistence layer on next save)
        }
    }

    // Narration end with state_delta field present (even if empty)
    messages.push(GameMessage::NarrationEnd {
        payload: NarrationEndPayload {
            state_delta: Some(sidequest_protocol::StateDelta {
                location: None,
                characters: None,
                quests: None,
                items_gained: None,
            }),
        },
        player_id: player_id.to_string(),
    });

    // Extract character class from JSON for PartyStatus
    let char_class = character_json
        .as_ref()
        .and_then(|cj| cj.get("char_class"))
        .and_then(|c| c.as_str())
        .unwrap_or("Adventurer");

    // Party status — build full party from shared session (multiplayer) or local only (single-player)
    {
        let mut party_members = vec![PartyMember {
            player_id: player_id.to_string(),
            name: player_name_for_save.to_string(),
            character_name: char_name.to_string(),
            current_hp: *hp,
            max_hp: *max_hp,
            statuses: vec![],
            class: char_class.to_string(),
            level: *level,
            portrait_url: None,
        }];
        // In multiplayer, include other players from the shared session
        let holder = shared_session_holder.lock().await;
        if let Some(ref ss_arc) = *holder {
            let ss = ss_arc.lock().await;
            for (pid, ps) in &ss.players {
                if pid == player_id {
                    continue; // Already added above with fresh local data
                }
                party_members.push(PartyMember {
                    player_id: pid.clone(),
                    name: ps.player_name.clone(),
                    character_name: ps.character_name.clone().unwrap_or_else(|| ps.player_name.clone()),
                    current_hp: ps.character_hp,
                    max_hp: ps.character_max_hp,
                    statuses: vec![],
                    class: String::new(),
                    level: ps.character_level,
                    portrait_url: None,
                });
            }
        }
        messages.push(GameMessage::PartyStatus {
            payload: PartyStatusPayload {
                members: party_members,
            },
            player_id: player_id.to_string(),
        });
    }

    // Bug 5: Inventory — now wired to actual inventory state
    messages.push(GameMessage::Inventory {
        payload: InventoryPayload {
            items: inventory
                .items
                .iter()
                .map(|item| sidequest_protocol::InventoryItem {
                    name: item.name.as_str().to_string(),
                    item_type: item.category.as_str().to_string(),
                    equipped: item.equipped,
                    quantity: item.quantity,
                    description: item.description.as_str().to_string(),
                })
                .collect(),
            gold: inventory.gold,
        },
        player_id: player_id.to_string(),
    });

    // Combat detection — intent-based (primary) + keyword scan (fallback).
    // If the intent classifier routed to creature_smith, that's a combat action.
    if !combat_state.in_combat() {
        if let Some(ref intent) = result.classified_intent {
            if intent == "Combat" {
                combat_state.set_in_combat(true);
                tracing::info!(intent = %intent, agent = ?result.agent_name, "combat.started — intent classifier triggered combat state");
                {
                    let holder = shared_session_holder.lock().await;
                    if let Some(ref ss_arc) = *holder {
                        let mut ss = ss_arc.lock().await;
                        let old_mode = std::mem::take(&mut ss.turn_mode);
                        ss.turn_mode = old_mode
                            .apply(sidequest_game::turn_mode::TurnModeTransition::CombatStarted);
                    }
                }
            }
        }
    }

    // Keyword-based combat detection — fallback for cases where intent
    // classification missed but narration clearly describes combat.
    {
        let narr_lower = clean_narration.to_lowercase();
        let combat_start_keywords = [
            "initiative",
            "combat begins",
            "roll for initiative",
            "attacks you",
            "lunges at",
            "swings at",
            "draws a weapon",
            "charges at",
            "opens fire",
            "enters combat",
        ];
        let combat_end_keywords = [
            "combat ends",
            "battle is over",
            "enemies defeated",
            "falls unconscious",
            "retreats",
            "flees",
            "surrenders",
            "combat resolved",
            "the fight is over",
        ];

        if combat_state.in_combat() {
            // Check for combat end
            if combat_end_keywords.iter().any(|kw| narr_lower.contains(kw)) {
                combat_state.set_in_combat(false);
                tracing::info!("Combat ended — detected end keyword in narration");
                // Transition turn mode: Structured → FreePlay
                {
                    let holder = shared_session_holder.lock().await;
                    if let Some(ref ss_arc) = *holder {
                        let mut ss = ss_arc.lock().await;
                        let old_mode = std::mem::take(&mut ss.turn_mode);
                        ss.turn_mode = old_mode
                            .apply(sidequest_game::turn_mode::TurnModeTransition::CombatEnded);
                        tracing::info!(new_mode = ?ss.turn_mode, "Turn mode transitioned on combat end");
                    }
                }
            }
        } else {
            // Check for combat start
            if combat_start_keywords
                .iter()
                .any(|kw| narr_lower.contains(kw))
            {
                combat_state.set_in_combat(true);
                tracing::info!("Combat started — detected start keyword in narration");
                // Transition turn mode: FreePlay → Structured
                {
                    let holder = shared_session_holder.lock().await;
                    if let Some(ref ss_arc) = *holder {
                        let mut ss = ss_arc.lock().await;
                        let old_mode = std::mem::take(&mut ss.turn_mode);
                        ss.turn_mode = old_mode
                            .apply(sidequest_game::turn_mode::TurnModeTransition::CombatStarted);
                        tracing::info!(new_mode = ?ss.turn_mode, "Turn mode transitioned on combat start");
                        // Initialize barrier if transitioning to structured mode
                        if ss.turn_mode.should_use_barrier() && ss.turn_barrier.is_none() {
                            let mp_session = sidequest_game::multiplayer::MultiplayerSession::with_player_ids(
                                ss.players.keys().cloned(),
                            );
                            let adaptive = sidequest_game::barrier::AdaptiveTimeout::default();
                            ss.turn_barrier = Some(sidequest_game::barrier::TurnBarrier::with_adaptive(
                                mp_session,
                                adaptive,
                            ));
                        }
                    }
                }
            }
        }
    }

    // Combat tick — uses persistent per-session CombatState
    let was_in_combat = combat_state.in_combat();
    tracing::debug!(
        in_combat = was_in_combat,
        round = combat_state.round(),
        drama_weight = combat_state.drama_weight(),
        "combat.pre_tick"
    );
    if combat_state.in_combat() {
        combat_state.tick_effects();
        combat_state.advance_round();
        state.send_watcher_event(WatcherEvent {
            timestamp: chrono::Utc::now(),
            component: "combat".to_string(),
            event_type: WatcherEventType::AgentSpanOpen,
            severity: Severity::Info,
            fields: {
                let mut f = HashMap::new();
                f.insert("round".to_string(), serde_json::json!(combat_state.round()));
                f.insert(
                    "drama_weight".to_string(),
                    serde_json::json!(combat_state.drama_weight()),
                );
                f
            },
        });
    }

    // Combat overlay — send whenever combat state is relevant
    if was_in_combat || combat_state.in_combat() {
        messages.push(GameMessage::CombatEvent {
            payload: CombatEventPayload {
                in_combat: combat_state.in_combat(),
                enemies: vec![],
                turn_order: vec![],
                current_turn: String::new(),
            },
            player_id: player_id.to_string(),
        });
    }

    // Bug 6: Chase detection and state tracking
    {
        let narr_lower = clean_narration.to_lowercase();
        let chase_start_keywords = [
            "chase begins",
            "gives chase",
            "starts chasing",
            "run!",
            "flee!",
            "pursues you",
            "pursuit begins",
            "races after",
            "sprints after",
            "bolts away",
        ];
        let chase_end_keywords = [
            "escape",
            "lost them",
            "chase ends",
            "caught up",
            "stopped running",
            "pursuit ends",
            "safe now",
            "shakes off",
            "outrun",
        ];

        if let Some(ref mut cs) = chase_state {
            // Update active chase
            if chase_end_keywords.iter().any(|kw| narr_lower.contains(kw)) {
                tracing::info!(rounds = cs.round(), "Chase resolved");
                *chase_state = None;
            } else {
                // Advance chase round, adjust separation based on narration
                let gain = if narr_lower.contains("gaining") || narr_lower.contains("closing") {
                    -1
                } else if narr_lower.contains("widening") || narr_lower.contains("pulling ahead") {
                    1
                } else {
                    0
                };
                cs.set_separation(cs.separation() + gain);
                cs.record_roll(0.5); // placeholder roll
                tracing::info!(round = cs.round(), separation = cs.separation(), gain, "chase.tick — round advanced");
            }
        } else if chase_start_keywords
            .iter()
            .any(|kw| narr_lower.contains(kw))
        {
            let cs = sidequest_game::ChaseState::new(sidequest_game::ChaseType::Footrace, 0.5);
            tracing::info!("Chase started — detected chase keyword in narration");
            *chase_state = Some(cs);
        }
    }

    // Scan narration for trope trigger keywords → activate matching tropes
    let narration_lower = clean_narration.to_lowercase();
    tracing::debug!(
        narration_len = narration_lower.len(),
        active_tropes = trope_states.len(),
        total_defs = trope_defs.len(),
        "Trope keyword scan starting"
    );
    for def in trope_defs.iter() {
        let id = match &def.id {
            Some(id) => id,
            None => continue,
        };
        // Skip already active tropes
        if trope_states.iter().any(|ts| ts.trope_definition_id() == id) {
            continue;
        }
        // Check if any trigger keyword appears in the narration
        let triggered = def
            .triggers
            .iter()
            .any(|t| narration_lower.contains(&t.to_lowercase()));
        if triggered {
            sidequest_game::trope::TropeEngine::activate(trope_states, id);
            tracing::info!(trope_id = %id, "Trope activated by narration keyword");
            state.send_watcher_event(WatcherEvent {
                timestamp: chrono::Utc::now(),
                component: "trope".to_string(),
                event_type: WatcherEventType::StateTransition,
                severity: Severity::Info,
                fields: {
                    let mut f = HashMap::new();
                    f.insert(
                        "event".to_string(),
                        serde_json::Value::String("trope_activated".to_string()),
                    );
                    f.insert(
                        "trope_id".to_string(),
                        serde_json::Value::String(id.clone()),
                    );
                    f.insert(
                        "trigger".to_string(),
                        serde_json::Value::String("narration_keyword".to_string()),
                    );
                    f
                },
            });
        }
    }

    // Trope engine tick — uses persistent per-session trope state and genre pack defs
    // Log pre-tick state for debugging
    for ts in trope_states.iter() {
        tracing::info!(
            trope_id = %ts.trope_definition_id(),
            status = ?ts.status(),
            progression = ts.progression(),
            fired_beats = ts.fired_beats().len(),
            "Trope pre-tick state"
        );
    }
    let fired = sidequest_game::trope::TropeEngine::tick(trope_states, trope_defs);
    sidequest_game::trope::TropeEngine::apply_keyword_modifiers(
        trope_states,
        trope_defs,
        &clean_narration,
    );
    tracing::info!(
        active_tropes = trope_states.len(),
        fired_beats = fired.len(),
        "Trope tick complete"
    );
    // Log post-tick state
    for ts in trope_states.iter() {
        tracing::debug!(
            trope_id = %ts.trope_definition_id(),
            status = ?ts.status(),
            progression = ts.progression(),
            "Trope post-tick state"
        );
    }
    for beat in &fired {
        tracing::info!(trope = %beat.trope_name, "Trope beat fired");
        state.send_watcher_event(WatcherEvent {
            timestamp: chrono::Utc::now(),
            component: "trope".to_string(),
            event_type: WatcherEventType::AgentSpanOpen,
            severity: Severity::Info,
            fields: {
                let mut f = HashMap::new();
                f.insert(
                    "trope".to_string(),
                    serde_json::Value::String(beat.trope_name.clone()),
                );
                f.insert(
                    "trope_id".to_string(),
                    serde_json::Value::String(beat.trope_id.clone()),
                );
                f
            },
        });
    }

    // Render pipeline — extract subject from narration, filter, enqueue
    let extraction_context = sidequest_game::ExtractionContext {
        current_location: extract_location_header(narration_text).unwrap_or_default(),
        in_combat: combat_state.in_combat(),
        known_npcs: npc_registry.iter().map(|e| e.name.clone()).collect(),
        ..Default::default()
    };
    if let Some(subject) = state
        .inner
        .subject_extractor
        .extract(&clean_narration, &extraction_context)
    {
        tracing::info!(
            prompt = %subject.prompt_fragment(),
            tier = ?subject.tier(),
            weight = subject.narrative_weight(),
            "Subject extracted from narration"
        );
        let filter_ctx = sidequest_game::FilterContext {
            in_combat: combat_state.in_combat(),
            scene_transition: extract_location_header(narration_text).is_some(),
            player_requested: false,
        };
        let decision = state
            .inner
            .beat_filter
            .lock()
            .await
            .evaluate(&subject, &filter_ctx);
        tracing::info!(decision = ?decision, "BeatFilter decision");
        if matches!(decision, sidequest_game::FilterDecision::Render { .. }) {
            if let Some(ref queue) = state.inner.render_queue {
                // Compose the full style string: location tag override + positive_suffix.
                // This flows through the render queue as "art_style" and gets combined
                // with the raw prompt fragment in the render closure to build positive_prompt.
                let (art_style, model) = match visual_style {
                    Some(ref vs) => {
                        // Match visual_tag_overrides against current location (substring match)
                        let location = extraction_context.current_location.to_lowercase();
                        let tag_override = if !location.is_empty() {
                            vs.visual_tag_overrides
                                .iter()
                                .find(|(key, _)| location.contains(key.as_str()))
                                .map(|(_, val)| val.as_str())
                        } else {
                            None
                        };
                        let style = match tag_override {
                            Some(tag) => format!("{}, {}", tag, vs.positive_suffix),
                            None => vs.positive_suffix.clone(),
                        };
                        (style, vs.preferred_model.clone())
                    }
                    None => ("oil_painting".to_string(), "flux-schnell".to_string()),
                };
                match queue.enqueue(subject, &art_style, &model).await {
                    Ok(result) => tracing::info!(result = ?result, "Render job enqueued"),
                    Err(e) => tracing::warn!(error = %e, "Render enqueue failed"),
                }
            }
        }
    } else {
        tracing::debug!(
            narration_len = clean_narration.len(),
            "No render subject extracted"
        );
    }

    // Audio cue — evaluate mood via MusicDirector, route through AudioMixer
    if let Some(ref mut director) = music_director {
        tracing::info!("music_director_present — evaluating mood");
        let mood_ctx = sidequest_game::MoodContext {
            in_combat: combat_state.in_combat(),
            in_chase: chase_state.is_some(),
            party_health_pct: if *max_hp > 0 {
                *hp as f32 / *max_hp as f32
            } else {
                1.0
            },
            quest_completed: {
                let narr = clean_narration.to_lowercase();
                narr.contains("quest complete") || narr.contains("mission accomplished")
                    || narr.contains("task done") || narr.contains("objective achieved")
            },
            npc_died: {
                let narr = clean_narration.to_lowercase();
                narr.contains("falls dead") || narr.contains("killed")
                    || narr.contains("dies") || narr.contains("slain")
                    || narr.contains("collapses lifeless")
            },
        };
        // Classify mood first so we can include it in the protocol message
        let classification = director.classify_mood(&clean_narration, &mood_ctx);
        let mood_key = classification.primary.as_key();
        tracing::info!(
            mood = mood_key,
            intensity = classification.intensity,
            confidence = classification.confidence,
            in_combat = mood_ctx.in_combat,
            "music_mood_classified"
        );
        if let Some(cue) = director.evaluate(&clean_narration, &mood_ctx) {
            tracing::info!(
                mood = mood_key,
                track = ?cue.track_id,
                action = %cue.action,
                volume = cue.volume,
                "music_cue_produced"
            );
            let mixer_cues = {
                let mut mixer_guard = audio_mixer.lock().await;
                if let Some(ref mut mixer) = *mixer_guard {
                    mixer.apply_cue(cue)
                } else {
                    vec![cue]
                }
            };
            tracing::info!(cue_count = mixer_cues.len(), "music_mixer_cues_ready");
            for c in &mixer_cues {
                messages.push(audio_cue_to_game_message(
                    c,
                    player_id,
                    genre_slug,
                    Some(mood_key),
                ));
            }
        } else {
            tracing::warn!(
                mood = mood_key,
                "music_evaluate_returned_none — no cue produced"
            );
        }
    } else {
        tracing::warn!("music_director_missing — audio cues skipped");
    }

    // Record this interaction in the turn manager (granular counter for chronology)
    turn_manager.record_interaction();
    tracing::info!(
        interaction = turn_manager.interaction(),
        round = turn_manager.round(),
        "turn_manager.record_interaction"
    );

    // Persist updated game state (location, narration log) for reconnection
    if !genre_slug.is_empty() && !world_slug.is_empty() {
        let location =
            extract_location_header(narration_text).unwrap_or_else(|| "Starting area".to_string());
        match state
            .persistence()
            .load(genre_slug, world_slug, player_name_for_save)
            .await
        {
            Ok(Some(saved)) => {
                let mut snapshot = saved.snapshot;
                snapshot.location = location;
                // Sync ALL game state to snapshot for persistence
                snapshot.turn_manager = turn_manager.clone();
                snapshot.npc_registry = npc_registry.clone();
                snapshot.axis_values = axis_values.clone();
                snapshot.combat = combat_state.clone();
                snapshot.chase = chase_state.clone();
                snapshot.quest_log = quest_log.clone();
                snapshot.discovered_regions = discovered_regions.clone();
                snapshot.active_tropes = trope_states.clone();
                // Sync character state (HP, XP, level, inventory, known_facts, affinities)
                if let Some(ref cj) = character_json {
                    if let Ok(ch) = serde_json::from_value::<sidequest_game::Character>(cj.clone()) {
                        if let Some(saved_ch) = snapshot.characters.first_mut() {
                            saved_ch.core.hp = *hp;
                            saved_ch.core.max_hp = *max_hp;
                            saved_ch.core.level = *level;
                            saved_ch.core.inventory = inventory.clone();
                            saved_ch.known_facts = ch.known_facts.clone();
                            saved_ch.affinities = ch.affinities.clone();
                            saved_ch.narrative_state = ch.narrative_state.clone();
                        }
                    }
                }
                // Append narration to log for recap on reconnect
                snapshot.narrative_log.push(sidequest_game::NarrativeEntry {
                    timestamp: 0,
                    round: turn_manager.interaction() as u32,
                    author: "narrator".to_string(),
                    content: clean_narration.clone(),
                    tags: vec![],
                    encounter_tags: vec![],
                    speaker: None,
                    entry_type: None,
                });
                match state
                    .persistence()
                    .save(genre_slug, world_slug, player_name_for_save, &snapshot)
                    .await
                {
                    Ok(_) => tracing::info!(
                        player = %player_name_for_save,
                        turn = turn_manager.interaction(),
                        location = %current_location,
                        hp = *hp,
                        items = inventory.items.len(),
                        "session.saved — game state persisted"
                    ),
                    Err(e) => tracing::warn!(error = %e, "Failed to persist updated game state"),
                }
            }
            Ok(None) => {
                tracing::debug!("No saved session to update");
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to load session for persistence update");
            }
        }
    }

    // TTS streaming — segment narration and spawn background synthesis task
    if !clean_narration.is_empty() && !state.tts_disabled() {
        let segmenter = sidequest_game::SentenceSegmenter::new();
        let segments = segmenter.segment(&clean_narration);
        tracing::info!(
            segment_count = segments.len(),
            narration_len = clean_narration.len(),
            "tts.segmented"
        );
        if !segments.is_empty() {
            let tts_segments: Vec<sidequest_game::tts_stream::TtsSegment> = segments
                .iter()
                .map(|seg| sidequest_game::tts_stream::TtsSegment {
                    text: strip_markdown_for_tts(&seg.text),
                    index: seg.index,
                    is_last: seg.is_last,
                    speaker: "narrator".to_string(),
                    pause_after_ms: if seg.is_last { 0 } else { 200 },
                })
                .collect();

            let player_id_for_tts = player_id.to_string();
            let state_for_tts = state.clone();
            let ss_holder_for_tts = shared_session_holder.clone();
            let tts_config = sidequest_game::tts_stream::TtsStreamConfig::default();
            let streamer = sidequest_game::tts_stream::TtsStreamer::new(tts_config);

            // Clone Arcs for the spawned TTS task (mixer ducking + prerender)
            let mixer_for_tts = std::sync::Arc::clone(audio_mixer);
            let prerender_for_tts = std::sync::Arc::clone(prerender_scheduler);
            let genre_slug_for_tts = genre_slug.to_string();
            let tts_segments_for_prerender = tts_segments.clone();
            let prerender_ctx = sidequest_game::PrerenderContext {
                in_combat: combat_state.in_combat(),
                combatant_names: if combat_state.in_combat() {
                    result.npcs_present.iter().map(|npc| npc.name.clone()).collect()
                } else {
                    vec![]
                },
                pending_destination: extract_location_header(narration_text).map(|s| s.to_string()),
                active_dialogue_npc: npc_registry.last().map(|e| e.name.clone()),
                art_style: match visual_style {
                    Some(ref vs) => vs.positive_suffix.clone(),
                    None => "oil_painting".to_string(),
                },
            };

            tokio::spawn(async move {
                let (msg_tx, mut msg_rx) =
                    tokio::sync::mpsc::channel::<sidequest_game::tts_stream::TtsMessage>(32);

                // Connect to daemon for synthesis
                let daemon_config = sidequest_daemon_client::DaemonConfig::default();
                let synthesizer = match sidequest_daemon_client::DaemonClient::connect(
                    daemon_config,
                )
                .await
                {
                    Ok(client) => DaemonSynthesizer {
                        client: tokio::sync::Mutex::new(client),
                    },
                    Err(e) => {
                        tracing::warn!(error = %e, "TTS daemon unavailable — skipping voice synthesis");
                        return;
                    }
                };

                // Spawn the streamer pipeline
                let stream_handle = tokio::spawn(async move {
                    if let Err(e) = streamer.stream(tts_segments, &synthesizer, msg_tx).await {
                        tracing::warn!(error = %e, "TTS stream failed");
                    }
                });

                // Helper: send a game message to the acting player only.
                // In multiplayer, routes via session channel. Single-player falls back to global broadcast.
                let send_to_acting_player = |msg: GameMessage, ss_holder: &Arc<tokio::sync::Mutex<Option<Arc<tokio::sync::Mutex<shared_session::SharedGameSession>>>>>, pid: &str, fallback_state: &AppState| {
                    let ss_holder = ss_holder.clone();
                    let pid = pid.to_string();
                    let fallback_state = fallback_state.clone();
                    let msg = msg.clone();
                    let msg_type = format!("{:?}", std::mem::discriminant(&msg));
                    tokio::spawn(async move {
                        let holder = ss_holder.lock().await;
                        if let Some(ref ss_arc) = *holder {
                            let ss = ss_arc.lock().await;
                            tracing::debug!(player_id = %pid, msg_type = %msg_type, "tts.send_to_acting_player — via session channel");
                            ss.send_to_player(msg, pid);
                        } else {
                            tracing::debug!(player_id = %pid, msg_type = %msg_type, "tts.send_to_acting_player — via global broadcast (single-player)");
                            let _ = fallback_state.broadcast(msg);
                        }
                    });
                };

                // Bridge TtsMessage → binary frames (chunks) or GameMessage (start/end)
                while let Some(tts_msg) = msg_rx.recv().await {
                    match tts_msg {
                        sidequest_game::tts_stream::TtsMessage::Start { total_segments } => {
                            // Duck audio channels during TTS — audio cues go to acting player only
                            {
                                let mut mixer_guard = mixer_for_tts.lock().await;
                                if let Some(ref mut mixer) = *mixer_guard {
                                    for duck_cue in mixer.on_tts_start() {
                                        send_to_acting_player(
                                            audio_cue_to_game_message(
                                                &duck_cue,
                                                &player_id_for_tts,
                                                &genre_slug_for_tts,
                                                None,
                                            ),
                                            &ss_holder_for_tts,
                                            &player_id_for_tts,
                                            &state_for_tts,
                                        );
                                    }
                                }
                            }
                            // Speculative prerender during TTS playback
                            {
                                let mut prerender_guard = prerender_for_tts.lock().await;
                                if let Some(ref mut prerender) = *prerender_guard {
                                    if let Some(subject) = prerender
                                        .on_tts_start(&tts_segments_for_prerender, &prerender_ctx)
                                    {
                                        if let Some(ref queue) = state_for_tts.inner.render_queue {
                                            let _ = queue
                                                .enqueue(
                                                    subject,
                                                    &prerender_ctx.art_style,
                                                    "flux-schnell",
                                                )
                                                .await;
                                        }
                                    }
                                }
                            }
                            let game_msg = GameMessage::TtsStart {
                                payload: sidequest_protocol::TtsStartPayload { total_segments },
                                player_id: player_id_for_tts.clone(),
                            };
                            send_to_acting_player(game_msg, &ss_holder_for_tts, &player_id_for_tts, &state_for_tts);
                        }
                        sidequest_game::tts_stream::TtsMessage::Chunk(chunk) => {
                            // Send NARRATION_CHUNK to acting player only (not global broadcast).
                            // Text reveals sentence-by-sentence synchronized with TTS playback.
                            if let Some(seg) = tts_segments_for_prerender.get(chunk.segment_index) {
                                let chunk_msg = GameMessage::NarrationChunk {
                                    payload: sidequest_protocol::NarrationChunkPayload {
                                        text: seg.text.clone(),
                                    },
                                    player_id: player_id_for_tts.clone(),
                                };
                                send_to_acting_player(chunk_msg, &ss_holder_for_tts, &player_id_for_tts, &state_for_tts);
                            }

                            // Build binary voice frame: [4-byte header len][JSON header][audio bytes]
                            // The daemon always returns raw PCM s16le — use that format string
                            // so the UI routes to playVoicePCM instead of decodeAudioData.
                            // NOTE: binary frames still use global broadcast — binary channel
                            // doesn't support per-player targeting yet.
                            let header = serde_json::json!({
                                "type": "VOICE_AUDIO",
                                "segment_id": format!("seg_{}", chunk.segment_index),
                                "sample_rate": 24000,
                                "format": "pcm_s16le"
                            });
                            let header_bytes = serde_json::to_vec(&header).unwrap_or_default();
                            let audio_bytes = &chunk.audio_raw;
                            let mut frame =
                                Vec::with_capacity(4 + header_bytes.len() + audio_bytes.len());
                            frame.extend_from_slice(&(header_bytes.len() as u32).to_be_bytes());
                            frame.extend_from_slice(&header_bytes);
                            frame.extend_from_slice(audio_bytes);
                            state_for_tts.broadcast_binary(frame);
                        }
                        sidequest_game::tts_stream::TtsMessage::End => {
                            // Restore audio channels after TTS — acting player only
                            {
                                let mut mixer_guard = mixer_for_tts.lock().await;
                                if let Some(ref mut mixer) = *mixer_guard {
                                    for restore_cue in mixer.on_tts_end() {
                                        send_to_acting_player(
                                            audio_cue_to_game_message(
                                                &restore_cue,
                                                &player_id_for_tts,
                                                &genre_slug_for_tts,
                                                None,
                                            ),
                                            &ss_holder_for_tts,
                                            &player_id_for_tts,
                                            &state_for_tts,
                                        );
                                    }
                                }
                            }
                            // Clear prerender pending state
                            {
                                let mut prerender_guard = prerender_for_tts.lock().await;
                                if let Some(ref mut prerender) = *prerender_guard {
                                    prerender.on_tts_end();
                                }
                            }
                            let game_msg = GameMessage::TtsEnd {
                                player_id: player_id_for_tts.clone(),
                            };
                            send_to_acting_player(game_msg, &ss_holder_for_tts, &player_id_for_tts, &state_for_tts);
                        }
                    }
                }

                let _ = stream_handle.await;
                tracing::info!(player_id = %player_id_for_tts, "TTS stream complete");
            });
        }
    }

    // GM Panel: emit full game state snapshot after all mutations
    {
        let turn_approx = turn_manager.interaction() as u32;
        let npc_data: Vec<serde_json::Value> = npc_registry
            .iter()
            .map(|e| {
                serde_json::json!({
                    "name": e.name,
                    "pronouns": e.pronouns,
                    "role": e.role,
                    "location": e.location,
                    "last_seen_turn": e.last_seen_turn,
                })
            })
            .collect();
        let inventory_names: Vec<String> = inventory
            .items
            .iter()
            .map(|i| i.name.as_str().to_string())
            .collect();
        let active_tropes: Vec<serde_json::Value> = trope_states
            .iter()
            .map(|ts| {
                serde_json::json!({
                    "id": ts.trope_definition_id(),
                    "progression": ts.progression(),
                    "status": format!("{:?}", ts.status()),
                })
            })
            .collect();
        state.send_watcher_event(WatcherEvent {
            timestamp: chrono::Utc::now(),
            component: "game".to_string(),
            event_type: WatcherEventType::StateTransition,
            severity: Severity::Info,
            fields: {
                let mut f = HashMap::new();
                f.insert(
                    "event".to_string(),
                    serde_json::json!("game_state_snapshot"),
                );
                f.insert("turn_number".to_string(), serde_json::json!(turn_approx));
                f.insert(
                    "location".to_string(),
                    serde_json::json!(current_location.as_str()),
                );
                f.insert("hp".to_string(), serde_json::json!(*hp));
                f.insert("max_hp".to_string(), serde_json::json!(*max_hp));
                f.insert("level".to_string(), serde_json::json!(*level));
                f.insert("xp".to_string(), serde_json::json!(*xp));
                f.insert("inventory".to_string(), serde_json::json!(inventory_names));
                f.insert("npc_registry".to_string(), serde_json::json!(npc_data));
                f.insert(
                    "active_tropes".to_string(),
                    serde_json::json!(active_tropes),
                );
                f.insert(
                    "in_combat".to_string(),
                    serde_json::json!(combat_state.in_combat()),
                );
                f.insert("player_id".to_string(), serde_json::json!(player_id));
                f.insert("character".to_string(), serde_json::json!(char_name));
                f
            },
        });
    }

    // Sync world-level state back to shared session and broadcast narration
    {
        let holder = shared_session_holder.lock().await;
        if let Some(ref ss_arc) = *holder {
            let mut ss = ss_arc.lock().await;
            ss.sync_from_locals(
                current_location,
                npc_registry,
                narration_history,
                discovered_regions,
                trope_states,
                player_id,
            );
            // Sync acting player's character data to PlayerState for other players' PARTY_STATUS
            if let Some(ps) = ss.players.get_mut(player_id) {
                ps.character_hp = *hp;
                ps.character_max_hp = *max_hp;
                ps.character_level = *level;
                ps.character_xp = *xp;
                ps.character_class = char_class.to_string();
                ps.inventory = inventory.clone();
                ps.combat_state = combat_state.clone();
                ps.chase_state = chase_state.clone();
                if ps.character_name.is_none() {
                    ps.character_name = Some(char_name.to_string());
                }
            }
            // Route messages to session members.
            // The acting player already receives via their direct tx channel (mpsc).
            // Other players get narration (without state_delta) via the session broadcast channel.
            // Fall back to all session members when cartography regions aren't available.
            let co_located = ss.co_located_players(player_id);
            let other_players: Vec<String> = if co_located.is_empty() {
                // No region data — fall back to all other session members
                ss.players.keys().filter(|pid| pid.as_str() != player_id).cloned().collect()
            } else {
                co_located
            };
            for msg in &messages {
                match msg {
                    GameMessage::Narration { payload, .. } => {
                        // Send the acting player's action to observers FIRST.
                        // This creates a turn boundary in NarrativeView (PLAYER_ACTION triggers flushChunks).
                        let observer_action = GameMessage::PlayerAction {
                            payload: sidequest_protocol::PlayerActionPayload {
                                action: format!("{} — {}", char_name, effective_action),
                                aside: false,
                            },
                            player_id: player_id.to_string(),
                        };
                        tracing::info!(
                            char_name = %char_name,
                            observer_count = other_players.len(),
                            "multiplayer.observer_action — broadcasting PLAYER_ACTION to observers"
                        );
                        for target_id in &other_players {
                            ss.send_to_player(observer_action.clone(), target_id.clone());
                        }
                        // Send narration (state_delta stripped) to other players.
                        // Apply perception filters if active.
                        for target_id in &other_players {
                            let text = if let Some(filter) = ss.perception_filters.get(target_id) {
                                let effects_desc = sidequest_game::perception::PerceptionRewriter::describe_effects(filter.effects());
                                format!(
                                    "[Your perception is altered: {}]\n\n{}",
                                    effects_desc, payload.text
                                )
                            } else {
                                payload.text.clone()
                            };
                            let narration_msg = GameMessage::Narration {
                                payload: sidequest_protocol::NarrationPayload {
                                    text,
                                    state_delta: None,
                                    footnotes: payload.footnotes.clone(),
                                },
                                player_id: target_id.clone(),
                            };
                            ss.send_to_player(narration_msg, target_id.clone());
                        }
                        tracing::info!(
                            observer_count = other_players.len(),
                            text_len = payload.text.len(),
                            "multiplayer.narration_broadcast — sent to observers via session channel"
                        );
                    }
                    GameMessage::NarrationEnd { .. } => {
                        // Broadcast NarrationEnd to all players so TTS sync works correctly
                        let player_ids: Vec<String> = ss.players.keys().cloned().collect();
                        for target_pid in &player_ids {
                            let end_msg = GameMessage::NarrationEnd {
                                payload: NarrationEndPayload { state_delta: None },
                                player_id: target_pid.clone(),
                            };
                            ss.send_to_player(end_msg, target_pid.clone());
                        }
                        // TURN_STATUS "resolved" — unlock input for all players after narration completes.
                        // Use global broadcast (not session channel) for reliability — session
                        // subscribers may miss messages sent before subscription.
                        if ss.players.len() > 1 {
                            let resolved_msg = GameMessage::TurnStatus {
                                payload: TurnStatusPayload {
                                    player_name: player_name_for_save.to_string(),
                                    status: "resolved".into(),
                                    state_delta: None,
                                },
                                player_id: player_id.to_string(),
                            };
                            let _ = state.broadcast(resolved_msg);
                            tracing::info!(player_name = %player_name_for_save, "turn_status.resolved broadcast to all clients");
                        }
                    }
                    GameMessage::ChapterMarker { ref payload, .. } => {
                        // Send only to OTHER players — the acting player already
                        // received this ChapterMarker via the direct response channel.
                        for target_pid in &other_players {
                            let marker = GameMessage::ChapterMarker {
                                payload: payload.clone(),
                                player_id: target_pid.clone(),
                            };
                            ss.send_to_player(marker, target_pid.clone());
                        }
                    }
                    GameMessage::PartyStatus { .. } => {
                        // Build targeted PARTY_STATUS per player so every player's
                        // player_id is set correctly (client HUD uses this for identity).
                        let members: Vec<PartyMember> = ss
                            .players
                            .iter()
                            .map(|(pid, ps)| PartyMember {
                                player_id: pid.clone(),
                                name: ps.player_name.clone(),
                                character_name: ps.character_name.clone().unwrap_or_else(|| ps.player_name.clone()),
                                current_hp: ps.character_hp,
                                max_hp: ps.character_max_hp,
                                statuses: vec![],
                                class: ps.character_class.clone(),
                                level: ps.character_level,
                                portrait_url: None,
                            })
                            .collect();
                        let player_ids: Vec<String> = ss.players.keys().cloned().collect();
                        for target_pid in &player_ids {
                            let party_msg = GameMessage::PartyStatus {
                                payload: PartyStatusPayload { members: members.clone() },
                                player_id: target_pid.clone(),
                            };
                            ss.send_to_player(party_msg, target_pid.clone());
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    messages
}

/// DaemonSynthesizer — implements TtsSynthesizer for the real daemon client.
struct DaemonSynthesizer {
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

/// Extract a location header from narration text.
///
/// Checks the first 3 non-empty lines for location patterns:
/// - `**Location Name**` (bold header — primary format)
/// - `## Location Name` (markdown h2)
/// - `[Location: Name]` (bracketed tag)
fn extract_location_header(text: &str) -> Option<String> {
    for line in text.lines().take(3) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Bold header: **Location Name**
        if trimmed.starts_with("**") && trimmed.ends_with("**") && trimmed.len() > 4 {
            return Some(trimmed[2..trimmed.len() - 2].to_string());
        }
        // Markdown h2: ## Location Name
        if trimmed.starts_with("## ") && trimmed.len() > 3 {
            return Some(trimmed[3..].trim().to_string());
        }
        // Bracketed tag: [Location: Name]
        if trimmed.starts_with("[Location:") && trimmed.ends_with(']') {
            let inner = &trimmed[10..trimmed.len() - 1].trim();
            if !inner.is_empty() {
                return Some(inner.to_string());
            }
        }
        // Only check the first non-empty line for the primary format,
        // but continue checking for h2/bracketed in lines 2-3.
        break;
    }
    // Second pass: check lines 2-3 for any format (narrator sometimes
    // puts flavor text before the location header)
    for line in text.lines().skip(1).take(2) {
        let trimmed = line.trim();
        if trimmed.starts_with("**") && trimmed.ends_with("**") && trimmed.len() > 4 {
            return Some(trimmed[2..trimmed.len() - 2].to_string());
        }
        if trimmed.starts_with("## ") && trimmed.len() > 3 {
            return Some(trimmed[3..].trim().to_string());
        }
    }
    None
}

/// Strip the location header line from narration text.
/// Handles all formats recognized by extract_location_header.
fn strip_location_header(text: &str) -> String {
    // Find which line (if any) contains the location header
    for (i, line) in text.lines().take(3).enumerate() {
        let trimmed = line.trim();
        let is_header = (trimmed.starts_with("**") && trimmed.ends_with("**") && trimmed.len() > 4)
            || (trimmed.starts_with("## ") && trimmed.len() > 3)
            || (trimmed.starts_with("[Location:") && trimmed.ends_with(']'));
        if is_header {
            return text
                .lines()
                .enumerate()
                .filter(|(idx, _)| *idx != i)
                .map(|(_, l)| l)
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_string();
        }
    }
    text.to_string()
}

/// Bug 5: Extract item acquisitions from narration text.
///
/// Looks for patterns like "you pick up {item}", "you find {item}", "receives {item}", etc.
/// Returns a list of (item_name, item_type) tuples.
fn extract_items_from_narration(text: &str) -> Vec<(String, String)> {
    let text_lower = text.to_lowercase();
    let mut items = Vec::new();

    // Tightened patterns — require 2nd person ("you") to avoid matching
    // NPC dialogue and reported speech. Removed ambiguous patterns like
    // "hands you", "gives you" which trigger on dialogue too easily.
    let acquisition_patterns = [
        "you pick up ",
        "you find a ",
        "you find an ",
        "you find the ",
        "you found a ",
        "you found an ",
        "you found the ",
        "you acquire ",
        "you take the ",
        "you grab the ",
        "you pocket the ",
        "you loot ",
    ];

    for pattern in &acquisition_patterns {
        let mut search_from = 0;
        while let Some(pos) = text_lower[search_from..].find(pattern) {
            let start = search_from + pos + pattern.len();
            if start >= text_lower.len() {
                break;
            }
            // Extract the item name: take until punctuation or newline
            let rest = &text[start..];
            let end = rest
                .find(|c: char| matches!(c, '.' | ',' | '!' | '?' | '\n' | ';' | ':'))
                .unwrap_or(rest.len());
            let item_name = rest[..end].trim();
            // Skip if too short
            if item_name.len() >= 3 {
                // Strip leading articles
                let after_article = item_name
                    .strip_prefix("a ")
                    .or_else(|| item_name.strip_prefix("an "))
                    .or_else(|| item_name.strip_prefix("the "))
                    .or_else(|| item_name.strip_prefix("some "))
                    .unwrap_or(item_name)
                    .trim();
                // Truncate at prepositional phrases and adverbs to get clean item names.
                // "compass with both hands" → "compass", "hammer again" → "hammer"
                let stop_words = [
                    " with ",
                    " from ",
                    " into ",
                    " onto ",
                    " against ",
                    " across ",
                    " along ",
                    " through ",
                    " around ",
                    " behind ",
                    " before ",
                    " after ",
                    " again",
                    " as ",
                    " and then",
                    " while ",
                    " that ",
                    " which ",
                ];
                let mut clean_end = after_article.len();
                for sw in &stop_words {
                    if let Some(pos) = after_article.to_lowercase().find(sw) {
                        if pos > 0 && pos < clean_end {
                            clean_end = pos;
                        }
                    }
                }
                // Also cap at 4 words max for item names
                let words: Vec<&str> = after_article[..clean_end].split_whitespace().collect();
                let clean_name = if words.len() > 4 {
                    words[..4].join(" ")
                } else {
                    words.join(" ")
                };
                if clean_name.len() >= 2 {
                    // Simple category heuristic
                    let lower_name = clean_name.to_lowercase();
                    let category = if lower_name.contains("sword")
                        || lower_name.contains("blade")
                        || lower_name.contains("axe")
                        || lower_name.contains("dagger")
                        || lower_name.contains("weapon")
                    {
                        "weapon"
                    } else if lower_name.contains("armor")
                        || lower_name.contains("shield")
                        || lower_name.contains("helmet")
                        || lower_name.contains("plate")
                    {
                        "armor"
                    } else if lower_name.contains("potion")
                        || lower_name.contains("salve")
                        || lower_name.contains("herb")
                        || lower_name.contains("food")
                        || lower_name.contains("drink")
                    {
                        "consumable"
                    } else if lower_name.contains("key")
                        || lower_name.contains("tool")
                        || lower_name.contains("rope")
                        || lower_name.contains("torch")
                        || lower_name.contains("lantern")
                    {
                        "tool"
                    } else if lower_name.contains("coin")
                        || lower_name.contains("gem")
                        || lower_name.contains("gold")
                        || lower_name.contains("jewel")
                    {
                        "treasure"
                    } else {
                        "misc"
                    };
                    items.push((clean_name.to_string(), category.to_string()));
                }
            }
            search_from = start;
        }
    }

    items
}

/// Strip markdown syntax from text for TTS voice synthesis.
/// Removes bold (**), italic (*/_), headers (#), links, images, code blocks,
/// and footnote markers ([1], [2], etc.) that cause phonemizer word-count mismatches.
fn strip_markdown_for_tts(text: &str) -> String {
    let mut result = text.to_string();
    // Bold and italic: **text**, *text*, __text__, _text_
    // Process ** before * to avoid partial matches
    result = result.replace("**", "");
    result = result.replace("__", "");
    // Single * and _ as italic markers (only between word boundaries)
    // Simple approach: remove standalone * and _ that look like formatting
    let mut cleaned = String::with_capacity(result.len());
    let chars: Vec<char> = result.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if (chars[i] == '*' || chars[i] == '_')
            && i + 1 < chars.len()
            && chars[i + 1].is_alphanumeric()
        {
            // Skip opening italic marker
            i += 1;
            continue;
        }
        if (chars[i] == '*' || chars[i] == '_') && i > 0 && chars[i - 1].is_alphanumeric() {
            // Skip closing italic marker
            i += 1;
            continue;
        }
        cleaned.push(chars[i]);
        i += 1;
    }
    // Remove markdown headers (# at start of line)
    cleaned = cleaned
        .lines()
        .map(|line| line.trim_start_matches('#').trim_start())
        .collect::<Vec<_>>()
        .join("\n");
    // Remove footnote markers [1], [2], etc. — these cause phonemizer
    // word-count mismatches because they aren't natural language tokens.
    // Also remove any bracketed numbers like [12] from narrator output.
    let mut tts_clean = String::with_capacity(cleaned.len());
    let clean_chars: Vec<char> = cleaned.chars().collect();
    let mut j = 0;
    while j < clean_chars.len() {
        if clean_chars[j] == '[' {
            // Look ahead for a closing bracket with only digits inside
            if let Some(close) = clean_chars[j + 1..].iter().position(|&c| c == ']') {
                let inside = &clean_chars[j + 1..j + 1 + close];
                if !inside.is_empty() && inside.iter().all(|c| c.is_ascii_digit()) {
                    // Skip the entire [N] marker
                    j += close + 2; // skip past ']'
                    continue;
                }
            }
        }
        tts_clean.push(clean_chars[j]);
        j += 1;
    }
    // Collapse any double-spaces left by removed markers
    while tts_clean.contains("  ") {
        tts_clean = tts_clean.replace("  ", " ");
    }
    tts_clean.trim().to_string()
}

/// Extract item losses from narration — trades, gifts, drops.
/// Returns a list of item names that the player lost.
fn extract_item_losses(text: &str) -> Vec<String> {
    let text_lower = text.to_lowercase();
    let mut lost = Vec::new();

    let loss_patterns = [
        "hand over ",
        "hands over ",
        "give away ",
        "gives away ",
        "trade the ",
        "trades the ",
        "trading the ",
        "hand the ",
        "hands the ",
        "surrender the ",
        "surrenders the ",
        "drop the ",
        "drops the ",
        "toss the ",
        "tosses the ",
        "you give ",
        "you hand ",
        "you trade ",
        "you surrender ",
        "you drop ",
        "you toss ",
        "parts with the ",
        "part with the ",
        "relinquish the ",
        "relinquishes the ",
    ];

    for pattern in &loss_patterns {
        let mut search_from = 0;
        while let Some(pos) = text_lower[search_from..].find(pattern) {
            let start = search_from + pos + pattern.len();
            if start >= text_lower.len() {
                break;
            }
            let rest = &text[start..];
            let end = rest
                .find(|c: char| matches!(c, '.' | ',' | '!' | '?' | '\n' | ';' | ':'))
                .unwrap_or(rest.len());
            let item_name = rest[..end].trim();
            if item_name.len() >= 2 && item_name.len() <= 60 {
                let after_article = item_name
                    .strip_prefix("a ")
                    .or_else(|| item_name.strip_prefix("an "))
                    .or_else(|| item_name.strip_prefix("the "))
                    .or_else(|| item_name.strip_prefix("some "))
                    .unwrap_or(item_name)
                    .trim();
                // Truncate at prepositions (same as acquisition extraction)
                let stop_words = [" to ", " for ", " in ", " with ", " from ", " as "];
                let mut clean_end = after_article.len();
                for sw in &stop_words {
                    if let Some(p) = after_article.to_lowercase().find(sw) {
                        if p > 0 && p < clean_end {
                            clean_end = p;
                        }
                    }
                }
                let words: Vec<&str> = after_article[..clean_end].split_whitespace().collect();
                let clean_name = if words.len() > 4 {
                    words[..4].join(" ")
                } else {
                    words.join(" ")
                };
                if clean_name.len() >= 2 {
                    lost.push(clean_name);
                }
            }
            search_from = start;
        }
    }

    lost
}

/// NPC registry entry — re-exported from sidequest-game for persistence.
pub(crate) type NpcRegistryEntry = sidequest_game::NpcRegistryEntry;

/// Extract NPC names from narration text and update the registry.
/// Looks for patterns like dialogue attribution ("Name says", "Name asks")
/// and introduction patterns ("a woman named Name", "Name, the blacksmith").
fn update_npc_registry(
    registry: &mut Vec<NpcRegistryEntry>,
    narration: &str,
    current_location: &str,
    turn_count: u32,
    location_names: &[&str],
) {
    // Build a set of names that should never become NPCs (location/region names).
    let rejected: Vec<String> = std::iter::once(current_location)
        .chain(location_names.iter().copied())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase())
        .collect();
    let is_location_name = |name: &str| -> bool {
        let lower = name.to_lowercase();
        rejected
            .iter()
            .any(|loc| lower == *loc || loc.contains(&lower) || lower.contains(loc.as_str()))
    };

    // Common English words that should never be NPC names. These frequently
    // appear capitalized at sentence starts and match our regex patterns.
    const COMMON_WORDS: &[&str] = &[
        "the", "a", "an", "it", "its", "this", "that", "these", "those",
        "there", "here", "then", "now", "but", "and", "or", "yet", "so",
        "you", "your", "my", "our", "their", "his", "her", "we", "they",
        "she", "he", "him", "hers", "herself", "himself", "themselves", "itself",
        "i", "me", "us", "them", "who", "whom", "whose", "what", "which",
        "something", "someone", "somebody", "nothing", "nobody", "anyone",
        "anything", "everything", "everyone", "one", "each", "every",
        "another", "other", "others", "both", "few", "many", "some",
        "after", "before", "above", "below", "behind", "between",
        "perhaps", "maybe", "also", "still", "just", "only", "even",
        "soon", "once", "when", "where", "while", "since", "until",
        "though", "although", "however", "meanwhile", "suddenly",
        "slowly", "quickly", "finally", "somehow",
    ];
    let is_common_word = |name: &str| -> bool {
        let lower = name.to_lowercase();
        // Single-word candidates: reject if they're a common word
        if !name.contains(' ') {
            return COMMON_WORDS.contains(&lower.as_str());
        }
        // Multi-word candidates: reject if the first word is a common non-name word
        // (e.g., "Something dark" → "something" is not a name)
        if let Some(first) = lower.split_whitespace().next() {
            if COMMON_WORDS.contains(&first) {
                return true;
            }
        }
        false
    };

    // Dialogue attribution: "Name says/asks/replies/shouts/whispers/mutters"
    let speech_verbs = [
        "says", "asks", "replies", "shouts", "whispers", "mutters", "growls", "calls", "declares",
        "speaks",
    ];
    let text_lower = narration.to_lowercase();

    for verb in &speech_verbs {
        let pattern = format!(" {}", verb);
        let mut search_from = 0;
        while let Some(pos) = text_lower[search_from..].find(&pattern) {
            let abs_pos = search_from + pos;
            // Walk backward to find the start of the name (capital letter after punctuation/newline/start)
            let before = &narration[..abs_pos];
            // Find the last sentence boundary before this verb
            let name_start = before
                .rfind(|c: char| matches!(c, '.' | '!' | '?' | '\n' | '"' | '\u{201c}'))
                .map(|i| i + 1)
                .unwrap_or(0);
            let candidate = before[name_start..].trim();
            // A valid NPC name: starts with uppercase, 2-40 chars, no lowercase-only words that look like common text
            if candidate.len() >= 2
                && candidate.len() <= 40
                && candidate.chars().next().map_or(false, |c| c.is_uppercase())
            {
                let name = candidate.to_string();
                // Skip common words, location names, and player references
                if !is_common_word(&name)
                    && !is_location_name(&name)
                {
                    // Update existing or add new — also check substring matches
                    // (e.g., "Toggler" is a substring of "Toggler Copperjaw")
                    let name_lower = name.to_lowercase();
                    if let Some(entry) = registry.iter_mut().find(|e| {
                        e.name == name
                            || e.name.to_lowercase().contains(&name_lower)
                            || name_lower.contains(&e.name.to_lowercase())
                    }) {
                        entry.last_seen_turn = turn_count;
                        if !current_location.is_empty() {
                            entry.location = current_location.to_string();
                        }
                        // If the new name is longer (more specific), upgrade
                        if name.len() > entry.name.len() {
                            entry.name = name;
                        }
                    } else {
                        registry.push(NpcRegistryEntry {
                            name,
                            pronouns: String::new(),
                            role: String::new(),
                            location: current_location.to_string(),
                            last_seen_turn: turn_count,
                            age: String::new(),
                            appearance: String::new(),
                        });
                    }
                }
            }
            search_from = abs_pos + 1;
        }
    }

    // Introduction patterns: "a woman named X", "a man called X", "named X", "called X"
    let intro_patterns = ["named ", "called ", "known as "];
    for pat in &intro_patterns {
        let mut search_from = 0;
        while let Some(pos) = text_lower[search_from..].find(pat) {
            let abs_pos = search_from + pos + pat.len();
            if abs_pos < narration.len() {
                // Extract the name that follows the pattern
                let rest = &narration[abs_pos..];
                let name_end = rest
                    .find(|c: char| {
                        matches!(c, ',' | '.' | '!' | '?' | ';' | '\n' | '"' | '\u{201d}')
                    })
                    .unwrap_or(rest.len());
                let candidate = rest[..name_end].trim();
                if candidate.len() >= 2
                    && candidate.len() <= 40
                    && candidate.chars().next().map_or(false, |c| c.is_uppercase())
                {
                    let name = candidate.to_string();
                    if !is_common_word(&name) && !is_location_name(&name) {
                        if !registry.iter().any(|e| e.name == name) {
                            // Try to infer role from "X, the blacksmith" pattern after name
                            let role = if name_end < rest.len() {
                                let after_name = &rest[name_end..];
                                if after_name.starts_with(", the ")
                                    || after_name.starts_with(", a ")
                                {
                                    let role_start =
                                        after_name.find(' ').map(|i| i + 1).unwrap_or(0);
                                    let role_text = &after_name[role_start..];
                                    let role_end = role_text
                                        .find(|c: char| matches!(c, ',' | '.' | '!' | '?'))
                                        .unwrap_or(role_text.len().min(40));
                                    role_text[..role_end].trim().to_string()
                                } else {
                                    String::new()
                                }
                            } else {
                                String::new()
                            };
                            registry.push(NpcRegistryEntry {
                                name,
                                pronouns: String::new(),
                                role,
                                location: current_location.to_string(),
                                last_seen_turn: turn_count,
                                age: String::new(),
                                appearance: String::new(),
                            });
                        }
                    }
                }
            }
            search_from = abs_pos;
        }
    }

    // Appositive pattern: "Name, the blacksmith" / "Name, a merchant"
    {
        use regex::Regex;
        let appos_re =
            Regex::new(r"\b([A-Z][a-z]+(?:\s[A-Z][a-z]+)?), (?:the|a|an) ([a-z][a-z ]{1,30})")
                .unwrap();
        for caps in appos_re.captures_iter(narration) {
            let name = caps[1].to_string();
            let role = caps[2]
                .trim_end_matches(|c: char| matches!(c, ',' | '.' | '!' | '?'))
                .trim()
                .to_string();
            if !is_common_word(&name) && !is_location_name(&name) {
                if let Some(entry) = registry.iter_mut().find(|e| e.name == name) {
                    if entry.role.is_empty() && !role.is_empty() {
                        entry.role = role;
                    }
                    entry.last_seen_turn = turn_count;
                    if !current_location.is_empty() {
                        entry.location = current_location.to_string();
                    }
                } else {
                    registry.push(NpcRegistryEntry {
                        name,
                        pronouns: String::new(),
                        role,
                        location: current_location.to_string(),
                        last_seen_turn: turn_count,
                        age: String::new(),
                        appearance: String::new(),
                    });
                }
            }
        }
    }

    // Proper nouns as sentence subjects: capitalized word(s) before a verb
    {
        use regex::Regex;
        let subject_re = Regex::new(r"(?:^|[.!?]\s+)([A-Z][a-z]+(?:\s[A-Z][a-z]+)?)\s+(?:is|was|has|had|walks|stands|sits|looks|turns|nods|shakes|moves|steps|reaches|pulls|holds|places|waves|smiles|frowns|laughs|sighs|watches|leads|appears|enters|exits|approaches|stares|glances|points|gestures|offers|hands|grabs|takes|gives|opens|closes|runs|stops|begins|continues)\b").unwrap();
        for caps in subject_re.captures_iter(narration) {
            let name = caps[1].to_string();
            if !is_common_word(&name) && !is_location_name(&name) {
                if let Some(entry) = registry.iter_mut().find(|e| e.name == name) {
                    entry.last_seen_turn = turn_count;
                    if !current_location.is_empty() {
                        entry.location = current_location.to_string();
                    }
                } else {
                    registry.push(NpcRegistryEntry {
                        name,
                        pronouns: String::new(),
                        role: String::new(),
                        location: current_location.to_string(),
                        last_seen_turn: turn_count,
                        age: String::new(),
                        appearance: String::new(),
                    });
                }
            }
        }
    }

    // Possessive form: "Name's" — extract names from possessive patterns
    {
        use regex::Regex;
        let poss_re = Regex::new(r"\b([A-Z][a-z]+(?:\s[A-Z][a-z]+)?)'s\b").unwrap();
        for caps in poss_re.captures_iter(narration) {
            let name = caps[1].to_string();
            if !is_common_word(&name) && !is_location_name(&name) {
                if let Some(entry) = registry.iter_mut().find(|e| e.name == name) {
                    entry.last_seen_turn = turn_count;
                    if !current_location.is_empty() {
                        entry.location = current_location.to_string();
                    }
                } else {
                    registry.push(NpcRegistryEntry {
                        name,
                        pronouns: String::new(),
                        role: String::new(),
                        location: current_location.to_string(),
                        last_seen_turn: turn_count,
                        age: String::new(),
                        appearance: String::new(),
                    });
                }
            }
        }
    }

    // Infer pronouns from narration context
    for entry in registry.iter_mut() {
        if !entry.pronouns.is_empty() {
            continue;
        }
        let name_lower = entry.name.to_lowercase();
        // Check narration for pronoun references near the name
        if let Some(name_pos) = text_lower.find(&name_lower) {
            let after = &text_lower[name_pos..];
            let window = &after[..after.len().min(200)];
            if window.contains(" she ") || window.contains(" her ") || window.contains(" hers ") {
                entry.pronouns = "she/her".to_string();
            } else if window.contains(" he ")
                || window.contains(" his ")
                || window.contains(" him ")
            {
                entry.pronouns = "he/him".to_string();
            } else if window.contains(" they ")
                || window.contains(" their ")
                || window.contains(" them ")
            {
                entry.pronouns = "they/them".to_string();
            }
        }
        // Default to they/them if no pronouns could be inferred
        if entry.pronouns.is_empty() {
            entry.pronouns = "they/them".to_string();
        }
    }
}

/// Build the NPC registry context string for the narrator prompt.
fn build_npc_registry_context(registry: &[NpcRegistryEntry]) -> String {
    if registry.is_empty() {
        return String::new();
    }
    let mut lines = vec!["\nACTIVE NPCs — CANONICAL IDENTITY (do NOT contradict):\nThese NPCs have been established in this session. Their names, pronouns, gender, physical appearance, and roles are LOCKED. If an NPC was described as male (\"Big man, missing an ear\"), they stay male in ALL future narration. Never flip gender, change names, or alter physical descriptions:".to_string()];
    for entry in registry {
        let mut desc = format!("- {}", entry.name);
        if !entry.pronouns.is_empty() {
            desc.push_str(&format!(" ({})", entry.pronouns));
        }
        if !entry.role.is_empty() {
            desc.push_str(&format!(", {}", entry.role));
        }
        // Physical description — age and appearance are identity-locked
        let mut physical: Vec<&str> = Vec::new();
        if !entry.age.is_empty() {
            physical.push(&entry.age);
        }
        if !entry.appearance.is_empty() {
            physical.push(&entry.appearance);
        }
        if !physical.is_empty() {
            desc.push_str(&format!(" [{}]", physical.join("; ")));
        }
        if !entry.location.is_empty() {
            desc.push_str(&format!(" — at {}", entry.location));
        }
        lines.push(desc);
    }
    lines.join("\n")
}

/// Build a name bank context string from genre pack cultures for the narrator prompt.
/// Extracts word lists and person name patterns so the LLM uses culturally appropriate names.
fn build_name_bank_context(cultures: &[sidequest_genre::Culture]) -> String {
    if cultures.is_empty() {
        return String::new();
    }
    let mut lines = vec!["\nNAME BANKS — When introducing new NPCs, you MUST draw names from these cultural name banks. Do NOT use generic Western fantasy names like Maren, Kael, or Ash.".to_string()];
    for culture in cultures {
        lines.push(format!(
            "\n## {} — {}",
            culture.name.as_str(),
            culture.description
        ));
        // Show word lists for each slot
        for (slot_name, slot) in &culture.slots {
            if let Some(ref words) = slot.word_list {
                if !words.is_empty() {
                    let sample: Vec<_> = words.iter().take(10).map(|s| s.as_str()).collect();
                    lines.push(format!("  {}: {}", slot_name, sample.join(", ")));
                }
            }
        }
        // Show person name patterns
        if !culture.person_patterns.is_empty() {
            lines.push(format!(
                "  Name patterns: {}",
                culture.person_patterns.join(", ")
            ));
        }
    }
    lines.join("\n")
}

/// Convert a game-internal AudioCue to a protocol GameMessage for WebSocket broadcast.
///
/// `genre_slug` is prepended to track paths so the client can fetch via `/genre/{slug}/{path}`.
/// `mood` is included so the client's deduplication logic works (it ignores cues without mood).
fn audio_cue_to_game_message(
    cue: &sidequest_game::AudioCue,
    player_id: &str,
    genre_slug: &str,
    mood: Option<&str>,
) -> GameMessage {
    let full_track = cue.track_id.as_ref().map(|path| {
        if genre_slug.is_empty() {
            path.clone()
        } else {
            format!("/genre/{}/{}", genre_slug, path)
        }
    });
    GameMessage::AudioCue {
        payload: AudioCuePayload {
            mood: mood.map(|s| s.to_string()),
            music_track: full_track,
            sfx_triggers: vec![],
            channel: Some(cue.channel.to_string()),
            action: Some(cue.action.to_string()),
            volume: Some(cue.volume),
        },
        player_id: player_id.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Watcher WebSocket Handler (Story 3-6)
// ---------------------------------------------------------------------------

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

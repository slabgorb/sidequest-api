//! SideQuest Server — axum HTTP/WebSocket server library.
//!
//! Exposes `build_router()`, `AppState`, and server lifecycle functions for the binary and tests.
//! The server depends on the `GameService` trait facade — never on game internals.

pub mod render_integration;

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
use sidequest_genre::{GenreCode, GenreLoader};
use sidequest_protocol::{
    AudioCuePayload, ChapterMarkerPayload, CharacterCreationPayload, CharacterSheetPayload,
    CharacterState, CombatEventPayload, ErrorPayload, GameMessage, InitialState,
    InventoryPayload, MapUpdatePayload, NarrationEndPayload, NarrationPayload, PartyMember,
    PartyStatusPayload, SessionEventPayload, ThinkingPayload,
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
                                let servable_url = if raw_path.starts_with('/') || raw_path.starts_with("C:\\") {
                                    let src = std::path::Path::new(raw_path);
                                    if let Some(filename) = src.file_name() {
                                        let renders_dir = std::env::var("SIDEQUEST_OUTPUT_DIR")
                                            .map(std::path::PathBuf::from)
                                            .unwrap_or_else(|_| {
                                                std::path::PathBuf::from(
                                                    std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string()),
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
                                } else if raw_path.starts_with("http://") || raw_path.starts_with("https://") || raw_path.starts_with("/api/") {
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
            }),
        }
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
                } = result
                {
                    // Rewrite absolute file paths to served URLs.
                    // The daemon returns paths like {output_dir}/flux/render_abc.png;
                    // strip the output_dir prefix and serve via /api/renders/{subpath}.
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
                        } else if let Some(filename) = img_path.file_name().and_then(|f| f.to_str()) {
                            format!("/api/renders/{}", filename)
                        } else {
                            image_url
                        }
                    };
                    let msg = GameMessage::Image {
                        payload: sidequest_protocol::ImagePayload {
                            url: served_url,
                            description: String::new(),
                            handout: false,
                            render_id: Some(job_id.to_string()),
                            tier: None,
                            scene_type: None,
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
            std::path::PathBuf::from(
                std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string()),
            )
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

    // Writer task: reads from mpsc channel + broadcast + binary, sends to WS
    let writer_player_id = player_id_str.clone();
    let writer_handle = tokio::spawn(async move {
        loop {
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
    let mut world_context: String = String::new();
    let mut visual_style: Option<sidequest_genre::VisualStyle> = None;
    let mut music_director: Option<sidequest_game::MusicDirector> = None;
    let mut npc_registry: Vec<NpcRegistryEntry> = vec![];
    // Bug 17: In-memory narration history for context accumulation across turns.
    // Each entry is "Player: <action>\nNarrator: <response>" for the last N turns.
    let mut narration_history: Vec<String> = vec![];
    let audio_mixer: std::sync::Arc<tokio::sync::Mutex<Option<sidequest_game::AudioMixer>>> =
        std::sync::Arc::new(tokio::sync::Mutex::new(None));
    let prerender_scheduler: std::sync::Arc<tokio::sync::Mutex<Option<sidequest_game::PrerenderScheduler>>> =
        std::sync::Arc::new(tokio::sync::Mutex::new(None));

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
                        &mut visual_style,
                        &mut music_director,
                        &audio_mixer,
                        &prerender_scheduler,
                        &mut npc_registry,
                        &mut narration_history,
                        &state,
                        &player_id_str,
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

    // Cleanup
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
    visual_style: &mut Option<sidequest_genre::VisualStyle>,
    music_director: &mut Option<sidequest_game::MusicDirector>,
    audio_mixer: &std::sync::Arc<tokio::sync::Mutex<Option<sidequest_game::AudioMixer>>>,
    prerender_scheduler: &std::sync::Arc<tokio::sync::Mutex<Option<sidequest_game::PrerenderScheduler>>>,
    npc_registry: &mut Vec<NpcRegistryEntry>,
    narration_history: &mut Vec<String>,
    state: &AppState,
    player_id: &str,
) -> Vec<GameMessage> {
    match &msg {
        GameMessage::SessionEvent { payload, .. } if payload.event == "connect" => {
            dispatch_connect(
                payload,
                session,
                builder,
                player_name_store,
                character_json,
                character_name,
                character_hp,
                character_max_hp,
                trope_defs,
                world_context,
                visual_style,
                music_director,
                audio_mixer,
                prerender_scheduler,
                state,
                player_id,
            )
            .await
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
                visual_style,
                npc_registry,
                narration_history,
                music_director,
                audio_mixer,
                prerender_scheduler,
                state,
                player_id,
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
                visual_style,
                npc_registry,
                narration_history,
                music_director,
                audio_mixer,
                prerender_scheduler,
                state,
                player_id,
                session.genre_slug().unwrap_or(""),
                session.world_slug().unwrap_or(""),
                player_name_store.as_deref().unwrap_or("Player"),
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
    trope_defs: &mut Vec<sidequest_genre::TropeDefinition>,
    world_context: &mut String,
    visual_style: &mut Option<sidequest_genre::VisualStyle>,
    music_director: &mut Option<sidequest_game::MusicDirector>,
    audio_mixer: &std::sync::Arc<tokio::sync::Mutex<Option<sidequest_game::AudioMixer>>>,
    prerender_scheduler: &std::sync::Arc<tokio::sync::Mutex<Option<sidequest_game::PrerenderScheduler>>>,
    state: &AppState,
    player_id: &str,
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
                            *character_name_store =
                                Some(character.core.name.as_str().to_string());
                            *character_hp = character.core.hp;
                            *character_max_hp = character.core.max_hp;
                        }

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
                                            statuses: c.core.statuses.clone(),
                                            inventory: c.core.inventory.items.iter().map(|i| i.name.as_str().to_string()).collect(),
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
                                    level: character.core.level as u32,
                                    stats: character.stats.iter().map(|(k, v)| (k.clone(), *v)).collect(),
                                    abilities: character.hooks.clone(),
                                    backstory: character.backstory.as_str().to_string(),
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
                            saved.snapshot.narrative_log.last().map(|e| e.content.clone())
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
                                payload: NarrationEndPayload {
                                    state_delta: None,
                                },
                                player_id: player_id.to_string(),
                            });
                        }

                        // PARTY_STATUS
                        {
                            let members: Vec<PartyMember> = saved.snapshot.characters.iter().map(|c| {
                                PartyMember {
                                    player_id: player_id.to_string(),
                                    name: c.core.name.as_str().to_string(),
                                    current_hp: c.core.hp,
                                    max_hp: c.core.max_hp,
                                    statuses: c.core.statuses.clone(),
                                    class: c.char_class.as_str().to_string(),
                                    level: c.core.level as u32,
                                    portrait_url: None,
                                }
                            }).collect();
                            responses.push(GameMessage::PartyStatus {
                                payload: PartyStatusPayload { members },
                                player_id: player_id.to_string(),
                            });
                        }

                        // Initialize audio subsystems for returning player
                        if let Ok(genre_code) = GenreCode::new(genre) {
                            let loader = GenreLoader::new(vec![state.genre_packs_path().to_path_buf()]);
                            if let Ok(pack) = loader.load(&genre_code) {
                                *visual_style = Some(pack.visual_style.clone());
                                *music_director = Some(sidequest_game::MusicDirector::new(&pack.audio));
                                *audio_mixer.lock().await = Some(sidequest_game::AudioMixer::new(
                                    sidequest_game::DuckConfig::default(),
                                ));
                                *prerender_scheduler.lock().await = Some(sidequest_game::PrerenderScheduler::new(
                                    sidequest_game::PrerenderConfig::default(),
                                ));
                                tracing::info!(genre = %genre, "Audio subsystems initialized for returning player");

                                // Inject name bank context for returning player
                                let cultures = pack.worlds.get(world)
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
                            builder, trope_defs, world_context, visual_style,
                            music_director, audio_mixer, prerender_scheduler,
                            genre, world, state, player_id,
                        ).await {
                            responses.push(scene_msg);
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to load saved session, starting fresh");
                        responses.push(connected_msg);
                        if let Some(scene_msg) = start_character_creation(
                            builder, trope_defs, world_context, visual_style,
                            music_director, audio_mixer, prerender_scheduler,
                            genre, world, state, player_id,
                        ).await {
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
                    music_director,
                    audio_mixer,
                    prerender_scheduler,
                    genre,
                    world,
                    state,
                    player_id,
                ).await {
                    responses.push(scene_msg);
                }
            }

            // Send theme_css SESSION_EVENT if the genre pack has a client_theme.css
            let css_path = state.genre_packs_path().join(genre).join("client_theme.css");
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
    music_director_out: &mut Option<sidequest_game::MusicDirector>,
    audio_mixer_lock: &std::sync::Arc<tokio::sync::Mutex<Option<sidequest_game::AudioMixer>>>,
    prerender_lock: &std::sync::Arc<tokio::sync::Mutex<Option<sidequest_game::PrerenderScheduler>>>,
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

    // Initialize audio subsystems from genre pack
    *music_director_out = Some(sidequest_game::MusicDirector::new(&pack.audio));
    *audio_mixer_lock.lock().await = Some(sidequest_game::AudioMixer::new(
        sidequest_game::DuckConfig::default(),
    ));
    *prerender_lock.lock().await = Some(sidequest_game::PrerenderScheduler::new(
        sidequest_game::PrerenderConfig::default(),
    ));
    tracing::info!(genre = %genre, "Audio subsystems initialized from genre pack");

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
    let cultures = pack.worlds.get(world_slug)
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
    visual_style: &Option<sidequest_genre::VisualStyle>,
    npc_registry: &mut Vec<NpcRegistryEntry>,
    narration_history: &mut Vec<String>,
    music_director: &mut Option<sidequest_game::MusicDirector>,
    audio_mixer: &std::sync::Arc<tokio::sync::Mutex<Option<sidequest_game::AudioMixer>>>,
    prerender_scheduler: &std::sync::Arc<tokio::sync::Mutex<Option<sidequest_game::PrerenderScheduler>>>,
    state: &AppState,
    player_id: &str,
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

                    // Store character data
                    *character_name_store = Some(character.core.name.as_str().to_string());
                    *character_hp = character.core.hp;
                    *character_max_hp = character.core.max_hp;
                    *character_json_store = Some(char_json.clone());

                    // Save to SQLite for reconnection across restarts (keyed by player)
                    let genre = session.genre_slug().unwrap_or("").to_string();
                    let world = session.world_slug().unwrap_or("").to_string();
                    let pname_for_save = player_name_store.as_deref().unwrap_or("Player").to_string();
                    let snapshot = sidequest_game::GameSnapshot {
                        genre_slug: genre.clone(),
                        world_slug: world.clone(),
                        characters: vec![character.clone()],
                        location: "Starting area".to_string(),
                        ..Default::default()
                    };
                    if let Err(e) = state.persistence().save(&genre, &world, &pname_for_save, &snapshot).await {
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
                        visual_style,
                        npc_registry,
                        narration_history,
                        music_director,
                        audio_mixer,
                        prerender_scheduler,
                        state,
                        player_id,
                        &genre,
                        &world,
                        &pname_for_save,
                    )
                    .await;

                    // Emit CHARACTER_SHEET for the UI overlay
                    let char_sheet = GameMessage::CharacterSheet {
                        payload: CharacterSheetPayload {
                            name: character.core.name.as_str().to_string(),
                            class: character.char_class.as_str().to_string(),
                            level: character.core.level as u32,
                            stats: character
                                .stats
                                .iter()
                                .map(|(k, v)| (k.clone(), *v))
                                .collect(),
                            abilities: character.hooks.clone(),
                            backstory: character.backstory.as_str().to_string(),
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
                        payload: NarrationEndPayload {
                            state_delta: None,
                        },
                        player_id: player_id.to_string(),
                    };

                    let mut msgs = vec![complete, char_sheet, backstory_narration, backstory_end, ready];
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
    character_json: &Option<serde_json::Value>,
    combat_state: &mut sidequest_game::combat::CombatState,
    chase_state: &mut Option<sidequest_game::ChaseState>,
    trope_states: &mut Vec<sidequest_game::trope::TropeState>,
    trope_defs: &[sidequest_genre::TropeDefinition],
    world_context: &str,
    visual_style: &Option<sidequest_genre::VisualStyle>,
    npc_registry: &mut Vec<NpcRegistryEntry>,
    narration_history: &mut Vec<String>,
    music_director: &mut Option<sidequest_game::MusicDirector>,
    audio_mixer: &std::sync::Arc<tokio::sync::Mutex<Option<sidequest_game::AudioMixer>>>,
    prerender_scheduler: &std::sync::Arc<tokio::sync::Mutex<Option<sidequest_game::PrerenderScheduler>>>,
    state: &AppState,
    player_id: &str,
    genre_slug: &str,
    world_slug: &str,
    player_name_for_save: &str,
) -> Vec<GameMessage> {
    // Watcher: action received
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
            f
        },
    });

    // THINKING indicator
    let thinking = GameMessage::Thinking {
        payload: ThinkingPayload {},
        player_id: player_id.to_string(),
    };

    // Slash command interception — route /commands to mechanical handlers, not the LLM.
    if action.starts_with('/') {
        use sidequest_game::slash_router::SlashRouter;
        use sidequest_game::commands::{StatusCommand, InventoryCommand, MapCommand, SaveCommand, GmCommand};
        use sidequest_game::state::GameSnapshot;

        let mut router = SlashRouter::new();
        router.register(Box::new(StatusCommand));
        router.register(Box::new(InventoryCommand));
        router.register(Box::new(MapCommand));
        router.register(Box::new(SaveCommand));
        router.register(Box::new(GmCommand));

        // Build a minimal GameSnapshot from the local session state.
        let snapshot = {
            let mut snap = GameSnapshot {
                genre_slug: genre_slug.to_string(),
                world_slug: world_slug.to_string(),
                location: current_location.clone(),
                combat: combat_state.clone(),
                chase: chase_state.clone(),
                active_tropes: trope_states.iter().map(|ts| ts.trope_definition_id().to_string()).collect(),
                ..GameSnapshot::default()
            };
            // Reconstruct a minimal Character from loose variables.
            if let Some(ref cj) = character_json {
                if let Ok(mut ch) = serde_json::from_value::<sidequest_game::Character>(cj.clone()) {
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
                    f.insert("slash_command".to_string(), serde_json::Value::String(action.to_string()));
                    f.insert("result_len".to_string(), serde_json::json!(text.len()));
                    f
                },
            });

            return vec![
                thinking,
                GameMessage::Narration {
                    payload: NarrationPayload {
                        text,
                        state_delta: None,
                        footnotes: vec![],
                    },
                    player_id: player_id.to_string(),
                },
                GameMessage::NarrationEnd {
                    payload: NarrationEndPayload {
                        state_delta: None,
                    },
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
            with_progression = trope_defs.iter().filter(|d| d.passive_progression.is_some()).count(),
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

    // Location constraint — prevent narrator from teleporting between scenes
    if !current_location.is_empty() {
        // Dialogue context: if the player interacted with an NPC in the last 2 turns,
        // any location mention in the action is likely dialogue (describing a place to
        // the NPC), not a travel intent. Strengthen the stay-put constraint.
        let turn_approx = trope_states.len() as u32 + 1;
        let recent_npc_interaction = npc_registry.iter().any(|e| {
            turn_approx.saturating_sub(e.last_seen_turn) <= 2
        });
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

    // Inventory constraint — the narrator must not allow players to use items they don't have
    state_summary.push_str("\n\nINVENTORY CONSTRAINT — THIS IS A HARD RULE:");
    if !inventory.items.is_empty() {
        state_summary.push_str("\nThe player ONLY has these items:");
        for item in &inventory.items {
            state_summary.push_str(&format!("\n- {}{}", item.name, if item.quantity > 1 { format!(" (x{})", item.quantity) } else { String::new() }));
        }
        state_summary.push_str("\nIf the player claims to have or use an item NOT on this list, the narrator MUST reject it. Describe the attempt failing — the item is simply not there. Do NOT invent items the player does not possess.");
    } else {
        state_summary.push_str("\nThe player has NO items. If the player claims to use any item, the narrator MUST reject it — they have nothing in their possession yet.");
    }

    // Bug 6: Include chase state if active
    if let Some(ref cs) = chase_state {
        state_summary.push_str(&format!(
            "\nACTIVE CHASE: {:?} (round {}, separation {})",
            cs.chase_type(), cs.round(), cs.separation()
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
    }

    if !world_context.is_empty() {
        state_summary.push('\n');
        state_summary.push_str(world_context);
    }
    if !trope_context.is_empty() {
        state_summary.push('\n');
        state_summary.push_str(&trope_context);
    }

    // Bug 17: Include recent narration history so the narrator maintains continuity
    if !narration_history.is_empty() {
        state_summary.push_str("\n\nRECENT CONVERSATION HISTORY (most recent last):\n");
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

    // Process the action through GameService
    let context = TurnContext {
        state_summary: Some(state_summary),
        in_combat: combat_state.in_combat(),
        in_chase: chase_state.is_some(),
    };
    let result = state.game_service().process_action(action, &context);

    // Watcher: narration generated
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
            f
        },
    });

    let mut messages = vec![thinking];

    // Extract location header from narration (format: **Location Name**\n\n...)
    // Bug 1: Update current_location so subsequent turns maintain continuity
    let narration_text = &result.narration;
    if let Some(location) = extract_location_header(narration_text) {
        *current_location = location.clone();
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
        messages.push(GameMessage::MapUpdate {
            payload: MapUpdatePayload {
                current_location: location,
                region: String::new(),
                explored: vec![],
                fog_bounds: None,
            },
            player_id: player_id.to_string(),
        });
    }

    // Strip the location header from narration text if present
    let clean_narration = strip_location_header(narration_text);

    // Bug 17: Accumulate narration history for context on subsequent turns.
    // Truncate narrator response to ~300 chars to keep context bounded.
    let truncated_narration: String = clean_narration.chars().take(300).collect();
    narration_history.push(format!("Player: {}\nNarrator: {}", action, truncated_narration));
    // Cap the buffer at 20 entries to prevent unbounded growth
    if narration_history.len() > 20 {
        narration_history.drain(..narration_history.len() - 20);
    }

    // Update NPC registry from narration — tracks names, pronouns, locations
    // so subsequent turns maintain NPC identity consistency.
    // Use trope_states len as a rough turn counter.
    let turn_approx = trope_states.len() as u32 + 1;
    update_npc_registry(npc_registry, &clean_narration, current_location, turn_approx);
    tracing::debug!(npc_count = npc_registry.len(), "NPC registry updated from narration");

    // Bug 4: Combat HP changes — scan narration for damage/healing indicators
    {
        let narr_lower = clean_narration.to_lowercase();
        let damage_keywords = [
            "strikes you", "hits you", "slashes you", "damages you",
            "wounds you", "hurts you", "burns you", "bites you",
            "stabs you", "pierces you", "deals damage", "takes damage",
            "you take damage", "you take a hit", "injures you", "smashes into you",
        ];
        let heal_keywords = [
            "heals you", "restores health", "mends your wounds",
            "you feel better", "healing energy", "bandage",
            "drink the potion", "drinks a potion", "health restored",
        ];
        let heavy_damage_keywords = [
            "critical hit", "devastating blow", "massive damage",
            "nearly kills", "grievous wound",
        ];

        if combat_state.in_combat() || damage_keywords.iter().any(|kw| narr_lower.contains(kw)) {
            if heavy_damage_keywords.iter().any(|kw| narr_lower.contains(kw)) {
                let delta = -((*max_hp as f64 * 0.25) as i32).max(3);
                *hp = sidequest_game::clamp_hp(*hp, delta, *max_hp);
                tracing::info!(delta = delta, new_hp = *hp, "Heavy combat damage applied");
            } else if damage_keywords.iter().any(|kw| narr_lower.contains(kw)) {
                let delta = -((*max_hp as f64 * 0.12) as i32).max(1);
                *hp = sidequest_game::clamp_hp(*hp, delta, *max_hp);
                tracing::info!(delta = delta, new_hp = *hp, "Combat damage applied");
            }
        }
        if heal_keywords.iter().any(|kw| narr_lower.contains(kw)) {
            let delta = ((*max_hp as f64 * 0.2) as i32).max(2);
            *hp = sidequest_game::clamp_hp(*hp, delta, *max_hp);
            tracing::info!(delta = delta, new_hp = *hp, "Healing applied");
        }
    }

    // Bug 3: XP award based on action type
    {
        let xp_award = if combat_state.in_combat() {
            25 // combat actions give more XP
        } else {
            10 // exploration/dialogue gives base XP
        };
        *xp += xp_award;
        tracing::info!(xp_award = xp_award, total_xp = *xp, level = *level, "XP awarded");

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

    // Bug 5: Extract items from narration and add to inventory
    {
        let items_found = extract_items_from_narration(&clean_narration);
        for (item_name, item_type) in &items_found {
            let item_id = item_name.to_lowercase().replace(' ', "_").replace(|c: char| !c.is_alphanumeric() && c != '_', "");
            // Skip if already in inventory
            if inventory.find(&item_id).is_some() {
                continue;
            }
            if let (Ok(id), Ok(name), Ok(desc), Ok(cat), Ok(rarity)) = (
                sidequest_protocol::NonBlankString::new(&item_id),
                sidequest_protocol::NonBlankString::new(item_name),
                sidequest_protocol::NonBlankString::new(&format!("A {} found during adventure", item_type)),
                sidequest_protocol::NonBlankString::new(item_type),
                sidequest_protocol::NonBlankString::new("common"),
            ) {
                let item = sidequest_game::Item {
                    id, name, description: desc, category: cat,
                    value: 0, weight: 1.0, rarity, narrative_weight: 0.3,
                    tags: vec![], equipped: false, quantity: 1,
                };
                let _ = inventory.add(item, 50);
                tracing::info!(item_name = %item_name, "Item added to inventory from narration");
            }
        }

        // Extract item losses from narration (trades, gifts, drops)
        let items_lost = extract_item_losses(&clean_narration);
        for lost_name in &items_lost {
            let item_id = lost_name.to_lowercase().replace(' ', "_").replace(|c: char| !c.is_alphanumeric() && c != '_', "");
            if inventory.find(&item_id).is_some() {
                let _ = inventory.remove(&item_id);
                tracing::info!(item_name = %lost_name, "Item removed from inventory from narration");
            }
        }
    }

    // Narration — include character state so the UI state mirror picks it up
    let inventory_names: Vec<String> = inventory.items.iter().map(|i| i.name.as_str().to_string()).collect();
    messages.push(GameMessage::Narration {
        payload: NarrationPayload {
            text: clean_narration.clone(),
            state_delta: Some(sidequest_protocol::StateDelta {
                location: extract_location_header(narration_text),
                characters: Some(vec![sidequest_protocol::CharacterState {
                    name: char_name.to_string(),
                    hp: *hp,
                    max_hp: *max_hp,
                    statuses: vec![],
                    inventory: inventory_names.clone(),
                }]),
                quests: None,
            }),
            footnotes: vec![],
        },
        player_id: player_id.to_string(),
    });

    // Narration end with state_delta field present (even if empty)
    messages.push(GameMessage::NarrationEnd {
        payload: NarrationEndPayload {
            state_delta: Some(sidequest_protocol::StateDelta {
                location: None,
                characters: None,
                quests: None,
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

    // Party status
    messages.push(GameMessage::PartyStatus {
        payload: PartyStatusPayload {
            members: vec![PartyMember {
                player_id: player_id.to_string(),
                name: char_name.to_string(),
                current_hp: *hp,
                max_hp: *max_hp,
                statuses: vec![],
                class: char_class.to_string(),
                level: *level,
                portrait_url: None,
            }],
        },
        player_id: player_id.to_string(),
    });

    // Bug 5: Inventory — now wired to actual inventory state
    messages.push(GameMessage::Inventory {
        payload: InventoryPayload {
            items: inventory.items.iter().map(|item| {
                sidequest_protocol::InventoryItem {
                    name: item.name.as_str().to_string(),
                    item_type: item.category.as_str().to_string(),
                    equipped: item.equipped,
                    quantity: item.quantity,
                    description: item.description.as_str().to_string(),
                }
            }).collect(),
            gold: inventory.gold,
        },
        player_id: player_id.to_string(),
    });

    // Combat detection — scan narration for combat start/end indicators.
    // This ensures combat_state.in_combat() is set correctly so the combat
    // tick runs and CombatState persists across non-combat actions.
    {
        let narr_lower = clean_narration.to_lowercase();
        let combat_start_keywords = [
            "initiative", "combat begins", "roll for initiative",
            "attacks you", "lunges at", "swings at", "draws a weapon",
            "charges at", "opens fire", "enters combat",
        ];
        let combat_end_keywords = [
            "combat ends", "battle is over", "enemies defeated",
            "falls unconscious", "retreats", "flees", "surrenders",
            "combat resolved", "the fight is over",
        ];

        if combat_state.in_combat() {
            // Check for combat end
            if combat_end_keywords.iter().any(|kw| narr_lower.contains(kw)) {
                combat_state.set_in_combat(false);
                tracing::info!("Combat ended — detected end keyword in narration");
            }
        } else {
            // Check for combat start
            if combat_start_keywords.iter().any(|kw| narr_lower.contains(kw)) {
                combat_state.set_in_combat(true);
                tracing::info!("Combat started — detected start keyword in narration");
            }
        }
    }

    // Combat tick — uses persistent per-session CombatState
    let was_in_combat = combat_state.in_combat();
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
            "chase begins", "gives chase", "starts chasing", "run!",
            "flee!", "pursues you", "pursuit begins", "races after",
            "sprints after", "bolts away",
        ];
        let chase_end_keywords = [
            "escape", "lost them", "chase ends", "caught up",
            "stopped running", "pursuit ends", "safe now",
            "shakes off", "outrun",
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
            }
        } else if chase_start_keywords.iter().any(|kw| narr_lower.contains(kw)) {
            let cs = sidequest_game::ChaseState::new(
                sidequest_game::ChaseType::Footrace,
                0.5,
            );
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
        in_combat: false,
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
            in_chase: false, // chase state not threaded yet
            party_health_pct: if *max_hp > 0 { *hp as f32 / *max_hp as f32 } else { 1.0 },
            quest_completed: false,
            npc_died: false,
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
                messages.push(audio_cue_to_game_message(c, player_id, genre_slug, Some(mood_key)));
            }
        } else {
            tracing::warn!(mood = mood_key, "music_evaluate_returned_none — no cue produced");
        }
    } else {
        tracing::warn!("music_director_missing — audio cues skipped");
    }

    // Persist updated game state (location, narration log) for reconnection
    if !genre_slug.is_empty() && !world_slug.is_empty() {
        let location = extract_location_header(narration_text)
            .unwrap_or_else(|| "Starting area".to_string());
        match state.persistence().load(genre_slug, world_slug, player_name_for_save).await {
            Ok(Some(saved)) => {
                let mut snapshot = saved.snapshot;
                snapshot.location = location;
                // Append narration to log for recap on reconnect
                snapshot.narrative_log.push(sidequest_game::NarrativeEntry {
                    timestamp: 0,
                    round: 0,
                    author: "narrator".to_string(),
                    content: clean_narration.clone(),
                    tags: vec![],
                });
                if let Err(e) = state.persistence().save(genre_slug, world_slug, player_name_for_save, &snapshot).await {
                    tracing::warn!(error = %e, "Failed to persist updated game state");
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
    if !clean_narration.is_empty() {
        let segmenter = sidequest_game::SentenceSegmenter::new();
        let segments = segmenter.segment(&clean_narration);
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
            let tts_config = sidequest_game::tts_stream::TtsStreamConfig::default();
            let streamer = sidequest_game::tts_stream::TtsStreamer::new(tts_config);

            // Clone Arcs for the spawned TTS task (mixer ducking + prerender)
            let mixer_for_tts = std::sync::Arc::clone(audio_mixer);
            let prerender_for_tts = std::sync::Arc::clone(prerender_scheduler);
            let genre_slug_for_tts = genre_slug.to_string();
            let tts_segments_for_prerender = tts_segments.clone();
            let prerender_ctx = sidequest_game::PrerenderContext {
                in_combat: combat_state.in_combat(),
                combatant_names: vec![], // TODO: extract from combat state when available
                pending_destination: extract_location_header(narration_text).map(|s| s.to_string()),
                active_dialogue_npc: None, // TODO: parse from narration when available
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
                let synthesizer = match sidequest_daemon_client::DaemonClient::connect(daemon_config).await {
                    Ok(client) => DaemonSynthesizer { client: tokio::sync::Mutex::new(client) },
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

                // Bridge TtsMessage → binary frames (chunks) or GameMessage (start/end)
                while let Some(tts_msg) = msg_rx.recv().await {
                    match tts_msg {
                        sidequest_game::tts_stream::TtsMessage::Start { total_segments } => {
                            // Duck audio channels during TTS
                            {
                                let mut mixer_guard = mixer_for_tts.lock().await;
                                if let Some(ref mut mixer) = *mixer_guard {
                                    for duck_cue in mixer.on_tts_start() {
                                        let _ = state_for_tts.broadcast(
                                            audio_cue_to_game_message(&duck_cue, &player_id_for_tts, &genre_slug_for_tts, None),
                                        );
                                    }
                                }
                            }
                            // Speculative prerender during TTS playback
                            {
                                let mut prerender_guard = prerender_for_tts.lock().await;
                                if let Some(ref mut prerender) = *prerender_guard {
                                    if let Some(subject) = prerender.on_tts_start(
                                        &tts_segments_for_prerender,
                                        &prerender_ctx,
                                    ) {
                                        if let Some(ref queue) = state_for_tts.inner.render_queue {
                                            let _ = queue.enqueue(subject, &prerender_ctx.art_style, "flux-schnell").await;
                                        }
                                    }
                                }
                            }
                            let game_msg = GameMessage::TtsStart {
                                payload: sidequest_protocol::TtsStartPayload { total_segments },
                                player_id: player_id_for_tts.clone(),
                            };
                            let _ = state_for_tts.broadcast(game_msg);
                        }
                        sidequest_game::tts_stream::TtsMessage::Chunk(chunk) => {
                            // Build binary voice frame: [4-byte header len][JSON header][audio bytes]
                            // The daemon always returns raw PCM s16le — use that format string
                            // so the UI routes to playVoicePCM instead of decodeAudioData.
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
                            frame.extend_from_slice(
                                &(header_bytes.len() as u32).to_be_bytes(),
                            );
                            frame.extend_from_slice(&header_bytes);
                            frame.extend_from_slice(audio_bytes);
                            state_for_tts.broadcast_binary(frame);
                        }
                        sidequest_game::tts_stream::TtsMessage::End => {
                            // Restore audio channels after TTS
                            {
                                let mut mixer_guard = mixer_for_tts.lock().await;
                                if let Some(ref mut mixer) = *mixer_guard {
                                    for restore_cue in mixer.on_tts_end() {
                                        let _ = state_for_tts.broadcast(
                                            audio_cue_to_game_message(&restore_cue, &player_id_for_tts, &genre_slug_for_tts, None),
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
                            let _ = state_for_tts.broadcast(game_msg);
                        }
                    }
                }

                let _ = stream_handle.await;
                tracing::info!(player_id = %player_id_for_tts, "TTS stream complete");
            });
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
            dyn std::future::Future<
                    Output = Result<Vec<u8>, sidequest_game::tts_stream::TtsError>,
                > + Send
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

/// Extract a location header from narration text (format: **Location Name**)
fn extract_location_header(text: &str) -> Option<String> {
    let first_line = text.lines().next()?.trim();
    if first_line.starts_with("**") && first_line.ends_with("**") && first_line.len() > 4 {
        Some(first_line[2..first_line.len() - 2].to_string())
    } else {
        None
    }
}

/// Strip the location header line from narration text
fn strip_location_header(text: &str) -> String {
    let first_line = text.lines().next().unwrap_or("").trim();
    if first_line.starts_with("**") && first_line.ends_with("**") && first_line.len() > 4 {
        text.lines()
            .skip(1)
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string()
    } else {
        text.to_string()
    }
}

/// Bug 5: Extract item acquisitions from narration text.
///
/// Looks for patterns like "you pick up {item}", "you find {item}", "receives {item}", etc.
/// Returns a list of (item_name, item_type) tuples.
fn extract_items_from_narration(text: &str) -> Vec<(String, String)> {
    let text_lower = text.to_lowercase();
    let mut items = Vec::new();

    let acquisition_patterns = [
        "pick up ", "picks up ", "you find ", "you found ",
        "receives ", "receive ", "you acquire ", "acquires ",
        "you take the ", "takes the ", "you grab ", "grabs ",
        "you pocket ", "pockets ", "hands you ", "gives you ",
        "you loot ", "loots ",
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
            let end = rest.find(|c: char| matches!(c, '.' | ',' | '!' | '?' | '\n' | ';' | ':'))
                .unwrap_or(rest.len());
            let item_name = rest[..end].trim();
            // Skip if too short
            if item_name.len() >= 3 {
                // Strip leading articles
                let after_article = item_name
                    .strip_prefix("a ").or_else(|| item_name.strip_prefix("an "))
                    .or_else(|| item_name.strip_prefix("the "))
                    .or_else(|| item_name.strip_prefix("some "))
                    .unwrap_or(item_name)
                    .trim();
                // Truncate at prepositional phrases and adverbs to get clean item names.
                // "compass with both hands" → "compass", "hammer again" → "hammer"
                let stop_words = [" with ", " from ", " into ", " onto ", " against ",
                    " across ", " along ", " through ", " around ", " behind ",
                    " before ", " after ", " again", " as ", " and then",
                    " while ", " that ", " which "];
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
                    let category = if lower_name.contains("sword") || lower_name.contains("blade") || lower_name.contains("axe") || lower_name.contains("dagger") || lower_name.contains("weapon") {
                        "weapon"
                    } else if lower_name.contains("armor") || lower_name.contains("shield") || lower_name.contains("helmet") || lower_name.contains("plate") {
                        "armor"
                    } else if lower_name.contains("potion") || lower_name.contains("salve") || lower_name.contains("herb") || lower_name.contains("food") || lower_name.contains("drink") {
                        "consumable"
                    } else if lower_name.contains("key") || lower_name.contains("tool") || lower_name.contains("rope") || lower_name.contains("torch") || lower_name.contains("lantern") {
                        "tool"
                    } else if lower_name.contains("coin") || lower_name.contains("gem") || lower_name.contains("gold") || lower_name.contains("jewel") {
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
/// Removes bold (**), italic (*/_), headers (#), links, images, code blocks.
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
        if (chars[i] == '*' || chars[i] == '_')
            && i > 0
            && chars[i - 1].is_alphanumeric()
        {
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
    cleaned
}

/// Extract item losses from narration — trades, gifts, drops.
/// Returns a list of item names that the player lost.
fn extract_item_losses(text: &str) -> Vec<String> {
    let text_lower = text.to_lowercase();
    let mut lost = Vec::new();

    let loss_patterns = [
        "hand over ", "hands over ", "give away ", "gives away ",
        "trade the ", "trades the ", "trading the ",
        "hand the ", "hands the ",
        "surrender the ", "surrenders the ",
        "drop the ", "drops the ",
        "toss the ", "tosses the ",
        "you give ", "you hand ",
        "you trade ", "you surrender ",
        "you drop ", "you toss ",
        "parts with the ", "part with the ",
        "relinquish the ", "relinquishes the ",
    ];

    for pattern in &loss_patterns {
        let mut search_from = 0;
        while let Some(pos) = text_lower[search_from..].find(pattern) {
            let start = search_from + pos + pattern.len();
            if start >= text_lower.len() {
                break;
            }
            let rest = &text[start..];
            let end = rest.find(|c: char| matches!(c, '.' | ',' | '!' | '?' | '\n' | ';' | ':'))
                .unwrap_or(rest.len());
            let item_name = rest[..end].trim();
            if item_name.len() >= 2 && item_name.len() <= 60 {
                let after_article = item_name
                    .strip_prefix("a ").or_else(|| item_name.strip_prefix("an "))
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
                let clean_name = if words.len() > 4 { words[..4].join(" ") } else { words.join(" ") };
                if clean_name.len() >= 2 {
                    lost.push(clean_name);
                }
            }
            search_from = start;
        }
    }

    lost
}

/// Lightweight NPC registry entry — tracks name, pronouns, role, and location
/// so the narrator prompt can maintain identity consistency across turns.
#[derive(Debug, Clone)]
struct NpcRegistryEntry {
    name: String,
    pronouns: String,
    role: String,
    location: String,
    last_seen_turn: u32,
}

/// Extract NPC names from narration text and update the registry.
/// Looks for patterns like dialogue attribution ("Name says", "Name asks")
/// and introduction patterns ("a woman named Name", "Name, the blacksmith").
fn update_npc_registry(registry: &mut Vec<NpcRegistryEntry>, narration: &str, current_location: &str, turn_count: u32) {
    // Dialogue attribution: "Name says/asks/replies/shouts/whispers/mutters"
    let speech_verbs = ["says", "asks", "replies", "shouts", "whispers", "mutters", "growls", "calls", "declares", "speaks"];
    let text_lower = narration.to_lowercase();

    for verb in &speech_verbs {
        let pattern = format!(" {}", verb);
        let mut search_from = 0;
        while let Some(pos) = text_lower[search_from..].find(&pattern) {
            let abs_pos = search_from + pos;
            // Walk backward to find the start of the name (capital letter after punctuation/newline/start)
            let before = &narration[..abs_pos];
            // Find the last sentence boundary before this verb
            let name_start = before.rfind(|c: char| matches!(c, '.' | '!' | '?' | '\n' | '"' | '\u{201c}'))
                .map(|i| i + 1)
                .unwrap_or(0);
            let candidate = before[name_start..].trim();
            // A valid NPC name: starts with uppercase, 2-40 chars, no lowercase-only words that look like common text
            if candidate.len() >= 2 && candidate.len() <= 40 && candidate.chars().next().map_or(false, |c| c.is_uppercase()) {
                let name = candidate.to_string();
                // Skip if it's the player character reference
                if !name.to_lowercase().contains("you") && name != "The" && name != "A" && name != "An" {
                    // Update existing or add new
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
                let name_end = rest.find(|c: char| matches!(c, ',' | '.' | '!' | '?' | ';' | '\n' | '"' | '\u{201d}'))
                    .unwrap_or(rest.len());
                let candidate = rest[..name_end].trim();
                if candidate.len() >= 2 && candidate.len() <= 40 && candidate.chars().next().map_or(false, |c| c.is_uppercase()) {
                    let name = candidate.to_string();
                    if !name.to_lowercase().contains("you") {
                        if !registry.iter().any(|e| e.name == name) {
                            // Try to infer role from "X, the blacksmith" pattern after name
                            let role = if name_end < rest.len() {
                                let after_name = &rest[name_end..];
                                if after_name.starts_with(", the ") || after_name.starts_with(", a ") {
                                    let role_start = after_name.find(' ').map(|i| i + 1).unwrap_or(0);
                                    let role_text = &after_name[role_start..];
                                    let role_end = role_text.find(|c: char| matches!(c, ',' | '.' | '!' | '?'))
                                        .unwrap_or(role_text.len().min(40));
                                    role_text[..role_end].trim().to_string()
                                } else { String::new() }
                            } else { String::new() };
                            registry.push(NpcRegistryEntry {
                                name,
                                pronouns: String::new(),
                                role,
                                location: current_location.to_string(),
                                last_seen_turn: turn_count,
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
        let appos_re = Regex::new(r"\b([A-Z][a-z]+(?:\s[A-Z][a-z]+)?), (?:the|a|an) ([a-z][a-z ]{1,30})").unwrap();
        for caps in appos_re.captures_iter(narration) {
            let name = caps[1].to_string();
            let role = caps[2].trim_end_matches(|c: char| matches!(c, ',' | '.' | '!' | '?')).trim().to_string();
            if !name.to_lowercase().contains("you") && name != "The" && name != "A" && name != "An" {
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
            if !name.to_lowercase().contains("you") && name != "The" && name != "A" && name != "An"
                && name != "It" && name != "This" && name != "That" && name != "There"
                && name != "Here" && name != "Then" && name != "Now" && name != "But"
                && name != "And" && name != "Or" && name != "Yet" && name != "So"
            {
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
            if !name.to_lowercase().contains("you") && name != "The" && name != "A" && name != "An"
                && name != "It" && name != "This" && name != "That"
            {
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
                    });
                }
            }
        }
    }

    // Infer pronouns from narration context
    for entry in registry.iter_mut() {
        if !entry.pronouns.is_empty() { continue; }
        let name_lower = entry.name.to_lowercase();
        // Check narration for pronoun references near the name
        if let Some(name_pos) = text_lower.find(&name_lower) {
            let after = &text_lower[name_pos..];
            let window = &after[..after.len().min(200)];
            if window.contains(" she ") || window.contains(" her ") || window.contains(" hers ") {
                entry.pronouns = "she/her".to_string();
            } else if window.contains(" he ") || window.contains(" his ") || window.contains(" him ") {
                entry.pronouns = "he/him".to_string();
            } else if window.contains(" they ") || window.contains(" their ") || window.contains(" them ") {
                entry.pronouns = "they/them".to_string();
            }
        }
    }
}

/// Build the NPC registry context string for the narrator prompt.
fn build_npc_registry_context(registry: &[NpcRegistryEntry]) -> String {
    if registry.is_empty() {
        return String::new();
    }
    let mut lines = vec!["\nACTIVE NPCs — you MUST use these exact names, pronouns, and roles. Do NOT rename, change gender, or alter these characters:".to_string()];
    for entry in registry {
        let mut desc = format!("- {}", entry.name);
        if !entry.pronouns.is_empty() {
            desc.push_str(&format!(" ({})", entry.pronouns));
        }
        if !entry.role.is_empty() {
            desc.push_str(&format!(", {}", entry.role));
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
        lines.push(format!("\n## {} — {}", culture.name.as_str(), culture.description));
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
            lines.push(format!("  Name patterns: {}", culture.person_patterns.join(", ")));
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
    AppState::new_with_game_service(Box::new(Orchestrator::new(watcher_tx)), genre_packs_path, save_dir)
}

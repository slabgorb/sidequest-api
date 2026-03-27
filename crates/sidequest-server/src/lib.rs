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
    CharacterState, ErrorPayload, GameMessage, InitialState, NarrationEndPayload, NarrationPayload,
    PartyMember, PartyStatusPayload, SessionEventPayload, ThinkingPayload,
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
// PlayerSession — stored for reconnection
// ---------------------------------------------------------------------------

/// Saved player session state, keyed by "player_name:genre:world".
/// Enables reconnection: a returning player skips character creation.
struct PlayerSession {
    character_json: serde_json::Value,
    character_name: String,
    hp: i32,
    max_hp: i32,
    genre_slug: String,
    world_slug: String,
    location: String,
}

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
    saved_sessions: Mutex<HashMap<String, PlayerSession>>,
    watcher_tx: broadcast::Sender<WatcherEvent>,
    // GameStore removed — rusqlite::Connection is not Send+Sync.
    // Save/load will use a dedicated async task with its own connection.
    // TODO: spawn persistence worker task with mpsc channel
    render_queue: Option<sidequest_game::RenderQueue>,
    subject_extractor: sidequest_game::SubjectExtractor,
    beat_filter: tokio::sync::Mutex<sidequest_game::BeatFilter>,
}

impl fmt::Debug for AppStateInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppStateInner")
            .field("genre_packs_path", &self.genre_packs_path)
            .finish_non_exhaustive()
    }
}

impl AppState {
    /// Create AppState with a specific GameService implementation.
    /// This is the facade pattern — the server depends on the trait, not the impl.
    pub fn new_with_game_service(
        game_service: Box<dyn GameService>,
        genre_packs_path: PathBuf,
    ) -> Self {
        let (broadcast_tx, _) = broadcast::channel(256);
        let (watcher_tx, _) = broadcast::channel(256);

        // Render pipeline — daemon client connects lazily on first render
        let render_queue = sidequest_game::RenderQueue::spawn(
            sidequest_game::RenderQueueConfig::default(),
            |prompt, art_style, tier| async move {
                tracing::info!(prompt_len = prompt.len(), art_style = %art_style, tier = %tier, "Render job starting — connecting to daemon");
                let config = sidequest_daemon_client::DaemonConfig::default();
                match sidequest_daemon_client::DaemonClient::connect(config).await {
                    Ok(mut client) => {
                        tracing::info!(tier = %tier, "Daemon connected, sending render request");
                        match client
                            .render(sidequest_daemon_client::RenderParams {
                                prompt: prompt.clone(),
                                art_style: art_style.clone(),
                                tier,
                            })
                            .await
                        {
                            Ok(result) => {
                                tracing::info!(url = %result.image_url, ms = result.generation_ms, "Render complete");
                                Ok((result.image_url, result.generation_ms))
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, prompt_preview = %&prompt[..prompt.len().min(80)], "Render request failed");
                                Err(format!("render failed: {e}"))
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Daemon connect failed");
                        Err(format!("daemon unavailable: {e}"))
                    }
                }
            },
        );

        Self {
            inner: Arc::new(AppStateInner {
                game_service,
                genre_packs_path,
                connections: Mutex::new(HashMap::new()),
                processing: Mutex::new(HashSet::new()),
                broadcast_tx,
                saved_sessions: Mutex::new(HashMap::new()),
                watcher_tx,
                render_queue: Some(render_queue),
                subject_extractor: sidequest_game::SubjectExtractor::new(),
                beat_filter: tokio::sync::Mutex::new(sidequest_game::BeatFilter::new(
                    sidequest_game::BeatFilterConfig::default(),
                )),
            }),
        }
    }

    /// Store a player session for reconnection.
    fn save_player_session(&self, key: String, session: PlayerSession) {
        self.inner
            .saved_sessions
            .lock()
            .unwrap()
            .insert(key, session);
    }

    /// Check if a player has a saved session.
    fn has_saved_session(&self, key: &str) -> bool {
        self.inner.saved_sessions.lock().unwrap().contains_key(key)
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
                    let msg = GameMessage::Image {
                        payload: sidequest_protocol::ImagePayload {
                            url: image_url,
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

    // Serve genre pack static assets (fonts, images) at /genre/assets/
    let genre_assets = ServeDir::new(state.genre_packs_path());

    Router::new()
        .route("/api/genres", get(list_genres))
        .route("/ws", get(ws_handler))
        .route("/ws/watcher", get(ws_watcher_handler))
        .nest_service("/genre", genre_assets)
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

    let player_id_str = player_id.to_string();

    // Writer task: reads from mpsc channel + broadcast, sends to WS
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
    let mut combat_state = sidequest_game::combat::CombatState::default();
    let mut trope_states: Vec<sidequest_game::trope::TropeState> = vec![];
    let mut trope_defs: Vec<sidequest_genre::TropeDefinition> = vec![];
    let mut world_context: String = String::new();

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
                        &mut combat_state,
                        &mut trope_states,
                        &mut trope_defs,
                        &mut world_context,
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
    combat_state: &mut sidequest_game::combat::CombatState,
    trope_states: &mut Vec<sidequest_game::trope::TropeState>,
    trope_defs: &mut Vec<sidequest_genre::TropeDefinition>,
    world_context: &mut String,
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
                state,
                player_id,
            )
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
                combat_state,
                trope_states,
                trope_defs,
                world_context,
                state,
                player_id,
            )
            .await
        }
        GameMessage::PlayerAction { payload, .. } => {
            if !session.is_playing() {
                return vec![error_response(
                    player_id,
                    &format!("Cannot process action in {} state", session.state_name()),
                )];
            }
            dispatch_player_action(
                &payload.action,
                character_name.as_deref().unwrap_or("Unknown"),
                *character_hp,
                *character_max_hp,
                combat_state,
                trope_states,
                trope_defs,
                world_context,
                state,
                player_id,
                session.genre_slug().unwrap_or(""),
            )
            .await
        }
        // All other valid message types in wrong state
        _ => {
            vec![error_response(
                player_id,
                &format!("Unexpected message in {} state", session.state_name()),
            )]
        }
    }
}

/// Handle SESSION_EVENT{connect}.
#[allow(clippy::too_many_arguments)]
fn dispatch_connect(
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
    state: &AppState,
    player_id: &str,
) -> Vec<GameMessage> {
    let genre = payload.genre.as_deref().unwrap_or("");
    let world = payload.world.as_deref().unwrap_or("");
    let pname = payload.player_name.as_deref().unwrap_or("Player");

    // Check for returning player
    let key = session_key(pname, genre, world);
    let returning = state.has_saved_session(&key);

    match session.handle_connect(genre, world, pname) {
        Ok(mut connected_msg) => {
            let mut responses = Vec::new();
            *player_name_store = Some(pname.to_string());

            if returning {
                // Returning player — set has_character=true and send ready with initial_state
                if let GameMessage::SessionEvent {
                    ref mut payload, ..
                } = connected_msg
                {
                    payload.has_character = Some(true);
                }
                responses.push(connected_msg);

                // Build initial_state from saved session
                let sessions = state.inner.saved_sessions.lock().unwrap();
                if let Some(saved) = sessions.get(&key) {
                    *character_json_store = Some(saved.character_json.clone());
                    *character_name_store = Some(saved.character_name.clone());
                    *character_hp = saved.hp;
                    *character_max_hp = saved.max_hp;

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
                                characters: vec![CharacterState {
                                    name: saved.character_name.clone(),
                                    hp: saved.hp,
                                    max_hp: saved.max_hp,
                                    statuses: vec![],
                                    inventory: vec![],
                                }],
                                location: saved.location.clone(),
                                quests: std::collections::HashMap::new(),
                            }),
                        },
                        player_id: player_id.to_string(),
                    };
                    responses.push(ready);
                }
            } else {
                // New player — send connected, then start character creation
                responses.push(connected_msg);

                // Load genre pack and create character builder
                if let Some(scene_msg) = start_character_creation(
                    builder,
                    trope_defs,
                    world_context,
                    genre,
                    world,
                    state,
                    player_id,
                ) {
                    responses.push(scene_msg);
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
fn start_character_creation(
    builder: &mut Option<CharacterBuilder>,
    trope_defs_out: &mut Vec<sidequest_genre::TropeDefinition>,
    world_context_out: &mut String,
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

    // Extract trope definitions from the genre pack for per-session use
    // Collect from genre-level tropes and all world tropes
    let mut all_tropes = pack.tropes.clone();
    for world in pack.worlds.values() {
        all_tropes.extend(world.tropes.clone());
    }
    *trope_defs_out = all_tropes;
    tracing::info!(count = trope_defs_out.len(), genre = %genre, "Loaded trope definitions");

    // Extract world description for narrator prompt context
    if let Some(world) = pack.worlds.get(world_slug) {
        let mut ctx = format!("World: {}", world.config.name);
        ctx.push_str(&format!("\n{}", world.config.description));
        if !world.lore.history.is_empty() {
            ctx.push_str(&format!(
                "\nHistory: {}",
                world.lore.history.chars().take(200).collect::<String>()
            ));
        }
        if !world.lore.geography.is_empty() {
            ctx.push_str(&format!(
                "\nGeography: {}",
                world.lore.geography.chars().take(200).collect::<String>()
            ));
        }
        *world_context_out = ctx;
        tracing::info!(world = %world_slug, context_len = world_context_out.len(), "Loaded world context");
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
    combat_state: &mut sidequest_game::combat::CombatState,
    trope_states: &mut Vec<sidequest_game::trope::TropeState>,
    trope_defs: &mut Vec<sidequest_genre::TropeDefinition>,
    world_context: &str,
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

                    // Save for reconnection
                    let genre = session.genre_slug().unwrap_or("").to_string();
                    let world = session.world_slug().unwrap_or("").to_string();
                    let key = session_key(pname, &genre, &world);
                    state.save_player_session(
                        key,
                        PlayerSession {
                            character_json: char_json.clone(),
                            character_name: character.core.name.as_str().to_string(),
                            hp: character.core.hp,
                            max_hp: character.core.max_hp,
                            genre_slug: genre.clone(),
                            world_slug: world,
                            location: "Starting area".to_string(),
                        },
                    );

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
                        },
                        player_id: player_id.to_string(),
                    };

                    // Auto-trigger an introductory narration so the game view isn't empty
                    let intro_messages = dispatch_player_action(
                        "I look around and take in my surroundings.",
                        character.core.name.as_str(),
                        character.core.hp,
                        character.core.max_hp,
                        combat_state,
                        trope_states,
                        trope_defs,
                        world_context,
                        state,
                        player_id,
                        &genre,
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

                    let mut msgs = vec![complete, char_sheet, ready];
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
async fn dispatch_player_action(
    action: &str,
    char_name: &str,
    hp: i32,
    max_hp: i32,
    combat_state: &mut sidequest_game::combat::CombatState,
    trope_states: &mut Vec<sidequest_game::trope::TropeState>,
    trope_defs: &[sidequest_genre::TropeDefinition],
    world_context: &str,
    state: &AppState,
    player_id: &str,
    genre_slug: &str,
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

    // Seed starter tropes if none are active yet (first turn)
    if trope_states.is_empty() && !trope_defs.is_empty() {
        // Activate the first 2-3 tropes from the genre pack
        let seed_count = trope_defs.len().min(3);
        for def in &trope_defs[..seed_count] {
            if let Some(id) = &def.id {
                sidequest_game::trope::TropeEngine::activate(trope_states, id);
                tracing::info!(trope_id = %id, "Seeded starter trope");
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

    // Build state summary for grounding narration
    let mut state_summary = format!(
        "Character: {} (HP {}/{})\nGenre: {}",
        char_name, hp, max_hp, genre_slug,
    );
    if !world_context.is_empty() {
        state_summary.push('\n');
        state_summary.push_str(world_context);
    }
    if !trope_context.is_empty() {
        state_summary.push('\n');
        state_summary.push_str(&trope_context);
    }

    // Process the action through GameService
    let context = TurnContext {
        state_summary: Some(state_summary),
        ..TurnContext::default()
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
    let narration_text = &result.narration;
    if let Some(location) = extract_location_header(narration_text) {
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
                location: Some(location),
            },
            player_id: player_id.to_string(),
        });
    }

    // Strip the location header from narration text if present
    let clean_narration = strip_location_header(narration_text);

    // Narration — include character state so the UI state mirror picks it up
    messages.push(GameMessage::Narration {
        payload: NarrationPayload {
            text: clean_narration.clone(),
            state_delta: Some(sidequest_protocol::StateDelta {
                location: extract_location_header(narration_text),
                characters: Some(vec![sidequest_protocol::CharacterState {
                    name: char_name.to_string(),
                    hp,
                    max_hp,
                    statuses: vec![],
                    inventory: vec![],
                }]),
                quests: None,
            }),
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

    // Party status
    messages.push(GameMessage::PartyStatus {
        payload: PartyStatusPayload {
            members: vec![PartyMember {
                player_id: player_id.to_string(),
                name: char_name.to_string(),
                current_hp: hp,
                max_hp,
                statuses: vec![],
                class: "Adventurer".to_string(),
                level: 1,
                portrait_url: None,
            }],
        },
        player_id: player_id.to_string(),
    });

    // Combat tick — uses persistent per-session CombatState
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

    // Scan narration for trope trigger keywords → activate matching tropes
    let narration_lower = clean_narration.to_lowercase();
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
    let fired = sidequest_game::trope::TropeEngine::tick(trope_states, trope_defs);
    sidequest_game::trope::TropeEngine::apply_keyword_modifiers(
        trope_states,
        trope_defs,
        &clean_narration,
    );
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
                match queue.enqueue(subject, "oil_painting", "flux-schnell").await {
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

    // Audio cue — trigger mood-based music from genre pack
    if !genre_slug.is_empty() {
        tracing::info!(genre = %genre_slug, mood = "exploration", "Emitting AUDIO_CUE");
        messages.push(GameMessage::AudioCue {
            payload: AudioCuePayload {
                mood: Some("exploration".to_string()),
                music_track: Some(format!(
                    "/genre/{}/audio/music/exploration_full.ogg",
                    genre_slug
                )),
                sfx_triggers: vec![],
            },
            player_id: player_id.to_string(),
        });
    }

    messages
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
    AppState::new_with_game_service(Box::new(Orchestrator::new(watcher_tx)), genre_packs_path)
}

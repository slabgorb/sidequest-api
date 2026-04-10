//! SideQuest Server — axum HTTP/WebSocket server library.
//!
//! Exposes `build_router()`, `AppState`, and server lifecycle functions for the binary and tests.
//! The server depends on the `GameService` trait facade — never on game internals.

pub(crate) mod debug_api;
mod dispatch;
pub(crate) mod extraction;
pub(crate) mod npc_context;
pub mod render_integration;
pub(crate) mod session;
pub mod shared_session;
pub mod tracing_setup;
pub(crate) mod watcher;

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

use sidequest_agents::orchestrator::GameService;
use sidequest_game::builder::CharacterBuilder;

/// Type alias — NpcRegistryEntry lives in sidequest-game, re-exported for crate use.
pub(crate) type NpcRegistryEntry = sidequest_game::NpcRegistryEntry;

use sidequest_genre::{GenreCache, GenreCode, GenreLoader};
use sidequest_protocol::{
    ErrorPayload, GameMessage, PartyMember, PartyStatusPayload, SessionEventPayload,
    TurnStatusPayload,
};

// ---------------------------------------------------------------------------
// Watcher Telemetry — re-exported from sidequest-telemetry (Story 3-6)
// ---------------------------------------------------------------------------

/// Maximum number of watcher events retained for replay to late-connecting GM panels.
const WATCHER_HISTORY_CAP: usize = 500;

// Re-export all telemetry types so existing `use crate::{WatcherEvent, ...}` still works.
pub use sidequest_telemetry::{
    emit as emit_watcher_event, Severity, WatcherEvent, WatcherEventBuilder, WatcherEventType,
};

// Tracing / Telemetry — extracted to tracing_setup.rs
pub use tracing_setup::{build_subscriber_with_filter, init_tracing, tracing_subscriber_for_test};

// ---------------------------------------------------------------------------
// Confrontation Defs — lookup helper (Story 28-1)
// ---------------------------------------------------------------------------

/// Find a ConfrontationDef by encounter_type string.
///
/// Used by dispatch to look up genre-declared encounter types (combat, chase,
/// standoff, negotiation, etc.) from the defs loaded at session start.
pub fn find_confrontation_def<'a>(
    defs: &'a [sidequest_genre::ConfrontationDef],
    encounter_type: &str,
) -> Option<&'a sidequest_genre::ConfrontationDef> {
    defs.iter().find(|d| d.confrontation_type == encounter_type)
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

    /// Run in headless mode (no image rendering).
    #[arg(long, default_value = "false")]
    headless: bool,

    /// Enable trace-level logging.
    #[arg(long, default_value = "false")]
    trace: bool,

    /// OTEL endpoint for Claude subprocess telemetry export (e.g. http://localhost:4318).
    #[arg(long)]
    otel_endpoint: Option<String>,
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

    /// Whether headless mode is enabled.
    pub fn headless(&self) -> bool {
        self.headless
    }

    /// OTEL endpoint for Claude subprocess telemetry.
    pub fn otel_endpoint(&self) -> Option<&str> {
        self.otel_endpoint.as_deref()
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
    genre_loader: GenreLoader,
    genre_cache: GenreCache,
    connections: Mutex<HashMap<PlayerId, mpsc::Sender<GameMessage>>>,
    processing: Mutex<HashSet<PlayerId>>,
    broadcast_tx: broadcast::Sender<GameMessage>,
    /// Accumulated watcher events for replay to late-connecting GM panels.
    /// Capped at WATCHER_HISTORY_CAP to bound memory.
    watcher_event_history: Mutex<Vec<WatcherEvent>>,
    persistence: sidequest_game::PersistenceHandle,
    render_queue: Option<sidequest_game::RenderQueue>,
    beat_filter: tokio::sync::Mutex<sidequest_game::BeatFilter>,
    /// Shared multiplayer sessions keyed by "genre:world".
    sessions: Mutex<HashMap<String, Arc<tokio::sync::Mutex<shared_session::SharedGameSession>>>>,
    /// OTEL endpoint for Claude subprocess telemetry export.
    otel_endpoint: Option<String>,
    /// Path to the sidequest-namegen binary for server-side NPC identity generation.
    /// None if the binary was not found at startup.
    namegen_binary_path: Option<PathBuf>,
    /// Path to the sidequest-encountergen binary for server-side encounter generation.
    encountergen_binary_path: Option<PathBuf>,
    /// Path to the sidequest-loadoutgen binary for server-side loadout generation.
    loadoutgen_binary_path: Option<PathBuf>,
    /// TurnRecord channel sender for training data capture (ADR-073).
    /// Cloned from the same channel that feeds the bridge task.
    watcher_tx: Option<tokio::sync::mpsc::Sender<sidequest_agents::turn_record::TurnRecord>>,
    /// Turn ID counter for monotonic turn numbering across the session.
    turn_id_counter: Mutex<sidequest_agents::turn_record::TurnIdCounter>,
    /// Set of genre slugs that have already produced a `lora_trigger_missing`
    /// ValidationWarning in this process. Used by `dispatch/render.rs` and
    /// `dispatch/audio.rs` to debounce the warn-every-turn flood pattern
    /// (Story 35-15 Rework Pass 2 Finding D). See `mark_lora_warned`.
    ///
    /// Process-scoped rather than per-session: the goal is log-flood
    /// prevention, not per-session uniqueness. A genre misconfig produces
    /// one warning per process lifetime, regardless of how many turns
    /// or sessions render with that genre.
    lora_warned_genres: Mutex<HashSet<String>>,
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

        // Initialize the global telemetry channel (idempotent).
        sidequest_telemetry::init_global_channel();

        // Render pipeline — daemon client connects lazily on first render
        let render_queue = sidequest_game::RenderQueue::spawn(
            sidequest_game::RenderQueueConfig::default(),
            |params: sidequest_game::RenderJobParams| async move {
                let sidequest_game::RenderJobParams {
                    prompt,
                    art_style,
                    tier,
                    width,
                    height,
                    variant,
                    lora_path,
                    lora_scale,
                    ..
                } = params;
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
                                width: Some(width),
                                height: Some(height),
                                variant: variant.clone(),
                                lora_path: lora_path.clone(),
                                lora_scale,
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

        let genre_loader = GenreLoader::new(vec![genre_packs_path.clone()]);
        let genre_cache = GenreCache::new();

        Self {
            inner: Arc::new(AppStateInner {
                game_service,
                genre_packs_path,
                genre_loader,
                genre_cache,
                connections: Mutex::new(HashMap::new()),
                processing: Mutex::new(HashSet::new()),
                broadcast_tx,
                watcher_event_history: Mutex::new(Vec::new()),
                persistence,
                render_queue: Some(render_queue),
                beat_filter: tokio::sync::Mutex::new(sidequest_game::BeatFilter::new(
                    sidequest_game::BeatFilterConfig::default(),
                )),
                sessions: Mutex::new(HashMap::new()),
                namegen_binary_path: None,
                encountergen_binary_path: None,
                loadoutgen_binary_path: None,
                otel_endpoint: None,
                watcher_tx: None,
                turn_id_counter: Mutex::new(sidequest_agents::turn_record::TurnIdCounter::new()),
                lora_warned_genres: Mutex::new(HashSet::new()),
            }),
        }
    }

    /// Disable image rendering by dropping the render queue (builder-style).
    /// In headless mode, no daemon connection is attempted and no IMAGE messages are broadcast.
    pub fn with_render_disabled(mut self) -> Self {
        Arc::get_mut(&mut self.inner)
            .expect("with_render_disabled must be called before cloning")
            .render_queue = None;
        self
    }

    /// Set the path to the sidequest-namegen binary for server-side NPC validation.
    pub fn with_namegen_binary(mut self, path: PathBuf) -> Self {
        Arc::get_mut(&mut self.inner)
            .expect("with_namegen_binary must be called before cloning")
            .namegen_binary_path = Some(path);
        self
    }

    /// Path to the sidequest-namegen binary, if available.
    pub fn namegen_binary_path(&self) -> Option<&Path> {
        self.inner.namegen_binary_path.as_deref()
    }

    /// Set the path to the sidequest-encountergen binary (builder-style).
    pub fn with_encountergen_binary(mut self, path: PathBuf) -> Self {
        Arc::get_mut(&mut self.inner)
            .expect("with_encountergen_binary must be called before cloning")
            .encountergen_binary_path = Some(path);
        self
    }

    /// Path to the sidequest-encountergen binary, if available.
    pub fn encountergen_binary_path(&self) -> Option<&Path> {
        self.inner.encountergen_binary_path.as_deref()
    }

    /// Set the path to the sidequest-loadoutgen binary (builder-style).
    pub fn with_loadoutgen_binary(mut self, path: PathBuf) -> Self {
        Arc::get_mut(&mut self.inner)
            .expect("with_loadoutgen_binary must be called before cloning")
            .loadoutgen_binary_path = Some(path);
        self
    }

    /// Set the TurnRecord channel sender for training data capture (builder-style, ADR-073).
    pub fn with_watcher_tx(
        mut self,
        tx: tokio::sync::mpsc::Sender<sidequest_agents::turn_record::TurnRecord>,
    ) -> Self {
        Arc::get_mut(&mut self.inner)
            .expect("with_watcher_tx must be called before cloning")
            .watcher_tx = Some(tx);
        self
    }

    /// TurnRecord channel sender, if configured.
    pub fn watcher_tx(
        &self,
    ) -> Option<&tokio::sync::mpsc::Sender<sidequest_agents::turn_record::TurnRecord>> {
        self.inner.watcher_tx.as_ref()
    }

    /// Allocate the next monotonic turn ID for TurnRecord construction.
    pub fn next_turn_id(&self) -> u64 {
        self.inner.turn_id_counter.lock().unwrap().next_turn_id()
    }

    /// Set the OTEL endpoint for Claude subprocess telemetry (builder-style).
    pub fn with_otel_endpoint(mut self, endpoint: String) -> Self {
        Arc::get_mut(&mut self.inner)
            .expect("with_otel_endpoint must be called before cloning")
            .otel_endpoint = Some(endpoint);
        self
    }

    /// OTEL endpoint for Claude subprocess telemetry, if configured.
    pub fn otel_endpoint(&self) -> Option<&str> {
        self.inner.otel_endpoint.as_deref()
    }

    /// Build a ClaudeClient with the configured OTEL endpoint (if any).
    pub fn create_claude_client(&self) -> sidequest_agents::client::ClaudeClient {
        if let Some(endpoint) = self.otel_endpoint() {
            sidequest_agents::client::ClaudeClient::builder()
                .otel_endpoint(endpoint.to_string())
                .build()
        } else {
            sidequest_agents::client::ClaudeClient::new()
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

    /// Record that a `lora_trigger_missing` warning has fired for this
    /// genre in the current process, returning `true` if this is the
    /// first time (and therefore the caller should actually emit the
    /// warning). Subsequent calls for the same genre return `false`.
    ///
    /// This debounces the `(Some(lora), None)` warn path in
    /// `dispatch/render.rs` and `dispatch/audio.rs` so a misconfigured
    /// genre pack does not flood the GM panel with ValidationWarning
    /// events on every render turn (Story 35-15 Rework Pass 2
    /// Finding D).
    pub fn mark_lora_warned(&self, genre_slug: &str) -> bool {
        self.inner
            .lora_warned_genres
            .lock()
            .unwrap()
            .insert(genre_slug.to_string())
    }

    /// Cached genre pack loader — loads from disk once, then returns the same `Arc`.
    pub fn genre_cache(&self) -> &GenreCache {
        &self.inner.genre_cache
    }

    /// Genre loader (search paths for genre pack directories).
    pub fn genre_loader(&self) -> &GenreLoader {
        &self.inner.genre_loader
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
                let was_present = session.players.remove(player_id).is_some();
                // Remove player from barrier roster if active
                if let Some(ref barrier) = session.turn_barrier {
                    let _ = barrier.remove_player(player_id);
                }
                let remaining = session.players.len();
                // Transition TurnMode when dropping back to solo —
                // only if we actually removed a player (skip for
                // superseded connections from reconnect)
                if was_present {
                    let old_mode = std::mem::take(&mut session.turn_mode);
                    session.turn_mode =
                        old_mode.apply(sidequest_game::turn_mode::TurnModeTransition::PlayerLeft {
                            player_count: remaining,
                        });
                    if !session.turn_mode.should_use_barrier() {
                        session.turn_barrier = None;
                    }
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

    /// Subscribe to the global watcher telemetry channel.
    pub fn subscribe_watcher(&self) -> broadcast::Receiver<WatcherEvent> {
        sidequest_telemetry::subscribe_global()
            .expect("telemetry channel not initialized — call init_global_channel() first")
    }

    /// Store a telemetry event in replay history.
    ///
    /// The global channel handles broadcasting; this method is for the
    /// history-capture task that stores events for late-connecting GM panels.
    pub fn store_watcher_event(&self, event: WatcherEvent) {
        let mut history = self.inner.watcher_event_history.lock().unwrap();
        if history.len() >= WATCHER_HISTORY_CAP {
            let drain_count = history.len() - WATCHER_HISTORY_CAP + 1;
            history.drain(..drain_count);
        }
        history.push(event);
    }

    /// Return a snapshot of all stored watcher events for replay to late-connecting clients.
    pub fn get_watcher_history(&self) -> Vec<WatcherEvent> {
        self.inner.watcher_event_history.lock().unwrap().clone()
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

// Session state machine — extracted to session.rs
pub(crate) use session::Session;

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
        .route("/api/debug/state", get(debug_api::debug_state))
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
        let mut session_rx: Option<broadcast::Receiver<crate::shared_session::TargetedMessage>> =
            None;

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
    let mut trope_states: Vec<sidequest_game::trope::TropeState> = vec![];
    let mut trope_defs: Vec<sidequest_genre::TropeDefinition> = vec![];
    let mut world_context: String = String::new();
    let mut opening_seed: Option<String> = None;
    let mut opening_directive: Option<String> = None;
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
    let mut quest_log: HashMap<String, String> = HashMap::new();
    let mut genie_wishes: Vec<sidequest_game::GenieWish> = vec![];
    // Phase 5: resource_state / resource_declarations removed. Pools live on
    // snapshot.resources; genre pack declarations flow through
    // init_resource_pools() once on connect and are not stored long-term.
    // `pools_initialized` prevents re-running init on every turn.
    let mut pools_initialized = false;
    let mut achievement_tracker = sidequest_game::achievement::AchievementTracker::default();
    // Canonical game snapshot — carried through the dispatch pipeline (story 15-8).
    let mut snapshot = sidequest_game::state::GameSnapshot::default();
    let narrator_verbosity = sidequest_protocol::NarratorVerbosity::default();
    let narrator_vocabulary = sidequest_protocol::NarratorVocabulary::default();
    let mut pending_trope_context: Option<String> = None;
    let audio_mixer: std::sync::Arc<tokio::sync::Mutex<Option<sidequest_game::AudioMixer>>> =
        std::sync::Arc::new(tokio::sync::Mutex::new(None));
    let prerender_scheduler: std::sync::Arc<
        tokio::sync::Mutex<Option<sidequest_game::PrerenderScheduler>>,
    > = std::sync::Arc::new(tokio::sync::Mutex::new(None));

    // Reader loop: read messages, deserialize, dispatch through session
    while let Some(msg) = ws_stream.next().await {
        match msg {
            Ok(AxumWsMessage::Text(text)) => {
                tracing::info!(
                    player_id = %player_id_str,
                    text_len = text.len(),
                    text_preview = %&text[..text.len().min(120)],
                    "ws.message_received"
                );
                match serde_json::from_str::<GameMessage>(&text) {
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
                            &mut trope_states,
                            &mut trope_defs,
                            &mut world_context,
                            &mut opening_seed,
                            &mut opening_directive,
                            &mut axes_config,
                            &mut axis_values,
                            &mut visual_style,
                            &mut music_director,
                            &audio_mixer,
                            &prerender_scheduler,
                            &mut npc_registry,
                            &mut narration_history,
                            &mut discovered_regions,
                            &mut turn_manager,
                            &mut lore_store,
                            &shared_session,
                            &state,
                            &player_id_str,
                            &mut continuity_corrections,
                            &mut quest_log,
                            &mut genie_wishes,
                            &mut achievement_tracker,
                            &mut snapshot,
                            narrator_verbosity,
                            narrator_vocabulary,
                            &mut pending_trope_context,
                            &tx,
                        )
                        .await;
                        tracing::info!(
                            player_id = %player_id_str,
                            response_count = responses.len(),
                            "dispatch_message.returned"
                        );

                        // Epic 16: Initialize resource pools from genre pack on first
                        // post-connect message. init_resource_pools is an upsert, so if
                        // the save already has pools (via backward-compat migration in
                        // GameSnapshotRaw), their `current` values are preserved and
                        // only metadata is refreshed from the pack.
                        if !pools_initialized {
                            if let (Some(genre), Some(_world)) =
                                (session.genre_slug(), session.world_slug())
                            {
                                if let Ok(genre_code) = GenreCode::new(genre) {
                                    if let Ok(pack) = state
                                        .genre_cache()
                                        .get_or_load(&genre_code, state.genre_loader())
                                    {
                                        let declarations = &pack.rules.resources;
                                        if !declarations.is_empty() {
                                            snapshot.init_resource_pools(declarations);
                                            tracing::info!(
                                                genre = %genre,
                                                resource_count = declarations.len(),
                                                pool_names = ?declarations.iter().map(|r| &r.name).collect::<Vec<_>>(),
                                                "resource_pools.initialized — genre resources loaded into snapshot"
                                            );
                                            WatcherEventBuilder::new(
                                                "resource_pool",
                                                WatcherEventType::StateTransition,
                                            )
                                            .field("event", "resource_pools.initialized")
                                            .field("genre", genre)
                                            .field("count", declarations.len())
                                            .field(
                                                "pools",
                                                declarations
                                                    .iter()
                                                    .map(|r| r.name.clone())
                                                    .collect::<Vec<_>>(),
                                            )
                                            .send();
                                        }
                                        pools_initialized = true;
                                    }
                                }
                            }
                        }

                        for resp in responses {
                            if let Err(e) = tx.send(resp).await {
                                tracing::error!(player_id = %player_id_str, error = %e, "Failed to send response to client");
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!(player_id = %player_id_str, error = %e, text_preview = %&text[..text.len().min(200)], "Invalid message — deserialization failed");
                        let err_msg =
                            error_response(&player_id_str, &format!("Invalid JSON: {}", e));
                        let _ = tx.send(err_msg).await;
                    }
                }
            }
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
                    // Only fire departure if this player_id is still registered
                    // in the shared session. A reconnect from the same player_name
                    // transfers the PlayerState to a new player_id and removes the
                    // old one, so the superseded connection should NOT emit departure.
                    if ss.players.contains_key(&player_id_str) {
                        // Send departure narration to remaining players (mirrors join narration)
                        let departure_name = ss
                            .players
                            .get(&player_id_str)
                            .and_then(|ps| ps.character_name.clone())
                            .or_else(|| player_name_for_session.clone())
                            .unwrap_or_else(|| "Unknown".to_string());
                        let departure_text = format!("{} has left the scene.", departure_name);
                        let remaining_pids: Vec<String> = ss
                            .players
                            .keys()
                            .filter(|pid| pid.as_str() != &*player_id_str)
                            .cloned()
                            .collect();
                        for target_pid in &remaining_pids {
                            ss.send_to_player(
                                GameMessage::Narration {
                                    payload: sidequest_protocol::NarrationPayload {
                                        text: departure_text.clone(),
                                        state_delta: None,
                                        footnotes: vec![],
                                    },
                                    player_id: target_pid.clone(),
                                },
                                target_pid.clone(),
                            );
                            ss.send_to_player(
                                GameMessage::NarrationEnd {
                                    payload: sidequest_protocol::NarrationEndPayload {
                                        state_delta: None,
                                    },
                                    player_id: target_pid.clone(),
                                },
                                target_pid.clone(),
                            );
                        }

                        let leave_msg = GameMessage::SessionEvent {
                            payload: SessionEventPayload {
                                event: "player_left".to_string(),
                                player_name: player_name_for_session.clone(),
                                genre: None,
                                world: None,
                                has_character: None,
                                initial_state: None,
                                css: None,
                                image_cooldown_seconds: None,
                                narrator_verbosity: None,
                                narrator_vocabulary: None,
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
                    } else {
                        tracing::info!(
                            player_id = %player_id_str,
                            "Skipping departure — player_id was superseded by reconnect"
                        );
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
        WatcherEventBuilder::new("multiplayer", WatcherEventType::StateTransition)
            .field("event", "session_left")
            .field("session_key", &key)
            .field("remaining_players", remaining)
            .send();
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
    trope_states: &mut Vec<sidequest_game::trope::TropeState>,
    trope_defs: &mut Vec<sidequest_genre::TropeDefinition>,
    world_context: &mut String,
    opening_seed: &mut Option<String>,
    opening_directive: &mut Option<String>,
    axes_config: &mut Option<sidequest_genre::AxesConfig>,
    axis_values: &mut Vec<sidequest_game::axis::AxisValue>,
    visual_style: &mut Option<sidequest_genre::VisualStyle>,
    music_director: &mut Option<sidequest_game::MusicDirector>,
    audio_mixer: &std::sync::Arc<tokio::sync::Mutex<Option<sidequest_game::AudioMixer>>>,
    prerender_scheduler: &std::sync::Arc<
        tokio::sync::Mutex<Option<sidequest_game::PrerenderScheduler>>,
    >,
    npc_registry: &mut Vec<NpcRegistryEntry>,
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
    quest_log: &mut HashMap<String, String>,
    genie_wishes: &mut Vec<sidequest_game::GenieWish>,
    achievement_tracker: &mut sidequest_game::achievement::AchievementTracker,
    snapshot: &mut sidequest_game::state::GameSnapshot,
    narrator_verbosity: sidequest_protocol::NarratorVerbosity,
    narrator_vocabulary: sidequest_protocol::NarratorVocabulary,
    pending_trope_context: &mut Option<String>,
    tx: &mpsc::Sender<GameMessage>,
) -> Vec<GameMessage> {
    tracing::info!(
        msg_type = ?std::mem::discriminant(&msg),
        session_state = %session.state_name(),
        player_id = %player_id,
        "dispatch_message.entry"
    );
    match &msg {
        GameMessage::SessionEvent { payload, .. } if payload.event == "connect" => {
            let mut responses = dispatch::connect::dispatch_connect(
                payload,
                session,
                builder,
                player_name_store,
                character_json,
                character_name,
                character_hp,
                character_max_hp,
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
                lore_store,
                opening_seed,
                opening_directive,
                state,
                player_id,
                continuity_corrections,
                inventory,
                snapshot,
                &tx,
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
                            if let Ok(pack) = state
                                .genre_cache()
                                .get_or_load(&genre_code, state.genre_loader())
                            {
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
                    ps.inventory = inventory.clone();
                    ps.character_json = character_json.clone();
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
                    if ss_guard.turn_mode.should_use_barrier() && ss_guard.turn_barrier.is_none() {
                        let mp_session =
                            sidequest_game::multiplayer::MultiplayerSession::with_player_ids(
                                ss_guard.players.keys().cloned(),
                            );
                        let config = sidequest_game::barrier::TurnBarrierConfig::disabled();
                        ss_guard.turn_barrier = Some(sidequest_game::barrier::TurnBarrier::new(
                            mp_session, config,
                        ));
                        {
                            let _span =
                                tracing::info_span!("barrier.activated", player_count = pc,)
                                    .entered();
                        }
                        tracing::info!(
                            player_count = pc,
                            "Initialized turn barrier for reconnecting player"
                        );

                        // Story 35-5: Spawn turn reminder task
                        let reminder_config =
                            sidequest_game::turn_reminder::ReminderConfig::default();
                        let reminder_barrier = ss_guard.turn_barrier.as_ref().unwrap().clone();
                        tokio::spawn(async move {
                            {
                                let _span = tracing::info_span!("reminder_spawned").entered();
                                tracing::info!("Turn reminder task spawned");
                            }
                            let result = reminder_barrier.run_reminder(&reminder_config).await;
                            if result.should_send() {
                                let _span = tracing::info_span!(
                                    "reminder_fired",
                                    idle_player_count = result.idle_players().len(),
                                )
                                .entered();
                                tracing::info!(
                                    idle_player_count = result.idle_players().len(),
                                    "Turn reminder fired for idle players"
                                );
                            }
                        });
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
                            current_location: ps.display_location.clone(),
                        })
                        .collect();
                    if !members.is_empty() {
                        let pids: Vec<String> = ss_guard.players.keys().cloned().collect();
                        for target_pid in &pids {
                            let party_msg = GameMessage::PartyStatus {
                                payload: PartyStatusPayload {
                                    members: members.clone(),
                                },
                                player_id: target_pid.clone(),
                            };
                            ss_guard.send_to_player(party_msg, target_pid.clone());
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
                            current_location: ps.display_location.clone(),
                        })
                        .collect();
                    let member_count = all_members.len();
                    responses.push(GameMessage::PartyStatus {
                        payload: PartyStatusPayload {
                            members: all_members,
                        },
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
            dispatch::connect::dispatch_character_creation(
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
                trope_states,
                trope_defs,
                world_context,
                opening_seed,
                opening_directive,
                axes_config,
                axis_values,
                visual_style,
                npc_registry,
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
                quest_log,
                genie_wishes,
                achievement_tracker,
                snapshot,
                narrator_verbosity,
                narrator_vocabulary,
                pending_trope_context,
                &tx,
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
            {
                let aside =
                    payload.action.starts_with("(aside)") || payload.action.starts_with("/aside");

                // Monster Manual: load from disk, seed if needed (ADR-059)
                let gs = session.genre_slug().unwrap_or("");
                let ws = session.world_slug().unwrap_or("");
                let mut monster_manual =
                    sidequest_game::monster_manual::MonsterManual::load(gs, ws);
                if monster_manual.needs_seeding() && !gs.is_empty() {
                    dispatch::pregen::seed_manual(state, gs, ws, &mut monster_manual);
                }

                let mut ctx = dispatch::DispatchContext {
                    action: &payload.action,
                    char_name: character_name.as_deref().unwrap_or("Unknown"),
                    player_id,
                    genre_slug: session.genre_slug().unwrap_or(""),
                    world_slug: session.world_slug().unwrap_or(""),
                    player_name_for_save: player_name_store.as_deref().unwrap_or("Player"),
                    hp: character_hp,
                    max_hp: character_max_hp,
                    level: character_level,
                    xp: character_xp,
                    current_location,
                    inventory,
                    character_json,
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
                    continuity_corrections,
                    genie_wishes,
                    sfx_library: {
                        let gs = session.genre_slug().unwrap_or("");
                        sidequest_genre::GenreCode::new(gs)
                            .ok()
                            .and_then(|gc| {
                                state
                                    .genre_cache()
                                    .get_or_load(&gc, state.genre_loader())
                                    .ok()
                            })
                            .map(|pack| pack.audio.sfx_library.clone())
                            .unwrap_or_default()
                    },
                    rooms: {
                        let gs = session.genre_slug().unwrap_or("");
                        let ws = session.world_slug().unwrap_or("");
                        sidequest_genre::GenreCode::new(gs)
                            .ok()
                            .and_then(|gc| {
                                state
                                    .genre_cache()
                                    .get_or_load(&gc, state.genre_loader())
                                    .ok()
                            })
                            .and_then(|pack| pack.worlds.get(ws).cloned())
                            .filter(|world| {
                                world.cartography.navigation_mode
                                    == sidequest_genre::NavigationMode::RoomGraph
                            })
                            .and_then(|world| world.cartography.rooms.clone())
                            .unwrap_or_default()
                    },
                    genre_affinities: {
                        let gs = session.genre_slug().unwrap_or("");
                        sidequest_genre::GenreCode::new(gs)
                            .ok()
                            .and_then(|gc| {
                                state
                                    .genre_cache()
                                    .get_or_load(&gc, state.genre_loader())
                                    .ok()
                            })
                            .map(|pack| pack.progression.affinities.clone())
                            .unwrap_or_default()
                    },
                    world_graph: {
                        let gs = session.genre_slug().unwrap_or("");
                        let ws = session.world_slug().unwrap_or("");
                        sidequest_genre::GenreCode::new(gs)
                            .ok()
                            .and_then(|gc| {
                                state
                                    .genre_cache()
                                    .get_or_load(&gc, state.genre_loader())
                                    .ok()
                            })
                            .and_then(|pack| pack.worlds.get(ws).cloned())
                            .and_then(|world| world.cartography.world_graph)
                    },
                    cartography_metadata: {
                        let gs = session.genre_slug().unwrap_or("");
                        let ws = session.world_slug().unwrap_or("");
                        sidequest_genre::GenreCode::new(gs)
                            .ok()
                            .and_then(|gc| {
                                state
                                    .genre_cache()
                                    .get_or_load(&gc, state.genre_loader())
                                    .ok()
                            })
                            .and_then(|pack| pack.worlds.get(ws).cloned())
                            .map(|world| {
                                let nav_mode = match world.cartography.navigation_mode {
                                    sidequest_genre::NavigationMode::Region => "region",
                                    sidequest_genre::NavigationMode::RoomGraph => "room_graph",
                                    sidequest_genre::NavigationMode::Hierarchical => "hierarchical",
                                };
                                sidequest_protocol::CartographyMetadata {
                                    navigation_mode: nav_mode.to_string(),
                                    starting_region: world.cartography.starting_region.clone(),
                                    regions: world
                                        .cartography
                                        .regions
                                        .iter()
                                        .map(|(slug, r)| {
                                            (
                                                slug.clone(),
                                                sidequest_protocol::CartographyRegion {
                                                    name: r.name.clone(),
                                                    description: r.description.clone(),
                                                    adjacent: r.adjacent.clone(),
                                                },
                                            )
                                        })
                                        .collect(),
                                    routes: world
                                        .cartography
                                        .routes
                                        .iter()
                                        .map(|r| sidequest_protocol::CartographyRoute {
                                            name: r.name.clone(),
                                            description: r.description.clone(),
                                            from_id: r.from_id.clone(),
                                            to_id: r.to_id.clone(),
                                        })
                                        .collect(),
                                }
                            })
                    },
                    confrontation_defs: {
                        let gs = session.genre_slug().unwrap_or("");
                        sidequest_genre::GenreCode::new(gs)
                            .ok()
                            .and_then(|gc| {
                                state
                                    .genre_cache()
                                    .get_or_load(&gc, state.genre_loader())
                                    .ok()
                            })
                            .map(|pack| pack.rules.confrontations.clone())
                            .unwrap_or_default()
                    },
                    aside,
                    opening_directive: None,
                    narrator_verbosity,
                    narrator_vocabulary,
                    pending_trope_context,
                    achievement_tracker,
                    snapshot,
                    tx: &tx,
                    monster_manual: &mut monster_manual,
                    morpheme_glossaries: Vec::new(),
                    name_banks: Vec::new(),
                    carry_mode: {
                        let gs = session.genre_slug().unwrap_or("");
                        sidequest_genre::GenreCode::new(gs)
                            .ok()
                            .and_then(|gc| {
                                state
                                    .genre_cache()
                                    .get_or_load(&gc, state.genre_loader())
                                    .ok()
                            })
                            .map(|pack| {
                                pack.inventory
                                    .as_ref()
                                    .and_then(|inv| inv.philosophy.as_ref())
                                    .map(|phil| phil.carry_mode)
                                    .unwrap_or_default()
                            })
                            .unwrap_or_default()
                    },
                    weight_limit: {
                        let gs = session.genre_slug().unwrap_or("");
                        sidequest_genre::GenreCode::new(gs)
                            .ok()
                            .and_then(|gc| {
                                state
                                    .genre_cache()
                                    .get_or_load(&gc, state.genre_loader())
                                    .ok()
                            })
                            .and_then(|pack| {
                                pack.inventory
                                    .as_ref()
                                    .and_then(|inv| inv.philosophy.as_ref())
                                    .and_then(|phil| phil.weight_limit)
                            })
                    },
                };
                // OTEL: log loaded confrontation defs (story 28-1)
                if !ctx.confrontation_defs.is_empty() {
                    WatcherEventBuilder::new("encounter", WatcherEventType::StateTransition)
                        .field("action", "defs_loaded")
                        .field("genre", ctx.genre_slug)
                        .field("count", ctx.confrontation_defs.len())
                        .field(
                            "types",
                            ctx.confrontation_defs
                                .iter()
                                .map(|d| d.confrontation_type.clone())
                                .collect::<Vec<_>>(),
                        )
                        .send();
                }

                let result = dispatch::dispatch_player_action(&mut ctx).await;

                // Save Manual after dispatch (entries may have been marked Active)
                ctx.monster_manual.save();

                result
            }
        }
        // Journal browse — on-demand KnownFact retrieval (story 9-13)
        GameMessage::JournalRequest { payload, .. } => {
            if !session.is_playing() {
                return vec![error_response(
                    player_id,
                    "Cannot browse journal before game starts",
                )];
            }
            let char_name = character_name.as_deref().unwrap_or("");
            let known_facts: &[sidequest_game::known_fact::KnownFact] = snapshot
                .characters
                .iter()
                .find(|c| c.core.name.as_str() == char_name)
                .map(|c| c.known_facts.as_slice())
                .unwrap_or(&[]);

            let filter = sidequest_game::journal::JournalFilter {
                category: payload.category,
                sort_by: payload.sort_by,
            };
            let entries = sidequest_game::journal::build_journal_entries(known_facts, &filter);

            tracing::info!(
                character = %char_name,
                entry_count = entries.len(),
                category_filter = ?payload.category,
                sort_by = ?payload.sort_by,
                "journal.request"
            );

            // Emit OTEL watcher event for GM panel visibility
            WatcherEventBuilder::new("journal", WatcherEventType::SubsystemExerciseSummary)
                .field("character", char_name)
                .field("entry_count", entries.len())
                .field("category_filter", format!("{:?}", payload.category))
                .send();

            vec![GameMessage::JournalResponse {
                payload: sidequest_protocol::JournalResponsePayload { entries },
                player_id: player_id.to_string(),
            }]
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

// Watcher WebSocket Handler — extracted to watcher.rs

// ---------------------------------------------------------------------------
// Server lifecycle
// ---------------------------------------------------------------------------

/// Create and run the server on a given port with a shutdown signal.
pub async fn create_server(
    state: AppState,
    port: u16,
    shutdown: oneshot::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
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
#[doc(hidden)]
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

//! SideQuest Server — axum HTTP/WebSocket server library.
//!
//! Exposes `build_router()`, `AppState`, and server lifecycle functions for the binary and tests.
//! The server depends on the `GameService` trait facade — never on game internals.

pub mod render_integration;
pub mod shared_session;

mod cli;
mod dispatch;
mod helpers;
mod lifecycle;
mod router;
mod session;
mod state;
mod telemetry;
mod types;
pub(crate) mod ws;

// --- Public API (preserves all existing import paths for main.rs + tests) ---
pub use cli::Args;
pub use lifecycle::{create_server, serve_with_listener, test_app_state};
pub use router::build_router;
pub use session::Session;
pub use state::AppState;
pub use telemetry::{
    build_subscriber_with_filter, init_tracing, tracing_subscriber_for_test, Severity,
    WatcherEvent, WatcherEventType,
};
pub use types::{
    error_response, reconnect_required_response, PlayerId, ProcessingGuard, ServerError,
};

// Crate-internal re-exports
pub(crate) use dispatch::dispatch_message;
pub(crate) use helpers::npc::NpcRegistryEntry;

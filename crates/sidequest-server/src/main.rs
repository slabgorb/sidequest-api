//! SideQuest Server — axum HTTP/WebSocket server.
//!
//! This is the main entry point for the SideQuest game server, providing HTTP and WebSocket
//! endpoints for the React frontend to interact with the game engine.

use clap::Parser;
use sidequest_agents::orchestrator::Orchestrator;
use sidequest_agents::turn_record::{run_validator, TurnRecord, WATCHER_CHANNEL_CAPACITY};
use sidequest_server::{create_server, AppState, Args};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    sidequest_server::init_tracing();

    let args = Args::parse();
    tracing::info!(port = args.port(), genre_packs = %args.genre_packs_path().display(), "SideQuest Server starting");

    let (watcher_tx, watcher_rx) =
        tokio::sync::mpsc::channel::<TurnRecord>(WATCHER_CHANNEL_CAPACITY);

    // Spawn the validator task on the cold path
    tokio::spawn(async move {
        run_validator(watcher_rx).await;
    });

    let state = AppState::new_with_game_service(
        Box::new(Orchestrator::new(watcher_tx)),
        args.genre_packs_path().to_path_buf(),
    );

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    // Shutdown on ctrl-c
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("SIGTERM received, initiating graceful shutdown");
        shutdown_tx.send(()).ok();
    });

    create_server(state, args.port(), shutdown_rx).await
}

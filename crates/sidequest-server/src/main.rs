//! SideQuest Server — axum HTTP/WebSocket server.
//!
//! This is the main entry point for the SideQuest game server, providing HTTP and WebSocket
//! endpoints for the React frontend to interact with the game engine.

use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    tracing::info!("SideQuest Server starting up...");

    // Placeholder: server setup will be implemented in subsequent stories
    println!("SideQuest Server — ready to build");
}

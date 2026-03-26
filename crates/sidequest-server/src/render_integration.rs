//! Render integration — IMAGE message broadcast from render queue.
//!
//! Story 4-5: When the render queue completes an image render, broadcast
//! an IMAGE message to all connected WebSocket clients.
//!
//! Flow: RenderQueue completion → RenderJobResult → GameMessage::Image → broadcast

use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use sidequest_game::render_queue::RenderJobResult;
use sidequest_game::subject::{RenderSubject, SceneType, SubjectTier};
use sidequest_protocol::GameMessage;

/// Metadata about the render subject, carried alongside the render result
/// so the broadcaster can populate the IMAGE payload with tier, scene_type,
/// and description.
#[derive(Debug, Clone)]
pub struct RenderResultContext {
    /// The render job result from the queue.
    pub result: RenderJobResult,
    /// The original subject that was rendered (for tier, scene_type, description).
    pub subject: RenderSubject,
}

/// Spawn the image broadcaster background task.
///
/// Subscribes to the render queue's result channel and forwards completed
/// renders as `GameMessage::Image` to the WebSocket broadcast channel.
///
/// - On `RenderJobResult::Success`: builds `ImagePayload` and broadcasts
/// - On `RenderJobResult::Failed`: logs a warning, does NOT send to clients
///
/// The task runs until the render channel closes (session disconnect).
/// Returns a `JoinHandle` for lifecycle management.
pub fn spawn_image_broadcaster(
    mut render_rx: broadcast::Receiver<RenderResultContext>,
    _ws_tx: broadcast::Sender<GameMessage>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        // Stub — consumes from channel but does NOT translate to GameMessage.
        // Implementation in story 4-5 GREEN phase.
        while let Ok(_result) = render_rx.recv().await {
            // No-op: swallows render results without broadcasting IMAGE messages.
        }
    })
}

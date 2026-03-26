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
use sidequest_protocol::{GameMessage, ImagePayload};

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

fn tier_to_string(tier: &SubjectTier) -> String {
    match tier {
        SubjectTier::Portrait => "portrait".to_string(),
        SubjectTier::Scene => "scene".to_string(),
        SubjectTier::Landscape => "landscape".to_string(),
        SubjectTier::Abstract => "abstract".to_string(),
        _ => "unknown".to_string(),
    }
}

fn scene_type_to_string(scene_type: &SceneType) -> String {
    match scene_type {
        SceneType::Combat => "combat".to_string(),
        SceneType::Dialogue => "dialogue".to_string(),
        SceneType::Exploration => "exploration".to_string(),
        SceneType::Discovery => "discovery".to_string(),
        SceneType::Transition => "transition".to_string(),
        _ => "unknown".to_string(),
    }
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
        while let Ok(ctx) = render_rx.recv().await {
            match ctx.result {
                RenderJobResult::Success {
                    job_id,
                    image_url,
                    generation_ms,
                } => {
                    let payload = ImagePayload {
                        url: image_url,
                        description: ctx.subject.prompt_fragment().to_string(),
                        handout: false,
                        render_id: Some(job_id.to_string()),
                        tier: Some(tier_to_string(ctx.subject.tier())),
                        scene_type: Some(scene_type_to_string(ctx.subject.scene_type())),
                        generation_ms: Some(generation_ms),
                    };

                    let msg = GameMessage::Image {
                        payload,
                        player_id: String::new(),
                    };

                    // Ignore send errors — no subscribers is fine (Rule #1)
                    let _ = _ws_tx.send(msg);
                }
                RenderJobResult::Failed { job_id, error } => {
                    tracing::warn!(
                        job_id = %job_id,
                        error = %error,
                        "Render job failed, not broadcasting IMAGE"
                    );
                }
                _ => {
                    // Handle future variants of RenderJobResult
                    tracing::debug!("Unrecognized render result variant");
                }
            }
        }
    })
}

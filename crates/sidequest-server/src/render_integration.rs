//! Render integration — IMAGE message broadcast from render queue.
//!
//! Story 4-5: When the render queue completes an image render, broadcast
//! an IMAGE message to all connected WebSocket clients.
//!
//! Story 14-6: Image pacing throttle — configurable cooldown between renders
//! to prevent image flooding during rapid turn sequences.
//!
//! Flow: RenderQueue completion → RenderJobResult → GameMessage::Image → broadcast

use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use sidequest_game::render_queue::RenderJobResult;
use sidequest_game::subject::{RenderSubject, SceneType, SubjectTier};
use sidequest_protocol::{GameMessage, ImagePayload};

/// Image pacing throttle — suppresses image generation within a configurable
/// cooldown window. Default: 30s solo, 60s multiplayer.
///
/// Story 14-6: Prevents image flooding during rapid turn sequences.
#[derive(Debug, Clone)]
pub struct ImagePacingThrottle {
    cooldown_secs: u32,
    last_render: Option<Instant>,
}

impl ImagePacingThrottle {
    /// Create a throttle with the default cooldown for the given player count.
    /// Solo (1 player): 30s. Multiplayer (2+): 60s.
    pub fn default_for_player_count(n: usize) -> Self {
        let cooldown = if n <= 1 { 30 } else { 60 };
        Self {
            cooldown_secs: cooldown,
            last_render: None,
        }
    }

    /// Create a throttle with a specific cooldown in seconds.
    /// A cooldown of 0 effectively disables throttling.
    pub fn with_cooldown(seconds: u32) -> Self {
        Self {
            cooldown_secs: seconds,
            last_render: None,
        }
    }

    /// Returns true if enough time has passed since the last render
    /// (or if no render has been recorded yet).
    /// A zero cooldown always allows.
    pub fn should_allow(&self) -> bool {
        if self.cooldown_secs == 0 {
            return true;
        }
        match self.last_render {
            None => true,
            Some(last) => last.elapsed().as_secs() >= u64::from(self.cooldown_secs),
        }
    }

    /// Record that a render was just performed. Resets the cooldown timer.
    pub fn record_render(&mut self) {
        self.last_render = Some(Instant::now());
    }

    /// DM force override — always returns true, does NOT reset the timer
    /// (the caller should call `record_render()` separately if desired).
    pub fn should_allow_forced(&mut self) -> bool {
        true
    }

    /// Seconds remaining in the current cooldown window, or 0 if expired.
    pub fn remaining_cooldown_seconds(&self) -> u32 {
        match self.last_render {
            None => 0,
            Some(last) => {
                let elapsed = last.elapsed().as_secs() as u32;
                self.cooldown_secs.saturating_sub(elapsed)
            }
        }
    }

    /// Update the cooldown duration mid-session.
    pub fn set_cooldown(&mut self, seconds: u32) {
        self.cooldown_secs = seconds;
    }

    /// Get the current cooldown duration in seconds.
    pub fn cooldown_seconds(&self) -> u32 {
        self.cooldown_secs
    }
}

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
    ws_tx: broadcast::Sender<GameMessage>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        while let Ok(ctx) = render_rx.recv().await {
            match ctx.result {
                RenderJobResult::Success {
                    job_id,
                    image_url,
                    generation_ms,
                    ..
                } => {
                    // Guard: if the URL is empty after the whole pipeline,
                    // something went very wrong — scream, don't broadcast garbage.
                    if image_url.trim().is_empty() {
                        tracing::error!(
                            job_id = %job_id,
                            "render_broadcast_blocked — image_url is empty, refusing to send broken IMAGE to clients"
                        );
                        continue;
                    }

                    let tier_str = tier_to_string(ctx.subject.tier());
                    let scene_str = scene_type_to_string(ctx.subject.scene_type());

                    tracing::info!(
                        job_id = %job_id,
                        image_url = %image_url,
                        generation_ms = generation_ms,
                        tier = %tier_str,
                        scene_type = %scene_str,
                        "render_broadcast — sending IMAGE to WebSocket clients"
                    );

                    let payload = ImagePayload {
                        url: image_url,
                        description: ctx.subject.prompt_fragment().to_string(),
                        handout: false,
                        render_id: Some(job_id.to_string()),
                        tier: Some(tier_str),
                        scene_type: Some(scene_str),
                        generation_ms: Some(generation_ms),
                    };

                    let msg = GameMessage::Image {
                        payload,
                        player_id: String::new(),
                    };

                    // Ignore send errors — no subscribers is fine (Rule #1)
                    let _ = ws_tx.send(msg);
                }
                RenderJobResult::Failed { job_id, error } => {
                    // Error level, not warn — a failed render means the player
                    // gets no scene illustration. That's visible breakage.
                    tracing::error!(
                        job_id = %job_id,
                        error = %error,
                        "render_broadcast_failed — render job failed, no IMAGE sent to clients"
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

/// Spawn the image broadcaster with an `ImagePacingThrottle`.
///
/// Same as `spawn_image_broadcaster`, but applies throttle logic before
/// broadcasting. Renders within the cooldown window are silently dropped.
///
/// Story 14-6: Image pacing throttle integration.
pub fn spawn_image_broadcaster_with_throttle(
    mut render_rx: broadcast::Receiver<RenderResultContext>,
    ws_tx: broadcast::Sender<GameMessage>,
    throttle: ImagePacingThrottle,
) -> JoinHandle<()> {
    let throttle = Arc::new(Mutex::new(throttle));
    tokio::spawn(async move {
        while let Ok(ctx) = render_rx.recv().await {
            match ctx.result {
                RenderJobResult::Success {
                    job_id,
                    image_url,
                    generation_ms,
                    ..
                } => {
                    if image_url.trim().is_empty() {
                        tracing::error!(
                            job_id = %job_id,
                            "render_broadcast_blocked — image_url is empty, refusing to send broken IMAGE to clients"
                        );
                        continue;
                    }

                    // Check throttle
                    let allowed = {
                        let mut t = throttle.lock().unwrap();
                        if t.should_allow() {
                            t.record_render();
                            true
                        } else {
                            false
                        }
                    };

                    if !allowed {
                        tracing::info!(
                            job_id = %job_id,
                            "render_throttled — image suppressed by pacing cooldown"
                        );
                        continue;
                    }

                    let tier_str = tier_to_string(ctx.subject.tier());
                    let scene_str = scene_type_to_string(ctx.subject.scene_type());

                    tracing::info!(
                        job_id = %job_id,
                        image_url = %image_url,
                        generation_ms = generation_ms,
                        tier = %tier_str,
                        scene_type = %scene_str,
                        "render_broadcast — sending IMAGE to WebSocket clients"
                    );

                    let payload = ImagePayload {
                        url: image_url,
                        description: ctx.subject.prompt_fragment().to_string(),
                        handout: false,
                        render_id: Some(job_id.to_string()),
                        tier: Some(tier_str),
                        scene_type: Some(scene_str),
                        generation_ms: Some(generation_ms),
                    };

                    let msg = GameMessage::Image {
                        payload,
                        player_id: String::new(),
                    };

                    let _ = ws_tx.send(msg);
                }
                RenderJobResult::Failed { job_id, error } => {
                    tracing::error!(
                        job_id = %job_id,
                        error = %error,
                        "render_broadcast_failed — render job failed, no IMAGE sent to clients"
                    );
                }
                _ => {
                    tracing::debug!("Unrecognized render result variant");
                }
            }
        }
    })
}

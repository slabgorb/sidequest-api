//! Render queue — async image generation queue with hash-based cache dedup.
//!
//! Decouples the game loop from image generation by accepting render jobs
//! asynchronously and processing them in a background worker. Content hashing
//! prevents duplicate renders for identical subjects.
//!
//! Story 4-4: Render queue — async image generation queue with hash-based
//! cache dedup.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use uuid::Uuid;

use crate::subject::{RenderSubject, SceneType, SubjectTier};

/// Maximum queue depth to prevent unbounded memory growth (CWE-400).
pub const MAX_QUEUE_DEPTH: usize = 1000;

/// Default cache TTL before stale entries are evicted.
pub const DEFAULT_CACHE_TTL: Duration = Duration::from_secs(300);

/// Status of a render job in the queue.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum RenderStatus {
    /// Job is waiting in the queue.
    Queued,
    /// Job is currently being processed by the daemon.
    InProgress,
    /// Job completed successfully.
    Complete {
        /// URL/path to the generated image.
        image_url: String,
        /// Time taken by the daemon in milliseconds.
        generation_ms: u64,
    },
    /// Job failed during rendering.
    Failed {
        /// Error description from the daemon.
        error: String,
    },
    /// Job was deduplicated against an existing identical request.
    Deduplicated {
        /// ID of the original job that this duplicates.
        original_id: Uuid,
    },
}

/// Result of an enqueue operation.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum EnqueueResult {
    /// Job was queued for rendering.
    Queued {
        /// The assigned job ID.
        job_id: Uuid,
    },
    /// Job was deduplicated — an identical render is already queued/complete.
    Deduplicated {
        /// The ID of the original matching job.
        original_id: Uuid,
    },
}

/// Errors from queue operations.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum QueueError {
    /// Queue is at capacity — backpressure applied.
    Full,
    /// Queue has been shut down.
    Closed,
}

impl std::fmt::Display for QueueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QueueError::Full => write!(f, "render queue is full"),
            QueueError::Closed => write!(f, "render queue is closed"),
        }
    }
}

impl std::error::Error for QueueError {}

/// A render result broadcast to subscribers when a job completes.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum RenderJobResult {
    /// Render succeeded.
    Success {
        /// The job ID.
        job_id: Uuid,
        /// URL/path to the generated image.
        image_url: String,
        /// Generation time in milliseconds.
        generation_ms: u64,
        /// Render tier for UI layout (e.g., "portrait", "landscape", "scene").
        tier: String,
        /// Scene type for UI context (e.g., "combat", "dialogue", "exploration").
        scene_type: String,
    },
    /// Render failed.
    Failed {
        /// The job ID.
        job_id: Uuid,
        /// Error description.
        error: String,
    },
}

/// Configuration for the render queue.
///
/// All fields are private with validated construction.
#[derive(Debug, Clone)]
pub struct RenderQueueConfig {
    queue_depth: usize,
    result_buffer: usize,
    cache_ttl: Duration,
}

impl RenderQueueConfig {
    /// Create a new config with validated values.
    ///
    /// Returns `None` if:
    /// - `queue_depth` is 0 or exceeds `MAX_QUEUE_DEPTH`
    /// - `result_buffer` is 0
    /// - `cache_ttl` is zero
    pub fn new(queue_depth: usize, result_buffer: usize, cache_ttl: Duration) -> Option<Self> {
        if queue_depth == 0 || queue_depth > MAX_QUEUE_DEPTH {
            return None;
        }
        if result_buffer == 0 {
            return None;
        }
        if cache_ttl.is_zero() {
            return None;
        }
        Some(Self {
            queue_depth,
            result_buffer,
            cache_ttl,
        })
    }

    /// Maximum number of pending jobs in the queue.
    pub fn queue_depth(&self) -> usize {
        self.queue_depth
    }

    /// Size of the broadcast result buffer.
    pub fn result_buffer(&self) -> usize {
        self.result_buffer
    }

    /// Time-to-live for cache entries before eviction.
    pub fn cache_ttl(&self) -> Duration {
        self.cache_ttl
    }
}

impl Default for RenderQueueConfig {
    fn default() -> Self {
        Self {
            queue_depth: 64,
            result_buffer: 32,
            cache_ttl: DEFAULT_CACHE_TTL,
        }
    }
}

/// Image dimensions derived from subject tier.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageDimensions {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// Map a subject tier to image dimensions.
///
/// - Portrait: tall aspect ratio (512x768)
/// - Scene: square (768x768)
/// - Landscape: wide aspect ratio (768x512)
/// - Abstract: square (512x512)
pub fn tier_to_dimensions(tier: &SubjectTier) -> ImageDimensions {
    match tier {
        SubjectTier::Portrait => ImageDimensions {
            width: 512,
            height: 768,
        },
        SubjectTier::Scene => ImageDimensions {
            width: 768,
            height: 768,
        },
        SubjectTier::Landscape => ImageDimensions {
            width: 768,
            height: 512,
        },
        SubjectTier::Abstract => ImageDimensions {
            width: 512,
            height: 512,
        },
        // Future-proof: any new tier variant defaults to square
        #[allow(unreachable_patterns)]
        _ => ImageDimensions {
            width: 512,
            height: 512,
        },
    }
}

/// Compute a content hash for dedup.
///
/// Hash is based on entities + scene_type + tier, ignoring minor prompt
/// wording differences. Two subjects with the same entities, scene type,
/// and tier produce the same hash.
///
/// Entity order is ignored (sorted before hashing) and entity names are
/// lowercased for case-insensitive dedup.
pub fn compute_content_hash(subject: &RenderSubject) -> u64 {
    let mut hasher = DefaultHasher::new();

    // Sort entities lowercase for order-independent, case-insensitive hashing
    let mut entities: Vec<String> = subject
        .entities()
        .iter()
        .map(|e| e.to_lowercase())
        .collect();
    entities.sort();
    for entity in &entities {
        entity.hash(&mut hasher);
    }

    // Hash scene type discriminant
    std::mem::discriminant(subject.scene_type()).hash(&mut hasher);

    // Hash tier discriminant
    std::mem::discriminant(subject.tier()).hash(&mut hasher);

    hasher.finish()
}

/// Internal job entry tracked by the queue.
struct JobEntry {
    status: RenderStatus,
    #[allow(dead_code)]
    content_hash: u64,
}

/// Shared state between the queue handle and background worker.
struct QueueState {
    /// Map from job_id to job entry.
    jobs: HashMap<Uuid, JobEntry>,
    /// Map from content_hash to the original job_id (for dedup).
    hash_to_job: HashMap<u64, Uuid>,
    /// Number of pending (not yet completed) jobs — for backpressure.
    pending_count: usize,
    /// Maximum pending jobs allowed.
    queue_depth: usize,
}

/// A render job sent through the channel to the worker.
struct RenderJob {
    job_id: Uuid,
    prompt: String,
    art_style: String,
    tier: String,
    scene_type: String,
    negative_prompt: String,
    /// Raw narration text — sent to daemon for LLM-based visual extraction.
    /// When present, the daemon runs SubjectExtractor instead of using the raw prompt.
    narration: String,
    /// Target image width in pixels (from tier_to_dimensions).
    width: u32,
    /// Target image height in pixels (from tier_to_dimensions).
    height: u32,
    /// Genre pack's preferred Flux variant (`"dev"` or `"schnell"`). Empty
    /// string means "use the daemon's tier default." Sourced from
    /// `visual_style.yaml::preferred_model`. Previously read in Rust and
    /// silently dropped at the enqueue boundary (parameter `_image_model`
    /// was prefixed with `_` → unused). Story 35-15 closes that wire.
    variant: String,
    /// Optional absolute path to a LoRA `.safetensors` file. Forwarded to
    /// the daemon as `RenderParams.lora_path`. Story 35-15.
    lora_path: Option<String>,
    /// Optional LoRA scale (0.0–1.0). `None` lets the daemon default to
    /// 1.0 — no silent fallback on the Rust side. Story 35-15.
    lora_scale: Option<f32>,
}

/// The async render queue.
///
/// Spawns a background tokio task that processes render jobs sequentially.
/// Uses hash-based dedup to avoid duplicate daemon calls.
pub struct RenderQueue {
    state: Arc<Mutex<QueueState>>,
    /// Channel sender for submitting jobs to the worker.
    job_tx: tokio::sync::mpsc::Sender<RenderJob>,
    /// Broadcast sender for notifying subscribers of job results.
    result_tx: tokio::sync::broadcast::Sender<RenderJobResult>,
    /// Handle to the background worker task.
    worker_handle: Option<tokio::task::JoinHandle<()>>,
}

impl RenderQueue {
    /// Spawn the render queue with a background worker.
    ///
    /// `render_fn` is called for each job — pass a closure that calls
    /// the daemon client. Returns `(image_url, generation_ms)` on success.
    pub fn spawn<F, Fut>(config: RenderQueueConfig, render_fn: F) -> Self
    where
        F: Fn(
                String,
                String,
                String,
                String,
                String,
                u32,
                u32,
                String,
                Option<String>,
                Option<f32>,
            ) -> Fut
            + Send
            + 'static,
        Fut: std::future::Future<Output = Result<(String, u64), String>> + Send,
    {
        let state = Arc::new(Mutex::new(QueueState {
            jobs: HashMap::new(),
            hash_to_job: HashMap::new(),
            pending_count: 0,
            queue_depth: config.queue_depth(),
        }));

        let (job_tx, mut job_rx) = tokio::sync::mpsc::channel::<RenderJob>(config.queue_depth());
        let (result_tx, _) =
            tokio::sync::broadcast::channel::<RenderJobResult>(config.result_buffer());

        let worker_state = Arc::clone(&state);
        let worker_result_tx = result_tx.clone();
        let worker_handle = tokio::spawn(async move {
            while let Some(job) = job_rx.recv().await {
                // Mark in-progress
                {
                    let mut guard = worker_state.lock().await;
                    if let Some(entry) = guard.jobs.get_mut(&job.job_id) {
                        entry.status = RenderStatus::InProgress;
                    }
                }

                // Call the render function
                let result = render_fn(
                    job.prompt,
                    job.art_style,
                    job.tier.clone(),
                    job.negative_prompt,
                    job.narration,
                    job.width,
                    job.height,
                    job.variant,
                    job.lora_path,
                    job.lora_scale,
                )
                .await;

                // Update state and broadcast
                let broadcast_msg = match result {
                    Ok((image_url, generation_ms)) => {
                        let mut guard = worker_state.lock().await;
                        if let Some(entry) = guard.jobs.get_mut(&job.job_id) {
                            entry.status = RenderStatus::Complete {
                                image_url: image_url.clone(),
                                generation_ms,
                            };
                        }
                        guard.pending_count = guard.pending_count.saturating_sub(1);
                        RenderJobResult::Success {
                            job_id: job.job_id,
                            image_url,
                            generation_ms,
                            tier: job.tier,
                            scene_type: job.scene_type,
                        }
                    }
                    Err(error) => {
                        let mut guard = worker_state.lock().await;
                        if let Some(entry) = guard.jobs.get_mut(&job.job_id) {
                            entry.status = RenderStatus::Failed {
                                error: error.clone(),
                            };
                        }
                        guard.pending_count = guard.pending_count.saturating_sub(1);
                        RenderJobResult::Failed {
                            job_id: job.job_id,
                            error,
                        }
                    }
                };

                let _ = worker_result_tx.send(broadcast_msg);
            }

            // Channel closed — mark remaining jobs as failed
            let mut guard = worker_state.lock().await;
            for entry in guard.jobs.values_mut() {
                if matches!(
                    entry.status,
                    RenderStatus::Queued | RenderStatus::InProgress
                ) {
                    entry.status = RenderStatus::Failed {
                        error: "queue shutdown".to_string(),
                    };
                }
            }
        });

        Self {
            state,
            job_tx,
            result_tx,
            worker_handle: Some(worker_handle),
        }
    }

    /// Subscribe to render job results.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<RenderJobResult> {
        self.result_tx.subscribe()
    }

    /// Enqueue a render subject for image generation.
    ///
    /// Returns immediately with `EnqueueResult::Queued` or
    /// `EnqueueResult::Deduplicated`. Returns `Err(QueueError::Full)`
    /// if the queue is at capacity.
    pub async fn enqueue(
        &self,
        subject: RenderSubject,
        art_style: &str,
        variant: &str,
        negative_prompt: &str,
        narration: &str,
        lora_path: Option<&str>,
        lora_scale: Option<f32>,
    ) -> Result<EnqueueResult, QueueError> {
        let content_hash = compute_content_hash(&subject);
        let mut guard = self.state.lock().await;

        // Dedup check
        if let Some(&original_id) = guard.hash_to_job.get(&content_hash) {
            return Ok(EnqueueResult::Deduplicated { original_id });
        }

        // Backpressure check
        if guard.pending_count >= guard.queue_depth {
            return Err(QueueError::Full);
        }

        let job_id = Uuid::new_v4();
        let prompt = subject.prompt_fragment().to_string();
        let tier = match subject.tier() {
            SubjectTier::Portrait => "portrait",
            SubjectTier::Scene => "scene_illustration",
            SubjectTier::Landscape => "landscape",
            SubjectTier::Abstract => "scene_illustration",
        }
        .to_string();
        guard.jobs.insert(
            job_id,
            JobEntry {
                status: RenderStatus::Queued,
                content_hash,
            },
        );
        guard.hash_to_job.insert(content_hash, job_id);
        guard.pending_count += 1;
        drop(guard);

        // Send to worker — if channel is full/closed, mark failed
        let scene_type = match subject.scene_type() {
            SceneType::Combat => "combat",
            SceneType::Dialogue => "dialogue",
            SceneType::Exploration => "exploration",
            SceneType::Discovery => "discovery",
            SceneType::Transition => "transition",
        }
        .to_string();
        let dims = tier_to_dimensions(subject.tier());
        tracing::info!(
            tier = %tier,
            width = dims.width,
            height = dims.height,
            "render.dimensions_set"
        );
        let job = RenderJob {
            job_id,
            prompt,
            art_style: art_style.to_string(),
            tier,
            scene_type,
            negative_prompt: negative_prompt.to_string(),
            narration: narration.to_string(),
            width: dims.width,
            height: dims.height,
            variant: variant.to_string(),
            lora_path: lora_path.map(str::to_string),
            lora_scale,
        };
        if self.job_tx.send(job).await.is_err() {
            let mut guard = self.state.lock().await;
            if let Some(entry) = guard.jobs.get_mut(&job_id) {
                entry.status = RenderStatus::Failed {
                    error: "worker channel closed".to_string(),
                };
            }
            return Err(QueueError::Closed);
        }

        Ok(EnqueueResult::Queued { job_id })
    }

    /// Get the current status of a job by ID.
    pub async fn job_status(&self, job_id: Uuid) -> Option<RenderStatus> {
        let guard = self.state.lock().await;
        guard.jobs.get(&job_id).map(|entry| entry.status.clone())
    }

    /// Number of jobs currently in the cache (including completed).
    pub async fn cache_len(&self) -> usize {
        let guard = self.state.lock().await;
        guard.hash_to_job.len()
    }

    /// Shut down the queue gracefully, stopping the background worker.
    pub async fn shutdown(mut self) {
        // Drop the job sender to close the channel, signaling the worker to stop.
        drop(self.job_tx);
        if let Some(handle) = self.worker_handle.take() {
            let _ = handle.await;
        }
    }
}

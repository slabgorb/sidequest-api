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
use std::time::{Duration, Instant};

use sidequest_telemetry::{WatcherEventBuilder, WatcherEventType};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::subject::{RenderSubject, SceneType, SubjectTier};

/// Maximum queue depth to prevent unbounded memory growth (CWE-400).
pub const MAX_QUEUE_DEPTH: usize = 1000;

/// Default cache TTL before stale entries are evicted.
pub const DEFAULT_CACHE_TTL: Duration = Duration::from_secs(300);

/// Default per-job TTL — how long a Queued/InProgress job may sit in the
/// dedup table before a fresh enqueue is allowed to evict it.
///
/// Sized so a healthy Flux render (typically 30–90s) is comfortably under
/// the threshold, but a wedged daemon (the 2026-04-10 playtest scenario,
/// where MPS deadlocked and a job latched the dedup table indefinitely)
/// gets evicted within ~2 minutes and the player sees images resume.
pub const DEFAULT_JOB_TTL: Duration = Duration::from_secs(120);

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
    job_ttl: Duration,
}

impl RenderQueueConfig {
    /// Create a new config with validated values.
    ///
    /// Returns `None` if:
    /// - `queue_depth` is 0 or exceeds `MAX_QUEUE_DEPTH`
    /// - `result_buffer` is 0
    /// - `cache_ttl` is zero
    /// - `job_ttl` is zero
    ///
    /// Uses [`DEFAULT_JOB_TTL`] for the per-job staleness threshold.
    pub fn new(queue_depth: usize, result_buffer: usize, cache_ttl: Duration) -> Option<Self> {
        Self::new_with_job_ttl(queue_depth, result_buffer, cache_ttl, DEFAULT_JOB_TTL)
    }

    /// Like [`Self::new`] but lets the caller override the per-job TTL.
    /// Used by the eviction tests to assert behavior on tight deadlines.
    pub fn new_with_job_ttl(
        queue_depth: usize,
        result_buffer: usize,
        cache_ttl: Duration,
        job_ttl: Duration,
    ) -> Option<Self> {
        if queue_depth == 0 || queue_depth > MAX_QUEUE_DEPTH {
            return None;
        }
        if result_buffer == 0 {
            return None;
        }
        if cache_ttl.is_zero() {
            return None;
        }
        if job_ttl.is_zero() {
            return None;
        }
        Some(Self {
            queue_depth,
            result_buffer,
            cache_ttl,
            job_ttl,
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

    /// Time after which a Queued/InProgress job is considered stale and
    /// can be evicted from the dedup table by a fresh enqueue.
    ///
    /// Distinct from [`Self::cache_ttl`], which applies to completed
    /// entries. This one is the "is the daemon hung?" deadline.
    pub fn job_ttl(&self) -> Duration {
        self.job_ttl
    }
}

impl Default for RenderQueueConfig {
    fn default() -> Self {
        Self {
            queue_depth: 64,
            result_buffer: 32,
            cache_ttl: DEFAULT_CACHE_TTL,
            job_ttl: DEFAULT_JOB_TTL,
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
    content_hash: u64,
    /// Wall-clock instant the job was first enqueued. Compared against
    /// [`RenderQueueConfig::job_ttl`] in the dedup path to evict stale
    /// in-flight jobs (the 2026-04-10 hung-daemon scenario).
    enqueued_at: Instant,
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
    /// Per-job staleness threshold copied from [`RenderQueueConfig::job_ttl`].
    job_ttl: Duration,
}

/// Parameters passed to the render callback closure registered with
/// [`RenderQueue::spawn`].
///
/// Introduced to replace a 10-positional-argument closure signature that
/// caused wide test-fixture churn every time a new field was added (see
/// story 35-15 commit body). Callers destructure in the closure parameter
/// position:
///
/// ```ignore
/// RenderQueue::spawn(config, |params: RenderJobParams| async move {
///     let RenderJobParams { prompt, art_style, tier, width, height, .. } = params;
///     // ...
/// });
/// ```
///
/// Marked `#[non_exhaustive]` so future fields can be added without
/// requiring `{ field1, field2, .. }` destructurings to be rewritten.
/// External crates cannot construct this via struct literal, which is
/// intentional — only the queue worker ever builds it.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RenderJobParams {
    /// The rendered prompt fragment from the subject.
    pub prompt: String,
    /// Composed art style string (positive_suffix + tag overrides).
    pub art_style: String,
    /// Tier label: `"portrait"`, `"scene_illustration"`, or `"landscape"`.
    pub tier: String,
    /// Negative prompt for exclusion.
    pub negative_prompt: String,
    /// Raw narration text for daemon-side visual extraction.
    pub narration: String,
    /// Target image width in pixels (from `tier_to_dimensions`).
    pub width: u32,
    /// Target image height in pixels (from `tier_to_dimensions`).
    pub height: u32,
    /// Genre pack's preferred Flux variant (`"dev"` or `"schnell"`, or
    /// empty to use the daemon's tier default).
    pub variant: String,
    /// Optional absolute path to a LoRA `.safetensors` file.
    pub lora_path: Option<String>,
    /// Optional LoRA scale (0.0–2.0). `None` lets the daemon default to 1.0.
    pub lora_scale: Option<f32>,
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
    /// `render_fn` is called for each job with a [`RenderJobParams`] struct.
    /// Pass a closure that calls the daemon client and returns
    /// `(image_url, generation_ms)` on success.
    ///
    /// The closure signature used to take 10 positional `String`/`u32`/
    /// `Option` arguments, which made adding a new field ripple through
    /// every test fixture. Packing them into one struct compresses that
    /// blast radius to a single destructuring site.
    pub fn spawn<F, Fut>(config: RenderQueueConfig, render_fn: F) -> Self
    where
        F: Fn(RenderJobParams) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<(String, u64), String>> + Send,
    {
        let state = Arc::new(Mutex::new(QueueState {
            jobs: HashMap::new(),
            hash_to_job: HashMap::new(),
            pending_count: 0,
            queue_depth: config.queue_depth(),
            job_ttl: config.job_ttl(),
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

                // Call the render function with packed params.
                // `job.tier` is cloned because it's also moved into the
                // success result below; everything else moves.
                let params = RenderJobParams {
                    prompt: job.prompt,
                    art_style: job.art_style,
                    tier: job.tier.clone(),
                    negative_prompt: job.negative_prompt,
                    narration: job.narration,
                    width: job.width,
                    height: job.height,
                    variant: job.variant,
                    lora_path: job.lora_path,
                    lora_scale: job.lora_scale,
                };
                let result = render_fn(params).await;

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
                        // Capture the content_hash BEFORE marking failed so we
                        // can scrub the dedup table — without this, the
                        // failed entry latches and every subsequent identical
                        // enqueue returns Deduplicated forever (the second
                        // half of the 2026-04-10 cascade bug).
                        let content_hash =
                            guard.jobs.get(&job.job_id).map(|entry| entry.content_hash);
                        if let Some(entry) = guard.jobs.get_mut(&job.job_id) {
                            entry.status = RenderStatus::Failed {
                                error: error.clone(),
                            };
                        }
                        if let Some(hash) = content_hash {
                            // Only evict if THIS job_id still owns the hash
                            // entry — a concurrent re-enqueue may have
                            // already replaced it via TTL eviction.
                            if guard.hash_to_job.get(&hash).copied() == Some(job.job_id) {
                                guard.hash_to_job.remove(&hash);
                            }
                        }
                        guard.pending_count = guard.pending_count.saturating_sub(1);
                        drop(guard);
                        WatcherEventBuilder::new("render", WatcherEventType::ValidationWarning)
                            .field("action", "dedup_evicted")
                            .field("reason", "daemon_error")
                            .field("job_id", job.job_id.to_string().as_str())
                            .field("error", error.as_str())
                            .send();
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
    ///
    /// Note: 8 args exceeds clippy's default of 7. Each argument is a
    /// distinct part of the render request (prompt context, style,
    /// negative prompt, narration for caption, LoRA selection). Folding
    /// them into a struct would require updating 8 call sites across
    /// 3 crates and represents an API redesign, not a lint fix.
    #[allow(clippy::too_many_arguments)]
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

        // Dedup check with TTL-based and failure-based eviction.
        //
        // The naive "if hash already mapped, return Deduplicated" was the
        // 2026-04-10 cascade trigger: when the daemon wedged on an
        // in-flight job, the dedup table latched on that job_id forever
        // and every subsequent identical enqueue returned Deduplicated
        // — image generation silently stopped for the rest of the session.
        //
        // Two evictable conditions:
        //   1. The existing job is Failed (manual retry-after-failure).
        //   2. The existing job is Queued/InProgress AND its enqueued_at
        //      is older than `job_ttl` — the daemon has had its window
        //      and we're freeing the slot for a fresh attempt.
        //
        // Both eviction paths emit `render.dedup_evicted` so the GM
        // panel sees the recovery; the TTL path also emits a separate
        // `render.job_stuck` ValidationWarning so the operator knows
        // a daemon round-trip exceeded the deadline (the lie detector
        // signal — Claude's narration would otherwise hide a wedged
        // image pipeline).
        if let Some(&original_id) = guard.hash_to_job.get(&content_hash) {
            let job_ttl = guard.job_ttl;
            let existing_status = guard.jobs.get(&original_id).map(|e| e.status.clone());
            let existing_age = guard
                .jobs
                .get(&original_id)
                .map(|e| e.enqueued_at.elapsed());
            let evict = match (&existing_status, existing_age) {
                (Some(RenderStatus::Failed { .. }), _) => Some("prior_failure"),
                (Some(RenderStatus::Queued | RenderStatus::InProgress), Some(age))
                    if age >= job_ttl =>
                {
                    Some("ttl_expired")
                }
                _ => None,
            };
            match evict {
                None => return Ok(EnqueueResult::Deduplicated { original_id }),
                Some(reason) => {
                    let age_secs = existing_age.map(|a| a.as_secs()).unwrap_or(0);
                    guard.hash_to_job.remove(&content_hash);
                    // The stale job's JobEntry is left in `jobs` so its
                    // job_id remains queryable via job_status — but it
                    // no longer holds the dedup slot. Drop the lock
                    // before emitting OTEL so the watcher channel never
                    // blocks the queue mutex.
                    drop(guard);
                    WatcherEventBuilder::new("render", WatcherEventType::ValidationWarning)
                        .field("action", "dedup_evicted")
                        .field("reason", reason)
                        .field("evicted_job_id", original_id.to_string().as_str())
                        .field("age_seconds", age_secs.to_string().as_str())
                        .send();
                    if reason == "ttl_expired" {
                        tracing::warn!(
                            evicted_job_id = %original_id,
                            age_seconds = age_secs,
                            job_ttl_seconds = job_ttl.as_secs(),
                            "render.job_stuck — dispatch never completed within TTL; evicting from dedup table"
                        );
                        WatcherEventBuilder::new("render", WatcherEventType::ValidationWarning)
                            .field("action", "job_stuck")
                            .field("evicted_job_id", original_id.to_string().as_str())
                            .field("age_seconds", age_secs.to_string().as_str())
                            .field("job_ttl_seconds", job_ttl.as_secs().to_string().as_str())
                            .send();
                    }
                    // Re-acquire the lock so the rest of enqueue runs
                    // against fresh state.
                    guard = self.state.lock().await;
                }
            }
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
                enqueued_at: Instant::now(),
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

    /// Mark a job as explicitly failed and evict it from the dedup table.
    ///
    /// Intended for callers that observe a daemon-side error before the
    /// background worker reaches it (e.g., a connection refused at the
    /// dispatch layer). The 2026-04-10 cascade caught this on the worker
    /// side already by removing failed jobs from `hash_to_job` in the
    /// worker's failure branch — this public API is the eager-eviction
    /// surface for callers that want to short-circuit before the worker
    /// gets there.
    ///
    /// Returns `true` if the job existed and was newly marked failed.
    /// Returns `false` if the job was unknown or already in a terminal
    /// state (idempotent).
    pub async fn mark_failed(&self, job_id: Uuid, error: impl Into<String>) -> bool {
        let error_str = error.into();
        let mut guard = self.state.lock().await;
        let Some(entry) = guard.jobs.get_mut(&job_id) else {
            return false;
        };
        // Idempotent — already terminal.
        if matches!(
            entry.status,
            RenderStatus::Failed { .. }
                | RenderStatus::Complete { .. }
                | RenderStatus::Deduplicated { .. }
        ) {
            return false;
        }
        let content_hash = entry.content_hash;
        entry.status = RenderStatus::Failed {
            error: error_str.clone(),
        };
        // Only evict the dedup slot if THIS job_id still owns it.
        if guard.hash_to_job.get(&content_hash).copied() == Some(job_id) {
            guard.hash_to_job.remove(&content_hash);
        }
        guard.pending_count = guard.pending_count.saturating_sub(1);
        drop(guard);
        WatcherEventBuilder::new("render", WatcherEventType::ValidationWarning)
            .field("action", "dedup_evicted")
            .field("reason", "mark_failed")
            .field("job_id", job_id.to_string().as_str())
            .field("error", error_str.as_str())
            .send();
        true
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

// ────────────────────────────────────────────────────────────────────────────
// Eviction tests — guards against the 2026-04-10 cascade regression where a
// hung daemon latched the dedup table and silently halted image generation.
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod eviction_tests {
    use super::*;
    use crate::subject::{RenderSubject, SceneType, SubjectTier};

    fn make_subject(name: &str) -> RenderSubject {
        RenderSubject::new(
            vec![name.to_string()],
            SceneType::Exploration,
            SubjectTier::Scene,
            format!("a sketch of {}", name),
            0.6,
        )
        .expect("valid subject")
    }

    /// Build a queue whose render closure parks forever, simulating a
    /// wedged daemon. Jobs sit in InProgress until the test exits.
    fn make_hung_queue(job_ttl: Duration) -> RenderQueue {
        let config = RenderQueueConfig::new_with_job_ttl(16, 16, Duration::from_secs(60), job_ttl)
            .expect("valid config");
        RenderQueue::spawn(config, |_params: RenderJobParams| async move {
            // Pretend the daemon never returns. The job stays InProgress
            // forever from the queue's POV — exactly the playtest scenario.
            std::future::pending::<()>().await;
            Ok(("never".to_string(), 0))
        })
    }

    #[tokio::test]
    async fn dedup_evicts_stale_in_flight_job() {
        // Sub-second TTL so the test runs fast.
        let queue = make_hung_queue(Duration::from_millis(50));
        let subject = make_subject("ruin");

        // First enqueue: succeeds, job latches the dedup slot.
        let r1 = queue
            .enqueue(subject.clone(), "", "", "", "", None, None)
            .await
            .expect("first enqueue ok");
        let original_id = match r1 {
            EnqueueResult::Queued { job_id } => job_id,
            other => panic!("first enqueue should be Queued, got {:?}", other),
        };

        // Wait until the TTL has elapsed.
        tokio::time::sleep(Duration::from_millis(120)).await;

        // Second enqueue: should NOT be deduplicated against the stale job.
        let r2 = queue
            .enqueue(subject.clone(), "", "", "", "", None, None)
            .await
            .expect("second enqueue ok");
        match r2 {
            EnqueueResult::Queued { job_id } => {
                assert_ne!(
                    job_id, original_id,
                    "TTL eviction must allocate a fresh job_id, not reuse the stale one"
                );
            }
            EnqueueResult::Deduplicated { .. } => {
                panic!("TTL eviction failed: stale in-flight job still latching dedup table");
            }
        }
    }

    #[tokio::test]
    async fn dedup_evicts_on_explicit_failure() {
        let queue = make_hung_queue(Duration::from_secs(60));
        let subject = make_subject("statue");

        let r1 = queue
            .enqueue(subject.clone(), "", "", "", "", None, None)
            .await
            .expect("first enqueue ok");
        let job_id = match r1 {
            EnqueueResult::Queued { job_id } => job_id,
            other => panic!("first enqueue should be Queued, got {:?}", other),
        };

        // Eagerly fail the job — simulates a dispatch-side observation
        // that the daemon is unreachable.
        let evicted = queue.mark_failed(job_id, "daemon down").await;
        assert!(evicted, "mark_failed should report success on first call");

        // Idempotent: second mark_failed returns false.
        assert!(
            !queue.mark_failed(job_id, "daemon down").await,
            "mark_failed must be idempotent — second call should return false"
        );

        // Re-enqueue: should NOT dedupe against the failed job.
        let r2 = queue
            .enqueue(subject.clone(), "", "", "", "", None, None)
            .await
            .expect("second enqueue ok");
        assert!(
            matches!(r2, EnqueueResult::Queued { .. }),
            "after mark_failed, identical re-enqueue must not be deduplicated, got {:?}",
            r2
        );
    }

    #[tokio::test]
    async fn dedup_still_works_for_fast_sequential_enqueues() {
        // Long TTL so neither enqueue can possibly age out.
        let queue = make_hung_queue(Duration::from_secs(60));
        let subject = make_subject("temple");

        let r1 = queue
            .enqueue(subject.clone(), "", "", "", "", None, None)
            .await
            .expect("first enqueue ok");
        let original_id = match r1 {
            EnqueueResult::Queued { job_id } => job_id,
            other => panic!("first enqueue should be Queued, got {:?}", other),
        };

        // Immediate re-enqueue while the original is still in-flight and
        // well within the TTL → must dedupe.
        let r2 = queue
            .enqueue(subject.clone(), "", "", "", "", None, None)
            .await
            .expect("second enqueue ok");
        match r2 {
            EnqueueResult::Deduplicated {
                original_id: dup_id,
            } => {
                assert_eq!(dup_id, original_id);
            }
            other => panic!("expected Deduplicated, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn mark_failed_returns_false_for_unknown_job() {
        let queue = make_hung_queue(Duration::from_secs(60));
        let unknown = Uuid::new_v4();
        assert!(
            !queue.mark_failed(unknown, "anything").await,
            "mark_failed on unknown job_id must return false"
        );
    }
}

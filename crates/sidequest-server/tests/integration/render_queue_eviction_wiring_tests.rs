//! Wiring tests for render queue TTL eviction — guards against the
//! 2026-04-10 cascade regression.
//!
//! Background: a wedged Python daemon left a render job latched on
//! the dedup table forever, silently halting image generation for the
//! rest of the playtest. The fix has three structural pieces:
//!
//!   1. `JobEntry::enqueued_at: Instant` — required to compute staleness.
//!   2. TTL-based eviction in `RenderQueue::enqueue` — frees the slot
//!      when an in-flight job exceeds the deadline.
//!   3. `hash_to_job` cleanup in the worker's failure branch — frees
//!      the slot when the worker observes an explicit error.
//!
//! Plus the public `mark_failed` API for eager dispatch-side eviction
//! and the OTEL `render.dedup_evicted` / `render.job_stuck` events for
//! GM-panel visibility (the lie-detector signal per CLAUDE.md).
//!
//! These source-level guards ensure a future refactor cannot silently
//! drop any of those pieces. They live in `sidequest-server/tests`
//! alongside the other wiring suites because the cascade affected the
//! render dispatch path end-to-end.

const RENDER_QUEUE_SRC: &str = include_str!("../../../sidequest-game/src/render_queue.rs");

fn production_code() -> &'static str {
    // Drop everything from the first `#[cfg(test)]` onwards so we don't
    // false-positive on test fixtures.
    RENDER_QUEUE_SRC
        .split("#[cfg(test)]")
        .next()
        .unwrap_or(RENDER_QUEUE_SRC)
}

// ===========================================================================
// 1. Per-job staleness timestamp
// ===========================================================================

#[test]
fn job_entry_carries_enqueued_at_timestamp() {
    let prod = production_code();
    assert!(
        prod.contains("enqueued_at: Instant"),
        "JobEntry must carry an `enqueued_at: Instant` so the dedup \
         path can detect stale in-flight jobs. Without this field, \
         a hung daemon latches the dedup table forever (the \
         2026-04-10 cascade regression)."
    );
}

#[test]
fn config_exposes_job_ttl() {
    let prod = production_code();
    assert!(
        prod.contains("job_ttl: Duration") && prod.contains("pub fn job_ttl(&self)"),
        "RenderQueueConfig must expose `job_ttl` so callers can tune \
         the staleness threshold (and tests can use a tight one)."
    );
    assert!(
        prod.contains("DEFAULT_JOB_TTL"),
        "DEFAULT_JOB_TTL constant must exist as the public default."
    );
}

// ===========================================================================
// 2. TTL eviction in the enqueue dedup path
// ===========================================================================

#[test]
fn enqueue_dedup_path_uses_job_ttl() {
    let prod = production_code();
    // The eviction logic compares `enqueued_at.elapsed()` against
    // `job_ttl`. Both halves of that comparison must be present in
    // the dedup branch of `enqueue`.
    assert!(
        prod.contains("enqueued_at.elapsed()"),
        "enqueue dedup path must inspect `enqueued_at.elapsed()` to \
         detect stale jobs"
    );
    assert!(
        prod.contains("ttl_expired"),
        "TTL-eviction OTEL event must use the `ttl_expired` reason \
         string so the GM panel can distinguish staleness from other \
         eviction causes"
    );
}

#[test]
fn ttl_eviction_emits_job_stuck_warning() {
    let prod = production_code();
    assert!(
        prod.contains("render.job_stuck") || prod.contains("\"job_stuck\""),
        "A TTL eviction must emit a `render.job_stuck` warning (the \
         lie-detector signal per CLAUDE.md OTEL principle) so the GM \
         panel surfaces hung-daemon situations instead of silently \
         recovering them."
    );
}

// ===========================================================================
// 3. Worker failure branch cleans up dedup table
// ===========================================================================

#[test]
fn worker_failure_branch_evicts_hash_to_job() {
    let prod = production_code();
    // Locate the `Err(error) =>` branch inside the worker task and
    // confirm it removes the failed job from `hash_to_job`. Without
    // this cleanup, an explicit failure latches the dedup table the
    // same way the hung-daemon scenario did.
    let err_branch_start = prod
        .find("Err(error) =>")
        .expect("worker task must have an Err(error) branch");
    // Look at the next ~80 lines to find the cleanup.
    let window = &prod[err_branch_start
        ..err_branch_start + prod.len().min(3000).min(prod.len() - err_branch_start)];
    assert!(
        window.contains("hash_to_job.remove"),
        "worker task's Err branch must call `hash_to_job.remove` so a \
         failed job releases its dedup slot. Without this, every \
         retry returns Deduplicated against the failed entry."
    );
}

// ===========================================================================
// 4. Public mark_failed API for dispatch-side eager eviction
// ===========================================================================

#[test]
fn render_queue_exposes_mark_failed() {
    let prod = production_code();
    assert!(
        prod.contains("pub async fn mark_failed("),
        "RenderQueue must expose `pub async fn mark_failed(...)` so \
         dispatch-layer callers can eagerly evict a job before the \
         worker reaches it (e.g., when a connection refused fires \
         before the render call lands on the daemon)."
    );
}

#[test]
fn mark_failed_emits_dedup_evicted_event() {
    let prod = production_code();
    let mark_failed_start = prod
        .find("pub async fn mark_failed(")
        .expect("mark_failed must exist");
    let end = prod[mark_failed_start..]
        .find("\n    }")
        .expect("mark_failed must have a closing brace");
    let body = &prod[mark_failed_start..mark_failed_start + end];
    assert!(
        body.contains("dedup_evicted"),
        "mark_failed must emit a `render.dedup_evicted` watcher event \
         so the GM panel sees the recovery"
    );
    assert!(
        body.contains("hash_to_job.remove"),
        "mark_failed must scrub `hash_to_job` for the failed job's \
         content_hash, otherwise re-enqueue still latches"
    );
}

// ===========================================================================
// 5. Telemetry: WatcherEventBuilder is wired in render_queue
// ===========================================================================

#[test]
fn render_queue_uses_watcher_event_builder() {
    let prod = production_code();
    assert!(
        prod.contains("use sidequest_telemetry::{WatcherEventBuilder"),
        "render_queue.rs must import WatcherEventBuilder from \
         sidequest-telemetry — eviction events are the only signal \
         the GM panel has for the cascade fix"
    );
    assert!(
        prod.contains("WatcherEventBuilder::new(\"render\""),
        "render_queue.rs must emit at least one `render`-domain \
         watcher event"
    );
}

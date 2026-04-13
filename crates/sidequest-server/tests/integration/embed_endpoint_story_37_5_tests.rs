//! Story 37-5: Embed endpoint failure tests.
//!
//! Playtest 2 (2026-04-12): /embed returned "Unknown error" for every
//! request, degrading RAG semantic search for the entire session. The
//! narrator compensated by improvising without lore grounding.
//!
//! These tests guard the embed worker's error handling and OTEL
//! observability — the systems that turn a daemon outage into a
//! visible, recoverable event instead of a silent quality degradation.
//!
//! Existing coverage:
//!   - `embed_story_15_7_tests.rs` (daemon-client): type existence + serialization
//!   - `lore_embedding_pending_wiring_tests.rs`: lore_sync pending/retry guards
//!
//! Gap these tests close:
//!   - Embed worker circuit breaker wiring (the 30s-open, 3-failure threshold)
//!   - Embed worker OTEL events on failure (distinct from lore_sync events)
//!   - Prompt.rs query embed fallback behaviour when daemon is down

const EMBED_WORKER_SRC: &str = include_str!("../../src/dispatch/lore_embed_worker.rs");

const PROMPT_SRC: &str = include_str!("../../src/dispatch/prompt.rs");

fn prod(src: &str) -> &str {
    src.split("#[cfg(test)]").next().unwrap_or(src)
}

// ===========================================================================
// 1. Embed worker circuit breaker — prevents hammering a sick daemon
// ===========================================================================

#[test]
fn embed_worker_has_failure_threshold_constant() {
    let s = prod(EMBED_WORKER_SRC);
    assert!(
        s.contains("FAILURE_THRESHOLD"),
        "Embed worker must define FAILURE_THRESHOLD for circuit breaker — \
         without it, a sick daemon gets hammered on every lore fragment"
    );
}

#[test]
fn embed_worker_has_circuit_open_duration() {
    let s = prod(EMBED_WORKER_SRC);
    assert!(
        s.contains("CIRCUIT_OPEN_DURATION"),
        "Embed worker must define CIRCUIT_OPEN_DURATION — the recovery \
         window where requests are skipped to give the daemon time to heal"
    );
}

#[test]
fn embed_worker_tracks_consecutive_failures() {
    let s = prod(EMBED_WORKER_SRC);
    assert!(
        s.contains("consecutive_failures"),
        "Embed worker must track consecutive failures — the circuit opens \
         after FAILURE_THRESHOLD consecutive failures"
    );
}

#[test]
fn embed_worker_circuit_breaker_marks_pending_on_skip() {
    let s = prod(EMBED_WORKER_SRC);
    // When the circuit is open, requests must be marked pending (not dropped)
    // so the next half-open probe can retry them.
    let circuit_block = {
        let start = s
            .find("if let Some(until) = state.circuit_open_until")
            .expect("circuit_open_until check must exist");
        &s[start..start + 500]
    };
    assert!(
        circuit_block.contains("mark_embedding_pending"),
        "When circuit is open, skipped requests must be marked pending — \
         dropping them loses lore fragments permanently"
    );
}

#[test]
fn embed_worker_resets_failures_on_success() {
    let s = prod(EMBED_WORKER_SRC);
    assert!(
        s.contains("consecutive_failures = 0"),
        "Embed worker must reset consecutive_failures to 0 on success — \
         otherwise the circuit stays at the threshold edge and flaps"
    );
}

// ===========================================================================
// 2. Embed worker OTEL events — the lie detector for embed health
// ===========================================================================

#[test]
fn embed_worker_emits_otel_on_connect_failure() {
    let s = prod(EMBED_WORKER_SRC);
    assert!(
        s.contains("lore.embed_worker.connect_failed"),
        "Embed worker must emit lore.embed_worker.connect_failed when \
         DaemonClient::connect fails — otherwise a stopped daemon is \
         invisible on the GM panel"
    );
}

#[test]
fn embed_worker_emits_otel_on_embed_failure() {
    let s = prod(EMBED_WORKER_SRC);
    assert!(
        s.contains("lore.embed_worker.embed_failed"),
        "Embed worker must emit lore.embed_worker.embed_failed when the \
         embed() RPC fails — the GM panel needs this to distinguish 'daemon \
         up but embed broken' from 'daemon down'"
    );
}

#[test]
fn embed_worker_emits_otel_on_success() {
    let s = prod(EMBED_WORKER_SRC);
    assert!(
        s.contains("\"lore.embedding_generated\""),
        "Embed worker must emit lore.embedding_generated on success — \
         the GM panel uses this as proof that embeddings are flowing"
    );
}

#[test]
fn embed_worker_emits_circuit_open_event() {
    let s = prod(EMBED_WORKER_SRC);
    assert!(
        s.contains("\"lore.embedding_circuit_open\""),
        "Embed worker must emit lore.embedding_circuit_open when the \
         circuit breaker trips — the GM panel needs a clear 'daemon \
         outage' signal, not a stream of individual failures"
    );
}

#[test]
fn embed_worker_emits_initial_sweep_events() {
    let s = prod(EMBED_WORKER_SRC);
    assert!(
        s.contains("lore.embed_worker.initial_sweep_start"),
        "Embed worker must emit lore.embed_worker.initial_sweep_start — \
         the startup sweep retries fragments from prior sessions"
    );
    assert!(
        s.contains("lore.embed_worker.initial_sweep_complete"),
        "Embed worker must emit lore.embed_worker.initial_sweep_complete — \
         the GM panel needs to see sweep results"
    );
}

// ===========================================================================
// 3. Embed worker processes embed outside the lore store lock
// ===========================================================================

#[test]
fn embed_worker_calls_embed_outside_lock() {
    let s = prod(EMBED_WORKER_SRC);
    // The process_request function must:
    // 1. Connect to daemon (no lock)
    // 2. Call client.embed(params) (no lock)
    // 3. Only lock for set_embedding (the sub-ms write)
    //
    // If embed() is called inside a lore_store.lock(), dispatch reads
    // are blocked for 10s+ during every embed call.
    let process_fn_start = s
        .find("async fn process_request")
        .expect("process_request must exist");
    let process_fn = &s[process_fn_start..];

    // The embed call must come before the lock acquisition for set_embedding
    let embed_pos = process_fn
        .find("client.embed(params)")
        .expect("must call client.embed(params)");
    // There should be a lock earlier for circuit breaker, so find the one
    // after the embed call
    let set_embed_lock_after_embed = process_fn[embed_pos..]
        .find("lore_store.lock()")
        .map(|p| p + embed_pos);
    assert!(
        set_embed_lock_after_embed.is_some(),
        "Must lock lore_store after embed() completes (for set_embedding)"
    );
}

// ===========================================================================
// 4. Embed worker connects per-request (not reusing stale connections)
// ===========================================================================

#[test]
fn embed_worker_creates_fresh_connection_per_request() {
    let s = prod(EMBED_WORKER_SRC);
    let process_fn_start = s
        .find("async fn process_request")
        .expect("process_request must exist");
    let process_fn = &s[process_fn_start..];
    assert!(
        process_fn.contains("DaemonClient::connect"),
        "Embed worker must create a fresh DaemonClient per request — \
         Unix socket connections are cheap and the daemon may have \
         restarted between requests"
    );
}

// ===========================================================================
// 5. Prompt.rs embed fallback is auditable
// ===========================================================================

#[test]
fn prompt_embed_fallback_is_not_silent() {
    let s = prod(PROMPT_SRC);
    // When the query embedding fails, prompt.rs must:
    // 1. Log a warning (tracing::warn)
    // 2. Emit a watcher event for the GM panel
    // 3. Fall back to keyword ranking
    //
    // All three must be present — just logging without a watcher event
    // means the GM panel can't see the degradation.
    assert!(
        s.contains("lore.query_embedding_failed"),
        "prompt.rs must surface query embedding failures via a watcher \
         event — without this, the GM panel can't detect RAG degradation"
    );
    assert!(
        s.contains("falling back to category ranking")
            || s.contains("falling back to keyword ranking"),
        "prompt.rs must log the fallback explicitly so operators know \
         semantic search is disabled"
    );
}

#[test]
fn prompt_embed_fallback_includes_error_detail() {
    let s = prod(PROMPT_SRC);
    // The watcher event must include the error detail so the GM panel
    // can diagnose whether it's a daemon down, timeout, or embed failure.
    let event_start = s
        .find("lore.query_embedding_failed")
        .expect("event must exist (checked above)");
    let event_block = &s[event_start.saturating_sub(200)..event_start + 300];
    assert!(
        event_block.contains("error_kind") || event_block.contains("error"),
        "query_embedding_failed event must include error_kind or error \
         field so the GM panel can diagnose the failure mode"
    );
}

// ===========================================================================
// 6. Embed worker shutdown is clean
// ===========================================================================

#[test]
fn embed_worker_logs_shutdown() {
    let s = prod(EMBED_WORKER_SRC);
    assert!(
        s.contains("lore.embed_worker.shutdown"),
        "Embed worker must log shutdown so the GM panel can see the \
         worker lifecycle"
    );
}

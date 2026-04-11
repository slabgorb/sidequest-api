//! Background lore-embedding worker (playtest 2026-04-11 fix).
//!
//! ## Why this module exists
//!
//! Prior to this fix, `dispatch/lore_sync.rs::accumulate_and_persist_lore`
//! awaited `DaemonClient::embed()` synchronously for every new lore fragment
//! AND ran a full retry sweep over all pending fragments at the start of every
//! call. When the daemon embed endpoint was wedged (each call timing out at
//! 10s), a turn that generated N lore events with M pending fragments incurred
//! N × M × 10s of blocking — turning a ~30s turn into 3+ minutes.
//!
//! ## The fix
//!
//! Dispatch no longer awaits any embedding work. Instead it non-blocking-sends
//! an [`EmbedRequest`] to this worker via an unbounded mpsc channel. The
//! worker owns a cloned `Arc<Mutex<LoreStore>>` and processes requests
//! serially, holding the 10s+ embed call **outside** the lock. Only the
//! sub-millisecond `set_embedding` write happens inside the critical section.
//!
//! ## Circuit breaker
//!
//! After [`FAILURE_THRESHOLD`] consecutive embed failures the worker enters
//! a circuit-open state for [`CIRCUIT_OPEN_DURATION`]. Requests received
//! while the circuit is open are dropped with an OTEL warning and re-queued
//! as "pending" in the lore store so the next successful half-open probe
//! can pick them back up. This prevents the worker from hammering a sick
//! daemon and gives operators a clear "daemon outage" signal on the GM
//! panel instead of a silent 3-minute hang.
//!
//! ## On-startup sweep
//!
//! At spawn time the worker does one initial pass over any fragments the
//! lore store already has marked `embedding_pending` (restored from SQLite
//! across sessions). This replaces the old per-turn retry sweep with a
//! one-shot catch-up that runs asynchronously and never blocks dispatch.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{mpsc::UnboundedReceiver, Mutex};
use tokio::task::JoinHandle;

use crate::{Severity, WatcherEventBuilder, WatcherEventType};

/// Consecutive failures before the circuit opens.
const FAILURE_THRESHOLD: u32 = 3;
/// How long the circuit stays open before the next half-open probe.
const CIRCUIT_OPEN_DURATION: Duration = Duration::from_secs(30);

/// Request pushed to the worker by dispatch when a new lore fragment needs
/// embedding. Contains only the ids/strings the worker needs — no borrows,
/// no locks held by the sender.
pub(crate) struct EmbedRequest {
    pub fragment_id: String,
    pub text: String,
}

/// Spawn the background embed worker. Returns the task handle; drop it (or
/// drop the sender) to tear the worker down when the session ends.
pub(crate) fn spawn(
    lore_store: Arc<Mutex<sidequest_game::LoreStore>>,
    mut rx: UnboundedReceiver<EmbedRequest>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        // Shared worker state — circuit breaker + failure counter persist
        // from the initial sweep through the main loop so a sustained
        // daemon outage during startup doesn't reset when we pivot to
        // processing live requests.
        let mut state = WorkerState::default();

        // Initial pending sweep — replaces the old retry_pending_embeddings
        // call in the dispatch hot path. Runs once at session start so any
        // fragments left pending from a prior session get healed without
        // blocking gameplay.
        let initial_pending = {
            let store = lore_store.lock().await;
            store.pending_embedding_fragments()
        };
        if !initial_pending.is_empty() {
            tracing::info!(
                count = initial_pending.len(),
                "lore.embed_worker.initial_sweep_start"
            );
            let mut swept_ok = 0usize;
            let mut swept_failed = 0usize;
            for (fragment_id, text) in initial_pending {
                let req = EmbedRequest { fragment_id, text };
                match process_request(&lore_store, &req, &mut state).await {
                    EmbedOutcome::Ok => swept_ok += 1,
                    EmbedOutcome::Failed | EmbedOutcome::CircuitOpen => swept_failed += 1,
                }
            }
            WatcherEventBuilder::new("lore", WatcherEventType::SubsystemExerciseSummary)
                .field("event", "lore.embed_worker.initial_sweep_complete")
                .field("swept_ok", swept_ok)
                .field("swept_failed", swept_failed)
                .send();
        }

        // Main loop: drain the channel. Exits when all senders are dropped
        // (i.e. when the session ends).
        while let Some(req) = rx.recv().await {
            let _ = process_request(&lore_store, &req, &mut state).await;
        }
        tracing::info!("lore.embed_worker.shutdown");
    })
}

#[derive(Default)]
struct WorkerState {
    circuit_open_until: Option<Instant>,
    consecutive_failures: u32,
}

enum EmbedOutcome {
    Ok,
    Failed,
    CircuitOpen,
}

/// Process a single embed request — connect, call embed, write result into
/// the lore store. The 10s+ embed call is held OUTSIDE the lore_store lock
/// so dispatch reads/writes are never blocked by daemon slowness.
async fn process_request(
    lore_store: &Arc<Mutex<sidequest_game::LoreStore>>,
    req: &EmbedRequest,
    state: &mut WorkerState,
) -> EmbedOutcome {
    // Circuit breaker check — if the circuit is open and hasn't yet
    // expired, skip this request entirely and mark the fragment pending
    // so the next round can retry it.
    if let Some(until) = state.circuit_open_until {
        if Instant::now() < until {
            {
                let mut store = lore_store.lock().await;
                let _ = store.mark_embedding_pending(&req.fragment_id);
            }
            return EmbedOutcome::CircuitOpen;
        }
        // Half-open: clear the gate and let this request act as the probe.
        state.circuit_open_until = None;
    }

    let config = sidequest_daemon_client::DaemonConfig::default();
    let mut client = match sidequest_daemon_client::DaemonClient::connect(config).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                error = %e,
                fragment_id = %req.fragment_id,
                "lore.embed_worker.connect_failed"
            );
            record_failure(lore_store, req, state, "daemon_connect_failed").await;
            return EmbedOutcome::Failed;
        }
    };

    let params = sidequest_daemon_client::EmbedParams {
        text: req.text.clone(),
    };
    match client.embed(params).await {
        Ok(embed_result) => {
            {
                let mut store = lore_store.lock().await;
                if let Err(e) = store.set_embedding(&req.fragment_id, embed_result.embedding) {
                    tracing::warn!(
                        error = %e,
                        fragment_id = %req.fragment_id,
                        "lore.embed_worker.set_embedding_failed"
                    );
                    state.consecutive_failures += 1;
                    return EmbedOutcome::Failed;
                }
            }
            // Success — reset the circuit and emit telemetry so the GM
            // panel can see embeddings healing.
            state.consecutive_failures = 0;
            WatcherEventBuilder::new("lore", WatcherEventType::StateTransition)
                .field("event", "lore.embedding_generated")
                .field("fragment_id", &req.fragment_id)
                .field("latency_ms", embed_result.latency_ms)
                .field("model", &embed_result.model)
                .send();
            EmbedOutcome::Ok
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                fragment_id = %req.fragment_id,
                "lore.embed_worker.embed_failed"
            );
            record_failure(lore_store, req, state, "embed_failed").await;
            EmbedOutcome::Failed
        }
    }
}

/// Mark the fragment pending, increment the failure counter, and open the
/// circuit if we've hit the threshold. Emits a ValidationWarning so operators
/// see the outage on the GM panel instead of a silent hang.
async fn record_failure(
    lore_store: &Arc<Mutex<sidequest_game::LoreStore>>,
    req: &EmbedRequest,
    state: &mut WorkerState,
    error_kind: &str,
) {
    {
        let mut store = lore_store.lock().await;
        let _ = store.mark_embedding_pending(&req.fragment_id);
    }
    state.consecutive_failures += 1;
    WatcherEventBuilder::new("lore", WatcherEventType::ValidationWarning)
        .field("event", "lore.embedding_pending")
        .field("fragment_id", &req.fragment_id)
        .field("error_kind", error_kind)
        .severity(Severity::Warn)
        .send();

    if state.consecutive_failures >= FAILURE_THRESHOLD && state.circuit_open_until.is_none() {
        state.circuit_open_until = Some(Instant::now() + CIRCUIT_OPEN_DURATION);
        tracing::warn!(
            consecutive_failures = state.consecutive_failures,
            circuit_open_for_secs = CIRCUIT_OPEN_DURATION.as_secs(),
            "lore.embed_worker.circuit_opened"
        );
        WatcherEventBuilder::new("lore", WatcherEventType::ValidationWarning)
            .field("event", "lore.embedding_circuit_open")
            .field("consecutive_failures", state.consecutive_failures)
            .field("reopen_in_secs", CIRCUIT_OPEN_DURATION.as_secs())
            .severity(Severity::Warn)
            .send();
    }
}

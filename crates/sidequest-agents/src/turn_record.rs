//! TurnRecord — durable, typed snapshot of each game turn.
//!
//! Story 3-2: Defines the TurnRecord struct and mpsc channel pipeline
//! from the orchestrator (hot path) to the validator (cold path).
//!
//! ADR-031: Game Watcher — hot-path/cold-path contract via TurnRecord.

use chrono::{DateTime, Utc};
use sidequest_game::{GameSnapshot, StateDelta};
use tokio::sync::mpsc;

use crate::agents::intent_router::Intent;

/// Buffer size for the watcher mpsc channel.
///
/// 32 slots = minutes of buffer at typical play pace (one turn every 10-30s).
/// If the validator can't keep up with 32 queued turns, dropping records
/// is the correct response — gameplay must never block on validation.
pub const WATCHER_CHANNEL_CAPACITY: usize = 32;

/// Summary of patches applied during a turn.
///
/// Lightweight representation of what changed, without the full patch payloads.
#[derive(Debug, Clone)]
pub struct PatchSummary {
    /// Type of patch (e.g., "world", "combat", "chase").
    pub patch_type: String,
    /// Which fields were modified by this patch.
    pub fields_changed: Vec<String>,
}

/// A durable, typed snapshot of a single game turn.
///
/// Assembled in `process_turn()` after delta computation and sent via
/// `try_send` through the watcher mpsc channel. Contains everything needed
/// to validate the turn asynchronously on the cold path.
///
/// All 15 fields per ADR-031.
#[derive(Debug, Clone)]
pub struct TurnRecord {
    /// Monotonically increasing turn identifier.
    pub turn_id: u64,
    /// When this turn was processed.
    pub timestamp: DateTime<Utc>,
    /// Raw player input (after sanitization).
    pub player_input: String,
    /// How the intent router classified this input.
    pub classified_intent: Intent,
    /// Which agent produced the narration.
    pub agent_name: String,
    /// The narrative text produced by the agent.
    pub narration: String,
    /// Summary of patches applied to game state.
    pub patches_applied: Vec<PatchSummary>,
    /// Game state snapshot before patches were applied.
    pub snapshot_before: GameSnapshot,
    /// Game state snapshot after patches were applied.
    pub snapshot_after: GameSnapshot,
    /// Delta between before and after snapshots.
    pub delta: StateDelta,
    /// Trope beats that fired during this turn: (trope_name, threshold).
    pub beats_fired: Vec<(String, f32)>,
    /// JSON extraction tier used (1=direct, 2=fenced, 3=regex).
    pub extraction_tier: u8,
    /// Input tokens consumed by the agent LLM call.
    pub token_count_in: usize,
    /// Output tokens produced by the agent LLM call.
    pub token_count_out: usize,
    /// Wall-clock duration of the agent call in milliseconds.
    pub agent_duration_ms: u64,
    /// Whether this turn used a degraded/fallback response.
    pub is_degraded: bool,
}

/// Counter for assigning monotonically increasing turn IDs.
///
/// Lives on the Orchestrator. Each call to `next_turn_id()` returns
/// a unique, strictly increasing u64.
pub struct TurnIdCounter {
    _next: u64,
}

impl TurnIdCounter {
    /// Create a new counter starting at turn 1.
    pub fn new() -> Self {
        Self { _next: 0 }
    }

    /// Return the next turn ID and advance the counter.
    pub fn next_turn_id(&mut self) -> u64 {
        // Stub: not yet implemented. Always returns 0.
        // Dev must implement: increment and return.
        0
    }
}

impl Default for TurnIdCounter {
    fn default() -> Self {
        Self::new()
    }
}

/// Validator task — receives TurnRecords from the watcher channel and
/// logs a structured summary for each.
///
/// Runs as a detached `tokio::spawn` task. When the orchestrator is dropped,
/// the sender drops, the channel closes, `rx.recv()` returns `None`, and
/// the validator exits cleanly.
///
/// Stories 3-3 through 3-5 will add actual validation checks.
/// For story 3-2, the validator logs receipt of each TurnRecord.
pub async fn run_validator(mut rx: mpsc::Receiver<TurnRecord>) -> Vec<u64> {
    let processed_turn_ids = Vec::new();

    while let Some(_record) = rx.recv().await {
        // Stub: receives records but does not process or log them.
        // Dev must implement:
        //   tracing::info!(
        //       turn_id = record.turn_id,
        //       intent = %record.classified_intent,
        //       agent = %record.agent_name,
        //       patches = record.patches_applied.len(),
        //       delta_empty = record.delta.is_empty(),
        //       extraction_tier = record.extraction_tier,
        //       is_degraded = record.is_degraded,
        //       "received TurnRecord"
        //   );
        //   processed_turn_ids.push(record.turn_id);
    }

    processed_turn_ids
}

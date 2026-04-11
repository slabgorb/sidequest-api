//! TurnRecord — durable, typed snapshot of each game turn.
//!
//! Story 3-2: Defines the TurnRecord struct and mpsc channel pipeline
//! from the orchestrator (hot path) to the validator (cold path).
//!
//! ADR-031: Game Watcher — hot-path/cold-path contract via TurnRecord.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sidequest_game::{GameSnapshot, StateDelta};
use tokio::sync::mpsc;

use crate::agents::intent_router::Intent;
use crate::patch_legality::{run_legality_checks, ValidationResult};
use sidequest_telemetry::{Severity, WatcherEventBuilder, WatcherEventType};

/// Buffer size for the watcher mpsc channel.
///
/// 32 slots = minutes of buffer at typical play pace (one turn every 10-30s).
/// If the validator can't keep up with 32 queued turns, dropping records
/// is the correct response — gameplay must never block on validation.
pub const WATCHER_CHANNEL_CAPACITY: usize = 32;

/// Summary of patches applied during a turn.
///
/// Lightweight representation of what changed, without the full patch payloads.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Input tokens consumed by the agent LLM call.
    pub token_count_in: usize,
    /// Output tokens produced by the agent LLM call.
    pub token_count_out: usize,
    /// Wall-clock duration of the agent call in milliseconds.
    pub agent_duration_ms: u64,
    /// Whether this turn used a degraded/fallback response.
    pub is_degraded: bool,
    /// Per-phase timing spans for flame chart visualization.
    /// Each entry: (name, start_ms relative to turn start, duration_ms).
    pub spans: Vec<(String, u64, u64)>,
    /// Full assembled prompt text sent to the LLM (ADR-073 training data).
    pub prompt_text: Option<String>,
    /// Raw LLM response text before extraction (ADR-073 training data).
    pub raw_response_text: Option<String>,
}

/// Counter for assigning monotonically increasing turn IDs.
///
/// Lives on the Orchestrator. Each call to `next_turn_id()` returns
/// a unique, strictly increasing u64 starting at 1.
pub struct TurnIdCounter {
    next: u64,
}

impl TurnIdCounter {
    /// Create a new counter starting at turn 1.
    pub fn new() -> Self {
        Self { next: 1 }
    }

    /// Return the next turn ID and advance the counter.
    pub fn next_turn_id(&mut self) -> u64 {
        let id = self.next;
        self.next += 1;
        id
    }
}

impl Default for TurnIdCounter {
    fn default() -> Self {
        Self::new()
    }
}

/// Send a TurnRecord through the watcher channel without blocking.
///
/// Uses `try_send` (non-blocking). If the channel is full or closed,
/// logs a warning with the dropped turn_id and continues. The hot path
/// (orchestrator) must never block on the cold path (validator).
pub fn try_send_record(tx: &mpsc::Sender<TurnRecord>, record: TurnRecord) {
    let turn_id = record.turn_id;
    if let Err(e) = tx.try_send(record) {
        tracing::warn!(
            error = %e,
            turn_id = turn_id,
            "watcher channel full or closed — dropping TurnRecord"
        );
    }
}

/// Validator task — receives TurnRecords from the watcher channel and
/// logs a structured summary for each.
///
/// Runs as a detached `tokio::spawn` task. When the orchestrator is dropped,
/// the sender drops, the channel closes, `rx.recv()` returns `None`, and
/// the validator exits cleanly.
///
/// Returns the list of processed turn IDs for testability.
/// Stories 3-3 through 3-5 will add actual validation checks.
pub async fn run_validator(mut rx: mpsc::Receiver<TurnRecord>) -> Vec<u64> {
    tracing::info!("watcher validator started, awaiting TurnRecords");

    let mut processed_turn_ids = Vec::new();

    while let Some(record) = rx.recv().await {
        tracing::info!(
            turn_id = record.turn_id,
            intent = %record.classified_intent,
            agent = %record.agent_name,
            patches = record.patches_applied.len(),
            delta_empty = record.delta.is_empty(),
            is_degraded = record.is_degraded,
            "received TurnRecord"
        );

        // Run patch legality checks (story 35-1)
        let results = run_legality_checks(&record);

        let mut violations = 0u64;
        let mut warnings = 0u64;
        for result in &results {
            match result {
                ValidationResult::Violation(msg) => {
                    violations += 1;
                    WatcherEventBuilder::new("patch_legality", WatcherEventType::ValidationWarning)
                        .field("check", "patch_legality")
                        .field("violation", msg.as_str())
                        .field("turn_id", record.turn_id)
                        .severity(Severity::Warn)
                        .send();
                }
                ValidationResult::Warning(msg) => {
                    warnings += 1;
                    WatcherEventBuilder::new("patch_legality", WatcherEventType::ValidationWarning)
                        .field("check", "patch_legality")
                        .field("warning", msg.as_str())
                        .field("turn_id", record.turn_id)
                        .severity(Severity::Warn)
                        .send();
                }
                ValidationResult::Ok => {}
            }
        }

        // Emit per-turn summary
        WatcherEventBuilder::new("patch_legality", WatcherEventType::SubsystemExerciseSummary)
            .field("turn_id", record.turn_id)
            .field("total_checks", results.len() as u64)
            .field("violations", violations)
            .field("warnings", warnings)
            .send();

        processed_turn_ids.push(record.turn_id);
    }

    tracing::info!("watcher validator shutting down (channel closed)");

    processed_turn_ids
}

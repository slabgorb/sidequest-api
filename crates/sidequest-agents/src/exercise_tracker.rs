//! Subsystem exercise tracker — agent invocation histogram and coverage gap detection.
//!
//! Story 3-5: Tracks cumulative agent invocation counts per session to detect
//! which subsystems are being exercised during play.
//!
//! Lives in the watcher validator task (cold path). For each TurnRecord received,
//! the validator calls `tracker.record(&record.agent_name)` to update the histogram.

use std::collections::HashMap;

/// The 8 expected agent types in SideQuest.
///
/// These are pre-seeded into the histogram at tracker creation so that
/// coverage gap detection can identify agents with zero invocations.
pub const EXPECTED_AGENTS: &[&str] = &[
    "narrator",
    "creature_smith",
    "ensemble",
    "troper",
    "world_builder",
    "dialectician",
    "resonator",
    "intent_router",
];

/// Tracks agent invocation counts per session.
///
/// Maintains a `HashMap<String, usize>` histogram of agent_name → call_count.
/// Emits `tracing::info!` summaries at configurable intervals and
/// `tracing::warn!` for coverage gaps after a configurable threshold.
pub struct SubsystemTracker {
    /// Agent name → invocation count.
    pub counts: HashMap<String, usize>,
    /// Total turns processed.
    pub turn_count: usize,
    /// Emit summary every N turns.
    pub summary_interval: usize,
    /// Warn about zero-invocation agents after N turns.
    pub gap_threshold: usize,
}

impl SubsystemTracker {
    /// Create a new tracker with the given thresholds.
    ///
    /// Pre-seeds all EXPECTED_AGENTS with count 0 so coverage gap detection
    /// can identify agents that were never invoked.
    pub fn new(summary_interval: usize, gap_threshold: usize) -> Self {
        let mut counts = HashMap::with_capacity(EXPECTED_AGENTS.len());
        for &agent in EXPECTED_AGENTS {
            counts.insert(agent.to_string(), 0);
        }
        Self {
            counts,
            turn_count: 0,
            summary_interval,
            gap_threshold,
        }
    }

    /// Record an agent invocation.
    ///
    /// Increments the count for `agent_name`, increments `turn_count`,
    /// and checks whether to emit a summary or coverage gap warning.
    pub fn record(&mut self, agent_name: &str) {
        *self.counts.entry(agent_name.to_string()).or_insert(0) += 1;
        self.turn_count += 1;

        // Emit periodic histogram summary at interval boundaries.
        if self.turn_count % self.summary_interval == 0 {
            let histogram_str = self.format_histogram();
            tracing::info!(
                component = "watcher",
                check = "subsystem_exercise",
                turn_count = self.turn_count,
                histogram = %histogram_str,
                "Subsystem exercise summary"
            );
        }

        // Check for coverage gaps at the threshold boundary.
        if self.turn_count == self.gap_threshold {
            let uncovered = self.uncovered_agents();
            if !uncovered.is_empty() {
                let missing = uncovered.join(", ");
                tracing::warn!(
                    component = "watcher",
                    check = "subsystem_exercise",
                    missing_agents = %missing,
                    turns = self.turn_count,
                    "Uncovered agents after threshold"
                );
            }
        }
    }

    /// Return the current histogram as a snapshot.
    pub fn histogram(&self) -> &HashMap<String, usize> {
        &self.counts
    }

    /// Return agent names with zero invocations from the expected set.
    pub fn uncovered_agents(&self) -> Vec<&str> {
        EXPECTED_AGENTS
            .iter()
            .copied()
            .filter(|&agent| self.counts.get(agent).copied().unwrap_or(0) == 0)
            .collect()
    }

    /// Format the histogram as a human-readable string for tracing output.
    fn format_histogram(&self) -> String {
        let mut entries: Vec<_> = self.counts.iter().collect();
        entries.sort_by_key(|(name, _)| (*name).clone());
        entries
            .iter()
            .map(|(name, count)| format!("{}={}", name, count))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

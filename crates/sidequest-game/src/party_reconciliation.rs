//! Party reconciliation on multiplayer session resume (Story 26-11).
//!
//! Detects divergent player locations when multiple players reconnect to
//! the same genre:world session and reconciles them to a single canonical
//! location. Prevents split-party states that lack narrative justification.

use sidequest_telemetry::{WatcherEventBuilder, WatcherEventType};
use std::collections::HashMap;

/// A player's location at session resume time.
#[derive(Debug, Clone)]
pub struct PlayerLocation {
    /// Stable player identifier.
    pub player_id: String,
    /// Display name of the player.
    pub player_name: String,
    /// Location slug the player is currently at.
    pub location: String,
}

/// A player who was moved during reconciliation (telemetry payload).
#[derive(Debug, Clone)]
pub struct MovedPlayer {
    /// Stable player identifier.
    pub player_id: String,
    /// Display name of the player.
    pub player_name: String,
    /// Where the player was before reconciliation.
    pub old_location: String,
    /// Where the player was moved to.
    pub new_location: String,
}

/// Result of party reconciliation.
#[derive(Debug)]
pub enum ReconciliationResult {
    /// All players are already at the same location (or ≤1 player).
    NoActionNeeded,
    /// Locations diverged but split-party is explicitly allowed.
    SplitPartyAllowed,
    /// Locations were reconciled to a single target.
    Reconciled {
        /// Canonical location all players were moved to.
        target_location: String,
        /// Per-player records for everyone who was relocated.
        players_moved: Vec<MovedPlayer>,
        /// Narration text describing the reconciliation event.
        narration_text: String,
    },
}

/// Party reconciliation logic.
pub struct PartyReconciliation;

impl PartyReconciliation {
    /// Reconcile divergent player locations on session resume.
    ///
    /// - `players`: all players reconnecting to the session with their persisted locations.
    /// - `split_party_allowed`: if true, divergent locations are preserved (no reconciliation).
    ///
    /// Returns `NoActionNeeded` when all players share the same location (or ≤1 player),
    /// `SplitPartyAllowed` when the flag is set and locations diverge, or `Reconciled`
    /// with the target location, moved players, and a narration line.
    pub fn reconcile(
        players: &[PlayerLocation],
        split_party_allowed: bool,
    ) -> ReconciliationResult {
        let player_count = players.len();


        // Nothing to reconcile with 0 or 1 players
        if player_count <= 1 {
            emit_watcher_event("no_action_needed", player_count, None, 0);
            return ReconciliationResult::NoActionNeeded;
        }

        // Count non-empty locations
        let mut location_counts: HashMap<&str, usize> = HashMap::new();
        for p in players {
            if !p.location.is_empty() {
                *location_counts.entry(p.location.as_str()).or_insert(0) += 1;
            }
        }

        // All locations empty → nothing to reconcile
        if location_counts.is_empty() {
            emit_watcher_event("no_action_needed", player_count, None, 0);
            return ReconciliationResult::NoActionNeeded;
        }

        // Check if all non-empty locations are the same
        let unique_locations: Vec<&&str> = location_counts.keys().collect();
        let all_same = unique_locations.len() <= 1
            && players
                .iter()
                .all(|p| p.location.is_empty() || p.location == **unique_locations[0]);

        // Players with empty locations count as divergent (they need to be moved)
        let has_empty = players.iter().any(|p| p.location.is_empty());
        let truly_same = all_same && !has_empty;

        if truly_same {
            emit_watcher_event("no_action_needed", player_count, None, 0);
            return ReconciliationResult::NoActionNeeded;
        }

        // Locations diverge — check split-party flag
        if split_party_allowed {
            emit_watcher_event("split_party_allowed", player_count, None, 0);
            return ReconciliationResult::SplitPartyAllowed;
        }

        // Pick target: majority location wins, alphabetical tie-break
        let target = location_counts
            .iter()
            .max_by(|(loc_a, count_a), (loc_b, count_b)| {
                count_a.cmp(count_b).then_with(|| loc_b.cmp(loc_a))
            })
            .map(|(loc, _)| *loc)
            .unwrap(); // safe: location_counts is non-empty

        let target_location = target.to_string();

        // Build moved-player list
        let players_moved: Vec<MovedPlayer> = players
            .iter()
            .filter(|p| p.location != target_location)
            .map(|p| MovedPlayer {
                player_id: p.player_id.clone(),
                player_name: p.player_name.clone(),
                old_location: p.old_location(),
                new_location: target_location.clone(),
            })
            .collect();

        let moved_count = players_moved.len();
        emit_watcher_event(
            "reconciled",
            player_count,
            Some(target_location.as_str()),
            moved_count,
        );

        // Generate narration
        let narration_text = format!("The party regroups at the {}.", target_location);

        ReconciliationResult::Reconciled {
            target_location,
            players_moved,
            narration_text,
        }
    }
}

/// Emit `party_reconciliation.reconciled` watcher event for all three result
/// variants (story 35-10). Single event component+action with the variant
/// discriminated by the `result` field. `target_location` is null and
/// `moved_count` is 0 for the no-op variants.
fn emit_watcher_event(
    result: &str,
    player_count: usize,
    target_location: Option<&str>,
    moved_count: usize,
) {
    let target_field = match target_location {
        Some(loc) => serde_json::Value::String(loc.to_string()),
        None => serde_json::Value::Null,
    };
    WatcherEventBuilder::new("party_reconciliation", WatcherEventType::StateTransition)
        .field("action", "reconciled")
        .field("result", result.to_string())
        .field("player_count", player_count as u64)
        .field("target_location", target_field)
        .field("moved_count", moved_count as u64)
        .send();
}

impl PlayerLocation {
    fn old_location(&self) -> String {
        if self.location.is_empty() {
            "(unknown)".to_string()
        } else {
            self.location.clone()
        }
    }
}

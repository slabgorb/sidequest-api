//! Turn barrier — concurrent timeout-based turn resolution.
//!
//! Story 8-2: Wraps `MultiplayerSession` behind `Arc<Mutex<_>>` with a
//! `tokio::sync::Notify` so `wait_for_turn()` can run in a spawned task
//! while `submit_action()` is called concurrently from WebSocket handlers.
//!
//! Design: `TurnBarrier` is `Clone` (shared via `Arc`). All methods take
//! `&self` and lock internally. The barrier mediates all session access —
//! callers use `barrier.submit_action()`, not `session_mut()`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::Notify;

use crate::character::Character;
use crate::multiplayer::{MultiplayerError, MultiplayerSession};

/// Configuration for the turn barrier timeout.
#[derive(Debug, Clone)]
pub struct TurnBarrierConfig {
    timeout: Duration,
    enabled: bool,
}

impl TurnBarrierConfig {
    /// Create a config with an explicit timeout duration.
    pub fn new(timeout: Duration) -> Self {
        Self {
            timeout,
            enabled: true,
        }
    }

    /// Create a disabled config (infinite wait, no auto-resolve).
    pub fn disabled() -> Self {
        Self {
            timeout: Duration::ZERO,
            enabled: false,
        }
    }

    /// The timeout duration.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Whether the timeout is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

impl Default for TurnBarrierConfig {
    fn default() -> Self {
        Self::new(Duration::from_secs(30))
    }
}

/// Result of waiting for a turn to resolve.
#[derive(Debug)]
pub struct TurnBarrierResult {
    /// Whether this particular task successfully claimed resolution.
    pub claimed_resolution: bool,
    /// Whether the turn was resolved by timeout (true) or full submission (false).
    pub timed_out: bool,
    /// Player IDs that didn't submit before the turn resolved.
    pub missing_players: Vec<String>,
    /// Per-player narration stubs, keyed by player_id.
    pub narration: HashMap<String, String>,
}

impl TurnBarrierResult {
    /// Format auto-resolved context for injection into the narrator prompt.
    ///
    /// Returns a string describing which characters were auto-resolved (timed out)
    /// so the narrator can condition its narration on intentional vs. auto-resolved
    /// actions. Returns empty string if no timeout occurred.
    pub fn format_auto_resolved_context(&self) -> String {
        if !self.timed_out || self.missing_players.is_empty() {
            return String::new();
        }

        // Extract character names from the narration entries for missing players.
        // Narration values are formatted as "CharName: action text".
        let missing_names: Vec<String> = self
            .missing_players
            .iter()
            .filter_map(|pid| {
                self.narration
                    .get(pid)
                    .and_then(|n| n.split(':').next())
                    .map(|name| name.trim().to_string())
            })
            .collect();

        if missing_names.is_empty() {
            return String::new();
        }

        format!(
            "The following characters did not act and were auto-resolved (timed out): {}. \
             They hesitate — narrate their inaction briefly, do not invent actions for them.",
            missing_names.join(", ")
        )
    }
}

/// Adaptive timeout — scales the collection window by player count.
///
/// Story 8-3: Default tiers are 2-3 players → 3s, 4+ players → 5s.
/// Custom tiers can be configured via `with_tiers()`.
#[derive(Debug, Clone)]
pub struct AdaptiveTimeout {
    /// Sorted ascending by threshold: `(min_player_count, duration)`.
    tiers: Vec<(usize, Duration)>,
    /// Base duration used when no tier threshold is met.
    base: Duration,
}

impl AdaptiveTimeout {
    /// Create an adaptive timeout with custom tiers and a base duration.
    ///
    /// Each tier is `(min_player_count, duration)`. When `player_count >=
    /// threshold`, the corresponding duration is used. The highest matching
    /// tier wins. If no tier matches, the base duration is returned.
    pub fn with_tiers(tiers: Vec<(usize, Duration)>, base: Duration) -> Self {
        let mut tiers = tiers;
        tiers.sort_by_key(|(threshold, _)| *threshold);
        Self { tiers, base }
    }

    /// Look up the timeout for a given player count.
    pub fn timeout_for(&self, player_count: usize) -> Duration {
        // Iterate in reverse — highest matching threshold wins
        for &(threshold, duration) in self.tiers.iter().rev() {
            if player_count >= threshold {
                return duration;
            }
        }
        self.base
    }

    /// Convenience: produce a `TurnBarrierConfig` for a player count.
    pub fn config_for(&self, player_count: usize) -> TurnBarrierConfig {
        TurnBarrierConfig::new(self.timeout_for(player_count))
    }
}

impl Default for AdaptiveTimeout {
    /// Default tiers: <4 players → 3s, 4+ players → 5s.
    fn default() -> Self {
        Self::with_tiers(vec![(4, Duration::from_secs(5))], Duration::from_secs(3))
    }
}

/// Shared interior state behind the `Arc`.
struct Inner {
    session: Mutex<MultiplayerSession>,
    config: Mutex<TurnBarrierConfig>,
    notify: Notify,
    adaptive: Mutex<Option<AdaptiveTimeout>>,
    /// Flag for handler election — protected by resolution_lock.
    /// First task to enter resolve() sets this to true; others see it already true.
    resolution_claimed: Mutex<bool>,
    /// Narration text stored by the claiming handler for non-claimers to retrieve.
    resolution_narration: Mutex<Option<String>>,
    /// Turn number tracker — used to detect when we've moved to the next turn
    /// so we can reset the claim state.
    last_resolved_turn: Mutex<u32>,
    /// Track the turn number of the last claim election.
    /// Only one task per turn should execute the claim election.
    last_claim_turn: Mutex<u32>,
    /// Track the turn number at the start of each wait_for_turn() call.
    /// All tasks that start with the same initial turn should claim together.
    current_resolution_turn: Mutex<u32>,
    /// Mutex protecting the resolution process — ensures only one task
    /// actually resolves a turn at a time.
    resolution_lock: Mutex<()>,
}

/// Concurrent turn barrier wrapping a `MultiplayerSession`.
///
/// `Clone` to share across tasks. All methods take `&self` and lock
/// internally. Uses `tokio::sync::Notify` to wake `wait_for_turn()`
/// immediately when the last player submits.
#[derive(Clone)]
pub struct TurnBarrier {
    inner: Arc<Inner>,
}

impl TurnBarrier {
    /// Create a new barrier wrapping the given session and config.
    pub fn new(session: MultiplayerSession, config: TurnBarrierConfig) -> Self {
        Self {
            inner: Arc::new(Inner {
                session: Mutex::new(session),
                config: Mutex::new(config),
                notify: Notify::new(),
                adaptive: Mutex::new(None),
                resolution_claimed: Mutex::new(false),
                resolution_narration: Mutex::new(None),
                last_resolved_turn: Mutex::new(0),
                last_claim_turn: Mutex::new(0),
                current_resolution_turn: Mutex::new(0),
                resolution_lock: Mutex::new(()),
            }),
        }
    }

    /// Create a barrier with adaptive timeout that auto-adjusts based on
    /// player count. The initial config is derived from the session's
    /// current player count.
    pub fn with_adaptive(session: MultiplayerSession, adaptive: AdaptiveTimeout) -> Self {
        let config = adaptive.config_for(session.player_count());
        Self {
            inner: Arc::new(Inner {
                session: Mutex::new(session),
                config: Mutex::new(config),
                notify: Notify::new(),
                adaptive: Mutex::new(Some(adaptive)),
                resolution_claimed: Mutex::new(false),
                resolution_narration: Mutex::new(None),
                last_resolved_turn: Mutex::new(0),
                last_claim_turn: Mutex::new(0),
                current_resolution_turn: Mutex::new(0),
                resolution_lock: Mutex::new(()),
            }),
        }
    }

    /// Current player count.
    pub fn player_count(&self) -> usize {
        self.inner.session.lock().unwrap().player_count()
    }

    /// Current turn number.
    pub fn turn_number(&self) -> u32 {
        self.inner.session.lock().unwrap().turn_number()
    }

    /// Current barrier configuration (cloned).
    pub fn config(&self) -> TurnBarrierConfig {
        self.inner.config.lock().unwrap().clone()
    }

    /// Update the barrier configuration.
    pub fn set_config(&self, config: TurnBarrierConfig) {
        *self.inner.config.lock().unwrap() = config;
    }

    /// Submit an action for a player. Wakes `wait_for_turn()` if the
    /// barrier is met (all players have submitted).
    pub fn submit_action(&self, player_id: &str, action: &str) {
        let mut session = self.inner.session.lock().unwrap();
        session.record_action(player_id, action);
        if session.is_barrier_met() {
            self.inner.notify.notify_one();
        }
    }

    /// Add a player to the session. Auto-adjusts timeout if adaptive
    /// config is set and the new count crosses a tier boundary.
    pub fn add_player(
        &self,
        player_id: String,
        character: Character,
    ) -> Result<usize, MultiplayerError> {
        let count = self
            .inner
            .session
            .lock()
            .unwrap()
            .add_player(player_id, character)?;
        self.maybe_reconfigure(count);
        Ok(count)
    }

    /// Remove a player from the session. Wakes `wait_for_turn()` if
    /// removal causes the barrier to be met. Auto-adjusts timeout if
    /// adaptive config is set and the new count crosses a tier boundary.
    pub fn remove_player(&self, player_id: &str) -> Result<usize, MultiplayerError> {
        let remaining = {
            let mut session = self.inner.session.lock().unwrap();
            let remaining = session.remove_player_no_resolve(player_id)?;
            if session.is_barrier_met() {
                self.inner.notify.notify_one();
            }
            remaining
        };
        self.maybe_reconfigure(remaining);
        Ok(remaining)
    }

    /// Wait for the current turn to resolve, either by all players
    /// submitting or by timeout expiry.
    ///
    /// Uses `tokio::select!` between `Notify::notified()` and
    /// `tokio::time::sleep_until()`. When the barrier is met (all
    /// submitted), the turn resolves immediately. On timeout, missing
    /// players get a "hesitates" action.
    pub async fn wait_for_turn(&self) -> TurnBarrierResult {
        let initial_turn = self.inner.session.lock().unwrap().turn_number();
        // Store this as the current resolution turn (if this is the first task for this turn)
        {
            let mut current = self.inner.current_resolution_turn.lock().unwrap();
            if initial_turn > *current {
                *current = initial_turn;
            }
        }
        
        let (deadline, enabled) = {
            let config = self.inner.config.lock().unwrap();
            if config.enabled {
                (Some(tokio::time::Instant::now() + config.timeout), true)
            } else {
                (None, false)
            }
        };

        loop {
            // Check if barrier is already met
            {
                let session = self.inner.session.lock().unwrap();
                if session.is_barrier_met() {
                    drop(session);
                    // Acquire the resolution lock to ensure only one task actually resolves
                    let _res_lock = self.inner.resolution_lock.lock().unwrap();
                    return self.resolve(false);
                }
            }

            // Wait for notification or timeout
            if enabled {
                let dl = deadline.unwrap();
                tokio::select! {
                    _ = self.inner.notify.notified() => {
                        // Woken by submit/remove — re-check at top of loop
                    }
                    _ = tokio::time::sleep_until(dl) => {
                        let _res_lock = self.inner.resolution_lock.lock().unwrap();
                        return self.resolve(true);
                    }
                }
            } else {
                // Disabled — wait only for notify, no timeout
                self.inner.notify.notified().await;
                // Re-check at top of loop
            }
        }
    }

    /// Expose `named_actions()` from the barrier's internal session.
    ///
    /// Returns actions keyed by character name. This reads from the barrier's
    /// own `MultiplayerSession`, NOT from any external shared session.
    pub fn named_actions(&self) -> HashMap<String, String> {
        self.inner.session.lock().unwrap().named_actions()
    }

    /// Attempt to claim resolution for this turn. Returns `true` exactly once
    /// per barrier resolution — the first caller wins and should run the narrator.
    /// All subsequent callers get `false` and should retrieve the stored result.
    pub fn try_claim_resolution(&self) -> bool {
        let mut claimed_flag = self.inner.resolution_claimed.lock().unwrap();
        let result = !*claimed_flag;
        *claimed_flag = true;
        result
    }

    /// Store the narration result after the claiming handler runs the narrator.
    /// Non-claiming handlers retrieve this via `get_resolution_narration()`.
    pub fn store_resolution_narration(&self, narration: String) {
        *self.inner.resolution_narration.lock().unwrap() = Some(narration);
    }

    /// Retrieve the stored narration result. Returns `None` if the claiming
    /// handler hasn't stored it yet.
    pub fn get_resolution_narration(&self) -> Option<String> {
        self.inner.resolution_narration.lock().unwrap().clone()
    }

    /// Reconfigure the timeout if adaptive mode is active and the player
    /// count crosses a tier boundary.
    fn maybe_reconfigure(&self, player_count: usize) {
        let new_config = {
            let adaptive = self.inner.adaptive.lock().unwrap();
            adaptive.as_ref().map(|a| a.config_for(player_count))
        };
        if let Some(config) = new_config {
            *self.inner.config.lock().unwrap() = config;
        }
    }

    /// Resolve the current turn and return the result.
    /// Attempts to claim resolution atomically during resolution (not after).
    /// This ensures only one task wins the claim election when multiple tasks
    /// wake up from wait_for_turn() concurrently.
    /// Resolve the current turn and return the result.
    /// Only the first task from a batch (all tasks that entered wait_for_turn() with the same
    /// initial turn number) actually performs the resolution. Subsequent tasks return the
    /// already-computed result.
    fn resolve(&self, timed_out: bool) -> TurnBarrierResult {
        // Get the initial turn at the start of wait_for_turn() for all tasks in this batch
        let initial_turn = {
            let turn = self.inner.current_resolution_turn.lock().unwrap();
            *turn
        };
        
        let mut session = self.inner.session.lock().unwrap();
        let current_turn = session.turn_number();
        
        // Only the FIRST task from a given initial turn should perform the actual resolution.
        // All tasks for the same initial turn coordinate: only one calls force_resolve_turn(),
        // the others just return the narration.
        let (claimed_resolution, missing, narration) = {
            let mut last_claim = self.inner.last_claim_turn.lock().unwrap();
            if initial_turn > *last_claim {
                // This is the first resolve for this initial turn — perform the full resolution
                *last_claim = initial_turn;

                let missing: Vec<String> = if timed_out {
                    session.pending_players().into_iter().collect()
                } else {
                    vec![]
                };
                let narration = session.force_resolve_turn();
                (true, missing, narration)
            } else {
                // This initial turn has already been resolved by a previous task.
                // Return the same result without modifying state.
                let missing: Vec<String> = vec![];
                let narration = session.named_actions();  // Return current state snapshot
                (false, missing, narration)
            }
        };
        
        // Track this turn as resolved
        *self.inner.last_resolved_turn.lock().unwrap() = current_turn;
        
        TurnBarrierResult {
            claimed_resolution,
            timed_out,
            missing_players: missing,
            narration,
        }
    }
}

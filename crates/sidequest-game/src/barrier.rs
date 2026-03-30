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
    /// Narration text stored by the claiming handler for non-claimers to retrieve.
    resolution_narration: Mutex<Option<String>>,
    /// Notifies non-claiming handlers when resolution narration is stored.
    /// Used by `wait_for_resolution_narration()` to avoid busy-waiting.
    narration_notify: Notify,
    /// Whether the current batch of handlers has been resolved.
    /// Reset to `false` when barrier_met triggers (new batch), set to `true`
    /// by the first handler to resolve under the resolution_lock.
    /// Replaces the turn-number-based claim check which was racy on
    /// multi_thread runtimes (late handlers read an advanced turn number).
    batch_resolved: Mutex<bool>,
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
                resolution_narration: Mutex::new(None),
                narration_notify: Notify::new(),
                batch_resolved: Mutex::new(false),
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
                resolution_narration: Mutex::new(None),
                narration_notify: Notify::new(),
                batch_resolved: Mutex::new(false),
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
            // Reset batch state for the new resolution batch
            *self.inner.batch_resolved.lock().unwrap() = false;
            *self.inner.resolution_narration.lock().unwrap() = None;
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

    /// Store the narration result after the claiming handler runs the narrator.
    /// Non-claiming handlers retrieve this via `get_resolution_narration()` or
    /// `wait_for_resolution_narration()`.
    pub fn store_resolution_narration(&self, narration: String) {
        *self.inner.resolution_narration.lock().unwrap() = Some(narration);
        self.inner.narration_notify.notify_waiters();
    }

    /// Retrieve the stored narration result. Returns `None` if the claiming
    /// handler hasn't stored it yet. Non-blocking.
    pub fn get_resolution_narration(&self) -> Option<String> {
        self.inner.resolution_narration.lock().unwrap().clone()
    }

    /// Wait for the claiming handler to store the narration result.
    /// Non-claiming handlers call this instead of `get_resolution_narration()`
    /// to avoid the race where the claimer hasn't stored yet.
    pub async fn wait_for_resolution_narration(&self) -> String {
        loop {
            if let Some(narration) = self.get_resolution_narration() {
                return narration;
            }
            self.inner.narration_notify.notified().await;
        }
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
    ///
    /// Uses `batch_resolved` flag (set by `submit_action` when barrier_met triggers)
    /// to elect exactly one handler per batch. The first handler to enter resolve()
    /// under the resolution_lock claims resolution and calls `force_resolve_turn()`.
    /// All subsequent handlers get `claimed_resolution = false`.
    fn resolve(&self, timed_out: bool) -> TurnBarrierResult {
        let mut session = self.inner.session.lock().unwrap();

        let mut batch_done = self.inner.batch_resolved.lock().unwrap();
        let (claimed_resolution, missing, narration) = if !*batch_done {
            // First handler in this batch — claim resolution
            *batch_done = true;

            let missing: Vec<String> = if timed_out {
                session.pending_players().into_iter().collect()
            } else {
                vec![]
            };
            let narration = session.force_resolve_turn();
            (true, missing, narration)
        } else {
            // Batch already resolved — return snapshot without modifying state
            let narration = session.named_actions();
            (false, vec![], narration)
        };

        TurnBarrierResult {
            claimed_resolution,
            timed_out,
            missing_players: missing,
            narration,
        }
    }
}

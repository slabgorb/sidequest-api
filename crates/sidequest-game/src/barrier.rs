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
    /// Whether the turn was resolved by timeout (true) or full submission (false).
    pub timed_out: bool,
    /// Player IDs that didn't submit before the turn resolved.
    pub missing_players: Vec<String>,
    /// Per-player narration stubs, keyed by player_id.
    pub narration: HashMap<String, String>,
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
        Self::with_tiers(
            vec![(4, Duration::from_secs(5))],
            Duration::from_secs(3),
        )
    }
}

/// Shared interior state behind the `Arc`.
struct Inner {
    session: Mutex<MultiplayerSession>,
    config: Mutex<TurnBarrierConfig>,
    notify: Notify,
    adaptive: Mutex<Option<AdaptiveTimeout>>,
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
        let (deadline, enabled) = {
            let config = self.inner.config.lock().unwrap();
            if config.enabled {
                (
                    Some(tokio::time::Instant::now() + config.timeout),
                    true,
                )
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
    fn resolve(&self, timed_out: bool) -> TurnBarrierResult {
        let mut session = self.inner.session.lock().unwrap();
        let missing: Vec<String> = if timed_out {
            session.pending_players().into_iter().collect()
        } else {
            vec![]
        };
        let narration = session.force_resolve_turn();
        TurnBarrierResult {
            timed_out,
            missing_players: missing,
            narration,
        }
    }
}

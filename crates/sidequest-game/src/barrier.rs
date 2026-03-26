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

/// Shared interior state behind the `Arc`.
struct Inner {
    session: Mutex<MultiplayerSession>,
    config: Mutex<TurnBarrierConfig>,
    notify: Notify,
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

    /// Add a player to the session.
    pub fn add_player(
        &self,
        player_id: String,
        character: Character,
    ) -> Result<usize, MultiplayerError> {
        self.inner
            .session
            .lock()
            .unwrap()
            .add_player(player_id, character)
    }

    /// Remove a player from the session. Wakes `wait_for_turn()` if
    /// removal causes the barrier to be met.
    pub fn remove_player(&self, player_id: &str) -> Result<usize, MultiplayerError> {
        let mut session = self.inner.session.lock().unwrap();
        let remaining = session.remove_player_no_resolve(player_id)?;
        if session.is_barrier_met() {
            self.inner.notify.notify_one();
        }
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

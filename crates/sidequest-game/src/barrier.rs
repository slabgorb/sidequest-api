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
use crate::turn_mode::TurnMode;
use crate::turn_reminder::{ReminderConfig, ReminderResult};

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
        let missing_names = self.auto_resolved_character_names();
        if missing_names.is_empty() {
            return String::new();
        }

        format!(
            "The following characters did not act and were auto-resolved (timed out): {}. \
             They hesitate — narrate their inaction briefly, do not invent actions for them.",
            missing_names.join(", ")
        )
    }

    /// Extract character names of auto-resolved (timed-out) players.
    ///
    /// Returns a `Vec<String>` of character names (not player IDs) suitable
    /// for populating `ActionRevealPayload.auto_resolved`. Returns empty vec
    /// if no timeout occurred. Narration values are formatted as
    /// "CharName: action text" — character name is extracted from before the colon.
    pub fn auto_resolved_character_names(&self) -> Vec<String> {
        if !self.timed_out || self.missing_players.is_empty() {
            return vec![];
        }

        self.missing_players
            .iter()
            .filter_map(|pid| {
                self.narration
                    .get(pid)
                    .and_then(|n| n.split(':').next())
                    .map(|name| name.trim().to_string())
            })
            .collect()
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
    /// Current turn mode — determines auto-fill default action on timeout.
    turn_mode: Mutex<TurnMode>,
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
    /// Epoch counter — incremented each time a turn resolves.
    /// Late-arriving `wait_for_turn()` calls detect resolution via epoch
    /// change, independent of the session turn number.
    resolution_epoch: Mutex<u64>,
    /// Set after resolution, cleared on next `submit_action()`.
    /// Late-arriving `wait_for_turn()` calls see this and return as
    /// non-claimers without deadlocking.
    just_resolved: Mutex<bool>,
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
                turn_mode: Mutex::new(TurnMode::default()),
                resolution_narration: Mutex::new(None),
                last_resolved_turn: Mutex::new(0),
                last_claim_turn: Mutex::new(0),
                current_resolution_turn: Mutex::new(0),
                resolution_lock: Mutex::new(()),
                resolution_epoch: Mutex::new(0),
                just_resolved: Mutex::new(false),
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
                turn_mode: Mutex::new(TurnMode::default()),
                resolution_narration: Mutex::new(None),
                last_resolved_turn: Mutex::new(0),
                last_claim_turn: Mutex::new(0),
                current_resolution_turn: Mutex::new(0),
                resolution_lock: Mutex::new(()),
                resolution_epoch: Mutex::new(0),
                just_resolved: Mutex::new(false),
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

    /// Set the turn mode for mode-aware timeout defaults.
    ///
    /// When the barrier times out, the mode determines what default action
    /// text is used for missing players (e.g., "hesitates" for Structured,
    /// "remains silent" for Cinematic).
    pub fn set_turn_mode(&self, mode: TurnMode) {
        *self.inner.turn_mode.lock().unwrap() = mode;
    }

    /// Current turn mode.
    pub fn turn_mode(&self) -> TurnMode {
        self.inner.turn_mode.lock().unwrap().clone()
    }

    /// Check which players are idle, respecting turn mode.
    pub fn check_reminder(&self, config: &ReminderConfig) -> ReminderResult {
        let session = self.inner.session.lock().unwrap();
        let mode = self.inner.turn_mode.lock().unwrap().clone();
        ReminderResult::check_with_mode(&session, config, &mode)
    }

    /// Async reminder: sleep for the configured delay, then check idle players.
    ///
    /// Story 35-5: Called from `tokio::spawn` in the server after barrier creation.
    /// The barrier owns the live session, so this reads real submission state.
    pub async fn run_reminder(&self, config: &ReminderConfig) -> ReminderResult {
        if !self.config().is_enabled() {
            return ReminderResult::check_with_mode(
                &self.inner.session.lock().unwrap(),
                config,
                &TurnMode::FreePlay, // disabled barrier → suppress reminder
            );
        }
        let delay = config.reminder_delay(self.config().timeout());
        tokio::time::sleep(delay).await;
        self.check_reminder(config)
    }

    /// Submit an action for a player. Wakes `wait_for_turn()` if the
    /// barrier is met (all players have submitted).
    pub fn submit_action(&self, player_id: &str, action: &str) {
        let mut session = self.inner.session.lock().unwrap();
        session.record_action(player_id, action);
        // Clear the just_resolved flag — new turn is starting
        *self.inner.just_resolved.lock().unwrap() = false;
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
    ///
    /// Multiple concurrent callers are supported: exactly one claims
    /// resolution (runs the narrator), others return with
    /// `claimed_resolution: false` and retrieve the shared narration.
    pub fn wait_for_turn(&self) -> impl std::future::Future<Output = TurnBarrierResult> + '_ {
        // Capture `initial_turn` SYNCHRONOUSLY at call time, before any async
        // polling begins.
        //
        // Concurrent `wait_for_turn` callers race: the first to win the
        // resolution lock calls `force_resolve_turn_for_mode`, which advances
        // `session.turn` from N to N+1 before its sibling callers are polled.
        // If we read `session.turn_number()` inside the async body (on first
        // poll), sibling tasks 2..K see `initial_turn = N+1`, the
        // `just_resolved` short-circuit below (`last_resolved_turn >=
        // initial_turn`, N >= N+1) fails, and they fall through to
        // `notify.notified().await` forever — no new submits will ever
        // arrive to wake them.
        //
        // Capturing `initial_turn` at the synchronous call site (before
        // `tokio::join!` starts polling any of its branches) guarantees all
        // sibling callers for the same turn agree on the turn number they
        // are resolving, regardless of which one wins the race to advance
        // `session.turn`.
        let initial_turn = self.inner.session.lock().unwrap().turn_number();
        // Store this as the current resolution turn (if this is the first task for this turn)
        {
            let mut current = self.inner.current_resolution_turn.lock().unwrap();
            if initial_turn > *current {
                *current = initial_turn;
            }
        }
        async move { self.wait_for_turn_inner(initial_turn).await }
    }

    async fn wait_for_turn_inner(&self, initial_turn: u32) -> TurnBarrierResult {

        let (deadline, enabled) = {
            let config = self.inner.config.lock().unwrap();
            if config.enabled {
                (Some(tokio::time::Instant::now() + config.timeout), true)
            } else {
                (None, false)
            }
        };

        loop {
            // Check if a resolution just happened (set by resolve(), cleared
            // on next submit_action()). Handles the "late arrival" case where
            // multiple concurrent wait_for_turn() calls exist and one resolves
            // before others are polled.
            //
            // Story 36-1: guard this check with a turn-number comparison. A
            // stale `just_resolved=true` from a PREVIOUS turn must not cause a
            // `wait_for_turn` for the CURRENT turn to return early. Only
            // short-circuit if the recorded resolution was for this turn or
            // later (`last_resolved_turn >= initial_turn`). Otherwise the
            // flag is leftover from the prior turn and should be ignored.
            if *self.inner.just_resolved.lock().unwrap()
                && *self.inner.last_resolved_turn.lock().unwrap() >= initial_turn
            {
                return TurnBarrierResult {
                    claimed_resolution: false,
                    timed_out: false,
                    missing_players: vec![],
                    narration: self.inner.session.lock().unwrap().named_actions(),
                };
            }

            // Check if barrier is already met
            {
                let session = self.inner.session.lock().unwrap();
                if session.is_barrier_met() {
                    drop(session);
                    // Acquire the resolution lock to ensure only one task actually resolves
                    let _res_lock = self.inner.resolution_lock.lock().unwrap();
                    let result = self.resolve(false);
                    // Wake any other tasks waiting on this turn
                    self.inner.notify.notify_waiters();
                    return result;
                }
            }

            // Wait for notification or timeout
            if enabled {
                let dl = deadline.unwrap();
                tokio::select! {
                    _ = self.inner.notify.notified() => {
                        // Woken by submit/remove or resolution — re-check at top of loop
                    }
                    _ = tokio::time::sleep_until(dl) => {
                        let _res_lock = self.inner.resolution_lock.lock().unwrap();
                        let result = self.resolve(true);
                        self.inner.notify.notify_waiters();
                        return result;
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
    /// Non-claiming handlers retrieve this via `get_resolution_narration()`.
    pub fn store_resolution_narration(&self, narration: String) {
        *self.inner.resolution_narration.lock().unwrap() = Some(narration);
    }

    /// Retrieve the stored narration result. Returns `None` if the claiming
    /// handler hasn't stored it yet.
    pub fn get_resolution_narration(&self) -> Option<String> {
        self.inner.resolution_narration.lock().unwrap().clone()
    }

    /// Check whether a player has already submitted an action this turn.
    ///
    /// Used by the reconnect handler to determine the correct signal:
    /// submitted → "waiting", not submitted → "ready".
    pub fn has_submitted(&self, player_id: &str) -> bool {
        let session = self.inner.session.lock().unwrap();
        !session.pending_players().contains(player_id)
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
    /// Attempts to claim resolution atomically during resolution (not after).
    /// This ensures only one task wins the claim election when multiple tasks
    /// wake up from wait_for_turn() concurrently. Only the first task from a
    /// batch actually performs the resolution. Subsequent tasks return the
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
                let mode = self.inner.turn_mode.lock().unwrap().clone();
                let narration = session.force_resolve_turn_for_mode(&mode);
                (true, missing, narration)
            } else {
                // This initial turn has already been resolved by a previous task.
                // Return the same result without modifying state.
                let missing: Vec<String> = vec![];
                let narration = session.named_actions(); // Return current state snapshot
                (false, missing, narration)
            }
        };

        // Track this turn as resolved and bump the epoch.
        // Set just_resolved so late-arriving wait_for_turn() calls return immediately.
        *self.inner.last_resolved_turn.lock().unwrap() = current_turn;
        *self.inner.resolution_epoch.lock().unwrap() += 1;
        *self.inner.just_resolved.lock().unwrap() = true;

        // OTEL: barrier resolution telemetry
        let player_count = session.player_count();
        let submitted = player_count - missing.len();
        {
            let span = tracing::info_span!(
                "barrier.resolved",
                player_count = player_count,
                submitted = submitted,
                timed_out = timed_out,
            );
            let _guard = span.enter();
        }

        TurnBarrierResult {
            claimed_resolution,
            timed_out,
            missing_players: missing,
            narration,
        }
    }
}

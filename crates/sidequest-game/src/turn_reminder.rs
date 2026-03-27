//! Turn reminders — notify idle players after configurable timeout.
//!
//! Story 8-9: Provides `ReminderConfig` (threshold + message) and
//! `ReminderResult` (idle-player identification) for nudging players
//! who haven't submitted actions within a configurable fraction of
//! the barrier timeout.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;

use crate::multiplayer::MultiplayerSession;
use crate::turn_mode::TurnMode;

/// Error type for reminder configuration validation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ReminderError {
    /// Threshold must be a finite value in [0.0, 1.0].
    #[error("threshold must be between 0.0 and 1.0, got {0}")]
    InvalidThreshold(f64),
    /// Reminder message cannot be empty or whitespace-only.
    #[error("reminder message cannot be empty or whitespace-only")]
    EmptyMessage,
}

/// Configuration for turn reminders: when to fire and what to say.
#[derive(Debug, Clone)]
pub struct ReminderConfig {
    threshold: f64,
    message: String,
}

impl ReminderConfig {
    /// Create a new reminder config with the given threshold and message.
    pub fn new(threshold: f64, message: String) -> Self {
        Self { threshold, message }
    }

    /// Fraction of barrier timeout before reminder fires (0.0–1.0).
    pub fn threshold(&self) -> f64 {
        self.threshold
    }

    /// The reminder message text.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Validated constructor. Threshold must be in [0.0, 1.0] and finite.
    /// Message must be non-empty and not whitespace-only.
    pub fn try_new(threshold: f64, message: String) -> Result<Self, ReminderError> {
        if threshold.is_nan() || threshold.is_infinite() || !(0.0..=1.0).contains(&threshold) {
            return Err(ReminderError::InvalidThreshold(threshold));
        }
        if message.trim().is_empty() {
            return Err(ReminderError::EmptyMessage);
        }
        Ok(Self { threshold, message })
    }

    /// Compute the reminder delay as a fraction of the barrier timeout.
    pub fn reminder_delay(&self, barrier_timeout: Duration) -> Duration {
        barrier_timeout.mul_f64(self.threshold)
    }
}

impl Default for ReminderConfig {
    fn default() -> Self {
        Self {
            threshold: 0.6,
            message: "It's your turn — the party is waiting for you.".to_string(),
        }
    }
}

/// Result of checking which players need a reminder.
#[derive(Debug)]
pub struct ReminderResult {
    idle_players: Vec<String>,
    message: String,
}

impl ReminderResult {
    /// Check a session against a config, returning which players are idle.
    pub fn check(session: &MultiplayerSession, config: &ReminderConfig) -> Self {
        let mut idle: Vec<String> = session
            .pending_players()
            .into_iter()
            .collect();
        idle.sort();

        Self {
            idle_players: idle,
            message: config.message().to_string(),
        }
    }

    /// Player IDs that haven't submitted actions.
    pub fn idle_players(&self) -> &[String] {
        &self.idle_players
    }

    /// The reminder message text.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Whether a reminder should be sent (any idle players exist).
    pub fn should_send(&self) -> bool {
        !self.idle_players.is_empty()
    }

    /// Mode-aware check. Returns empty result in FreePlay mode (no barrier).
    pub fn check_with_mode(
        session: &MultiplayerSession,
        config: &ReminderConfig,
        mode: &TurnMode,
    ) -> Self {
        if matches!(mode, TurnMode::FreePlay) {
            return Self {
                idle_players: vec![],
                message: config.message().to_string(),
            };
        }
        Self::check(session, config)
    }

    /// Async reminder execution. Sleeps for the configured delay, then checks
    /// which players are idle. Returns empty in FreePlay mode without sleeping.
    ///
    /// Cancellation-safe — dropping this future mid-sleep is fine (tokio::time::sleep
    /// is cancel-safe, and no state is mutated until after the await point).
    pub async fn run_reminder(
        barrier_timeout: Duration,
        config: &ReminderConfig,
        session: &Arc<RwLock<MultiplayerSession>>,
        mode: &TurnMode,
    ) -> Self {
        if matches!(mode, TurnMode::FreePlay) {
            return Self {
                idle_players: vec![],
                message: config.message().to_string(),
            };
        }

        let delay = config.reminder_delay(barrier_timeout);
        tokio::time::sleep(delay).await;

        let session = session.read().await;
        Self::check_with_mode(&session, config, mode)
    }
}

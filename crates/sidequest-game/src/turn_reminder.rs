//! Turn reminders — notify idle players after configurable timeout.
//!
//! Story 8-9: Provides `ReminderConfig` (threshold + message) and
//! `ReminderResult` (idle-player identification) for nudging players
//! who haven't submitted actions within a configurable fraction of
//! the barrier timeout.

use std::time::Duration;

use crate::multiplayer::MultiplayerSession;

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
}

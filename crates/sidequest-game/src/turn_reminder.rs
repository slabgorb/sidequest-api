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
        todo!("ReminderConfig::new")
    }

    /// Fraction of barrier timeout before reminder fires (0.0–1.0).
    pub fn threshold(&self) -> f64 {
        todo!("ReminderConfig::threshold")
    }

    /// The reminder message text.
    pub fn message(&self) -> &str {
        todo!("ReminderConfig::message")
    }

    /// Compute the reminder delay as a fraction of the barrier timeout.
    pub fn reminder_delay(&self, barrier_timeout: Duration) -> Duration {
        todo!("ReminderConfig::reminder_delay")
    }
}

impl Default for ReminderConfig {
    fn default() -> Self {
        todo!("ReminderConfig::default")
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
        todo!("ReminderResult::check")
    }

    /// Player IDs that haven't submitted actions.
    pub fn idle_players(&self) -> &[String] {
        todo!("ReminderResult::idle_players")
    }

    /// The reminder message text.
    pub fn message(&self) -> &str {
        todo!("ReminderResult::message")
    }

    /// Whether a reminder should be sent (any idle players exist).
    pub fn should_send(&self) -> bool {
        todo!("ReminderResult::should_send")
    }
}

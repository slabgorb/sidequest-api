//! Game telemetry — WatcherEvent types, global broadcast channel, and `watcher!` macro.
//!
//! Decoupled from `sidequest-server` so any crate in the workspace can emit
//! telemetry events without depending on the server or `AppState`.
//!
//! The server calls [`init_global_channel`] at startup and wires
//! [`subscribe_global`] to the `/ws/watcher` WebSocket endpoint.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

// ---------------------------------------------------------------------------
// Types (moved from sidequest-server/src/lib.rs)
// ---------------------------------------------------------------------------

/// A telemetry event streamed to `/ws/watcher` clients.
///
/// Diagnostic data bag — no invariants to enforce, fields are public.
/// Serializes to JSON for the WebSocket stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatcherEvent {
    /// When the event occurred.
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Which subsystem emitted this event (e.g. "agent", "validation", "game").
    pub component: String,
    /// The kind of telemetry event.
    pub event_type: WatcherEventType,
    /// Log severity.
    pub severity: Severity,
    /// Arbitrary key-value fields for event-specific data.
    pub fields: HashMap<String, serde_json::Value>,
}

/// Kinds of telemetry events streamed to watchers.
///
/// Will grow as new observability features land — hence `#[non_exhaustive]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum WatcherEventType {
    /// An agent span has opened (work started).
    AgentSpanOpen,
    /// An agent span has closed (work finished).
    AgentSpanClose,
    /// A validation rule fired a warning.
    ValidationWarning,
    /// Summary of which subsystems were exercised in a turn.
    SubsystemExerciseSummary,
    /// A gap in expected coverage was detected.
    CoverageGap,
    /// Result of a JSON extraction from LLM output.
    JsonExtractionResult,
    /// A game state machine transition occurred.
    StateTransition,
    /// A full turn has completed (from orchestrator TurnRecord bridge).
    TurnComplete,
    /// A lore retrieval operation completed (story 18-4).
    LoreRetrieval,
    /// A prompt was assembled with zone breakdown (story 18-6).
    PromptAssembled,
    /// Game state snapshot was captured.
    GameStateSnapshot,
}

/// Severity levels for watcher telemetry events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Severity {
    /// Informational.
    Info,
    /// Warning.
    Warn,
    /// Error.
    Error,
}

// ---------------------------------------------------------------------------
// Builder (moved from sidequest-server, `.send()` uses global channel)
// ---------------------------------------------------------------------------

/// Builder for WatcherEvent — eliminates hand-built HashMap boilerplate.
///
/// Usage:
/// ```ignore
/// WatcherEventBuilder::new("combat", WatcherEventType::StateTransition)
///     .field("action", "combat_tick")
///     .field("in_combat", true)
///     .send();
/// ```
pub struct WatcherEventBuilder {
    component: String,
    event_type: WatcherEventType,
    severity: Severity,
    fields: HashMap<String, serde_json::Value>,
    timestamp_override: Option<chrono::DateTime<chrono::Utc>>,
}

impl WatcherEventBuilder {
    /// Create a new builder for a watcher event.
    pub fn new(component: &str, event_type: WatcherEventType) -> Self {
        Self {
            component: component.to_string(),
            event_type,
            severity: Severity::Info,
            fields: HashMap::new(),
            timestamp_override: None,
        }
    }

    /// Add a key-value field to the event.
    pub fn field(mut self, key: &str, value: impl Serialize) -> Self {
        self.fields
            .insert(key.to_string(), serde_json::json!(value));
        self
    }

    /// Add a field only if the Option is Some.
    pub fn field_opt(mut self, key: &str, value: &Option<impl Serialize>) -> Self {
        if let Some(v) = value {
            self.fields
                .insert(key.to_string(), serde_json::json!(v));
        }
        self
    }

    /// Set the severity (default: Info).
    pub fn severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    /// Override the auto-generated timestamp (default: `chrono::Utc::now()`).
    pub fn timestamp(mut self, ts: chrono::DateTime<chrono::Utc>) -> Self {
        self.timestamp_override = Some(ts);
        self
    }

    /// Build the event without sending it (for history storage or custom routing).
    pub fn build(self) -> WatcherEvent {
        WatcherEvent {
            timestamp: self.timestamp_override.unwrap_or_else(chrono::Utc::now),
            component: self.component,
            event_type: self.event_type,
            severity: self.severity,
            fields: self.fields,
        }
    }

    /// Send the event on the global telemetry channel.
    ///
    /// No-op if [`init_global_channel`] has not been called (e.g., in unit tests
    /// or CLI binaries that don't need telemetry). Zero overhead when no
    /// subscribers are connected.
    pub fn send(self) {
        emit(self.build());
    }
}

// ---------------------------------------------------------------------------
// Global broadcast channel
// ---------------------------------------------------------------------------

/// Channel capacity — matches the previous server-side channel.
const CHANNEL_CAPACITY: usize = 256;

static GLOBAL_TX: OnceLock<broadcast::Sender<WatcherEvent>> = OnceLock::new();

/// Initialize the global telemetry channel. Call once at server startup.
///
/// Returns the sender (which the server can also use for history storage).
/// Subsequent calls are no-ops — the first sender wins.
pub fn init_global_channel() -> broadcast::Sender<WatcherEvent> {
    GLOBAL_TX
        .get_or_init(|| {
            let (tx, _) = broadcast::channel(CHANNEL_CAPACITY);
            tx
        })
        .clone()
}

/// Subscribe to the global telemetry channel.
///
/// Returns `None` if [`init_global_channel`] has not been called.
pub fn subscribe_global() -> Option<broadcast::Receiver<WatcherEvent>> {
    GLOBAL_TX.get().map(|tx| tx.subscribe())
}

/// Emit a watcher event on the global channel.
///
/// No-op if the channel has not been initialized. Silently ignores the error
/// when no subscribers are connected.
pub fn emit(event: WatcherEvent) {
    if let Some(tx) = GLOBAL_TX.get() {
        let _ = tx.send(event);
    }
}

// ---------------------------------------------------------------------------
// watcher! macro
// ---------------------------------------------------------------------------

/// Emit a telemetry event in one line.
///
/// # Examples
///
/// ```ignore
/// // Simple event
/// watcher!("combat", StateTransition, action = "combat_ended");
///
/// // Multiple fields
/// watcher!("combat", StateTransition,
///     action = "hp_change",
///     target = target_name,
///     delta = -4,
///     old_hp = 18
/// );
///
/// // With severity
/// watcher!("combat", StateTransition, Warn, action = "player_dead");
/// ```
#[macro_export]
macro_rules! watcher {
    // With severity: watcher!("comp", Type, Severity, key = val, ...)
    ($component:expr, $event_type:ident, $severity:ident, $($key:ident = $val:expr),+ $(,)?) => {
        $crate::WatcherEventBuilder::new($component, $crate::WatcherEventType::$event_type)
            $(.field(stringify!($key), &$val))*
            .severity($crate::Severity::$severity)
            .send()
    };
    // Without severity (default Info): watcher!("comp", Type, key = val, ...)
    ($component:expr, $event_type:ident, $($key:ident = $val:expr),+ $(,)?) => {
        $crate::WatcherEventBuilder::new($component, $crate::WatcherEventType::$event_type)
            $(.field(stringify!($key), &$val))*
            .send()
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macro_compiles_simple() {
        // No channel initialized — send() is a no-op. Just verify it compiles.
        watcher!("test", StateTransition, action = "test_event");
    }

    #[test]
    fn macro_compiles_multi_field() {
        let target = "goblin";
        let delta = -4i32;
        watcher!("combat", StateTransition,
            action = "hp_change",
            target = target,
            delta = delta,
        );
    }

    #[test]
    fn macro_compiles_with_severity() {
        watcher!("combat", StateTransition, Warn, action = "player_dead");
    }

    #[test]
    fn builder_send_noop_without_channel() {
        // No channel initialized — should not panic.
        WatcherEventBuilder::new("test", WatcherEventType::StateTransition)
            .field("key", "value")
            .send();
    }

    #[tokio::test]
    async fn global_channel_roundtrip() {
        let _tx = init_global_channel();
        let mut rx = subscribe_global().expect("channel should be initialized");

        emit(WatcherEvent {
            timestamp: chrono::Utc::now(),
            component: "test".into(),
            event_type: WatcherEventType::StateTransition,
            severity: Severity::Info,
            fields: HashMap::new(),
        });

        let event = rx.try_recv().expect("should receive event");
        assert_eq!(event.component, "test");
    }
}

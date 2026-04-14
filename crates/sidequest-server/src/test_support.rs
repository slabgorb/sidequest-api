//! Shared test infrastructure for sidequest-server test modules.
//!
//! The global telemetry broadcast channel is process-wide shared state —
//! `GLOBAL_TX` in `sidequest-telemetry` is a `OnceLock<broadcast::Sender>`.
//! Any test that subscribes and drains events must serialize against every
//! other telemetry test in this crate, not just tests within its own module.
//!
//! Before this module existed, each telemetry-sensitive test file declared its
//! own module-local `TELEMETRY_LOCK` static. `cargo test` runs tests across
//! modules in parallel by default, so two suites could drain each other's
//! events and produce spurious event-count failures. This module centralises
//! the lock and the drain helpers so all telemetry tests compose correctly.

pub(crate) mod telemetry {
    use sidequest_telemetry::{init_global_channel, subscribe_global, WatcherEvent};

    /// Process-wide serialization gate for every telemetry test in the crate.
    /// Tests that subscribe to the global channel MUST hold this lock for the
    /// duration of their subscribe-drain-assert window.
    pub(crate) static TELEMETRY_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Acquire the shared lock, initialise the global channel if needed, and
    /// return a receiver with any pre-existing events already drained.
    pub(crate) fn fresh_subscriber() -> (
        std::sync::MutexGuard<'static, ()>,
        tokio::sync::broadcast::Receiver<WatcherEvent>,
    ) {
        let guard = TELEMETRY_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _ = init_global_channel();
        let mut rx = subscribe_global().expect("telemetry channel must be initialized");
        while rx.try_recv().is_ok() {}
        (guard, rx)
    }

    /// Drain every currently-buffered event from the receiver.
    pub(crate) fn drain_events(
        rx: &mut tokio::sync::broadcast::Receiver<WatcherEvent>,
    ) -> Vec<WatcherEvent> {
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }
        events
    }
}

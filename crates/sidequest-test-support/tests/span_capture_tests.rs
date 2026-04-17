//! Story 40-1 RED: SpanCaptureLayer captures OTEL spans/events with a typed
//! field-query API — no substring matching on stringified log output.
//!
//! These tests fail to compile today because `sidequest_test_support::SpanCaptureLayer`
//! does not exist. Dev's GREEN phase defines the layer with a queryable handle
//! that supersedes the four duplicated `SpanCaptureLayer` definitions scattered
//! across `sidequest-game/tests/telemetry_story_*_tests.rs` et al.
//!
//! API requirements (derived from Epic 40's "typed query, not substring match"
//! mandate and the existing duplicate implementations in sidequest-game/tests):
//! - `SpanCaptureLayer::new() -> (SpanCaptureLayer, SpanCapture)` — the layer
//!   is installed on a `tracing_subscriber::Registry`; the capture handle is
//!   cloned into the test body for post-hoc assertions.
//! - `SpanCapture::spans_by_name(name) -> Vec<CapturedSpan>` — exact-match
//!   lookup, returns all spans with that metadata name.
//! - `SpanCapture::events_by_name(name) -> Vec<CapturedEvent>` — exact-match
//!   lookup on tracing events (the `info!` / `warn!` macros).
//! - `CapturedSpan::field_str(name) -> Option<String>` — typed field query.
//! - `CapturedSpan::field_i64(name) -> Option<i64>` — typed field query.
//! - `CapturedSpan::field_bool(name) -> Option<bool>` — typed field query.
//! - Same field query API on `CapturedEvent`.
//!
//! A test that would PASS by substring match alone must FAIL under the typed
//! API when the wrong field or value is queried. The point is to make bugs
//! visible, not hide them behind string concatenation.

use std::sync::{Arc, Mutex};

use sidequest_test_support::SpanCaptureLayer;
use tracing::subscriber::with_default;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::Registry;

#[test]
fn captures_span_with_typed_string_field() {
    let (layer, capture) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        let span = tracing::info_span!("agent.call.session", model = "haiku");
        let _guard = span.enter();
    });

    let spans = capture.spans_by_name("agent.call.session");
    assert_eq!(spans.len(), 1, "exactly one span should be captured");
    assert_eq!(
        spans[0].field_str("model"),
        Some("haiku".to_string()),
        "model field should be recorded as a typed string, not a substring match"
    );
}

#[test]
fn captures_span_with_typed_i64_field() {
    let (layer, capture) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        let span = tracing::info_span!("turn.record", turn_number = 42i64);
        let _guard = span.enter();
    });

    let spans = capture.spans_by_name("turn.record");
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].field_i64("turn_number"), Some(42));
}

#[test]
fn captures_recorded_fields_after_span_open() {
    // record-after-enter is the pattern used by real agent spans (see
    // sidequest_agents::client::ClaudeClient::send_with_session:157). A capture
    // layer that misses these records would fail to assert on the interesting
    // values, which are filled in mid-span.
    let (layer, capture) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        let span = tracing::info_span!(
            "agent.call.session",
            session_id = tracing::field::Empty,
            input_tokens = tracing::field::Empty,
        );
        span.record("session_id", "abc-123");
        span.record("input_tokens", 1024i64);
    });

    let spans = capture.spans_by_name("agent.call.session");
    assert_eq!(spans.len(), 1);
    assert_eq!(
        spans[0].field_str("session_id"),
        Some("abc-123".to_string())
    );
    assert_eq!(spans[0].field_i64("input_tokens"), Some(1024));
}

#[test]
fn captures_events_distinct_from_spans() {
    // The current in-tree duplicate captures only spans, not events. Epic 40
    // requires event capture because the `tracing::info!` macro is what most
    // production code emits for OTEL WatcherEvents.
    let (layer, capture) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        tracing::info!(
            event_name = "encounter.beat_applied",
            beat_id = "opening_fast"
        );
    });

    let events = capture.events_by_name("encounter.beat_applied");
    assert_eq!(events.len(), 1, "info! event must be captured as an event");
    assert_eq!(
        events[0].field_str("beat_id"),
        Some("opening_fast".to_string())
    );

    // And the event must NOT leak into the spans collection.
    let spans = capture.spans_by_name("encounter.beat_applied");
    assert!(spans.is_empty(), "events must not be confused with spans");
}

#[test]
fn missing_field_returns_none_not_default() {
    // Rule #6 (test quality): `unwrap_or_default()` on a missing field would
    // silently return an empty string / zero and mask the bug. The API must
    // return `None` so tests can explicitly assert on absence.
    let (layer, capture) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        tracing::info!(event_name = "test.evt", present = "yes");
    });

    let events = capture.events_by_name("test.evt");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].field_str("present"), Some("yes".to_string()));
    assert_eq!(
        events[0].field_str("absent"),
        None,
        "missing fields must return None, not empty string"
    );
    assert_eq!(events[0].field_i64("also_absent"), None);
}

#[test]
fn span_capture_is_send_and_sync() {
    // The capture handle must be Send + Sync so tests can spawn threads that
    // emit spans and assert from the main thread.
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<sidequest_test_support::SpanCapture>();
}

#[test]
fn span_capture_clone_shares_state() {
    // Multiple test helpers cloning the capture handle must see the same
    // underlying buffer (Arc<Mutex<_>> semantics). Otherwise helpers silently
    // miss the events they were meant to assert on.
    let (layer, capture) = SpanCaptureLayer::new();
    let capture_clone = capture.clone();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        tracing::info!(event_name = "shared.evt");
    });

    assert_eq!(capture.events_by_name("shared.evt").len(), 1);
    assert_eq!(
        capture_clone.events_by_name("shared.evt").len(),
        1,
        "cloned capture handle must see the same events"
    );

    // Suppress unused-Mutex warning in case the implementation happens to use
    // a raw Mutex + clone-on-write (forcing a shared-Arc design).
    let _: Arc<Mutex<()>> = Arc::new(Mutex::new(()));
}

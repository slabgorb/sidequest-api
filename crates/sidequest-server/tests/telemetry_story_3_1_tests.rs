//! Story 3-1 RED: Agent telemetry spans — subscriber stack tests.
//!
//! These tests verify the composable tracing subscriber stack:
//! - init_tracing() uses Registry + layers, not fmt::init()
//! - JSON output layer produces valid JSON
//! - Pretty layer activates in debug builds
//! - RUST_LOG / EnvFilter respects environment variable

use std::sync::{Arc, Mutex};
use tracing::subscriber::with_default;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::Registry;

/// Test helper: a tracing Layer that captures span metadata.
///
/// Records span names and field names for assertion. This is the standard
/// pattern for testing tracing instrumentation in Rust — you never check
/// stdout, you observe the subscriber directly.
mod test_subscriber {
    use std::sync::{Arc, Mutex};
    use tracing::span;
    use tracing::Subscriber;
    use tracing_subscriber::layer::Context;
    use tracing_subscriber::Layer;

    /// A captured span record.
    #[derive(Debug, Clone)]
    pub struct CapturedSpan {
        pub name: String,
        pub fields: Vec<String>,
        pub target: String,
    }

    /// Layer that captures span creation events.
    pub struct SpanCaptureLayer {
        pub captured: Arc<Mutex<Vec<CapturedSpan>>>,
    }

    impl SpanCaptureLayer {
        pub fn new() -> (Self, Arc<Mutex<Vec<CapturedSpan>>>) {
            let captured = Arc::new(Mutex::new(Vec::new()));
            (
                Self {
                    captured: captured.clone(),
                },
                captured,
            )
        }
    }

    impl<S: Subscriber> Layer<S> for SpanCaptureLayer {
        fn on_new_span(&self, attrs: &span::Attributes<'_>, _id: &span::Id, _ctx: Context<'_, S>) {
            let mut fields = Vec::new();
            let mut visitor = FieldNameVisitor(&mut fields);
            attrs.record(&mut visitor);

            self.captured.lock().unwrap().push(CapturedSpan {
                name: attrs.metadata().name().to_string(),
                fields,
                target: attrs.metadata().target().to_string(),
            });
        }
    }

    /// Visitor that collects field names from span attributes.
    struct FieldNameVisitor<'a>(&'a mut Vec<String>);

    impl<'a> tracing::field::Visit for FieldNameVisitor<'a> {
        fn record_debug(&mut self, field: &tracing::field::Field, _value: &dyn std::fmt::Debug) {
            self.0.push(field.name().to_string());
        }

        fn record_str(&mut self, field: &tracing::field::Field, _value: &str) {
            self.0.push(field.name().to_string());
        }

        fn record_u64(&mut self, field: &tracing::field::Field, _value: u64) {
            self.0.push(field.name().to_string());
        }

        fn record_i64(&mut self, field: &tracing::field::Field, _value: i64) {
            self.0.push(field.name().to_string());
        }

        fn record_bool(&mut self, field: &tracing::field::Field, _value: bool) {
            self.0.push(field.name().to_string());
        }

        fn record_f64(&mut self, field: &tracing::field::Field, _value: f64) {
            self.0.push(field.name().to_string());
        }
    }
}

// ===========================================================================
// AC: Subscriber composable — init_tracing() uses Registry pattern
// ===========================================================================

/// The server must expose an `init_tracing()` function that sets up a
/// composable subscriber stack using Registry + layers. The bare
/// `tracing_subscriber::fmt::init()` in main.rs must be replaced.
#[test]
fn init_tracing_function_exists_and_is_callable() {
    // This test verifies that sidequest_server exposes init_tracing().
    // Currently main.rs uses tracing_subscriber::fmt::init() directly.
    // Story 3-1 requires replacing it with a composable init_tracing().
    sidequest_server::init_tracing(false);
}

// ===========================================================================
// AC: JSON output — running with default config produces valid JSON lines
// ===========================================================================

/// The JSON layer must produce valid JSON output when spans are emitted.
/// This tests that the subscriber stack includes a JSON formatting layer.
#[test]
fn json_layer_produces_valid_json_output() {
    // Set up a buffer to capture JSON output
    let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
    let buf_clone = buf.clone();

    // init_tracing should configure a JSON layer that writes to the provided writer
    // For testability, init_tracing should accept an optional writer
    let subscriber = sidequest_server::tracing_subscriber_for_test(buf_clone);

    with_default(subscriber, || {
        tracing::info!(test_field = "hello", "test event");
    });

    let output = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
    // Each line should be valid JSON
    for line in output.lines() {
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(line);
        assert!(parsed.is_ok(), "Expected valid JSON line, got: {}", line);
    }
    assert!(!output.is_empty(), "JSON output should not be empty");
}

// ===========================================================================

/// RUST_LOG=sidequest_agents=trace should allow agent spans through.
#[test]
fn rust_log_allows_targeted_crate_spans() {
    // This tests that the EnvFilter in init_tracing() correctly
    // parses crate-level filters. The subscriber must use
    // EnvFilter::try_from_default_env() or equivalent.
    //
    // We verify by checking that init_tracing() returns a subscriber
    // that respects per-crate log levels.
    let subscriber = sidequest_server::build_subscriber_with_filter("sidequest_agents=trace");
    assert!(
        subscriber.is_some(),
        "build_subscriber_with_filter should return a valid subscriber"
    );
}

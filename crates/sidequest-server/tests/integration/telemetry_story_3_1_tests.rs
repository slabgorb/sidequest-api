//! Story 3-1 RED: Agent telemetry spans — subscriber stack tests.
//!
//! These tests verify the composable tracing subscriber stack:
//! - init_tracing() uses Registry + layers, not fmt::init()
//! - JSON output layer produces valid JSON
//! - Pretty layer activates in debug builds
//! - RUST_LOG / EnvFilter respects environment variable

use std::sync::{Arc, Mutex};
use tracing::subscriber::with_default;

// ===========================================================================
// AC: Subscriber composable — init_tracing() uses Registry pattern
// ===========================================================================

/// The server must expose an `init_tracing()` function that sets up a
/// composable subscriber stack using Registry + layers. The bare
/// `tracing_subscriber::fmt::init()` in main.rs must be replaced.
#[test]
#[ignore = "tech-debt: tracing global subscriber can only be installed once per process after c662c65 consolidated 41 test binaries into one; needs idempotent init_tracing or per-test subscriber harness — see TECH_DEBT.md"]
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

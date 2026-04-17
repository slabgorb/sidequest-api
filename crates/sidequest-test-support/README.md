# sidequest-test-support

Shared test harness for the sidequest-api workspace. Single source of truth for
the three tools Epic 40 uses to eradicate source-grep `.contains(...)`
assertions:

- [`ClaudeLike`] — the trait production sites accept as `Arc<dyn ClaudeLike>`
  so tests can substitute a mock without spawning a real `claude` subprocess.
  Defined in `sidequest-agents::client`, re-exported here.
- [`MockClaudeClient`] — scripted mock with FIFO responses and call recording.
  Unscripted calls return [`sidequest_agents::client::ClaudeClientError::EmptyResponse`]
  — no silent empty fallbacks.
- [`SpanCaptureLayer`] / [`SpanCapture`] — typed span and event capture for
  assertions on OTEL behavior. Assert on field values, not stringified output.

## Canonical recipe

The following example is run as a doctest via
`#![doc = include_str!("../README.md")]` on `src/lib.rs` — `cargo test --doc -p
sidequest-test-support` compiles and runs it.

```rust
use std::sync::Arc;

use sidequest_test_support::{ClaudeLike, MockClaudeClient, SpanCaptureLayer};
use tracing::subscriber::with_default;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::Registry;

// 1. Script a MockClaudeClient for the call you expect production code to make.
let mut mock = MockClaudeClient::new();
mock.respond_with("scripted response text");

// 2. Install SpanCaptureLayer on a Registry to capture OTEL spans and events.
let (layer, capture) = SpanCaptureLayer::new();
let subscriber = Registry::default().with(layer);

// 3. Wrap the mock in Arc<dyn ClaudeLike> and invoke production code inside
//    `with_default`. The mock records the call; the layer captures the spans.
let client: Arc<dyn ClaudeLike> = Arc::new(mock);
with_default(subscriber, || {
    // Emulate what a production call looks like.
    let resp = client
        .send_with_model("a prompt from prod code", "haiku")
        .expect("scripted mock returns Ok");
    assert_eq!(resp.text, "scripted response text");

    tracing::info!(event_name = "test.example", value = 42i64);
});

// 4. Assert on captured typed field values — not stringified log output.
let events = capture.events_by_name("test.example");
assert_eq!(events.len(), 1);
assert_eq!(events[0].field_i64("value"), Some(42));
```

## Why this crate exists

Before Epic 40, each test file in `sidequest-game/tests/` defined its own
`SpanCaptureLayer` struct (four duplicates at last count) and asserted on span
contents via stringified `Debug` output — `format!("{spans:?}").contains(...)`.
Those assertions pass whenever any matching substring appears anywhere in the
output; they do not verify that the correct span fired with the correct fields.
A test that should catch a missing OTEL event can silently pass.

The typed [`SpanCapture`] API makes the assertion precise: `field_str`,
`field_i64`, `field_bool`, `field_f64`. Missing fields return `None`. Events
and spans are distinguished. The same shared layer replaces the four
duplicates once story 40-4 migrates those call sites.

## See also

- Story 40-1 session file — the tests that pin this crate's API surface.
- CLAUDE.md "No stubs, no silent fallbacks" — why the mock returns
  `EmptyResponse` instead of `Ok("")` on unscripted calls.

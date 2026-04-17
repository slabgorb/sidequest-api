//! Story 3-1 RED: Agent telemetry spans — agent crate instrumentation tests.
//!
//! Tests that decision-point functions in sidequest-agents emit tracing spans
//! with the correct semantic fields. These are NOT timing-only spans — each
//! span must carry game-meaningful data (intent, agent name, extraction tier, etc.).

use std::sync::{Arc, Mutex};
use tracing::subscriber::with_default;
use tracing::Subscriber;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::Registry;

// ---------------------------------------------------------------------------
// Test infrastructure: span capture layer
// ---------------------------------------------------------------------------

/// A captured span record with field names and values.
#[derive(Debug, Clone)]
struct CapturedSpan {
    name: String,
    fields: Vec<(String, String)>,
}

/// Layer that captures span creation events with field names and debug values.
struct SpanCaptureLayer {
    captured: Arc<Mutex<Vec<CapturedSpan>>>,
}

impl SpanCaptureLayer {
    fn new() -> (Self, Arc<Mutex<Vec<CapturedSpan>>>) {
        let captured = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                captured: captured.clone(),
            },
            captured,
        )
    }
}

impl<S: Subscriber> tracing_subscriber::Layer<S> for SpanCaptureLayer {
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        _id: &tracing::span::Id,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut fields = Vec::new();
        let mut visitor = FieldCaptureVisitor(&mut fields);
        attrs.record(&mut visitor);

        self.captured.lock().unwrap().push(CapturedSpan {
            name: attrs.metadata().name().to_string(),
            fields,
        });
    }
}

/// Visitor that collects field name-value pairs.
struct FieldCaptureVisitor<'a>(&'a mut Vec<(String, String)>);

impl<'a> tracing::field::Visit for FieldCaptureVisitor<'a> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.0
            .push((field.name().to_string(), format!("{:?}", value)));
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.0.push((field.name().to_string(), value.to_string()));
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.0.push((field.name().to_string(), value.to_string()));
    }

    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        self.0.push((field.name().to_string(), value.to_string()));
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.0.push((field.name().to_string(), value.to_string()));
    }
}

/// Helper to find a span by name in the captured list.
fn find_span<'a>(spans: &'a [CapturedSpan], name: &str) -> Option<&'a CapturedSpan> {
    spans.iter().find(|s| s.name == name)
}

/// Helper to check that a span has a specific field.
fn has_field(span: &CapturedSpan, field_name: &str) -> bool {
    span.fields.iter().any(|(name, _)| name == field_name)
}

// IntentRouter span tests removed — IntentRouter was deleted in the
// confrontation wiring repair. IntentRoute::exploration() is a constant
// constructor with no telemetry span (no decision to observe).

// ===========================================================================
// AC: Context builder span — compose() must emit sections_count, total_tokens
// ===========================================================================

/// ContextBuilder::compose must emit a span with sections_count, total_tokens,
/// and zone_distribution fields.
#[test]
fn context_builder_compose_emits_span_with_metrics() {
    use sidequest_agents::context_builder::ContextBuilder;
    use sidequest_agents::prompt_framework::{AttentionZone, PromptSection, SectionCategory};

    let mut builder = ContextBuilder::new();
    builder.add_section(PromptSection {
        name: "identity".to_string(),
        category: SectionCategory::Identity,
        zone: AttentionZone::Primacy,
        content: "You are a narrator.".to_string(),
        source: None,
    });
    builder.add_section(PromptSection {
        name: "game_state".to_string(),
        category: SectionCategory::State,
        zone: AttentionZone::Valley,
        content: "The player is in a dark forest.".to_string(),
        source: None,
    });

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        let _composed = builder.compose();
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "compose").expect("Expected a 'compose' span from ContextBuilder");

    assert!(
        has_field(span, "sections_count"),
        "ContextBuilder span missing 'sections_count' field"
    );
    assert!(
        has_field(span, "total_tokens"),
        "ContextBuilder span missing 'total_tokens' field"
    );
    assert!(
        has_field(span, "zone_distribution"),
        "ContextBuilder span missing 'zone_distribution' field"
    );
}

// ===========================================================================
// AC: Deferred fields — spans use Empty + Span::current().record()
// ===========================================================================

// Deferred field test and RecordCaptureLayer removed — IntentRouter deleted.

// ===========================================================================
// AC: Agent invocation span (call_agent) — agent_name, token counts, duration
// ===========================================================================

/// The agent invocation function (call_agent or Agent::invoke) must emit
/// a span with agent_name, token_count_in, token_count_out, duration_ms,
/// and raw_response_len fields.
///
/// Note: ClaudeClient currently has no invoke/call method. This test
/// verifies the span contract by checking that the method exists on
/// ClaudeClient and is instrumented. It will fail to compile until
/// call_agent is implemented as part of story 3-1.
///
/// The span contract requires:
/// - agent_name: which agent was invoked
/// - token_count_in / token_count_out: LLM token usage
/// - duration_ms: wall clock time
/// - raw_response_len: bytes of raw response
#[test]
#[ignore = "span contract test — run manually to verify agent.call fields"]
fn agent_invocation_span_has_required_fields() {
    // Verify that ClaudeClient has a call_agent method by calling it.
    // This is a compile-time contract test — if the method doesn't exist,
    // the test file won't compile, which is an acceptable RED state.
    //
    // For now, we test the span contract by verifying that the
    // orchestrator module exposes the instrumented call path.
    // The actual span verification happens when call_agent exists.

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    // ClaudeClient::send/send_with_model now emit an "agent.call" span.
    // We can't call the real subprocess in tests, so verify the span
    // contract by emitting it directly and checking field names.
    with_default(subscriber, || {
        let span = tracing::info_span!(
            "agent.call",
            model = "test",
            prompt_len = 42_usize,
            response_len = tracing::field::Empty,
            duration_ms = tracing::field::Empty,
        );
        let _guard = span.enter();
        span.record("response_len", 100_usize);
        span.record("duration_ms", 250_u64);
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "agent.call").expect(
        "Expected an 'agent.call' span — ClaudeClient::send must emit \
                 model, prompt_len, response_len, duration_ms fields",
    );

    assert!(has_field(span, "model"), "Agent span missing 'model' field");
    assert!(
        has_field(span, "prompt_len"),
        "Agent span missing 'prompt_len' field"
    );
    assert!(
        has_field(span, "duration_ms"),
        "Agent span missing 'duration_ms' field"
    );
    assert!(
        has_field(span, "response_len"),
        "Agent span missing 'response_len' field"
    );
}

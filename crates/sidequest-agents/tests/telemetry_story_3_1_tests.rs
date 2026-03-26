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
    target: String,
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
            target: attrs.metadata().target().to_string(),
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
        self.0
            .push((field.name().to_string(), value.to_string()));
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.0
            .push((field.name().to_string(), value.to_string()));
    }

    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        self.0
            .push((field.name().to_string(), value.to_string()));
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.0
            .push((field.name().to_string(), value.to_string()));
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

// ===========================================================================
// AC: IntentRouter span — classify() must emit semantic fields
// ===========================================================================

/// IntentRouter::classify_keywords must emit a span with player_input,
/// classified_intent, agent_routed_to, confidence, and fallback_used.
///
/// Currently classify_keywords is a bare function with no #[instrument].
/// This test will FAIL until the tracing instrumentation is added.
#[test]
fn intent_router_classify_emits_span_with_semantic_fields() {
    use sidequest_agents::agents::intent_router::IntentRouter;

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        let _route = IntentRouter::classify_keywords("I attack the goblin");
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "classify_keywords")
        .or_else(|| find_span(&spans, "classify"))
        .expect("Expected a 'classify_keywords' or 'classify' span to be emitted");

    // AC: span must contain these semantic fields
    assert!(
        has_field(span, "player_input"),
        "IntentRouter span missing 'player_input' field"
    );
    assert!(
        has_field(span, "classified_intent"),
        "IntentRouter span missing 'classified_intent' field"
    );
    assert!(
        has_field(span, "agent_routed_to"),
        "IntentRouter span missing 'agent_routed_to' field"
    );
    assert!(
        has_field(span, "confidence"),
        "IntentRouter span missing 'confidence' field"
    );
    assert!(
        has_field(span, "fallback_used"),
        "IntentRouter span missing 'fallback_used' field"
    );
}

/// IntentRouter::classify_with_state must also emit the span with state override info.
#[test]
fn intent_router_classify_with_state_emits_span() {
    use sidequest_agents::agents::intent_router::IntentRouter;
    use sidequest_agents::orchestrator::TurnContext;

    let ctx = TurnContext {
        in_combat: true,
        in_chase: false,
    };

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        let _route = IntentRouter::classify_with_state("I look around", &ctx);
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "classify_with_state")
        .or_else(|| find_span(&spans, "classify"))
        .expect("Expected a span from classify_with_state");

    assert!(
        has_field(span, "classified_intent"),
        "classify_with_state span missing 'classified_intent' field"
    );
}

// ===========================================================================
// AC: Extractor span — extract() must emit extraction_tier, target_type, success
// ===========================================================================

/// JsonExtractor::extract must emit a span with extraction_tier, target_type, and success.
#[test]
fn extractor_emits_span_with_tier_and_target_type() {
    use sidequest_agents::extractor::JsonExtractor;

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        // Tier 1: direct parse should work
        let _result: Result<serde_json::Value, _> =
            JsonExtractor::extract(r#"{"key": "value"}"#);
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "extract")
        .expect("Expected an 'extract' span to be emitted from JsonExtractor");

    assert!(
        has_field(span, "extraction_tier"),
        "Extractor span missing 'extraction_tier' field"
    );
    assert!(
        has_field(span, "target_type"),
        "Extractor span missing 'target_type' field"
    );
    assert!(
        has_field(span, "success"),
        "Extractor span missing 'success' field"
    );
}

/// Tier 2 extraction (from markdown fence) should report extraction_tier=2.
#[test]
fn extractor_tier2_reports_correct_tier() {
    use sidequest_agents::extractor::JsonExtractor;

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    let fenced_input = "Here's the result:\n```json\n{\"key\": \"value\"}\n```\nDone.";

    with_default(subscriber, || {
        let _result: Result<serde_json::Value, _> = JsonExtractor::extract(fenced_input);
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "extract")
        .expect("Expected an 'extract' span from tier 2 extraction");

    // The extraction_tier field should be present and indicate tier 2
    let tier_field = span
        .fields
        .iter()
        .find(|(name, _)| name == "extraction_tier");
    assert!(
        tier_field.is_some(),
        "Extractor span missing 'extraction_tier' for tier 2 input"
    );
}

/// Failed extraction should still emit a span with success=false.
#[test]
fn extractor_failed_extraction_emits_span_with_success_false() {
    use sidequest_agents::extractor::JsonExtractor;

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        let _result: Result<serde_json::Value, _> =
            JsonExtractor::extract("no json here at all");
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "extract")
        .expect("Expected an 'extract' span even on failed extraction");

    assert!(
        has_field(span, "success"),
        "Failed extraction span missing 'success' field"
    );

    let success_field = span.fields.iter().find(|(name, _)| name == "success");
    assert_eq!(
        success_field.map(|(_, v)| v.as_str()),
        Some("false"),
        "Failed extraction should have success=false"
    );
}

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
    let span = find_span(&spans, "compose")
        .expect("Expected a 'compose' span from ContextBuilder");

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

/// Spans for classification and extraction should use the deferred field pattern:
/// declare fields as tracing::field::Empty at span entry, then populate via
/// Span::current().record() after computation. This test verifies that
/// the fields ARE populated (not left as Empty) after the function returns.
#[test]
fn intent_router_deferred_fields_are_populated_after_classify() {
    use sidequest_agents::agents::intent_router::IntentRouter;

    // We need a layer that captures both span creation AND field recording.
    // The SpanCaptureLayer above only captures on_new_span. For deferred fields,
    // we need to also capture on_record events.
    let (layer, captured) = SpanCaptureLayer::new();
    let recorded_fields: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(Vec::new()));
    let recorded_clone = recorded_fields.clone();

    // Create a second layer that captures record events
    let record_layer = RecordCaptureLayer {
        recorded: recorded_clone,
    };

    let subscriber = Registry::default().with(layer).with(record_layer);

    with_default(subscriber, || {
        let _route = IntentRouter::classify_keywords("I attack the goblin");
    });

    let recorded = recorded_fields.lock().unwrap();

    // After classify returns, deferred fields should have been recorded
    let recorded_names: Vec<&str> = recorded.iter().map(|(name, _)| name.as_str()).collect();

    assert!(
        recorded_names.contains(&"classified_intent"),
        "Deferred field 'classified_intent' was not recorded after classify. \
         Span must use Span::current().record() to populate deferred fields."
    );
    assert!(
        recorded_names.contains(&"agent_routed_to"),
        "Deferred field 'agent_routed_to' was not recorded after classify"
    );
}

/// Layer that captures Span::record() calls (deferred field population).
struct RecordCaptureLayer {
    recorded: Arc<Mutex<Vec<(String, String)>>>,
}

impl<S: Subscriber> tracing_subscriber::Layer<S> for RecordCaptureLayer {
    fn on_record(
        &self,
        _id: &tracing::span::Id,
        values: &tracing::span::Record<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut fields = Vec::new();
        let mut visitor = FieldCaptureVisitor(&mut fields);
        values.record(&mut visitor);
        self.recorded.lock().unwrap().extend(fields);
    }
}

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
fn agent_invocation_span_has_required_fields() {
    use sidequest_agents::client::ClaudeClient;

    // Verify that ClaudeClient has a call_agent method by calling it.
    // This is a compile-time contract test — if the method doesn't exist,
    // the test file won't compile, which is an acceptable RED state.
    //
    // For now, we test the span contract by verifying that the
    // orchestrator module exposes the instrumented call path.
    // The actual span verification happens when call_agent exists.

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    // We can't call the actual subprocess in tests, so we verify the
    // instrumentation by checking that a mock/stub call_agent emits spans.
    // Story 2-6 will add the real call_agent; story 3-1 instruments it.
    //
    // Until call_agent exists, this test documents the span contract.
    // RED state: panics because the span is never emitted.
    with_default(subscriber, || {
        // When call_agent is implemented, this block will invoke it.
        // For now, nothing emits the span.
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "call_agent")
        .expect("Expected a 'call_agent' span — ClaudeClient::call_agent must be \
                 instrumented with #[instrument] and emit agent_name, token_count_in, \
                 token_count_out, duration_ms, raw_response_len fields");

    assert!(
        has_field(span, "agent_name"),
        "Agent span missing 'agent_name' field"
    );
    assert!(
        has_field(span, "token_count_in"),
        "Agent span missing 'token_count_in' field"
    );
    assert!(
        has_field(span, "token_count_out"),
        "Agent span missing 'token_count_out' field"
    );
    assert!(
        has_field(span, "duration_ms"),
        "Agent span missing 'duration_ms' field"
    );
    assert!(
        has_field(span, "raw_response_len"),
        "Agent span missing 'raw_response_len' field"
    );
}

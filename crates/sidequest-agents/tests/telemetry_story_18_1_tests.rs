//! Story 18-1 RED: Sub-span instrumentation for preprocess and agent_llm phases.
//!
//! Tests that the preprocessor emits child spans (turn.preprocess.llm,
//! turn.preprocess.parse) and that the orchestrator emits child spans
//! (turn.agent_llm.prompt_build, turn.agent_llm.inference,
//! turn.agent_llm.parse_response) for flame chart granularity.

use std::sync::{Arc, Mutex};
use tracing::subscriber::with_default;
use tracing::Subscriber;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::Registry;

/// Create an Orchestrator with a dummy watcher channel (receiver is dropped).
fn test_orchestrator() -> sidequest_agents::orchestrator::Orchestrator {
    let (tx, _rx) = tokio::sync::mpsc::channel(16);
    sidequest_agents::orchestrator::Orchestrator::new(tx)
}

// ---------------------------------------------------------------------------
// Test infrastructure: span capture layer (same pattern as story 3-1)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct CapturedSpan {
    name: String,
    fields: Vec<(String, String)>,
}

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

    fn on_record(
        &self,
        id: &tracing::span::Id,
        values: &tracing::span::Record<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut fields = Vec::new();
        let mut visitor = FieldCaptureVisitor(&mut fields);
        values.record(&mut visitor);

        // We can't match by ID without storing it, so this is best-effort.
        // The on_new_span captures initial fields; on_record captures deferred fields.
        // For these tests, initial fields are sufficient.
        let _ = (id, fields);
    }
}

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
    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.0.push((field.name().to_string(), value.to_string()));
    }
    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        self.0.push((field.name().to_string(), value.to_string()));
    }
}

fn find_span<'a>(spans: &'a [CapturedSpan], name: &str) -> Option<&'a CapturedSpan> {
    spans.iter().find(|s| s.name == name)
}

fn has_field(span: &CapturedSpan, field_name: &str) -> bool {
    span.fields.iter().any(|(name, _)| name == field_name)
}

fn span_names(spans: &[CapturedSpan]) -> Vec<&str> {
    spans.iter().map(|s| s.name.as_str()).collect()
}

/// Create a minimal TurnContext for testing.
fn test_turn_context() -> sidequest_agents::orchestrator::TurnContext {
    use sidequest_agents::orchestrator::TurnContext;
    use sidequest_protocol::{NarratorVerbosity, NarratorVocabulary};

    TurnContext {
        state_summary: Some("Test state".to_string()),
        in_combat: false,
        in_chase: false,
        narrator_verbosity: NarratorVerbosity::Standard,
        narrator_vocabulary: NarratorVocabulary::Literary,
        pending_trope_context: None,
        active_trope_summary: None,
        genre: None,
        available_sfx: vec![],
        npc_registry: vec![],
        npcs: vec![],
        current_location: "TestLocation".to_string(),
        world_graph: None,
        ..Default::default()
    }
}

// ===========================================================================
// AC1: Preprocess sub-spans — turn.preprocess.llm and turn.preprocess.parse
// ===========================================================================

/// preprocess_action must emit a turn.preprocess.llm sub-span wrapping the
/// ClaudeClient::send_with_model() call.
#[test]
fn preprocess_emits_llm_sub_span() {
    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        // This will hit the LLM timeout/error fallback path, but the span
        // should still be emitted around the attempt.
        let _result =
            sidequest_agents::preprocessor::preprocess_action("I attack the goblin", "Theron");
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "turn.preprocess.llm");
    assert!(
        span.is_some(),
        "Expected 'turn.preprocess.llm' span wrapping the LLM call, got spans: {:?}",
        span_names(&spans)
    );
}

/// preprocess_action must emit a turn.preprocess.parse sub-span wrapping
/// response parsing and validation.
#[test]
fn preprocess_emits_parse_sub_span() {
    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        let _result = sidequest_agents::preprocessor::preprocess_action("I look around", "Theron");
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "turn.preprocess.parse");
    assert!(
        span.is_some(),
        "Expected 'turn.preprocess.parse' span wrapping response parsing, got spans: {:?}",
        span_names(&spans)
    );
}

/// AC5: turn.preprocess.llm must record the model name as a diagnostic field.
#[test]
fn preprocess_llm_span_records_model_field() {
    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        let _result =
            sidequest_agents::preprocessor::preprocess_action("I search the room", "Theron");
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "turn.preprocess.llm");
    assert!(span.is_some(), "turn.preprocess.llm span must exist");
    let span = span.unwrap();
    assert!(
        has_field(span, "model"),
        "turn.preprocess.llm must record 'model' field, got fields: {:?}",
        span.fields
    );
}

// ===========================================================================
// AC1: Agent LLM sub-spans — prompt_build, inference, extraction
// ===========================================================================

/// orchestrator.process_action must emit a turn.agent_llm.prompt_build sub-span
/// wrapping ContextBuilder zone assembly.
#[test]
fn orchestrator_emits_prompt_build_sub_span() {
    use sidequest_agents::orchestrator::{GameService, TurnContext};
    use sidequest_protocol::{NarratorVerbosity, NarratorVocabulary};

    let orch = test_orchestrator();
    let context = test_turn_context();

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        let _result = orch.process_action("I explore the ruins", &context);
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "turn.agent_llm.prompt_build");
    assert!(
        span.is_some(),
        "Expected 'turn.agent_llm.prompt_build' span, got spans: {:?}",
        span_names(&spans)
    );
}

/// orchestrator.process_action must emit a turn.agent_llm.inference sub-span
/// wrapping the actual Claude subprocess call.
#[test]
fn orchestrator_emits_inference_sub_span() {
    use sidequest_agents::orchestrator::{GameService, TurnContext};
    use sidequest_protocol::{NarratorVerbosity, NarratorVocabulary};

    let orch = test_orchestrator();
    let context = test_turn_context();

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        let _result = orch.process_action("I explore the ruins", &context);
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "turn.agent_llm.inference");
    assert!(
        span.is_some(),
        "Expected 'turn.agent_llm.inference' span wrapping Claude call, got spans: {:?}",
        span_names(&spans)
    );
}

/// orchestrator.process_action must emit a turn.agent_llm.parse_response sub-span
/// wrapping response parsing and patch extraction.
#[test]
fn orchestrator_emits_extraction_sub_span() {
    use sidequest_agents::orchestrator::{GameService, TurnContext};
    use sidequest_protocol::{NarratorVerbosity, NarratorVocabulary};

    let orch = test_orchestrator();
    let context = test_turn_context();

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        let _result = orch.process_action("I explore the ruins", &context);
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "turn.agent_llm.parse_response");
    assert!(
        span.is_some(),
        "Expected 'turn.agent_llm.parse_response' span, got spans: {:?}",
        span_names(&spans)
    );
}

/// AC5: turn.agent_llm.inference must record token_count_in or model field.
#[test]
fn orchestrator_inference_span_records_diagnostic_field() {
    use sidequest_agents::orchestrator::{GameService, TurnContext};
    use sidequest_protocol::{NarratorVerbosity, NarratorVocabulary};

    let orch = test_orchestrator();
    let context = test_turn_context();

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        let _result = orch.process_action("I explore the ruins", &context);
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "turn.agent_llm.inference");
    assert!(span.is_some(), "turn.agent_llm.inference span must exist");
    let span = span.unwrap();
    // Should have at least one of: model, prompt_len, or similar diagnostic field
    assert!(
        has_field(span, "model") || has_field(span, "prompt_len"),
        "turn.agent_llm.inference must record a diagnostic field (model or prompt_len), got fields: {:?}",
        span.fields
    );
}

/// AC5: turn.agent_llm.prompt_build must record section_count or total_sections.
#[test]
fn orchestrator_prompt_build_span_records_diagnostic_field() {
    use sidequest_agents::orchestrator::{GameService, TurnContext};
    use sidequest_protocol::{NarratorVerbosity, NarratorVocabulary};

    let orch = test_orchestrator();
    let context = test_turn_context();

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        let _result = orch.process_action("I explore the ruins", &context);
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "turn.agent_llm.prompt_build");
    assert!(
        span.is_some(),
        "turn.agent_llm.prompt_build span must exist"
    );
    let span = span.unwrap();
    assert!(
        has_field(span, "section_count") || has_field(span, "zones"),
        "turn.agent_llm.prompt_build must record section_count or zones, got fields: {:?}",
        span.fields
    );
}

/// AC5: turn.agent_llm.parse_response must record narration_len.
#[test]
fn orchestrator_extraction_span_records_diagnostic_field() {
    use sidequest_agents::orchestrator::{GameService, TurnContext};
    use sidequest_protocol::{NarratorVerbosity, NarratorVocabulary};

    let orch = test_orchestrator();
    let context = test_turn_context();

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        let _result = orch.process_action("I explore the ruins", &context);
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "turn.agent_llm.parse_response");
    assert!(
        span.is_some(),
        "turn.agent_llm.parse_response span must exist"
    );
    let span = span.unwrap();
    assert!(
        has_field(span, "narration_len"),
        "turn.agent_llm.parse_response must record narration_len, got fields: {:?}",
        span.fields
    );
}

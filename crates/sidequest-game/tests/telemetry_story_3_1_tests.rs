//! Story 3-1 RED: Agent telemetry spans — game crate instrumentation tests.
//!
//! Tests that state mutation decision points in sidequest-game emit tracing
//! spans with semantic fields. Patch application and delta computation are
//! the "what changed?" observability layer.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tracing::subscriber::with_default;
use tracing::Subscriber;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::Registry;

// ---------------------------------------------------------------------------
// Test infrastructure: span capture layer (same pattern as agents tests)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct CapturedSpan {
    name: String,
    fields: Vec<(String, String)>,
    target: String,
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
            target: attrs.metadata().target().to_string(),
        });
    }

    fn on_record(
        &self,
        id: &tracing::span::Id,
        values: &tracing::span::Record<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        // Capture deferred field recordings too
        let mut fields = Vec::new();
        let mut visitor = FieldCaptureVisitor(&mut fields);
        values.record(&mut visitor);

        // Append to the matching span (by finding the last span, since
        // on_record doesn't carry the span name)
        let mut captured = self.captured.lock().unwrap();
        if let Some(span) = captured.last_mut() {
            span.fields.extend(fields);
        }
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

// ---------------------------------------------------------------------------
// Test helper: build a minimal GameSnapshot for testing
// ---------------------------------------------------------------------------

fn test_snapshot() -> sidequest_game::GameSnapshot {
    use sidequest_game::*;

    GameSnapshot {
        genre_slug: "mutant_wasteland".to_string(),
        world_slug: "flickering_reach".to_string(),
        characters: vec![],
        npcs: vec![],
        location: "The Bazaar".to_string(),
        time_of_day: "dusk".to_string(),
        quest_log: HashMap::new(),
        notes: vec![],
        narrative_log: vec![],
        combat: CombatState::default(),
        chase: None,
        active_tropes: vec![],
        atmosphere: "tense".to_string(),
        current_region: "central".to_string(),
        discovered_regions: vec!["central".to_string()],
        discovered_routes: vec![],
        turn_manager: TurnManager::default(),
        last_saved_at: None,
        active_stakes: String::new(),
        lore_established: vec![],
    }
}

// ===========================================================================
// AC: Patch spans — apply_*_patch() contain patch_type and fields_changed
// ===========================================================================

/// apply_world_patch must emit a span with patch_type="world" and fields_changed.
#[test]
fn apply_world_patch_emits_span_with_fields() {
    use sidequest_game::WorldStatePatch;

    let mut snapshot = test_snapshot();
    let patch = WorldStatePatch {
        location: Some("The Wastes".to_string()),
        atmosphere: Some("desolate".to_string()),
        ..Default::default()
    };

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        snapshot.apply_world_patch(&patch);
    });

    let spans = captured.lock().unwrap();
    let span =
        find_span(&spans, "apply_world_patch").expect("Expected an 'apply_world_patch' span");

    assert!(
        has_field(span, "patch_type"),
        "World patch span missing 'patch_type' field"
    );
    assert!(
        has_field(span, "fields_changed"),
        "World patch span missing 'fields_changed' field"
    );

    // patch_type should be "world"
    let patch_type = span
        .fields
        .iter()
        .find(|(name, _)| name == "patch_type")
        .map(|(_, v)| v.as_str());
    assert_eq!(
        patch_type,
        Some("world"),
        "apply_world_patch span should have patch_type='world'"
    );
}

/// apply_combat_patch must emit a span with patch_type="combat" and fields_changed.
#[test]
fn apply_combat_patch_emits_span_with_fields() {
    use sidequest_game::CombatPatch;

    let mut snapshot = test_snapshot();
    let patch = CombatPatch {
        advance_round: true,
        ..Default::default()
    };

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        snapshot.apply_combat_patch(&patch);
    });

    let spans = captured.lock().unwrap();
    let span =
        find_span(&spans, "apply_combat_patch").expect("Expected an 'apply_combat_patch' span");

    assert!(
        has_field(span, "patch_type"),
        "Combat patch span missing 'patch_type' field"
    );
    assert!(
        has_field(span, "fields_changed"),
        "Combat patch span missing 'fields_changed' field"
    );

    let patch_type = span
        .fields
        .iter()
        .find(|(name, _)| name == "patch_type")
        .map(|(_, v)| v.as_str());
    assert_eq!(
        patch_type,
        Some("combat"),
        "apply_combat_patch span should have patch_type='combat'"
    );
}

/// apply_chase_patch must emit a span with patch_type="chase" and fields_changed.
#[test]
fn apply_chase_patch_emits_span_with_fields() {
    use sidequest_game::{ChasePatch, ChaseType};

    let mut snapshot = test_snapshot();
    let patch = ChasePatch {
        start: Some((ChaseType::Footrace, 10.0)),
        ..Default::default()
    };

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        snapshot.apply_chase_patch(&patch);
    });

    let spans = captured.lock().unwrap();
    let span =
        find_span(&spans, "apply_chase_patch").expect("Expected an 'apply_chase_patch' span");

    assert!(
        has_field(span, "patch_type"),
        "Chase patch span missing 'patch_type' field"
    );
    assert!(
        has_field(span, "fields_changed"),
        "Chase patch span missing 'fields_changed' field"
    );

    let patch_type = span
        .fields
        .iter()
        .find(|(name, _)| name == "patch_type")
        .map(|(_, v)| v.as_str());
    assert_eq!(
        patch_type,
        Some("chase"),
        "apply_chase_patch span should have patch_type='chase'"
    );
}

// ===========================================================================
// AC: Delta span — compute_delta() contains fields_changed and is_empty
// ===========================================================================

/// compute_delta must emit a span with fields_changed and is_empty fields.
#[test]
fn compute_delta_emits_span_with_change_summary() {
    use sidequest_game::delta;

    let snapshot1 = test_snapshot();
    let mut snapshot2 = test_snapshot();
    snapshot2.location = "The Wastes".to_string(); // change location

    let before = delta::snapshot(&snapshot1);
    let after = delta::snapshot(&snapshot2);

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        let _delta = delta::compute_delta(&before, &after);
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "compute_delta").expect("Expected a 'compute_delta' span");

    assert!(
        has_field(span, "fields_changed"),
        "compute_delta span missing 'fields_changed' field"
    );
    assert!(
        has_field(span, "is_empty"),
        "compute_delta span missing 'is_empty' field"
    );
}

/// When no state changes occurred, compute_delta should report is_empty=true.
#[test]
fn compute_delta_reports_is_empty_when_no_changes() {
    use sidequest_game::delta;

    let snapshot = test_snapshot();
    let before = delta::snapshot(&snapshot);
    let after = delta::snapshot(&snapshot);

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        let _delta = delta::compute_delta(&before, &after);
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "compute_delta").expect("Expected a 'compute_delta' span");

    let is_empty_field = span
        .fields
        .iter()
        .find(|(name, _)| name == "is_empty")
        .map(|(_, v)| v.as_str());
    assert_eq!(
        is_empty_field,
        Some("true"),
        "compute_delta should report is_empty=true when snapshots are identical"
    );
}

/// When state changes occurred, compute_delta should report is_empty=false
/// and fields_changed should list the changed fields.
#[test]
fn compute_delta_reports_changed_fields() {
    use sidequest_game::delta;

    let snapshot1 = test_snapshot();
    let mut snapshot2 = test_snapshot();
    snapshot2.location = "New Location".to_string();
    snapshot2.atmosphere = "eerie".to_string();

    let before = delta::snapshot(&snapshot1);
    let after = delta::snapshot(&snapshot2);

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        let _delta = delta::compute_delta(&before, &after);
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "compute_delta").expect("Expected a 'compute_delta' span");

    let is_empty_field = span
        .fields
        .iter()
        .find(|(name, _)| name == "is_empty")
        .map(|(_, v)| v.as_str());
    assert_eq!(
        is_empty_field,
        Some("false"),
        "compute_delta should report is_empty=false when fields changed"
    );

    // fields_changed should mention location and atmosphere
    let fields_changed = span
        .fields
        .iter()
        .find(|(name, _)| name == "fields_changed")
        .map(|(_, v)| v.as_str());
    assert!(
        fields_changed.is_some(),
        "compute_delta should report which fields changed"
    );
    let changed = fields_changed.unwrap();
    assert!(
        changed.contains("location"),
        "fields_changed should include 'location', got: {changed}"
    );
    assert!(
        changed.contains("atmosphere"),
        "fields_changed should include 'atmosphere', got: {changed}"
    );
}

//! Story 28-12 RED: OTEL for game crate internals — CreatureCore, trope tick,
//! disposition, turn phases, barrier resolution.
//!
//! Tests that five subsystems emit tracing spans with semantic fields for
//! observability. These are the remaining LLM Compensation blind spots —
//! subsystems where Claude can narrate around missing mechanics without
//! observable evidence in the GM panel.

use std::sync::{Arc, Mutex};
use tracing::subscriber::with_default;
use tracing::Subscriber;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::Registry;

// ---------------------------------------------------------------------------
// Test infrastructure: span capture layer (same pattern as story 13-1)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct CapturedSpan {
    id: u64,
    name: String,
    fields: Vec<(String, String)>,
    #[allow(dead_code)]
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
        id: &tracing::span::Id,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut fields = Vec::new();
        let mut visitor = FieldCaptureVisitor(&mut fields);
        attrs.record(&mut visitor);

        self.captured.lock().unwrap().push(CapturedSpan {
            id: id.into_u64(),
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
        let mut fields = Vec::new();
        let mut visitor = FieldCaptureVisitor(&mut fields);
        values.record(&mut visitor);

        let span_id = id.into_u64();
        let mut captured = self.captured.lock().unwrap();
        if let Some(span) = captured.iter_mut().find(|s| s.id == span_id) {
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
    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.0.push((field.name().to_string(), value.to_string()));
    }
}

fn find_span<'a>(spans: &'a [CapturedSpan], name: &str) -> Option<&'a CapturedSpan> {
    spans.iter().find(|s| s.name == name)
}

fn find_spans<'a>(spans: &'a [CapturedSpan], name: &str) -> Vec<&'a CapturedSpan> {
    spans.iter().filter(|s| s.name == name).collect()
}

fn has_field(span: &CapturedSpan, field_name: &str) -> bool {
    span.fields.iter().any(|(name, _)| name == field_name)
}

fn field_value<'a>(span: &'a CapturedSpan, field_name: &str) -> Option<&'a str> {
    span.fields
        .iter()
        .find(|(name, _)| name == field_name)
        .map(|(_, v)| v.as_str())
}

// ===========================================================================
// CREATURE CORE — apply_hp_delta OTEL
// ===========================================================================

/// Helper: build a CreatureCore with known values for testing.
fn test_creature_core() -> sidequest_game::CreatureCore {
    use sidequest_game::CreatureCore;
    use sidequest_game::Inventory;
    use sidequest_protocol::NonBlankString;

    CreatureCore {
        name: NonBlankString::new("Grek the Mutant").unwrap(),
        description: NonBlankString::new("A scarred wasteland survivor").unwrap(),
        personality: NonBlankString::new("cautious").unwrap(),
        level: 5,
        edge: sidequest_game::creature_core::placeholder_edge_pool(),
        acquired_advancements: vec![],
        xp: 0,
        inventory: Inventory::default(),
        statuses: vec![],
    }
}

/// apply_hp_delta must emit a creature.hp_delta span with name, old_hp,
/// new_hp, delta fields.
#[test]
fn creature_hp_delta_emits_span() {
    let mut creature = test_creature_core();

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        creature.edge.apply_delta(-5);
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "creature.hp_delta")
        .expect("Expected a 'creature.hp_delta' span — apply_hp_delta must emit OTEL");

    assert!(
        has_field(span, "name"),
        "creature.hp_delta span missing 'name' field"
    );
    assert!(
        has_field(span, "old_hp"),
        "creature.hp_delta span missing 'old_hp' field"
    );
    assert!(
        has_field(span, "new_hp"),
        "creature.hp_delta span missing 'new_hp' field"
    );
    assert!(
        has_field(span, "delta"),
        "creature.hp_delta span missing 'delta' field"
    );
}

/// apply_hp_delta must include the creature's display name in the span.
#[test]
fn creature_hp_delta_includes_name() {
    let mut creature = test_creature_core();

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        creature.edge.apply_delta(-5);
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "creature.hp_delta").expect("Expected a 'creature.hp_delta' span");

    assert_eq!(
        field_value(span, "name"),
        Some("Grek the Mutant"),
        "name field should contain the creature's display name"
    );
}

/// apply_hp_delta must report correct old_hp and new_hp values.
#[test]
fn creature_hp_delta_reports_correct_values() {
    let mut creature = test_creature_core();
    // hp starts at 20, delta -5 → new_hp 15

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        creature.edge.apply_delta(-5);
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "creature.hp_delta").expect("Expected a 'creature.hp_delta' span");

    assert_eq!(
        field_value(span, "old_hp"),
        Some("20"),
        "old_hp should be 20 (starting value)"
    );
    assert_eq!(
        field_value(span, "new_hp"),
        Some("15"),
        "new_hp should be 15 after -5 delta"
    );
    assert_eq!(field_value(span, "delta"), Some("-5"), "delta should be -5");
}

/// apply_hp_delta must report clamped=true when damage exceeds current HP.
#[test]
fn creature_hp_delta_reports_clamped_on_overkill() {
    let mut creature = test_creature_core();
    // hp=20, delta=-100 → clamped to 0

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        creature.edge.apply_delta(-100);
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "creature.hp_delta").expect("Expected a 'creature.hp_delta' span");

    assert!(
        has_field(span, "clamped"),
        "creature.hp_delta span missing 'clamped' field"
    );
    assert_eq!(
        field_value(span, "clamped"),
        Some("true"),
        "clamped should be true when damage exceeds HP (floored at 0)"
    );
    assert_eq!(
        field_value(span, "new_hp"),
        Some("0"),
        "new_hp should be 0 after overkill"
    );
}

/// apply_hp_delta must report clamped=true when healing exceeds max_hp.
#[test]
fn creature_hp_delta_reports_clamped_on_overheal() {
    let mut creature = test_creature_core();
    // hp=20, max_hp=30, delta=+100 → clamped to 30

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        creature.edge.apply_delta(100);
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "creature.hp_delta").expect("Expected a 'creature.hp_delta' span");

    assert_eq!(
        field_value(span, "clamped"),
        Some("true"),
        "clamped should be true when healing exceeds max_hp"
    );
    assert_eq!(
        field_value(span, "new_hp"),
        Some("30"),
        "new_hp should be capped at max_hp=30"
    );
}

/// apply_hp_delta must report clamped=false on normal (non-clamped) damage.
#[test]
fn creature_hp_delta_reports_unclamped_on_normal_damage() {
    let mut creature = test_creature_core();
    // hp=20, delta=-5 → 15, not clamped

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        creature.edge.apply_delta(-5);
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "creature.hp_delta").expect("Expected a 'creature.hp_delta' span");

    assert_eq!(
        field_value(span, "clamped"),
        Some("false"),
        "clamped should be false when damage doesn't hit floor or ceiling"
    );
}

// ===========================================================================
// TROPE ENGINE — tick OTEL (per-trope progression tracking)
// ===========================================================================

fn test_trope_def() -> sidequest_genre::TropeDefinition {
    use sidequest_genre::{PassiveProgression, TropeDefinition, TropeEscalation};
    use sidequest_protocol::NonBlankString;

    TropeDefinition {
        id: Some("forbidden_knowledge".to_string()),
        name: NonBlankString::new("Forbidden Knowledge").unwrap(),
        description: Some("Dark secrets surface".to_string()),
        category: "revelation".to_string(),
        triggers: vec!["research".to_string()],
        narrative_hints: vec![],
        tension_level: Some(0.5),
        resolution_hints: None,
        resolution_patterns: None,
        tags: vec![],
        passive_progression: Some(PassiveProgression {
            rate_per_turn: 0.1,
            rate_per_day: 0.0,
            accelerators: vec!["forbidden".to_string()],
            decelerators: vec!["ignore".to_string()],
            accelerator_bonus: 0.15,
            decelerator_penalty: 0.05,
        }),
        escalation: vec![TropeEscalation {
            at: 0.5,
            event: "Whispers grow louder".to_string(),
            npcs_involved: vec![],
            stakes: "sanity".to_string(),
        }],
        is_abstract: false,
        extends: None,
    }
}

/// TropeEngine::tick must emit a per-trope trope.tick span with progression
/// tracking fields (progression_before, progression_after).
#[test]
fn trope_tick_emits_per_trope_progression_span() {
    use sidequest_game::trope::{TropeEngine, TropeState};

    let mut tropes = vec![TropeState::new("forbidden_knowledge")];
    let defs = vec![test_trope_def()];

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        let _fired = TropeEngine::tick(&mut tropes, &defs);
    });

    let spans = captured.lock().unwrap();
    let trope_spans = find_spans(&spans, "trope.tick");
    assert!(
        !trope_spans.is_empty(),
        "Expected at least one 'trope.tick' span — tick must emit per-trope progression events"
    );

    let span = trope_spans[0];
    assert!(
        has_field(span, "trope_id"),
        "trope.tick span missing 'trope_id' field"
    );
    assert!(
        has_field(span, "progression_before"),
        "trope.tick span missing 'progression_before' field"
    );
    assert!(
        has_field(span, "progression_after"),
        "trope.tick span missing 'progression_after' field"
    );
}

/// trope.tick must report threshold_crossed when an escalation fires.
#[test]
fn trope_tick_reports_threshold_crossed() {
    use sidequest_game::trope::{TropeEngine, TropeState};

    // Start trope at 0.45 — with rate_per_turn=0.1, one tick takes it to 0.55,
    // crossing the 0.5 threshold
    let mut trope = TropeState::new("forbidden_knowledge");
    trope.set_progression(0.45);
    let mut tropes = vec![trope];
    let defs = vec![test_trope_def()];

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        let _fired = TropeEngine::tick(&mut tropes, &defs);
    });

    let spans = captured.lock().unwrap();
    let trope_spans = find_spans(&spans, "trope.tick");
    assert!(
        !trope_spans.is_empty(),
        "Expected 'trope.tick' span for threshold crossing"
    );

    let span = trope_spans[0];
    assert!(
        has_field(span, "threshold_crossed"),
        "trope.tick span missing 'threshold_crossed' field — \
         must report when escalation thresholds are crossed"
    );
    assert_eq!(
        field_value(span, "threshold_crossed"),
        Some("true"),
        "threshold_crossed should be true when progression crosses escalation.at"
    );
}

/// trope.tick must report threshold_crossed=false when no escalation fires.
#[test]
fn trope_tick_reports_no_threshold_crossed() {
    use sidequest_game::trope::{TropeEngine, TropeState};

    // Start at 0.0 — tick advances to 0.1, no threshold (0.5) crossed
    let mut tropes = vec![TropeState::new("forbidden_knowledge")];
    let defs = vec![test_trope_def()];

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        let _fired = TropeEngine::tick(&mut tropes, &defs);
    });

    let spans = captured.lock().unwrap();
    let trope_spans = find_spans(&spans, "trope.tick");
    assert!(
        !trope_spans.is_empty(),
        "Expected 'trope.tick' span even without threshold crossing"
    );

    let span = trope_spans[0];
    assert_eq!(
        field_value(span, "threshold_crossed"),
        Some("false"),
        "threshold_crossed should be false when no escalation fires"
    );
}

// ===========================================================================
// DISPOSITION — apply_delta OTEL
// ===========================================================================

/// Disposition::apply_delta must emit a disposition.shift span with
/// old_attitude, new_attitude, and delta fields.
#[test]
fn disposition_shift_emits_span() {
    use sidequest_game::Disposition;

    let mut disp = Disposition::new(8); // Neutral (within -10..=10)

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        disp.apply_delta(5); // 8+5=13 → crosses into Friendly
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "disposition.shift")
        .expect("Expected a 'disposition.shift' span — apply_delta must emit OTEL");

    assert!(
        has_field(span, "old_attitude"),
        "disposition.shift span missing 'old_attitude' field"
    );
    assert!(
        has_field(span, "new_attitude"),
        "disposition.shift span missing 'new_attitude' field"
    );
    assert!(
        has_field(span, "delta"),
        "disposition.shift span missing 'delta' field"
    );
}

/// disposition.shift must report correct attitude transition (neutral → friendly).
#[test]
fn disposition_shift_tracks_neutral_to_friendly() {
    use sidequest_game::Disposition;

    let mut disp = Disposition::new(8); // Neutral

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        disp.apply_delta(5); // → 13, Friendly
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "disposition.shift").expect("Expected a 'disposition.shift' span");

    assert_eq!(
        field_value(span, "old_attitude"),
        Some("neutral"),
        "old_attitude should be neutral (value was 8)"
    );
    assert_eq!(
        field_value(span, "new_attitude"),
        Some("friendly"),
        "new_attitude should be friendly (value is 13)"
    );
}

/// disposition.shift must report correct attitude transition (neutral → hostile).
#[test]
fn disposition_shift_tracks_neutral_to_hostile() {
    use sidequest_game::Disposition;

    let mut disp = Disposition::new(-8); // Neutral

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        disp.apply_delta(-5); // → -13, Hostile
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "disposition.shift").expect("Expected a 'disposition.shift' span");

    assert_eq!(
        field_value(span, "old_attitude"),
        Some("neutral"),
        "old_attitude should be neutral (value was -8)"
    );
    assert_eq!(
        field_value(span, "new_attitude"),
        Some("hostile"),
        "new_attitude should be hostile (value is -13)"
    );
}

/// disposition.shift must include old_value and new_value for numeric tracking.
#[test]
fn disposition_shift_includes_numeric_values() {
    use sidequest_game::Disposition;

    let mut disp = Disposition::new(5);

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        disp.apply_delta(3); // → 8, still Neutral
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "disposition.shift").expect("Expected a 'disposition.shift' span");

    assert!(
        has_field(span, "old_value"),
        "disposition.shift span missing 'old_value' field"
    );
    assert!(
        has_field(span, "new_value"),
        "disposition.shift span missing 'new_value' field"
    );
    assert_eq!(
        field_value(span, "old_value"),
        Some("5"),
        "old_value should be 5"
    );
    assert_eq!(
        field_value(span, "new_value"),
        Some("8"),
        "new_value should be 8 after +3 delta"
    );
}

// ===========================================================================
// TURN MANAGER — phase transition OTEL
// ===========================================================================

/// advance_phase must emit a turn.phase_transition span with from_phase
/// and to_phase fields.
#[test]
fn turn_phase_transition_emits_span() {
    use sidequest_game::TurnManager;

    let mut tm = TurnManager::new();
    // Starts at InputCollection

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        tm.advance_phase(); // InputCollection → IntentRouting
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "turn.phase_transition")
        .expect("Expected a 'turn.phase_transition' span — advance_phase must emit OTEL");

    assert!(
        has_field(span, "from_phase"),
        "turn.phase_transition span missing 'from_phase' field"
    );
    assert!(
        has_field(span, "to_phase"),
        "turn.phase_transition span missing 'to_phase' field"
    );
}

/// turn.phase_transition must report correct phase names.
#[test]
fn turn_phase_transition_reports_correct_phases() {
    use sidequest_game::TurnManager;

    let mut tm = TurnManager::new();

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        tm.advance_phase(); // InputCollection → IntentRouting
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "turn.phase_transition")
        .expect("Expected a 'turn.phase_transition' span");

    assert_eq!(
        field_value(span, "from_phase"),
        Some("InputCollection"),
        "from_phase should be InputCollection"
    );
    assert_eq!(
        field_value(span, "to_phase"),
        Some("IntentRouting"),
        "to_phase should be IntentRouting"
    );
}

/// Multiple advance_phase calls should emit one span per transition.
#[test]
fn turn_phase_transitions_emit_per_transition() {
    use sidequest_game::TurnManager;

    let mut tm = TurnManager::new();

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        tm.advance_phase(); // InputCollection → IntentRouting
        tm.advance_phase(); // IntentRouting → AgentExecution
        tm.advance_phase(); // AgentExecution → StatePatch
    });

    let spans = captured.lock().unwrap();
    let phase_spans = find_spans(&spans, "turn.phase_transition");
    assert_eq!(
        phase_spans.len(),
        3,
        "Each advance_phase should emit one turn.phase_transition span"
    );

    // Verify the chain
    assert_eq!(
        field_value(phase_spans[0], "to_phase"),
        Some("IntentRouting")
    );
    assert_eq!(
        field_value(phase_spans[1], "to_phase"),
        Some("AgentExecution")
    );
    assert_eq!(field_value(phase_spans[2], "to_phase"), Some("StatePatch"));
}

/// turn.phase_transition should include the current round number for context.
#[test]
fn turn_phase_transition_includes_round() {
    use sidequest_game::TurnManager;

    let mut tm = TurnManager::new();

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    with_default(subscriber, || {
        tm.advance_phase();
    });

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "turn.phase_transition")
        .expect("Expected a 'turn.phase_transition' span");

    assert!(
        has_field(span, "round"),
        "turn.phase_transition span missing 'round' field — \
         should include current round for correlation"
    );
    assert_eq!(
        field_value(span, "round"),
        Some("1"),
        "round should be 1 (initial round)"
    );
}

// ===========================================================================
// BARRIER — resolution OTEL
// ===========================================================================

/// Helper: build a test Character with minimal fields.
fn test_character(name: &str) -> sidequest_game::Character {
    use sidequest_game::Character;
    use sidequest_protocol::NonBlankString;

    Character {
        core: {
            let mut c = test_creature_core();
            c.name = NonBlankString::new(name).unwrap();
            c
        },
        backstory: NonBlankString::new("A wanderer").unwrap(),
        narrative_state: String::new(),
        hooks: vec![],
        char_class: NonBlankString::new("Fighter").unwrap(),
        race: NonBlankString::new("Human").unwrap(),
        pronouns: String::new(),
        stats: std::collections::HashMap::new(),
        abilities: vec![],
        known_facts: vec![],
        affinities: vec![],
        is_friendly: true,
        resolved_archetype: None,
        archetype_provenance: None,
    }
}

/// Barrier resolution must emit a barrier.resolved span with outcome fields.
#[tokio::test]
async fn barrier_resolved_emits_span() {
    use sidequest_game::barrier::{TurnBarrier, TurnBarrierConfig};
    use sidequest_game::multiplayer::MultiplayerSession;
    use std::time::Duration;

    let mut players = std::collections::HashMap::new();
    players.insert("player1".to_string(), test_character("Grek"));
    let session = MultiplayerSession::new(players);

    let config = TurnBarrierConfig::new(Duration::from_millis(100));
    let barrier = TurnBarrier::new(session, config);

    // Submit action so barrier resolves immediately
    barrier.submit_action("player1", "I attack the goblin");

    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    let barrier_clone = barrier.clone();
    // Run wait_for_turn within the tracing subscriber context
    let _result = {
        let _guard = tracing::subscriber::set_default(subscriber);
        barrier_clone.wait_for_turn().await
    };

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "barrier.resolved")
        .expect("Expected a 'barrier.resolved' span — barrier resolution must emit OTEL");

    assert!(
        has_field(span, "player_count"),
        "barrier.resolved span missing 'player_count' field"
    );
    assert!(
        has_field(span, "timed_out"),
        "barrier.resolved span missing 'timed_out' field"
    );
}

/// Barrier resolution via full submission should report timed_out=false.
#[tokio::test]
async fn barrier_resolved_full_submission_not_timed_out() {
    use sidequest_game::barrier::{TurnBarrier, TurnBarrierConfig};
    use sidequest_game::multiplayer::MultiplayerSession;
    use std::time::Duration;

    let mut players = std::collections::HashMap::new();
    players.insert("player1".to_string(), test_character("Grek"));
    let session = MultiplayerSession::new(players);

    let config = TurnBarrierConfig::new(Duration::from_millis(100));
    let barrier = TurnBarrier::new(session, config);

    barrier.submit_action("player1", "I search the room");

    let barrier_clone = barrier.clone();
    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    let _result = {
        let _guard = tracing::subscriber::set_default(subscriber);
        barrier_clone.wait_for_turn().await
    };

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "barrier.resolved").expect("Expected a 'barrier.resolved' span");

    assert_eq!(
        field_value(span, "timed_out"),
        Some("false"),
        "timed_out should be false when all players submitted"
    );
}

/// Barrier resolution via timeout should report timed_out=true and
/// include the count of submitted players.
#[tokio::test]
async fn barrier_resolved_timeout_reports_timed_out() {
    use sidequest_game::barrier::{TurnBarrier, TurnBarrierConfig};
    use sidequest_game::multiplayer::MultiplayerSession;
    use std::time::Duration;

    let mut players = std::collections::HashMap::new();
    players.insert("player1".to_string(), test_character("Grek"));
    players.insert("player2".to_string(), test_character("Zara"));
    let session = MultiplayerSession::new(players);

    // Very short timeout so test doesn't block
    let config = TurnBarrierConfig::new(Duration::from_millis(50));
    let barrier = TurnBarrier::new(session, config);

    // Only player1 submits — player2 times out
    barrier.submit_action("player1", "I ready my weapon");

    let barrier_clone = barrier.clone();
    let (layer, captured) = SpanCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    let _result = {
        let _guard = tracing::subscriber::set_default(subscriber);
        barrier_clone.wait_for_turn().await
    };

    let spans = captured.lock().unwrap();
    let span = find_span(&spans, "barrier.resolved")
        .expect("Expected a 'barrier.resolved' span on timeout");

    assert_eq!(
        field_value(span, "timed_out"),
        Some("true"),
        "timed_out should be true when barrier expires before all players submit"
    );
    assert!(
        has_field(span, "submitted"),
        "barrier.resolved span missing 'submitted' field — \
         should report how many players actually submitted"
    );
}

// ===========================================================================
// WIRING: verify all span names are semantically distinct
// ===========================================================================

/// All five OTEL span names from this story must be distinct and follow
/// the dot-notation naming convention.
#[test]
fn otel_span_names_are_semantically_distinct() {
    let expected_spans = [
        "creature.hp_delta",
        "trope.tick",
        "disposition.shift",
        "turn.phase_transition",
        "barrier.resolved",
    ];

    // Verify no duplicates
    let mut seen = std::collections::HashSet::new();
    for name in &expected_spans {
        assert!(seen.insert(name), "Duplicate span name detected: {name}");
    }

    // Verify dot-notation convention
    for name in &expected_spans {
        assert!(
            name.contains('.'),
            "Span name '{name}' should use dot notation (subsystem.event)"
        );
    }
}

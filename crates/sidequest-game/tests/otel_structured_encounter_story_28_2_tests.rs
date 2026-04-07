//! Story 28-2: OTEL for StructuredEncounter — instrument apply_beat, metric changes, resolution
//!
//! RED phase — These tests verify that StructuredEncounter methods and
//! CreatureCore::apply_hp_delta() emit WatcherEvents through the global
//! telemetry channel. Currently NO events are emitted — all tests FAIL.
//!
//! Events required:
//!   encounter.beat_applied   — from apply_beat(), with encounter_type, beat_id,
//!                              stat_check, metric_before, metric_after, phase
//!   encounter.resolved       — when apply_beat() triggers resolution, with
//!                              encounter_type, beats_total, outcome
//!   encounter.phase_transition — when phase changes during apply_beat(), with
//!                              encounter_type, old_phase, new_phase
//!   encounter.escalated      — from escalate_to_combat(), with from_type, to_type
//!   creature.hp_delta        — from apply_hp_delta(), with name, old_hp, new_hp,
//!                              delta, max_hp, clamped

use sidequest_game::encounter::{
    EncounterActor, EncounterPhase, StructuredEncounter,
};
use sidequest_game::creature_core::CreatureCore;
use sidequest_game::inventory::Inventory;
use sidequest_genre::ConfrontationDef;
use sidequest_protocol::NonBlankString;
use sidequest_telemetry::{init_global_channel, subscribe_global, WatcherEvent};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Standard standoff definition used across all tests.
fn standoff_def() -> ConfrontationDef {
    let yaml = r#"
type: standoff
label: "Standoff"
category: pre_combat
metric:
  name: tension
  direction: ascending
  starting: 0
  threshold_high: 10
beats:
  - id: size_up
    label: "Size Up"
    metric_delta: 2
    stat_check: CUNNING
    reveals: opponent_detail
  - id: bluff
    label: "Bluff"
    metric_delta: 3
    stat_check: NERVE
    risk: "opponent may call it — immediate draw"
  - id: flinch
    label: "Flinch"
    metric_delta: -1
    stat_check: NERVE
  - id: draw
    label: "Draw"
    metric_delta: 0
    stat_check: DRAW
    resolution: true
secondary_stats:
  - name: focus
    source_stat: NERVE
    spendable: true
escalates_to: combat
mood: standoff
"#;
    serde_yaml::from_str(yaml).expect("standoff def should parse")
}

/// Create a test CreatureCore.
fn test_creature() -> CreatureCore {
    CreatureCore {
        name: NonBlankString::new("Goblin Scout").unwrap(),
        description: NonBlankString::new("A sneaky goblin").unwrap(),
        personality: NonBlankString::new("Cowardly but cunning").unwrap(),
        level: 2,
        hp: 15,
        max_hp: 20,
        ac: 12,
        xp: 0,
        inventory: Inventory::default(),
        statuses: vec![],
    }
}

/// Initialize the global telemetry channel (idempotent via OnceLock).
/// Subscribe and drain any stale events, returning a clean receiver.
fn fresh_subscriber() -> tokio::sync::broadcast::Receiver<WatcherEvent> {
    let _ = init_global_channel();
    let mut rx = subscribe_global().expect("channel must be initialized");
    // Drain any stale events from prior tests
    while rx.try_recv().is_ok() {}
    rx
}

/// Collect all available events from the receiver (non-blocking).
fn drain_events(rx: &mut tokio::sync::broadcast::Receiver<WatcherEvent>) -> Vec<WatcherEvent> {
    let mut events = Vec::new();
    loop {
        match rx.try_recv() {
            Ok(event) => events.push(event),
            Err(_) => break,
        }
    }
    events
}

/// Find events matching a component and action field value.
fn find_events_by_action(events: &[WatcherEvent], component: &str, action: &str) -> Vec<WatcherEvent> {
    events
        .iter()
        .filter(|e| {
            e.component == component
                && e.fields
                    .get("action")
                    .and_then(serde_json::Value::as_str)
                    .map_or(false, |a| a == action)
        })
        .cloned()
        .collect()
}

// =========================================================================
// AC: apply_beat OTEL — encounter.beat_applied
// =========================================================================

/// apply_beat() must emit an encounter.beat_applied WatcherEvent.
#[test]
fn apply_beat_emits_beat_applied_event() {
    let mut rx = fresh_subscriber();
    let def = standoff_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    let _ = encounter.apply_beat("size_up", &def);

    let events = drain_events(&mut rx);
    let beat_events = find_events_by_action(&events, "encounter", "beat_applied");

    assert!(
        !beat_events.is_empty(),
        "apply_beat() must emit an encounter.beat_applied WatcherEvent, got 0 events"
    );
}

/// beat_applied event includes encounter_type field.
#[test]
fn beat_applied_has_encounter_type() {
    let mut rx = fresh_subscriber();
    let def = standoff_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    let _ = encounter.apply_beat("size_up", &def);

    let events = drain_events(&mut rx);
    let beat_events = find_events_by_action(&events, "encounter", "beat_applied");

    assert!(!beat_events.is_empty(), "must emit beat_applied event");
    let event = &beat_events[0];
    assert_eq!(
        event.fields.get("encounter_type").and_then(serde_json::Value::as_str),
        Some("standoff"),
        "beat_applied must include encounter_type='standoff'"
    );
}

/// beat_applied event includes beat_id field.
#[test]
fn beat_applied_has_beat_id() {
    let mut rx = fresh_subscriber();
    let def = standoff_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    let _ = encounter.apply_beat("bluff", &def);

    let events = drain_events(&mut rx);
    let beat_events = find_events_by_action(&events, "encounter", "beat_applied");

    assert!(!beat_events.is_empty(), "must emit beat_applied event");
    assert_eq!(
        beat_events[0].fields.get("beat_id").and_then(serde_json::Value::as_str),
        Some("bluff"),
        "beat_applied must include beat_id='bluff'"
    );
}

/// beat_applied event includes stat_check field.
#[test]
fn beat_applied_has_stat_check() {
    let mut rx = fresh_subscriber();
    let def = standoff_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    let _ = encounter.apply_beat("size_up", &def);

    let events = drain_events(&mut rx);
    let beat_events = find_events_by_action(&events, "encounter", "beat_applied");

    assert!(!beat_events.is_empty(), "must emit beat_applied event");
    assert_eq!(
        beat_events[0].fields.get("stat_check").and_then(serde_json::Value::as_str),
        Some("CUNNING"),
        "beat_applied must include stat_check from the beat def"
    );
}

/// beat_applied event includes metric_before and metric_after fields.
#[test]
fn beat_applied_has_metric_before_after() {
    let mut rx = fresh_subscriber();
    let def = standoff_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    // Starting tension = 0, size_up adds +2
    let _ = encounter.apply_beat("size_up", &def);

    let events = drain_events(&mut rx);
    let beat_events = find_events_by_action(&events, "encounter", "beat_applied");

    assert!(!beat_events.is_empty(), "must emit beat_applied event");
    let event = &beat_events[0];

    assert_eq!(
        event.fields.get("metric_before").and_then(serde_json::Value::as_i64),
        Some(0),
        "metric_before should be 0 (starting tension)"
    );
    assert_eq!(
        event.fields.get("metric_after").and_then(serde_json::Value::as_i64),
        Some(2),
        "metric_after should be 2 (after size_up +2)"
    );
}

/// beat_applied event includes the current phase.
#[test]
fn beat_applied_has_phase() {
    let mut rx = fresh_subscriber();
    let def = standoff_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    let _ = encounter.apply_beat("size_up", &def);

    let events = drain_events(&mut rx);
    let beat_events = find_events_by_action(&events, "encounter", "beat_applied");

    assert!(!beat_events.is_empty(), "must emit beat_applied event");
    // After first beat, phase should be "Opening"
    let phase = beat_events[0].fields.get("phase").and_then(serde_json::Value::as_str);
    assert!(
        phase.is_some(),
        "beat_applied must include phase field"
    );
}

// =========================================================================
// AC: Resolution OTEL — encounter.resolved
// =========================================================================

/// When a resolution beat fires, apply_beat must emit encounter.resolved.
#[test]
fn resolution_beat_emits_resolved_event() {
    let mut rx = fresh_subscriber();
    let def = standoff_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    // Draw is a resolution beat
    let _ = encounter.apply_beat("draw", &def);
    assert!(encounter.resolved, "draw should resolve the encounter");

    let events = drain_events(&mut rx);
    let resolved_events = find_events_by_action(&events, "encounter", "resolved");

    assert!(
        !resolved_events.is_empty(),
        "resolution must emit an encounter.resolved WatcherEvent"
    );
}

/// encounter.resolved includes encounter_type and beats_total.
#[test]
fn resolved_event_has_encounter_type_and_beats() {
    let mut rx = fresh_subscriber();
    let def = standoff_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    // Play a few beats then resolve
    let _ = encounter.apply_beat("size_up", &def); // beat 1
    let _ = encounter.apply_beat("bluff", &def);   // beat 2
    let _ = encounter.apply_beat("draw", &def);    // beat 3, resolves

    let events = drain_events(&mut rx);
    let resolved_events = find_events_by_action(&events, "encounter", "resolved");

    assert!(!resolved_events.is_empty(), "must emit resolved event");
    let event = &resolved_events[0];

    assert_eq!(
        event.fields.get("encounter_type").and_then(serde_json::Value::as_str),
        Some("standoff"),
        "resolved must include encounter_type"
    );
    assert_eq!(
        event.fields.get("beats_total").and_then(serde_json::Value::as_i64),
        Some(3),
        "resolved must include beats_total=3"
    );
}

/// Threshold-triggered resolution also emits encounter.resolved.
#[test]
fn threshold_resolution_emits_resolved_event() {
    let mut rx = fresh_subscriber();
    let def = standoff_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    // Push tension past threshold: bluff(3)*4 = 12 > threshold 10
    let _ = encounter.apply_beat("bluff", &def); // 3
    let _ = encounter.apply_beat("bluff", &def); // 6
    let _ = encounter.apply_beat("bluff", &def); // 9
    let _ = encounter.apply_beat("bluff", &def); // 12

    assert!(encounter.resolved);

    let events = drain_events(&mut rx);
    let resolved_events = find_events_by_action(&events, "encounter", "resolved");

    assert!(
        !resolved_events.is_empty(),
        "threshold crossing must also emit encounter.resolved"
    );
}

// =========================================================================
// AC: Phase OTEL — encounter.phase_transition
// =========================================================================

/// Phase transitions during apply_beat must emit encounter.phase_transition.
#[test]
fn phase_transition_emits_event() {
    let mut rx = fresh_subscriber();
    let def = standoff_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    // Starting phase: Setup → after first beat: Opening
    let _ = encounter.apply_beat("size_up", &def);

    let events = drain_events(&mut rx);
    let phase_events = find_events_by_action(&events, "encounter", "phase_transition");

    assert!(
        !phase_events.is_empty(),
        "phase change (Setup→Opening) must emit encounter.phase_transition"
    );
}

/// phase_transition event includes old_phase and new_phase.
#[test]
fn phase_transition_has_old_and_new_phase() {
    let mut rx = fresh_subscriber();
    let def = standoff_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    // Setup → Opening on first beat
    let _ = encounter.apply_beat("size_up", &def);

    let events = drain_events(&mut rx);
    let phase_events = find_events_by_action(&events, "encounter", "phase_transition");

    assert!(!phase_events.is_empty(), "must emit phase_transition");
    let event = &phase_events[0];

    assert_eq!(
        event.fields.get("old_phase").and_then(serde_json::Value::as_str),
        Some("Setup"),
        "old_phase should be 'Setup'"
    );
    assert_eq!(
        event.fields.get("new_phase").and_then(serde_json::Value::as_str),
        Some("Opening"),
        "new_phase should be 'Opening'"
    );
}

/// phase_transition includes encounter_type.
#[test]
fn phase_transition_has_encounter_type() {
    let mut rx = fresh_subscriber();
    let def = standoff_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    let _ = encounter.apply_beat("size_up", &def);

    let events = drain_events(&mut rx);
    let phase_events = find_events_by_action(&events, "encounter", "phase_transition");

    assert!(!phase_events.is_empty(), "must emit phase_transition");
    assert_eq!(
        phase_events[0].fields.get("encounter_type").and_then(serde_json::Value::as_str),
        Some("standoff"),
        "phase_transition must include encounter_type"
    );
}

/// No phase_transition event when phase does not change.
#[test]
fn no_phase_transition_when_phase_unchanged() {
    let mut rx = fresh_subscriber();
    let def = standoff_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    // Beat 1: Setup → Opening (phase changes)
    let _ = encounter.apply_beat("size_up", &def);
    // Drain those events
    let _ = drain_events(&mut rx);

    // Beat 2 and 3: both are Escalation phase — beat 2 changes Opening→Escalation,
    // but beat 3 stays Escalation
    let _ = encounter.apply_beat("size_up", &def); // beat 2: Opening→Escalation
    let _ = drain_events(&mut rx);

    let _ = encounter.apply_beat("size_up", &def); // beat 3: stays Escalation
    let events = drain_events(&mut rx);
    let phase_events = find_events_by_action(&events, "encounter", "phase_transition");

    assert!(
        phase_events.is_empty(),
        "no phase_transition should fire when phase stays the same (Escalation→Escalation)"
    );
}

// =========================================================================
// AC: Escalation OTEL — encounter.escalated
// =========================================================================

/// escalate_to_combat() must emit encounter.escalated event.
#[test]
fn escalate_to_combat_emits_escalated_event() {
    let mut rx = fresh_subscriber();
    let def = standoff_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);
    encounter.actors = vec![
        EncounterActor {
            name: "Blondie".to_string(),
            role: "duelist".to_string(),
        },
    ];

    // Must resolve first
    let _ = encounter.apply_beat("draw", &def);
    let _ = drain_events(&mut rx); // clear beat_applied/resolved events

    let combat = encounter.escalate_to_combat();
    assert!(combat.is_some(), "should produce combat encounter");

    let events = drain_events(&mut rx);
    let escalated_events = find_events_by_action(&events, "encounter", "escalated");

    assert!(
        !escalated_events.is_empty(),
        "escalate_to_combat() must emit encounter.escalated WatcherEvent"
    );
}

/// escalated event includes from_type and to_type.
#[test]
fn escalated_event_has_from_and_to_type() {
    let mut rx = fresh_subscriber();
    let def = standoff_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);
    encounter.actors = vec![
        EncounterActor {
            name: "Blondie".to_string(),
            role: "duelist".to_string(),
        },
    ];

    let _ = encounter.apply_beat("draw", &def);
    let _ = drain_events(&mut rx);

    let _ = encounter.escalate_to_combat();
    let events = drain_events(&mut rx);
    let escalated_events = find_events_by_action(&events, "encounter", "escalated");

    assert!(!escalated_events.is_empty(), "must emit escalated event");
    let event = &escalated_events[0];

    assert_eq!(
        event.fields.get("from_type").and_then(serde_json::Value::as_str),
        Some("standoff"),
        "escalated must include from_type='standoff'"
    );
    assert_eq!(
        event.fields.get("to_type").and_then(serde_json::Value::as_str),
        Some("combat"),
        "escalated must include to_type='combat'"
    );
}

// =========================================================================
// AC: HP delta OTEL — creature.hp_delta
// =========================================================================

/// apply_hp_delta() must emit a creature.hp_delta WatcherEvent.
#[test]
fn apply_hp_delta_emits_event() {
    let mut rx = fresh_subscriber();
    let mut creature = test_creature();

    creature.apply_hp_delta(-5);

    let events = drain_events(&mut rx);
    let hp_events = find_events_by_action(&events, "creature", "hp_delta");

    assert!(
        !hp_events.is_empty(),
        "apply_hp_delta() must emit a creature.hp_delta WatcherEvent"
    );
}

/// hp_delta event includes name, old_hp, new_hp, delta, max_hp.
#[test]
fn hp_delta_event_has_required_fields() {
    let mut rx = fresh_subscriber();
    let mut creature = test_creature();
    // creature starts at hp=15, max_hp=20

    creature.apply_hp_delta(-5);

    let events = drain_events(&mut rx);
    let hp_events = find_events_by_action(&events, "creature", "hp_delta");

    assert!(!hp_events.is_empty(), "must emit hp_delta event");
    let event = &hp_events[0];

    assert_eq!(
        event.fields.get("name").and_then(serde_json::Value::as_str),
        Some("Goblin Scout"),
        "hp_delta must include creature name"
    );
    assert_eq!(
        event.fields.get("old_hp").and_then(serde_json::Value::as_i64),
        Some(15),
        "hp_delta must include old_hp=15"
    );
    assert_eq!(
        event.fields.get("new_hp").and_then(serde_json::Value::as_i64),
        Some(10),
        "hp_delta must include new_hp=10 (15 - 5)"
    );
    assert_eq!(
        event.fields.get("delta").and_then(serde_json::Value::as_i64),
        Some(-5),
        "hp_delta must include delta=-5"
    );
    assert_eq!(
        event.fields.get("max_hp").and_then(serde_json::Value::as_i64),
        Some(20),
        "hp_delta must include max_hp=20"
    );
}

/// hp_delta event includes clamped=true when HP would exceed max.
#[test]
fn hp_delta_clamped_true_on_overheal() {
    let mut rx = fresh_subscriber();
    let mut creature = test_creature();
    // hp=15, max_hp=20, heal +100 → clamped to 20

    creature.apply_hp_delta(100);

    let events = drain_events(&mut rx);
    let hp_events = find_events_by_action(&events, "creature", "hp_delta");

    assert!(!hp_events.is_empty(), "must emit hp_delta event");
    assert_eq!(
        hp_events[0].fields.get("clamped").and_then(serde_json::Value::as_bool),
        Some(true),
        "clamped must be true when HP exceeds max (15 + 100 → clamped to 20)"
    );
}

/// hp_delta event includes clamped=true when HP would go below zero.
#[test]
fn hp_delta_clamped_true_on_overkill() {
    let mut rx = fresh_subscriber();
    let mut creature = test_creature();
    // hp=15, damage -100 → clamped to 0

    creature.apply_hp_delta(-100);

    let events = drain_events(&mut rx);
    let hp_events = find_events_by_action(&events, "creature", "hp_delta");

    assert!(!hp_events.is_empty(), "must emit hp_delta event");
    assert_eq!(
        hp_events[0].fields.get("clamped").and_then(serde_json::Value::as_bool),
        Some(true),
        "clamped must be true when HP goes below 0"
    );
}

/// hp_delta event has clamped=false when no clamping occurs.
#[test]
fn hp_delta_clamped_false_when_within_range() {
    let mut rx = fresh_subscriber();
    let mut creature = test_creature();
    // hp=15, max_hp=20, damage -3 → 12 (no clamping)

    creature.apply_hp_delta(-3);

    let events = drain_events(&mut rx);
    let hp_events = find_events_by_action(&events, "creature", "hp_delta");

    assert!(!hp_events.is_empty(), "must emit hp_delta event");
    assert_eq!(
        hp_events[0].fields.get("clamped").and_then(serde_json::Value::as_bool),
        Some(false),
        "clamped must be false when HP stays within [0, max_hp]"
    );
}

// =========================================================================
// Wiring: WatcherEventBuilder used in non-test code paths
// =========================================================================

/// Verify that encounter.rs imports and uses WatcherEventBuilder in production code.
/// This is a compile-time wiring assertion — fails if the import is missing.
#[test]
fn encounter_rs_uses_watcher_event_builder() {
    // Read the source file and verify WatcherEventBuilder is imported
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let encounter_src = std::fs::read_to_string(
        format!("{}/src/encounter.rs", manifest_dir)
    ).expect("should read encounter.rs");

    assert!(
        encounter_src.contains("WatcherEventBuilder") || encounter_src.contains("watcher!"),
        "encounter.rs must use WatcherEventBuilder or watcher! macro in production code"
    );
}

/// Verify that creature_core.rs imports and uses WatcherEventBuilder in production code.
#[test]
fn creature_core_rs_uses_watcher_event_builder() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let creature_src = std::fs::read_to_string(
        format!("{}/src/creature_core.rs", manifest_dir)
    ).expect("should read creature_core.rs");

    assert!(
        creature_src.contains("WatcherEventBuilder") || creature_src.contains("watcher!"),
        "creature_core.rs must use WatcherEventBuilder or watcher! macro in production code"
    );
}

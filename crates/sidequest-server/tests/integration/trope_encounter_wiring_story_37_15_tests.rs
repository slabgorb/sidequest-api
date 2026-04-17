//! Story 37-15 rework: Trope-encounter handshake wiring tests.
//!
//! Round 1 tests (in sidequest-game/tests/) proved the building blocks work
//! in isolation. Reviewer rejected because:
//! 1. `resolve_from_trope()` has zero non-test callers — dispatch/tropes.rs unmodified
//! 2. Auto-resolve emits only tracing::info, no WatcherEvent — GM panel blind
//!
//! These tests enforce the wiring and OTEL requirements.

use crate::test_helpers;

use sidequest_game::encounter::StructuredEncounter;
use sidequest_game::trope::{TropeEngine, TropeState, TropeStatus};
use sidequest_genre::{PassiveProgression, TropeDefinition, TropeEscalation};
use sidequest_protocol::NonBlankString;
use sidequest_telemetry::{init_global_channel, subscribe_global, WatcherEvent};

// ── Wiring: dispatch/tropes.rs must call resolve_from_trope ──────────

#[test]
fn wiring_dispatch_tropes_calls_resolve_from_trope() {
    let dispatch_src = test_helpers::dispatch_source_combined();
    assert!(
        dispatch_src.contains("resolve_from_trope"),
        "dispatch/tropes.rs (or another dispatch file) must call \
         resolve_from_trope() to wire the trope-encounter handshake. \
         Currently resolve_from_trope has zero non-test callers."
    );
}

// ── OTEL: auto-resolve must emit WatcherEvent ────────────────────────

fn fast_trope_def() -> TropeDefinition {
    TropeDefinition {
        id: Some("the_standoff".to_string()),
        name: NonBlankString::new("The Standoff").unwrap(),
        description: Some("A tense confrontation reaches its breaking point".to_string()),
        category: "conflict".to_string(),
        triggers: vec![],
        narrative_hints: vec![],
        tension_level: Some(0.8),
        resolution_hints: None,
        resolution_patterns: None,
        tags: vec![],
        escalation: vec![
            TropeEscalation {
                at: 0.5,
                event: "Tensions rise".to_string(),
                npcs_involved: vec![],
                stakes: "Pride".to_string(),
            },
            TropeEscalation {
                at: 1.0,
                event: "The standoff breaks".to_string(),
                npcs_involved: vec![],
                stakes: "Everything".to_string(),
            },
        ],
        passive_progression: Some(PassiveProgression {
            rate_per_turn: 0.6,
            rate_per_day: 0.0,
            accelerators: vec![],
            decelerators: vec![],
            accelerator_bonus: 0.0,
            decelerator_penalty: 0.0,
        }),
        is_abstract: false,
        extends: None,
    }
}

fn drain_events(rx: &mut tokio::sync::broadcast::Receiver<WatcherEvent>) -> Vec<WatcherEvent> {
    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }
    events
}

#[test]
fn auto_resolve_emits_watcher_event() {
    let _ = init_global_channel();
    let mut rx = subscribe_global().expect("telemetry channel must be initialized");

    let mut tropes = vec![TropeState::new("the_standoff")];
    let defs = vec![fast_trope_def()];

    // First tick: 0.0 + 0.6 = 0.6
    TropeEngine::tick(&mut tropes, &defs);
    // Second tick: 0.6 + 0.6 = 1.0 → auto-resolve fires
    TropeEngine::tick(&mut tropes, &defs);

    assert_eq!(tropes[0].status(), TropeStatus::Resolved);

    let events = drain_events(&mut rx);
    let auto_resolved: Vec<&WatcherEvent> = events
        .iter()
        .filter(|e| {
            e.component == "trope"
                && e.fields
                    .get("event")
                    .and_then(serde_json::Value::as_str)
                    == Some("trope.auto_resolved")
        })
        .collect();

    assert!(
        !auto_resolved.is_empty(),
        "auto-resolve must emit a WatcherEvent with event='trope.auto_resolved' — \
         currently only emits tracing::info which is invisible to the GM panel"
    );

    // Verify the event carries the trope_id
    let event = auto_resolved[0];
    assert_eq!(
        event.fields.get("trope_id").and_then(serde_json::Value::as_str),
        Some("the_standoff"),
        "trope.auto_resolved event must carry the trope_id"
    );
}

// ── End-to-end: trope completion must resolve the encounter ──────────

#[test]
fn resolve_from_trope_emits_watcher_event() {
    let _ = init_global_channel();
    let mut rx = subscribe_global().expect("telemetry channel must be initialized");

    let mut encounter = StructuredEncounter::combat(vec![], 100);
    encounter.resolve_from_trope("the_standoff");

    let events = drain_events(&mut rx);
    let resolved_events: Vec<&WatcherEvent> = events
        .iter()
        .filter(|e| {
            e.component == "encounter"
                && e.fields
                    .get("event")
                    .and_then(serde_json::Value::as_str)
                    == Some("encounter.state.resolved_by_trope")
        })
        .collect();

    assert!(
        !resolved_events.is_empty(),
        "resolve_from_trope must emit encounter.state.resolved_by_trope WatcherEvent"
    );
}

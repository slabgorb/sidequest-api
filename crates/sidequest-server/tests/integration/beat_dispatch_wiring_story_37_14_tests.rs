//! Story 37-14 — true integration test (pass 2 rework).
//!
//! The first 37-14 pass shipped two source-scan "wiring tests"
//! (`wiring_dispatch_mod_calls_apply_beat_dispatch` and
//! `wiring_no_bare_is_some_guard_on_beat_selection_loop`) in the `src/`
//! tree. Those prove the function NAME and regression-guard TOKEN appear
//! in `dispatch/mod.rs`, but they don't prove the call site actually
//! executes. Per CLAUDE.md:
//!
//! > Every Test Suite Needs a Wiring Test
//! > Unit tests prove a component works in isolation. That's not enough.
//! > Every set of tests must include at least one integration test that
//! > verifies the component is wired into the system — imported, called,
//! > and reachable from production code paths.
//!
//! Reviewer pass 1 flagged this gap. A dead-but-present `apply_beat_dispatch`
//! function would pass all 8 source-scan + unit tests. This file closes
//! the gap with a real integration test that imports `apply_beat_dispatch`
//! from outside the `src/` tree (via the crate's public API) and drives a
//! live `GameSnapshot` through the helper.
//!
//! To make this test compile, Dev must add a `pub use` re-export at the
//! crate root exposing `apply_beat_dispatch` and `BeatDispatchOutcome`
//! (they are currently `pub(crate)`). The integration test's compile
//! failure is the RED signal that the symbol is not publicly reachable.
//!
//! Scope: a single end-to-end assertion that verifies:
//!   1. The symbols are publicly reachable from outside the crate.
//!   2. A real `GameSnapshot` with a live encounter can be passed through
//!      the helper and produce the `Applied` outcome.
//!   3. The canonical `encounter.beat_applied` WatcherEvent fires on the
//!      real telemetry channel, carries `source="narrator_beat_selection"`,
//!      and reports the actual beat_id.
//!   4. No misleading `beat_apply_failed` or `beat_skipped_resolved` event
//!      is emitted on a clean Applied path.
//!
//! This is a complement to the src-level unit tests in
//! `src/beat_dispatch_story_37_14_tests.rs`, not a replacement — the unit
//! tests still drive each `BeatDispatchOutcome` branch in isolation with
//! minimal fixtures; this file drives one happy-path case through the
//! crate's actual public surface.

use sidequest_game::encounter::{
    EncounterActor, EncounterMetric, MetricDirection, StructuredEncounter,
};
use sidequest_game::state::GameSnapshot;
use sidequest_genre::ConfrontationDef;
use sidequest_telemetry::{
    init_global_channel, subscribe_global, WatcherEvent, WatcherEventType,
};

// The public-path import. This must resolve for the integration test to
// compile — proving the helper is reachable from outside the src/ tree.
// Dev must add:
//     pub use dispatch::beat::{apply_beat_dispatch, BeatDispatchOutcome};
// at the crate root of sidequest-server. The integration test's compile
// failure is the RED signal.
use sidequest_server::{apply_beat_dispatch, BeatDispatchOutcome};

fn combat_yaml() -> &'static str {
    r#"
type: combat
label: "Combat"
category: combat
metric:
  name: hp
  direction: descending
  starting: 20
  threshold_low: 0
beats:
  - id: attack
    label: "Attack"
    metric_delta: -3
    stat_check: STRENGTH
  - id: defend
    label: "Defend"
    metric_delta: 0
    stat_check: CONSTITUTION
"#
}

fn live_combat_encounter() -> StructuredEncounter {
    StructuredEncounter {
        encounter_type: "combat".to_string(),
        metric: EncounterMetric {
            name: "hp".to_string(),
            current: 20,
            starting: 20,
            direction: MetricDirection::Descending,
            threshold_high: None,
            threshold_low: Some(0),
        },
        beat: 0,
        structured_phase: None,
        secondary_stats: None,
        actors: vec![EncounterActor {
            name: "Goblin".to_string(),
            role: "npc".to_string(),
            per_actor_state: std::collections::HashMap::new(),
        }],
        outcome: None,
        resolved: false,
        mood_override: None,
        narrator_hints: vec![],
    }
}

/// Find events on the `encounter` component whose `event=` field matches.
fn find_encounter_events(events: &[WatcherEvent], event_name: &str) -> Vec<WatcherEvent> {
    events
        .iter()
        .filter(|e| {
            e.component == "encounter"
                && e.fields.get("event").and_then(serde_json::Value::as_str) == Some(event_name)
        })
        .cloned()
        .collect()
}

/// Drain every currently-buffered event from the receiver.
fn drain_events(
    rx: &mut tokio::sync::broadcast::Receiver<WatcherEvent>,
) -> Vec<WatcherEvent> {
    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }
    events
}

/// Real integration test: drive `apply_beat_dispatch` from outside the
/// crate's `src/` tree using only the public API, with a live encounter
/// fixture, and verify the canonical `encounter.beat_applied` event fires
/// on the real global telemetry channel with correct attribution.
///
/// This is the integration-test counterpart to the src-level case_a unit
/// test. **Both sets of tests subscribe to the same `subscribe_global`
/// telemetry channel** (the unit test path goes through
/// `crate::test_support::telemetry::fresh_subscriber`, which itself wraps
/// `init_global_channel` + `subscribe_global`). The real distinction is the
/// import path: this integration test calls `apply_beat_dispatch` via
/// `use sidequest_server::apply_beat_dispatch` — the crate's public API —
/// which proves the symbol is reachable from outside the `src/` tree, not
/// just via the in-tree `crate::dispatch::beat::` path the unit tests use.
/// Reviewer pass-2 finding #12 corrected the previous misleading contrast.
#[test]
fn integration_applied_beat_reaches_global_telemetry_channel() {
    // Subscribe to the real global telemetry channel — the same one the
    // GM panel consumes in production. Must be done BEFORE the helper runs
    // so we don't miss the event.
    let _ = init_global_channel();
    let mut rx = subscribe_global().expect("global telemetry channel must initialize");
    // Drain any leftover events from other tests in this integration binary.
    while rx.try_recv().is_ok() {}

    // Build a live GameSnapshot with a combat encounter at full HP.
    let defs: Vec<ConfrontationDef> = vec![serde_yaml::from_str(combat_yaml()).unwrap()];
    let mut snapshot = GameSnapshot {
        encounter: Some(live_combat_encounter()),
        ..GameSnapshot::default()
    };
    let hp_before = snapshot.encounter.as_ref().unwrap().metric.current;

    // Drive the production helper through the public API surface.
    let outcome = apply_beat_dispatch(&mut snapshot, "attack", &defs);

    // Verify the outcome is Applied with the correct carried data.
    match &outcome {
        BeatDispatchOutcome::Applied {
            encounter_type,
            beat_id,
        } => {
            assert_eq!(encounter_type, "combat");
            assert_eq!(beat_id, "attack");
        }
        other => panic!(
            "Integration: apply_beat_dispatch on a live combat encounter must \
             return Applied {{ encounter_type, beat_id }}, got {:?}",
            other
        ),
    }

    // Verify the encounter actually mutated — the helper's contract is
    // that Applied means apply_beat() ran and succeeded.
    let after = snapshot.encounter.as_ref().unwrap();
    assert!(
        after.metric.current < hp_before,
        "Integration: Applied outcome must mean apply_beat actually mutated \
         the metric — got HP {} (was {})",
        after.metric.current,
        hp_before
    );
    assert_eq!(after.beat, 1, "Integration: beat counter must advance on Applied");

    // Verify the canonical event reached the real global telemetry channel.
    let events = drain_events(&mut rx);
    let applied = find_encounter_events(&events, "encounter.beat_applied");
    assert_eq!(
        applied.len(),
        1,
        "Integration: exactly one encounter.beat_applied event must reach \
         the global telemetry channel (the one the GM panel consumes). A \
         dead-but-present apply_beat_dispatch would fail this test even if \
         every src-level unit test passed."
    );
    assert!(
        matches!(applied[0].event_type, WatcherEventType::StateTransition),
        "Integration: encounter.beat_applied must be a StateTransition event"
    );
    assert_eq!(
        applied[0]
            .fields
            .get("source")
            .and_then(|v| v.as_str()),
        Some("narrator_beat_selection"),
        "Integration: the GM-panel-facing event must carry \
         source=narrator_beat_selection for attribution"
    );
    assert_eq!(
        applied[0]
            .fields
            .get("beat_id")
            .and_then(|v| v.as_str()),
        Some("attack"),
        "Integration: the event must record the real beat_id"
    );
    assert_eq!(
        applied[0]
            .fields
            .get("encounter_type")
            .and_then(|v| v.as_str()),
        Some("combat")
    );

    // No misleading events on the clean Applied path — this locks in the
    // pass-2 rework fix for the per-actor breadcrumb and the
    // beat_apply_failed regression.
    assert!(
        find_encounter_events(&events, "encounter.beat_apply_failed").is_empty(),
        "Integration: a clean Applied path must NOT emit beat_apply_failed"
    );
    assert!(
        find_encounter_events(&events, "encounter.beat_skipped_resolved").is_empty(),
        "Integration: a clean Applied path must NOT emit beat_skipped_resolved"
    );
}

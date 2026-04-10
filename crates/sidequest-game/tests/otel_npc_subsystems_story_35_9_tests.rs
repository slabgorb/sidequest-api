//! Story 35-9: OTEL watcher events for NPC subsystems.
//!
//! Verifies that four NPC-decision subsystems — `belief_state`, `disposition`,
//! `npc_actions`, and `gossip` — emit `WatcherEvent`s so the GM panel can
//! observe NPC decision-making in real time (ADR-031 / CLAUDE.md OTEL rule).
//!
//! Each subsystem is exercised directly, and a wiring assertion grep confirms
//! the production code path that reaches each subsystem is still in place
//! (CLAUDE.md A5 — "Every Test Suite Needs a Wiring Test").

use std::collections::HashMap;

use rand::SeedableRng;
use sidequest_game::belief_state::{Belief, BeliefSource, BeliefState};
use sidequest_game::disposition::Disposition;
use sidequest_game::gossip::GossipEngine;
use sidequest_game::npc_actions::{select_npc_action, ScenarioRole};
use sidequest_telemetry::{init_global_channel, subscribe_global, WatcherEvent};

// ---------------------------------------------------------------------------
// Test infrastructure — matches the pattern from
// otel_structured_encounter_story_28_2_tests.rs.
// ---------------------------------------------------------------------------

/// Serialize telemetry tests — the global broadcast channel is shared state,
/// so tests that emit and read events must not run concurrently.
static TELEMETRY_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Initialize the global telemetry channel (idempotent via OnceLock),
/// acquire the serialization lock, drain any stale events, and return a
/// clean receiver.
fn fresh_subscriber() -> (
    std::sync::MutexGuard<'static, ()>,
    tokio::sync::broadcast::Receiver<WatcherEvent>,
) {
    // Recover from a previously-poisoned lock: a panic in an earlier test
    // should fail that test only, not cascade into every subsequent test.
    let guard = TELEMETRY_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _ = init_global_channel();
    let mut rx = subscribe_global().expect("channel must be initialized");
    while rx.try_recv().is_ok() {}
    (guard, rx)
}

/// Drain every currently-buffered event from the receiver.
fn drain_events(rx: &mut tokio::sync::broadcast::Receiver<WatcherEvent>) -> Vec<WatcherEvent> {
    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }
    events
}

/// Find events emitted by `component` whose `action` field matches `action`.
fn find_events(events: &[WatcherEvent], component: &str, action: &str) -> Vec<WatcherEvent> {
    events
        .iter()
        .filter(|e| {
            e.component == component
                && e.fields
                    .get("action")
                    .and_then(serde_json::Value::as_str)
                    == Some(action)
        })
        .cloned()
        .collect()
}

// ===========================================================================
// belief_state — add_belief + update_credibility
// ===========================================================================

#[test]
fn belief_state_add_belief_emits_watcher_event() {
    let (_guard, mut rx) = fresh_subscriber();

    let mut state = BeliefState::new();
    state.add_belief(Belief::Fact {
        subject: "victim".to_string(),
        content: "found in the library".to_string(),
        turn_learned: 3,
        source: BeliefSource::Witnessed,
    });

    let events = drain_events(&mut rx);
    let added = find_events(&events, "belief_state", "belief_added");

    assert!(
        !added.is_empty(),
        "add_belief() must emit belief_state.belief_added; got {} other events",
        events.len()
    );

    let evt = &added[0];
    assert_eq!(
        evt.fields.get("variant").and_then(serde_json::Value::as_str),
        Some("fact"),
        "belief variant must be recorded"
    );
    assert_eq!(
        evt.fields.get("subject").and_then(serde_json::Value::as_str),
        Some("victim")
    );
    assert_eq!(
        evt.fields
            .get("source")
            .and_then(serde_json::Value::as_str),
        Some("witnessed")
    );
    assert_eq!(
        evt.fields
            .get("beliefs_count_after")
            .and_then(serde_json::Value::as_u64),
        Some(1),
        "belief count must reflect post-insert size"
    );
}

#[test]
fn belief_state_update_credibility_emits_watcher_event() {
    let (_guard, mut rx) = fresh_subscriber();

    let mut state = BeliefState::new();
    state.update_credibility("alice", 0.3);

    let events = drain_events(&mut rx);
    let updates = find_events(&events, "belief_state", "credibility_updated");

    assert!(
        !updates.is_empty(),
        "update_credibility() must emit belief_state.credibility_updated"
    );
    let evt = &updates[0];
    assert_eq!(
        evt.fields
            .get("target_npc")
            .and_then(serde_json::Value::as_str),
        Some("alice")
    );
    // `new_score` is stored as f32 then widened to f64 during JSON
    // serialization — compare with tolerance rather than exact equality.
    let new_score = evt
        .fields
        .get("new_score")
        .and_then(serde_json::Value::as_f64)
        .expect("new_score field must be a number");
    assert!(
        (new_score - 0.3).abs() < 1e-5,
        "new_score should be ~0.3, got {new_score}"
    );
    // First update — previous score is absent (serialized as null).
    assert!(
        evt.fields
            .get("previous_score")
            .map(|v| v.is_null())
            .unwrap_or(true),
        "first-time credibility update should have null previous_score"
    );
}

// ===========================================================================
// disposition — apply_delta
// ===========================================================================

#[test]
fn disposition_apply_delta_emits_watcher_event_on_shift() {
    let (_guard, mut rx) = fresh_subscriber();

    let mut disp = Disposition::new(8);
    disp.apply_delta(5); // 8 → 13, crosses neutral/friendly threshold

    let events = drain_events(&mut rx);
    let shifted = find_events(&events, "disposition", "disposition_shifted");

    assert!(
        !shifted.is_empty(),
        "apply_delta() must emit disposition.disposition_shifted"
    );
    let evt = &shifted[0];
    assert_eq!(
        evt.fields.get("delta").and_then(serde_json::Value::as_i64),
        Some(5)
    );
    assert_eq!(
        evt.fields
            .get("old_value")
            .and_then(serde_json::Value::as_i64),
        Some(8)
    );
    assert_eq!(
        evt.fields
            .get("new_value")
            .and_then(serde_json::Value::as_i64),
        Some(13)
    );
    assert_eq!(
        evt.fields
            .get("old_attitude")
            .and_then(serde_json::Value::as_str),
        Some("neutral")
    );
    assert_eq!(
        evt.fields
            .get("new_attitude")
            .and_then(serde_json::Value::as_str),
        Some("friendly")
    );
    assert_eq!(
        evt.fields
            .get("attitude_changed")
            .and_then(serde_json::Value::as_bool),
        Some(true),
        "threshold crossing must flag attitude_changed=true"
    );
}

// ===========================================================================
// npc_actions — select_npc_action
// ===========================================================================

#[test]
fn select_npc_action_emits_watcher_event() {
    let (_guard, mut rx) = fresh_subscriber();

    let beliefs = BeliefState::new();
    let mut rng = rand::rngs::StdRng::from_seed([42u8; 32]);
    let _action = select_npc_action(
        "suspect_01",
        &ScenarioRole::Guilty,
        &beliefs,
        0.9,
        &mut rng,
    );

    let events = drain_events(&mut rx);
    let selected = find_events(&events, "npc_actions", "action_selected");

    assert!(
        !selected.is_empty(),
        "select_npc_action() must emit npc_actions.action_selected"
    );
    let evt = &selected[0];
    assert_eq!(
        evt.fields.get("npc_id").and_then(serde_json::Value::as_str),
        Some("suspect_01"),
        "npc_id field must be populated (no longer underscored-ignored)"
    );
    assert_eq!(
        evt.fields.get("role").and_then(serde_json::Value::as_str),
        Some("guilty")
    );
    let tension = evt
        .fields
        .get("tension")
        .and_then(serde_json::Value::as_f64)
        .expect("tension field must be a number");
    assert!(
        (tension - 0.9).abs() < 1e-5,
        "tension should be ~0.9, got {tension}"
    );
    assert!(
        evt.fields
            .get("selected_variant")
            .and_then(serde_json::Value::as_str)
            .is_some(),
        "selected_variant must be populated"
    );
    assert!(
        evt.fields
            .get("available_count")
            .and_then(serde_json::Value::as_u64)
            .map(|n| n >= 1)
            .unwrap_or(false),
        "available_count must be >= 1"
    );
}

// ===========================================================================
// gossip — propagate_turn
// ===========================================================================

#[test]
fn gossip_propagate_turn_emits_watcher_event() {
    let (_guard, mut rx) = fresh_subscriber();

    // Two NPCs, alice and bob, fully connected. Alice holds one fact;
    // after propagation bob should have picked it up as a claim.
    let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();
    adjacency.insert("alice".to_string(), vec!["bob".to_string()]);
    adjacency.insert("bob".to_string(), vec!["alice".to_string()]);

    let mut alice = BeliefState::new();
    alice.add_belief(Belief::Fact {
        subject: "treasure".to_string(),
        content: "buried under the oak".to_string(),
        turn_learned: 1,
        source: BeliefSource::Witnessed,
    });
    let bob = BeliefState::new();

    let mut npcs: HashMap<String, BeliefState> = HashMap::new();
    npcs.insert("alice".to_string(), alice);
    npcs.insert("bob".to_string(), bob);

    // Drain the belief_added events from fixture setup so we only measure
    // the gossip propagation below.
    let _ = drain_events(&mut rx);

    let engine = GossipEngine::new(adjacency);
    let result = engine.propagate_turn(&mut npcs, 2);

    let events = drain_events(&mut rx);
    let gossip_events = find_events(&events, "gossip", "turn_propagated");

    assert!(
        !gossip_events.is_empty(),
        "propagate_turn() must emit gossip.turn_propagated"
    );
    let evt = &gossip_events[0];
    assert_eq!(
        evt.fields.get("turn").and_then(serde_json::Value::as_u64),
        Some(2)
    );
    assert_eq!(
        evt.fields
            .get("npc_count")
            .and_then(serde_json::Value::as_u64),
        Some(2)
    );
    assert_eq!(
        evt.fields
            .get("claims_spread")
            .and_then(serde_json::Value::as_u64),
        Some(u64::from(result.claims_spread))
    );

    // Propagation should have added a belief to bob, which also emits a
    // belief_state.belief_added event — confirming cross-subsystem telemetry
    // flows through the same channel.
    let added = find_events(&events, "belief_state", "belief_added");
    assert!(
        !added.is_empty(),
        "gossip propagation should trigger at least one belief_added event"
    );
}

// ===========================================================================
// A5 wiring assertions — production code paths reach these subsystems.
// ===========================================================================
//
// These grep-based checks guard against silent removal of the production
// callers. If any of these assertions fail, the subsystem has been
// orphaned and the OTEL events are no longer reachable.

#[test]
fn wiring_belief_state_reached_by_dispatch_connect() {
    let src = include_str!("../../sidequest-server/src/dispatch/connect.rs");
    assert!(
        src.contains("belief_state.add_belief"),
        "dispatch/connect.rs must call belief_state.add_belief() — \
         without this call the belief_state OTEL events are unreachable \
         from production code."
    );
}

#[test]
fn wiring_disposition_reached_by_state_apply_patch() {
    let src = include_str!("../src/state.rs");
    assert!(
        src.contains("disposition.apply_delta"),
        "state.rs must call npc.disposition.apply_delta() — \
         without this call the disposition OTEL events are unreachable \
         from production code."
    );
}

#[test]
fn wiring_gossip_and_npc_actions_reached_by_scenario_state() {
    let src = include_str!("../src/scenario_state.rs");
    assert!(
        src.contains("gossip_engine.propagate_turn"),
        "scenario_state.rs must call GossipEngine::propagate_turn() — \
         without this call the gossip OTEL events are unreachable from \
         production code."
    );
    assert!(
        src.contains("select_npc_action("),
        "scenario_state.rs must call select_npc_action() — without this \
         call the npc_actions OTEL events are unreachable from production \
         code."
    );
}

#[test]
fn wiring_scenario_state_reached_by_dispatch_pipeline() {
    let src = include_str!("../../sidequest-server/src/dispatch/mod.rs");
    assert!(
        src.contains("process_between_turns("),
        "dispatch/mod.rs must call scenario_state.process_between_turns() — \
         this is the entry point that drives gossip + npc_actions + \
         belief_state updates during a turn. Without it the entire \
         scenario subsystem is dark."
    );
}


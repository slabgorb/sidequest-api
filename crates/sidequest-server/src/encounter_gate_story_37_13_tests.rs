//! Story 37-13: Encounter creation gate silent-drop fix — RED phase tests.
//!
//! The existing gate in `dispatch/mod.rs:1773-1819` silently discards a new
//! `confrontation_type` when `ctx.snapshot.encounter` is `Some(unresolved)`.
//! No OTEL event, no state transition, no warning. This is the root cause for
//! 37-12 and a direct CLAUDE.md "No Silent Fallbacks" violation.
//!
//! These tests spec a pure helper
//! `crate::dispatch::apply_confrontation_gate(...)` that encapsulates every
//! decision branch and emits a distinct `WatcherEvent` for each. Dev (GREEN)
//! will extract the inline gate code into that helper and update
//! `dispatch_player_action` to call it.
//!
//! Test matrix — one case per Case letter in context-story-37-13.md:
//!   A. None → create
//!   B. Some(resolved) → create
//!   C. Some(unresolved, same type) → no-op redeclare
//!   D. Some(unresolved, different, beat == 0) → replace
//!   E. Some(unresolved, different, beat > 0) → reject
//!   F. Unknown type (def missing) → validation warning
//!
//! Plus one source-scanning wiring test that verifies `dispatch/mod.rs` no
//! longer contains the silent inline branch and actually calls the helper.

use sidequest_agents::orchestrator::NpcMention;
use sidequest_game::encounter::{
    EncounterActor, EncounterMetric, MetricDirection, StructuredEncounter,
};
use sidequest_game::state::GameSnapshot;
use sidequest_genre::ConfrontationDef;
use sidequest_telemetry::{init_global_channel, subscribe_global, WatcherEvent, WatcherEventType};

use crate::dispatch::apply_confrontation_gate;
use crate::dispatch::encounter_gate::ConfrontationGateOutcome;

// ---------------------------------------------------------------------------
// Test infrastructure — global broadcast channel is shared state, so each
// test serializes on TELEMETRY_LOCK and drains stale events before exercising.
// Mirrors otel_dice_spans_34_11_tests.rs.
// ---------------------------------------------------------------------------

static TELEMETRY_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn fresh_subscriber() -> (
    std::sync::MutexGuard<'static, ()>,
    tokio::sync::broadcast::Receiver<WatcherEvent>,
) {
    let guard = TELEMETRY_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _ = init_global_channel();
    let mut rx = subscribe_global().expect("telemetry channel must be initialized");
    while rx.try_recv().is_ok() {}
    (guard, rx)
}

fn drain_events(rx: &mut tokio::sync::broadcast::Receiver<WatcherEvent>) -> Vec<WatcherEvent> {
    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }
    events
}

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

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

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

fn standoff_yaml() -> &'static str {
    r#"
type: standoff
label: "Tense Standoff"
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
  - id: bluff
    label: "Bluff"
    metric_delta: 3
    stat_check: NERVE
  - id: draw
    label: "Draw"
    metric_delta: 5
    stat_check: DRAW
    resolution: true
"#
}

fn load_defs() -> Vec<ConfrontationDef> {
    vec![
        serde_yaml::from_str(combat_yaml()).expect("combat yaml parses"),
        serde_yaml::from_str(standoff_yaml()).expect("standoff yaml parses"),
    ]
}

fn npc(name: &str) -> NpcMention {
    NpcMention {
        name: name.to_string(),
        pronouns: String::new(),
        role: String::new(),
        appearance: String::new(),
        is_new: false,
    }
}

fn empty_snapshot() -> GameSnapshot {
    GameSnapshot::default()
}

fn snapshot_with(encounter: StructuredEncounter) -> GameSnapshot {
    GameSnapshot {
        encounter: Some(encounter),
        ..GameSnapshot::default()
    }
}

/// Build a `StructuredEncounter` of the given type with a specific beat counter.
/// Used to pre-populate `snapshot.encounter` for gate tests. Not a production
/// builder — a test-local shortcut so we don't need the full narrator pipeline.
fn existing_encounter(encounter_type: &str, beat: u32, resolved: bool) -> StructuredEncounter {
    StructuredEncounter {
        encounter_type: encounter_type.to_string(),
        metric: EncounterMetric {
            name: "hp".to_string(),
            current: 15,
            starting: 20,
            direction: MetricDirection::Descending,
            threshold_high: None,
            threshold_low: Some(0),
        },
        beat,
        structured_phase: None,
        secondary_stats: None,
        actors: vec![EncounterActor {
            name: "Old Combatant".to_string(),
            role: "combatant".to_string(),
            per_actor_state: {
                let mut m = std::collections::HashMap::new();
                m.insert("stance".to_string(), serde_json::json!("guarded"));
                m
            },
        }],
        outcome: None,
        resolved,
        mood_override: None,
        narrator_hints: vec![],
    }
}

// ---------------------------------------------------------------------------
// Case A — No current encounter → create
// ---------------------------------------------------------------------------

#[test]
fn case_a_no_current_encounter_creates_new() {
    let (_guard, mut rx) = fresh_subscriber();
    let defs = load_defs();
    let mut snapshot = empty_snapshot();

    let outcome = apply_confrontation_gate(&mut snapshot, "combat", &defs, &[]);

    assert_eq!(outcome, ConfrontationGateOutcome::Created);
    assert!(
        snapshot.encounter.is_some(),
        "new encounter must be stored on snapshot"
    );
    assert_eq!(
        snapshot.encounter.as_ref().unwrap().encounter_type,
        "combat"
    );
    assert_eq!(snapshot.encounter.as_ref().unwrap().beat, 0);
    assert!(!snapshot.encounter.as_ref().unwrap().resolved);

    let events = drain_events(&mut rx);
    let created = find_encounter_events(&events, "encounter.created");
    assert_eq!(
        created.len(),
        1,
        "Case A must emit exactly one encounter.created event"
    );
    assert!(
        matches!(created[0].event_type, WatcherEventType::StateTransition),
        "encounter.created must be a StateTransition event"
    );
    assert_eq!(
        created[0]
            .fields
            .get("encounter_type")
            .and_then(|v| v.as_str()),
        Some("combat")
    );
}

// ---------------------------------------------------------------------------
// Case B — Current encounter resolved → create new
// ---------------------------------------------------------------------------

#[test]
fn case_b_resolved_current_encounter_creates_new() {
    let (_guard, mut rx) = fresh_subscriber();
    let defs = load_defs();
    let mut snapshot = snapshot_with(existing_encounter("standoff", 3, true));

    let outcome = apply_confrontation_gate(&mut snapshot, "combat", &defs, &[]);

    assert_eq!(outcome, ConfrontationGateOutcome::Created);
    assert_eq!(
        snapshot.encounter.as_ref().unwrap().encounter_type,
        "combat",
        "resolved old encounter should be replaced by new type"
    );
    assert_eq!(snapshot.encounter.as_ref().unwrap().beat, 0);

    let events = drain_events(&mut rx);
    let created = find_encounter_events(&events, "encounter.created");
    assert_eq!(created.len(), 1);
}

// ---------------------------------------------------------------------------
// Case C — Same type, unresolved → no-op redeclare
// ---------------------------------------------------------------------------

#[test]
fn case_c_same_type_redeclare_is_noop() {
    let (_guard, mut rx) = fresh_subscriber();
    let defs = load_defs();
    let mut snapshot = snapshot_with(existing_encounter("combat", 2, false));
    let before_metric = snapshot.encounter.as_ref().unwrap().metric.current;
    let before_actors = snapshot.encounter.as_ref().unwrap().actors.clone();
    let before_beat = snapshot.encounter.as_ref().unwrap().beat;

    let outcome = apply_confrontation_gate(&mut snapshot, "combat", &defs, &[]);

    assert_eq!(outcome, ConfrontationGateOutcome::Redeclared);
    let after = snapshot.encounter.as_ref().unwrap();
    assert_eq!(after.metric.current, before_metric, "metric unchanged");
    assert_eq!(after.beat, before_beat, "beat counter unchanged");
    assert_eq!(
        after.actors.len(),
        before_actors.len(),
        "actor list unchanged"
    );

    let events = drain_events(&mut rx);
    let redeclare = find_encounter_events(&events, "encounter.redeclare_noop");
    assert_eq!(
        redeclare.len(),
        1,
        "Case C must emit exactly one encounter.redeclare_noop"
    );
    assert!(
        find_encounter_events(&events, "encounter.created").is_empty(),
        "Case C must NOT emit encounter.created"
    );
    assert!(
        find_encounter_events(&events, "encounter.replaced_pre_beat").is_empty(),
        "Case C must NOT emit encounter.replaced_pre_beat"
    );
}

// ---------------------------------------------------------------------------
// Case D — Different type, unresolved, beat == 0 → replace
// ---------------------------------------------------------------------------

#[test]
fn case_d_different_type_pre_beat_replaces() {
    let (_guard, mut rx) = fresh_subscriber();
    let defs = load_defs();
    let mut snapshot = snapshot_with(existing_encounter("standoff", 0, false));

    let outcome = apply_confrontation_gate(&mut snapshot, "combat", &defs, &[]);

    assert_eq!(outcome, ConfrontationGateOutcome::ReplacedPreBeat);
    let after = snapshot.encounter.as_ref().unwrap();
    assert_eq!(
        after.encounter_type, "combat",
        "new encounter type must replace old"
    );
    assert_eq!(after.beat, 0, "replacement starts fresh at beat 0");
    assert!(!after.resolved, "replacement is not resolved");

    let events = drain_events(&mut rx);
    let replaced = find_encounter_events(&events, "encounter.replaced_pre_beat");
    assert_eq!(
        replaced.len(),
        1,
        "Case D must emit exactly one encounter.replaced_pre_beat"
    );
    assert_eq!(
        replaced[0]
            .fields
            .get("previous_encounter_type")
            .and_then(|v| v.as_str()),
        Some("standoff"),
        "replacement event must record the previous encounter type"
    );
    assert_eq!(
        replaced[0]
            .fields
            .get("encounter_type")
            .and_then(|v| v.as_str()),
        Some("combat")
    );
}

#[test]
fn case_d_replacement_repopulates_actors_from_snapshot_and_npcs() {
    let (_guard, _rx) = fresh_subscriber();
    let defs = load_defs();
    let mut snapshot = snapshot_with(existing_encounter("standoff", 0, false));
    let npcs = vec![npc("Toggler Copperjaw"), npc("Nub")];

    let outcome = apply_confrontation_gate(&mut snapshot, "combat", &defs, &npcs);

    assert_eq!(outcome, ConfrontationGateOutcome::ReplacedPreBeat);
    let actors = &snapshot.encounter.as_ref().unwrap().actors;

    // Players: GameSnapshot::default() has zero characters, so player count is 0.
    // NPC actors come from the narrator's npcs_present list.
    let npc_names: Vec<&str> = actors
        .iter()
        .filter(|a| a.role == "npc")
        .map(|a| a.name.as_str())
        .collect();
    assert!(
        npc_names.contains(&"Toggler Copperjaw"),
        "replacement must pull NPCs from narrator_npcs, got: {npc_names:?}"
    );
    assert!(
        npc_names.contains(&"Nub"),
        "replacement must pull all NPCs from narrator_npcs, got: {npc_names:?}"
    );

    // Critically: the old "Old Combatant" actor must NOT carry over from the
    // previous standoff encounter. A replace is a fresh actor set.
    assert!(
        !actors.iter().any(|a| a.name == "Old Combatant"),
        "replacement must NOT carry forward actors from the old encounter"
    );
}

#[test]
fn case_d_replacement_drops_old_per_actor_state() {
    let (_guard, _rx) = fresh_subscriber();
    let defs = load_defs();
    // Old encounter had an actor with populated per_actor_state.
    let mut snapshot = snapshot_with(existing_encounter("standoff", 0, false));

    let _ = apply_confrontation_gate(&mut snapshot, "combat", &defs, &[npc("Toggler Copperjaw")]);

    let actors = &snapshot.encounter.as_ref().unwrap().actors;
    for actor in actors {
        assert!(
            actor.per_actor_state.is_empty(),
            "new encounter actors must start with empty per_actor_state (leak check), \
             found {:?} on {}",
            actor.per_actor_state,
            actor.name
        );
    }
}

// ---------------------------------------------------------------------------
// Case E — Different type, unresolved, mid-encounter → reject with warning
// ---------------------------------------------------------------------------

#[test]
fn case_e_different_type_mid_encounter_rejects_with_validation_warning() {
    let (_guard, mut rx) = fresh_subscriber();
    let defs = load_defs();
    let mut snapshot = snapshot_with(existing_encounter("standoff", 3, false));
    let before = snapshot.encounter.clone().unwrap();

    let outcome = apply_confrontation_gate(&mut snapshot, "combat", &defs, &[]);

    assert_eq!(outcome, ConfrontationGateOutcome::RejectedMidEncounter);
    let after = snapshot.encounter.as_ref().unwrap();
    assert_eq!(
        after.encounter_type, before.encounter_type,
        "rejected gate must NOT mutate encounter_type"
    );
    assert_eq!(
        after.beat, before.beat,
        "rejected gate must NOT touch beat counter"
    );
    assert_eq!(
        after.metric.current, before.metric.current,
        "rejected gate must NOT touch metric"
    );
    assert_eq!(
        after.actors.len(),
        before.actors.len(),
        "rejected gate must NOT touch actors"
    );

    let events = drain_events(&mut rx);
    let rejected = find_encounter_events(&events, "encounter.new_type_rejected_mid_encounter");
    assert_eq!(
        rejected.len(),
        1,
        "Case E must emit exactly one encounter.new_type_rejected_mid_encounter"
    );
    assert!(
        matches!(rejected[0].event_type, WatcherEventType::ValidationWarning),
        "rejection event must use ValidationWarning type — this is a narrator/state divergence"
    );
    assert_eq!(
        rejected[0]
            .fields
            .get("previous_encounter_type")
            .and_then(|v| v.as_str()),
        Some("standoff")
    );
    assert_eq!(
        rejected[0]
            .fields
            .get("encounter_type")
            .and_then(|v| v.as_str()),
        Some("combat"),
        "rejection event must record the incoming (rejected) type"
    );
    assert_eq!(
        rejected[0]
            .fields
            .get("beat_count")
            .and_then(|v| v.as_u64()),
        Some(3),
        "rejection event must record the current beat count for the GM panel"
    );
}

// ---------------------------------------------------------------------------
// Case F — Unknown confrontation type → validation warning
// ---------------------------------------------------------------------------

#[test]
fn case_f_unknown_confrontation_type_emits_validation_warning() {
    let (_guard, mut rx) = fresh_subscriber();
    let defs = load_defs();
    let mut snapshot = empty_snapshot();

    let outcome = apply_confrontation_gate(&mut snapshot, "interpretive_dance", &defs, &[]);

    assert_eq!(outcome, ConfrontationGateOutcome::UnknownType);
    assert!(
        snapshot.encounter.is_none(),
        "unknown type must NOT populate encounter"
    );

    let events = drain_events(&mut rx);
    let failed = find_encounter_events(&events, "encounter.creation_failed_unknown_type");
    assert_eq!(
        failed.len(),
        1,
        "Case F must emit exactly one encounter.creation_failed_unknown_type"
    );
    assert!(
        matches!(failed[0].event_type, WatcherEventType::ValidationWarning),
        "encounter.creation_failed_unknown_type must be ValidationWarning"
    );
    assert_eq!(
        failed[0]
            .fields
            .get("encounter_type")
            .and_then(|v| v.as_str()),
        Some("interpretive_dance")
    );
}

// ---------------------------------------------------------------------------
// Wiring test — the production dispatch path must actually call the helper
// and the old silent-drop branch must be gone. Per CLAUDE.md, every test
// suite needs at least one wiring test so we don't ship unwired green tests.
// ---------------------------------------------------------------------------

#[test]
fn wiring_dispatch_mod_calls_apply_confrontation_gate() {
    let source = include_str!("dispatch/mod.rs");

    assert!(
        source.contains("apply_confrontation_gate"),
        "dispatch/mod.rs must call apply_confrontation_gate — the gate logic \
         cannot live as an inline block any longer"
    );

    // The old silent-drop shape was:
    //   if let Some(ref confrontation_type) = result.confrontation {
    //       if ctx.snapshot.encounter.is_none()
    //           || ctx.snapshot.encounter.as_ref().is_some_and(|e| e.resolved)
    //       { ... } // <-- no else
    //   }
    //
    // If Dev leaves this pattern in place without delegating to the helper,
    // the bug is unfixed regardless of what the unit tests say.
    let has_inline_is_none_or_resolved = source.contains(
        "ctx.snapshot.encounter.is_none()\n            || ctx.snapshot.encounter.as_ref().is_some_and(|e| e.resolved)",
    );
    assert!(
        !has_inline_is_none_or_resolved,
        "dispatch/mod.rs still contains the old inline silent-drop branch — \
         delegate to apply_confrontation_gate instead"
    );
}

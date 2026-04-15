//! Tests for `dispatch::apply_beat_dispatch` — every branch observable.
//!
//! Story 37-14 sibling to 37-13. The encounter creation gate (37-13) made
//! every narrator-declared confrontation transition emit a WatcherEvent.
//! This story does the same thing one layer down: every narrator-emitted
//! `beat_selection` must produce exactly one canonical `WatcherEvent` on
//! the `encounter` component, keyed by the `event` field so the GM panel's
//! standard filter picks it up.
//!
//! Playtest 2 symptom: narrator emitted 2–3 beat_selections per turn for
//! roughly twenty minutes, and zero `encounter.beat_applied` events fired.
//! Root causes found during RED design:
//!
//! 1. `dispatch/mod.rs` wrapped the beat-selection loop in
//!    `if ctx.snapshot.encounter.is_some()`, silently skipping every beat
//!    when no encounter was live. No OTEL.
//! 2. `dispatch/beat.rs` only emitted a WatcherEvent on the
//!    `encounter.beat_id.unknown` branch. The "no active encounter",
//!    "missing confrontation def", and "apply_beat returned Err" branches
//!    were `tracing::warn!`-only — invisible to the GM panel.
//! 3. `StructuredEncounter::apply_beat` emitted its OTEL event using the
//!    field key `action: "beat_applied"` (not `event: "encounter.beat_applied"`),
//!    so any filter scanning the `event` key silently missed it.
//!
//! The fix is to mirror 37-13's `apply_confrontation_gate` pattern:
//! a typed `BeatDispatchOutcome` returned by a single helper function, with
//! every branch emitting exactly one canonical event using the `event` field
//! key and `source: "narrator_beat_selection"` for attribution.
//!
//! Test matrix — one case per variant in `BeatDispatchOutcome`:
//!   A. Encounter live, def found, beat found, apply_beat OK → Applied
//!   B. `snapshot.encounter` is None                         → NoEncounter
//!   C. Encounter live but its type has no ConfrontationDef  → NoDef
//!   D. Encounter live, def found, beat_id not in def.beats  → UnknownBeatId
//!   E. Encounter live, def found, beat found, apply_beat Err → ApplyFailed
//!
//! Plus a multi-call regression guard (the 2-3-per-turn playtest symptom)
//! and two source-scanning wiring tests against `dispatch/mod.rs`.

use sidequest_game::encounter::{
    EncounterActor, EncounterMetric, MetricDirection, StructuredEncounter,
};
use sidequest_game::state::GameSnapshot;
use sidequest_genre::ConfrontationDef;
use sidequest_telemetry::{WatcherEvent, WatcherEventType};

use crate::dispatch::beat::{apply_beat_dispatch, BeatDispatchOutcome};
use crate::test_support::telemetry::{drain_events, fresh_subscriber};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

/// Every event the beat dispatch helper emits must carry
/// `source = "narrator_beat_selection"` so the GM panel can attribute it to
/// the narrator-driven beat subsystem (as distinct from the structured
/// `BeatSelection` protocol preprocessing in `lib.rs`, which is a different
/// attribution string). Centralising this check keeps per-case tests focused
/// on their own invariants while guaranteeing the attribution field never
/// silently drops.
fn assert_source_is_narrator_beat(event: &WatcherEvent) {
    assert_eq!(
        event.fields.get("source").and_then(|v| v.as_str()),
        Some("narrator_beat_selection"),
        "beat dispatch events must carry source=narrator_beat_selection"
    );
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// A combat ConfrontationDef with two beats: `attack` (damage, non-terminal)
/// and `finisher` (flagged `resolution: true`). `attack` is the canonical
/// happy-path beat for Case A. `finisher` only exists so tests that need a
/// resolution-capable beat have one available — tests that don't use it can
/// ignore it.
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
  - id: finisher
    label: "Finisher"
    metric_delta: -20
    stat_check: STRENGTH
    resolution: true
"#
}

fn load_defs() -> Vec<ConfrontationDef> {
    vec![serde_yaml::from_str(combat_yaml()).expect("combat yaml parses")]
}

/// Build a live `StructuredEncounter` of the given type sitting at beat `beat`.
/// Descending HP metric so the "already resolved" case E can be built by
/// flipping `resolved = true` without having to drain HP to the threshold.
fn live_encounter(encounter_type: &str, beat: u32, resolved: bool) -> StructuredEncounter {
    StructuredEncounter {
        encounter_type: encounter_type.to_string(),
        metric: EncounterMetric {
            name: "hp".to_string(),
            current: 20,
            starting: 20,
            direction: MetricDirection::Descending,
            threshold_high: None,
            threshold_low: Some(0),
        },
        beat,
        structured_phase: None,
        secondary_stats: None,
        actors: vec![EncounterActor {
            name: "Combatant".to_string(),
            role: "npc".to_string(),
            per_actor_state: std::collections::HashMap::new(),
        }],
        outcome: None,
        resolved,
        mood_override: None,
        narrator_hints: vec![],
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

// ---------------------------------------------------------------------------
// Case A — live encounter + def + known beat_id → Applied
// ---------------------------------------------------------------------------

#[test]
fn case_a_happy_path_emits_encounter_beat_applied() {
    let (_guard, mut rx) = fresh_subscriber();
    let defs = load_defs();
    let mut snapshot = snapshot_with(live_encounter("combat", 0, false));
    let hp_before = snapshot.encounter.as_ref().unwrap().metric.current;

    let outcome = apply_beat_dispatch(&mut snapshot, "attack", &defs);

    assert_eq!(outcome, BeatDispatchOutcome::Applied);

    let after = snapshot.encounter.as_ref().unwrap();
    assert!(
        after.metric.current < hp_before,
        "apply_beat must have reduced the HP metric — without this check the \
         helper could emit encounter.beat_applied without actually applying \
         the beat (vacuous success path)"
    );
    assert_eq!(after.beat, 1, "beat counter must advance on a successful apply");

    let events = drain_events(&mut rx);
    let applied = find_encounter_events(&events, "encounter.beat_applied");
    assert_eq!(
        applied.len(),
        1,
        "Case A must emit exactly one encounter.beat_applied event (event= field, \
         not action= field — the GM panel filters on event=)"
    );
    assert!(
        matches!(applied[0].event_type, WatcherEventType::StateTransition),
        "encounter.beat_applied must be a StateTransition event"
    );
    assert_eq!(
        applied[0]
            .fields
            .get("beat_id")
            .and_then(|v| v.as_str()),
        Some("attack"),
        "event must record the beat_id that was applied"
    );
    assert_eq!(
        applied[0]
            .fields
            .get("encounter_type")
            .and_then(|v| v.as_str()),
        Some("combat"),
        "event must record the encounter type for GM panel grouping"
    );
    assert_source_is_narrator_beat(&applied[0]);
}

// ---------------------------------------------------------------------------
// Case B — no active encounter → NoEncounter (the primary playtest bug)
// ---------------------------------------------------------------------------

#[test]
fn case_b_no_active_encounter_emits_warning_not_silent_drop() {
    let (_guard, mut rx) = fresh_subscriber();
    let defs = load_defs();
    let mut snapshot = empty_snapshot();

    let outcome = apply_beat_dispatch(&mut snapshot, "attack", &defs);

    assert_eq!(outcome, BeatDispatchOutcome::NoEncounter);
    assert!(
        snapshot.encounter.is_none(),
        "Case B must NOT fabricate an encounter just to have something to apply the beat to"
    );

    let events = drain_events(&mut rx);
    let warn = find_encounter_events(&events, "encounter.beat_no_encounter");
    assert_eq!(
        warn.len(),
        1,
        "Case B must emit exactly one encounter.beat_no_encounter event — this \
         is the silent-drop path that caused 2-3 beat_selections per turn to \
         vanish for 20 minutes in playtest 2"
    );
    assert!(
        matches!(warn[0].event_type, WatcherEventType::ValidationWarning),
        "beat_no_encounter must be a ValidationWarning — the narrator picked a \
         beat with nothing to apply it to, which is a narrator/state divergence"
    );
    assert_eq!(
        warn[0].fields.get("beat_id").and_then(|v| v.as_str()),
        Some("attack"),
        "event must record the dropped beat_id for post-mortem"
    );
    assert_source_is_narrator_beat(&warn[0]);

    assert!(
        find_encounter_events(&events, "encounter.beat_applied").is_empty(),
        "Case B must NOT emit encounter.beat_applied — there is no encounter \
         to apply against"
    );
}

// ---------------------------------------------------------------------------
// Case C — encounter live but no ConfrontationDef matches its type → NoDef
// ---------------------------------------------------------------------------

#[test]
fn case_c_missing_confrontation_def_emits_warning() {
    let (_guard, mut rx) = fresh_subscriber();
    let defs = load_defs();
    // Encounter claims to be type "interpretive_dance", but the only loaded
    // def is "combat". This is a state/def mismatch — the dispatch layer
    // cannot resolve the beat without a def, but the current code silently
    // tracing::warn!'s and returns.
    let mut snapshot = snapshot_with(live_encounter("interpretive_dance", 2, false));
    let before = snapshot.encounter.clone().unwrap();

    let outcome = apply_beat_dispatch(&mut snapshot, "attack", &defs);

    assert_eq!(outcome, BeatDispatchOutcome::NoDef);
    let after = snapshot.encounter.as_ref().unwrap();
    assert_eq!(
        after.beat, before.beat,
        "no-def path must NOT advance the beat counter"
    );
    assert_eq!(
        after.metric.current, before.metric.current,
        "no-def path must NOT mutate the metric"
    );

    let events = drain_events(&mut rx);
    let warn = find_encounter_events(&events, "encounter.beat_no_def");
    assert_eq!(
        warn.len(),
        1,
        "Case C must emit exactly one encounter.beat_no_def event — a live \
         encounter whose type has no def is a configuration bug that must be \
         visible on the GM panel, not buried in stdout"
    );
    assert!(
        matches!(warn[0].event_type, WatcherEventType::ValidationWarning),
        "beat_no_def must be a ValidationWarning"
    );
    assert_eq!(
        warn[0]
            .fields
            .get("encounter_type")
            .and_then(|v| v.as_str()),
        Some("interpretive_dance"),
        "event must record the unresolvable encounter type"
    );
    assert_eq!(
        warn[0].fields.get("beat_id").and_then(|v| v.as_str()),
        Some("attack")
    );
    assert_source_is_narrator_beat(&warn[0]);
}

// ---------------------------------------------------------------------------
// Case D — beat_id not in def.beats → UnknownBeatId
// ---------------------------------------------------------------------------

#[test]
fn case_d_unknown_beat_id_emits_warning_with_available_ids() {
    let (_guard, mut rx) = fresh_subscriber();
    let defs = load_defs();
    let mut snapshot = snapshot_with(live_encounter("combat", 0, false));
    let before = snapshot.encounter.clone().unwrap();

    // "parry" is not in the fixture — combat only has attack + finisher.
    let outcome = apply_beat_dispatch(&mut snapshot, "parry", &defs);

    assert_eq!(outcome, BeatDispatchOutcome::UnknownBeatId);
    let after = snapshot.encounter.as_ref().unwrap();
    assert_eq!(
        after.beat, before.beat,
        "unknown-beat path must NOT advance the beat counter"
    );
    assert_eq!(
        after.metric.current, before.metric.current,
        "unknown-beat path must NOT mutate the metric"
    );

    let events = drain_events(&mut rx);
    let warn = find_encounter_events(&events, "encounter.beat_id.unknown");
    assert_eq!(
        warn.len(),
        1,
        "Case D must emit exactly one encounter.beat_id.unknown event"
    );
    assert!(
        matches!(warn[0].event_type, WatcherEventType::ValidationWarning),
        "beat_id.unknown must be a ValidationWarning"
    );
    assert_eq!(
        warn[0].fields.get("beat_id").and_then(|v| v.as_str()),
        Some("parry"),
        "event must record the unknown beat_id the narrator submitted"
    );
    assert_eq!(
        warn[0]
            .fields
            .get("encounter_type")
            .and_then(|v| v.as_str()),
        Some("combat")
    );
    // The available_ids field exists in the current code — preserve it so
    // the GM panel can show the user what was actually valid at the moment
    // of the drop. This is a diagnostic affordance, not cosmetic.
    let available = warn[0]
        .fields
        .get("available_ids")
        .and_then(|v| v.as_str())
        .expect("available_ids must be populated for unknown-beat diagnostics");
    assert!(
        available.contains("attack"),
        "available_ids must list the real beat ids so the GM can see what the \
         narrator should have picked, got: {available}"
    );
    assert!(
        available.contains("finisher"),
        "available_ids must list every beat, not just the first one, got: {available}"
    );
    assert_source_is_narrator_beat(&warn[0]);
}

// ---------------------------------------------------------------------------
// Case E — apply_beat returns Err (already resolved) → ApplyFailed
// ---------------------------------------------------------------------------

#[test]
fn case_e_apply_beat_on_resolved_encounter_emits_apply_failed() {
    let (_guard, mut rx) = fresh_subscriber();
    let defs = load_defs();
    // Resolved encounter: apply_beat() returns Err("encounter is already resolved").
    // This is distinct from Case D (unknown beat_id) — here the beat exists
    // in the def but the encounter state rejects the apply. The current code
    // silently tracing::warn!'s this branch.
    let mut snapshot = snapshot_with(live_encounter("combat", 2, true));

    let outcome = apply_beat_dispatch(&mut snapshot, "attack", &defs);

    assert_eq!(outcome, BeatDispatchOutcome::ApplyFailed);

    let events = drain_events(&mut rx);
    let failed = find_encounter_events(&events, "encounter.beat_apply_failed");
    assert_eq!(
        failed.len(),
        1,
        "Case E must emit exactly one encounter.beat_apply_failed event — \
         apply_beat returning Err after the lookup succeeded is a state \
         invariant violation that must reach the GM panel"
    );
    assert!(
        matches!(failed[0].event_type, WatcherEventType::ValidationWarning),
        "beat_apply_failed must be a ValidationWarning"
    );
    assert_eq!(
        failed[0].fields.get("beat_id").and_then(|v| v.as_str()),
        Some("attack"),
        "event must record the beat_id that failed to apply"
    );
    // The error string from apply_beat must be preserved so the GM panel
    // shows WHY it failed, not just that it failed.
    let err = failed[0]
        .fields
        .get("error")
        .and_then(|v| v.as_str())
        .expect("beat_apply_failed must carry the apply_beat() error string");
    assert!(
        err.contains("resolved"),
        "error must preserve apply_beat's 'already resolved' message, got: {err}"
    );
    assert_source_is_narrator_beat(&failed[0]);
}

// ---------------------------------------------------------------------------
// Regression — multiple beat_selections in one turn must each emit an event
// ---------------------------------------------------------------------------

/// The playtest 2 symptom was "2-3 beat_selections per turn for 20 minutes,
/// zero events". That shape implies the loop dropped every beat silently,
/// not just one. This test locks in that every invocation of the helper
/// produces exactly one event — a future refactor that accidentally
/// deduplicates events per-turn would re-open the silent-drop hole.
#[test]
fn multiple_calls_without_encounter_emit_one_warning_per_call() {
    let (_guard, mut rx) = fresh_subscriber();
    let defs = load_defs();
    let mut snapshot = empty_snapshot();

    let o1 = apply_beat_dispatch(&mut snapshot, "attack", &defs);
    let o2 = apply_beat_dispatch(&mut snapshot, "finisher", &defs);
    let o3 = apply_beat_dispatch(&mut snapshot, "attack", &defs);

    assert_eq!(o1, BeatDispatchOutcome::NoEncounter);
    assert_eq!(o2, BeatDispatchOutcome::NoEncounter);
    assert_eq!(o3, BeatDispatchOutcome::NoEncounter);

    let events = drain_events(&mut rx);
    let warns = find_encounter_events(&events, "encounter.beat_no_encounter");
    assert_eq!(
        warns.len(),
        3,
        "three beat_selection inputs must produce three warning events, not \
         one deduplicated event and not zero — the 20-minute silent window \
         in playtest 2 came from dropping the whole loop, not from coalescing"
    );
    // Each event must carry the beat_id it was triggered by, so the GM panel
    // can show the full sequence of drops rather than collapsing them.
    let beat_ids: Vec<&str> = warns
        .iter()
        .filter_map(|e| e.fields.get("beat_id").and_then(|v| v.as_str()))
        .collect();
    assert_eq!(
        beat_ids,
        vec!["attack", "finisher", "attack"],
        "each warning must carry its own beat_id in the order the helper was called"
    );
}

// ---------------------------------------------------------------------------
// Wiring — the production dispatch path must call the helper, and the old
// silent-drop `is_some()` outer guard must be gone. Per CLAUDE.md every test
// suite needs at least one wiring test so we don't ship unwired green tests.
// ---------------------------------------------------------------------------

#[test]
fn wiring_dispatch_mod_calls_apply_beat_dispatch() {
    let source = include_str!("dispatch/mod.rs");

    // Positive side: scan for an actual call expression, not a bare identifier.
    // A comment that merely mentions the function name would be fooled by a
    // plain substring scan — the opening paren forces a real invocation.
    assert!(
        source.contains("apply_beat_dispatch("),
        "dispatch/mod.rs must call apply_beat_dispatch(...) — the per-branch \
         beat dispatch logic cannot live as an inline match any longer"
    );
}

#[test]
fn wiring_no_bare_is_some_guard_on_beat_selection_loop() {
    let source = include_str!("dispatch/mod.rs");

    // Regression guard: the original silent-drop shape was
    //     if ctx.snapshot.encounter.is_some() {
    //         for bs in result.beat_selections.iter() { ... }
    //     }
    // That outer guard is exactly what vanished the 2-3 beats per turn for
    // 20 minutes — when the encounter was None the loop was skipped with
    // zero OTEL. The helper owns the None check now, so this token must not
    // reappear in dispatch/mod.rs.
    assert!(
        !source.contains("ctx.snapshot.encounter.is_some()"),
        "dispatch/mod.rs contains `ctx.snapshot.encounter.is_some()` — that \
         check belongs inside apply_beat_dispatch, not at the call site. The \
         outer is_some() guard was the root cause of the 37-14 silent drop."
    );
}

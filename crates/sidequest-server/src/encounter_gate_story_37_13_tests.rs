//! Tests for `dispatch::apply_confrontation_gate` — every branch observable.
//!
//! The gate routes the narrator's `"confrontation": <type>` signal given the
//! current `snapshot.encounter` state. Each of the six cases (A-F) emits a
//! distinct `WatcherEvent` so the GM panel can verify the decision.
//!
//! Test matrix — one case per letter in the module doc of `encounter_gate.rs`:
//!   A. None                          → Created
//!   B. Some(resolved)                → Created
//!   C. Some(unresolved, same type)   → Redeclared (no-op)
//!   D. Some(unresolved, diff, b==0)  → ReplacedPreBeat
//!   E. Some(unresolved, diff, b>0)   → RejectedMidEncounter
//!   F. Unknown type (def missing)    → UnknownType
//!
//! Plus a source-scanning wiring test asserting `dispatch/mod.rs` calls the
//! helper and does not contain the old inline branch.

use sidequest_agents::orchestrator::NpcMention;
use sidequest_game::encounter::{
    EncounterActor, EncounterMetric, MetricDirection, StructuredEncounter,
};
use sidequest_game::state::GameSnapshot;
use sidequest_genre::ConfrontationDef;
use sidequest_telemetry::{WatcherEvent, WatcherEventType};

use crate::dispatch::apply_confrontation_gate;
use crate::dispatch::encounter_gate::ConfrontationGateOutcome;
use crate::test_support::telemetry::{drain_events, fresh_subscriber};

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

/// Every event the gate emits must carry `source = "narrator_confrontation"` so
/// the GM panel can attribute it to the narrator subsystem. Centralising this
/// check keeps per-case tests focused on their own invariants while guaranteeing
/// the attribution field never silently drops.
fn assert_source_is_narrator(event: &WatcherEvent) {
    assert_eq!(
        event.fields.get("source").and_then(|v| v.as_str()),
        Some("narrator_confrontation"),
        "encounter events must carry source=narrator_confrontation"
    );
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

fn test_character(name: &str) -> sidequest_game::Character {
    use sidequest_game::creature_core::CreatureCore;
    use sidequest_game::{Character, Inventory};
    use sidequest_protocol::NonBlankString;
    Character {
        core: CreatureCore {
            name: NonBlankString::new(name).unwrap(),
            description: NonBlankString::new("Test character for gate tests").unwrap(),
            personality: NonBlankString::new("Brave").unwrap(),
            level: 1,
            hp: 10,
            max_hp: 10,
            ac: 10,
            xp: 0,
            inventory: Inventory {
                items: vec![],
                gold: 0,
            },
            statuses: vec![],
        },
        backstory: NonBlankString::new("Test backstory").unwrap(),
        narrative_state: "exploring".to_string(),
        hooks: vec![],
        char_class: NonBlankString::new("Fighter").unwrap(),
        race: NonBlankString::new("Human").unwrap(),
        pronouns: String::new(),
        stats: std::collections::HashMap::new(),
        abilities: vec![],
        known_facts: vec![],
        affinities: vec![],
        is_friendly: true,
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
    assert_source_is_narrator(&created[0]);
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
    assert_source_is_narrator(&redeclare[0]);
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
    assert_source_is_narrator(&replaced[0]);
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

    let outcome =
        apply_confrontation_gate(&mut snapshot, "combat", &defs, &[npc("Toggler Copperjaw")]);
    assert_eq!(
        outcome,
        ConfrontationGateOutcome::ReplacedPreBeat,
        "Case D must return ReplacedPreBeat — without this assertion the leak \
         check below would pass even if the gate silently no-opped"
    );

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
    assert_source_is_narrator(&rejected[0]);
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
    assert_source_is_narrator(&failed[0]);
}

// ---------------------------------------------------------------------------
// Regression guards — edge cases that the Case A-F matrix leaves implicit.
// ---------------------------------------------------------------------------

/// The match arms put Case C (same type) before Case D (beat == 0). Both
/// conditions can be true simultaneously — same type AND beat zero — and the
/// gate must treat that intersection as a redeclare, not a replace. This test
/// locks in the arm ordering so a future reorder can't silently change
/// semantics by promoting the `beat == 0` guard above the same-type check.
#[test]
fn case_c_same_type_with_beat_zero_still_redeclare() {
    let (_guard, mut rx) = fresh_subscriber();
    let defs = load_defs();
    let mut snapshot = snapshot_with(existing_encounter("combat", 0, false));

    let outcome = apply_confrontation_gate(&mut snapshot, "combat", &defs, &[]);

    assert_eq!(
        outcome,
        ConfrontationGateOutcome::Redeclared,
        "same-type redeclare at beat 0 must NOT fall through to ReplacedPreBeat"
    );
    assert_eq!(
        snapshot.encounter.as_ref().unwrap().encounter_type,
        "combat",
        "redeclare must leave the old encounter in place"
    );

    let events = drain_events(&mut rx);
    assert_eq!(
        find_encounter_events(&events, "encounter.redeclare_noop").len(),
        1
    );
    assert!(
        find_encounter_events(&events, "encounter.replaced_pre_beat").is_empty(),
        "must NOT emit encounter.replaced_pre_beat when types match"
    );
}

/// An empty incoming type goes through `find_confrontation_def`, which cannot
/// match any def (no def has an empty type), and therefore routes to Case F.
/// Lock that in so a future refactor doesn't accidentally create a bypass for
/// the empty string — the gate should always route to `UnknownType` with a
/// `ValidationWarning` event, not silently no-op.
#[test]
fn case_f_empty_incoming_type_routes_to_unknown() {
    let (_guard, mut rx) = fresh_subscriber();
    let defs = load_defs();
    let mut snapshot = empty_snapshot();

    let outcome = apply_confrontation_gate(&mut snapshot, "", &defs, &[]);

    assert_eq!(outcome, ConfrontationGateOutcome::UnknownType);
    assert!(snapshot.encounter.is_none());

    let events = drain_events(&mut rx);
    let failed = find_encounter_events(&events, "encounter.creation_failed_unknown_type");
    assert_eq!(
        failed.len(),
        1,
        "empty incoming type must still emit a validation warning"
    );
    assert_eq!(
        failed[0]
            .fields
            .get("encounter_type")
            .and_then(|v| v.as_str()),
        Some("")
    );
    assert_source_is_narrator(&failed[0]);
}

/// Case F must also fire cleanly when an existing encounter is present — the
/// def-missing check happens before the match on `snapshot.encounter`, so the
/// current encounter must remain untouched.
#[test]
fn case_f_unknown_type_with_existing_encounter_preserves_state() {
    let (_guard, mut rx) = fresh_subscriber();
    let defs = load_defs();
    let mut snapshot = snapshot_with(existing_encounter("combat", 2, false));
    let before = snapshot.encounter.clone().unwrap();

    let outcome = apply_confrontation_gate(&mut snapshot, "interpretive_dance", &defs, &[]);

    assert_eq!(outcome, ConfrontationGateOutcome::UnknownType);
    let after = snapshot.encounter.as_ref().unwrap();
    assert_eq!(after.encounter_type, before.encounter_type);
    assert_eq!(after.beat, before.beat);
    assert_eq!(after.metric.current, before.metric.current);

    let events = drain_events(&mut rx);
    assert_eq!(
        find_encounter_events(&events, "encounter.creation_failed_unknown_type").len(),
        1
    );
}

/// The `build_encounter` helper populates actors from BOTH
/// `snapshot.characters` (role = "player") AND `narrator_npcs` (role = "npc").
/// Every other test exercises one path at a time; this one exercises both so
/// the player-actor loop is never silently untested.
#[test]
fn case_a_populates_both_player_and_npc_actors() {
    let (_guard, _rx) = fresh_subscriber();
    let defs = load_defs();
    let mut snapshot = empty_snapshot();
    snapshot.characters.push(test_character("Aragorn"));
    let npcs = vec![npc("Strider the Second")];

    let outcome = apply_confrontation_gate(&mut snapshot, "combat", &defs, &npcs);

    assert_eq!(outcome, ConfrontationGateOutcome::Created);
    let actors = &snapshot.encounter.as_ref().unwrap().actors;

    let player_names: Vec<&str> = actors
        .iter()
        .filter(|a| a.role == "player")
        .map(|a| a.name.as_str())
        .collect();
    let npc_names: Vec<&str> = actors
        .iter()
        .filter(|a| a.role == "npc")
        .map(|a| a.name.as_str())
        .collect();

    assert_eq!(player_names, vec!["Aragorn"]);
    assert_eq!(npc_names, vec!["Strider the Second"]);
}

// ---------------------------------------------------------------------------
// Wiring test — the production dispatch path must actually call the helper
// and the old silent-drop branch must be gone. Per CLAUDE.md, every test
// suite needs at least one wiring test so we don't ship unwired green tests.
// ---------------------------------------------------------------------------

#[test]
fn wiring_dispatch_mod_calls_apply_confrontation_gate() {
    let source = include_str!("dispatch/mod.rs");

    // Positive side: scan for an actual call expression, not a bare identifier.
    // A comment that merely mentions the function name would be fooled by a
    // plain substring scan — the opening paren forces a real invocation.
    assert!(
        source.contains("apply_confrontation_gate("),
        "dispatch/mod.rs must call apply_confrontation_gate(...) — the gate \
         logic cannot live as an inline block any longer"
    );

    // Negative side: the old silent-drop shape combined `encounter.is_none()`
    // and `is_some_and(|e| e.resolved)` in a single boolean without an `else`
    // branch. Scan for the semantic token pair rather than a whitespace-exact
    // multiline match — the latter is silently broken by `cargo fmt` line wraps.
    let has_is_none_token = source.contains("ctx.snapshot.encounter.is_none()");
    let has_is_some_and_resolved_token =
        source.contains("ctx.snapshot.encounter.as_ref().is_some_and(|e| e.resolved)");
    assert!(
        !(has_is_none_token && has_is_some_and_resolved_token),
        "dispatch/mod.rs still contains the old inline silent-drop branch \
         (both `ctx.snapshot.encounter.is_none()` and \
         `is_some_and(|e| e.resolved)` present) — delegate to \
         apply_confrontation_gate instead"
    );
}

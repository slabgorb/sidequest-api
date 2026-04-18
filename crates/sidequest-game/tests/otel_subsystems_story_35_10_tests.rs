//! Story 35-10: OTEL watcher events for consequence, combatant,
//! party_reconciliation, and progression subsystems.
//!
//! Verifies that four production subsystems — `consequence`, `combatant`,
//! `party_reconciliation`, and `progression` — emit `WatcherEvent`s so the
//! GM panel can observe their decisions in real time (ADR-031 / CLAUDE.md
//! OTEL rule: "the GM panel is the lie detector").
//!
//! Each subsystem is exercised directly, and grep-based wiring assertions
//! confirm the production code path that reaches each subsystem is still in
//! place (CLAUDE.md A5 — "Every Test Suite Needs a Wiring Test").
//!
//! Pattern matches `otel_npc_subsystems_story_35_9_tests.rs`:
//! `TELEMETRY_LOCK` serializes the global broadcast channel; events are
//! drained into a Vec and filtered by `(component, action)`.

use std::collections::HashMap;

use sidequest_game::character::Character;
use sidequest_game::consequence::WishConsequenceEngine;
use sidequest_game::creature_core::CreatureCore;
use sidequest_game::delta::{compute_delta, snapshot as state_snapshot};
use sidequest_game::inventory::Inventory;
use sidequest_game::party_reconciliation::{
    PartyReconciliation, PlayerLocation, ReconciliationResult,
};
use sidequest_game::progression;
use sidequest_game::state::{broadcast_state_changes, GameSnapshot};
use sidequest_protocol::NonBlankString;
use sidequest_telemetry::{init_global_channel, subscribe_global, WatcherEvent};

// ---------------------------------------------------------------------------
// Test infrastructure — matches the pattern from
// otel_npc_subsystems_story_35_9_tests.rs.
// ---------------------------------------------------------------------------

/// Serialize telemetry tests — the global broadcast channel is shared state,
/// so tests that emit and read events must not run concurrently.
static TELEMETRY_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Initialize the global telemetry channel (idempotent), acquire the
/// serialization lock, drain any stale events, and return a clean receiver.
fn fresh_subscriber() -> (
    std::sync::MutexGuard<'static, ()>,
    tokio::sync::broadcast::Receiver<WatcherEvent>,
) {
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
                && e.fields.get("action").and_then(serde_json::Value::as_str) == Some(action)
        })
        .cloned()
        .collect()
}

/// Build a friendly `Character` with the given HP / max HP for bloodied-threshold
/// tests. All other fields are given sensible defaults — these tests care only
/// about the `is_friendly` flag and the HP pair that drives the bloodied check
/// inside `broadcast_state_changes`.
fn make_friendly(name: &str, hp: i32, max_hp: i32) -> Character {
    Character {
        core: CreatureCore {
            name: NonBlankString::new(name).unwrap(),
            description: NonBlankString::new("A test combatant").unwrap(),
            personality: NonBlankString::new("stoic").unwrap(),
            level: 3,
            hp,
            max_hp,
            ac: 15,
            xp: 0,
            inventory: Inventory::default(),
            statuses: vec![],
        },
        backstory: NonBlankString::new("Test backstory").unwrap(),
        narrative_state: "Exploring".to_string(),
        hooks: vec![],
        char_class: NonBlankString::new("Fighter").unwrap(),
        race: NonBlankString::new("Human").unwrap(),
        pronouns: String::new(),
        stats: HashMap::new(),
        abilities: vec![],
        known_facts: vec![],
        affinities: vec![],
        is_friendly: true,
        resolved_archetype: None,
        archetype_provenance: None,
    }
}

/// Build a minimal `GameSnapshot` containing the given characters. All other
/// fields inherit from `GameSnapshot::default()` — sufficient for the narrow
/// `broadcast_state_changes` path exercised by the bloodied tests.
fn snapshot_with_characters(characters: Vec<Character>) -> GameSnapshot {
    GameSnapshot {
        characters,
        ..GameSnapshot::default()
    }
}

// ===========================================================================
// consequence — WishConsequenceEngine::evaluate
// ===========================================================================
//
// Fires on every evaluate() call so the GM panel can see when the genie
// wish detector engaged AND when it declined to act. Both signals matter:
// the absence of an event means the engine wasn't called at all (a wiring
// gap), not that it ran and decided not to act.

#[test]
fn consequence_evaluate_power_grab_emits_watcher_event() {
    let (_guard, mut rx) = fresh_subscriber();

    let mut engine = WishConsequenceEngine::new();
    let _wish = engine.evaluate("Thorin", "wish for unlimited gold", true);

    let events = drain_events(&mut rx);
    let evaluated = find_events(&events, "consequence", "wish_evaluated");

    assert!(
        !evaluated.is_empty(),
        "WishConsequenceEngine::evaluate() must emit consequence.wish_evaluated; \
         got {} other events",
        events.len()
    );

    let evt = &evaluated[0];
    assert_eq!(
        evt.fields
            .get("is_power_grab")
            .and_then(serde_json::Value::as_bool),
        Some(true),
        "is_power_grab field must reflect the input"
    );
    assert_eq!(
        evt.fields
            .get("wisher_name")
            .and_then(serde_json::Value::as_str),
        Some("Thorin")
    );
    assert_eq!(
        evt.fields
            .get("category")
            .and_then(serde_json::Value::as_str),
        Some("backfire"),
        "first power-grab in rotation must assign 'backfire'"
    );
    assert_eq!(
        evt.fields
            .get("rotation_counter")
            .and_then(serde_json::Value::as_u64),
        Some(1),
        "rotation_counter must reflect post-evaluation value"
    );
}

#[test]
fn consequence_evaluate_non_power_grab_emits_watcher_event_with_null_category() {
    let (_guard, mut rx) = fresh_subscriber();

    let mut engine = WishConsequenceEngine::new();
    let result = engine.evaluate("Normal", "open the door", false);
    assert!(result.is_none(), "non-power-grab returns None");

    let events = drain_events(&mut rx);
    let evaluated = find_events(&events, "consequence", "wish_evaluated");

    assert!(
        !evaluated.is_empty(),
        "evaluate() must emit consequence.wish_evaluated even when is_power_grab=false; \
         the absence of an event makes it impossible to distinguish 'engine declined' \
         from 'engine never called'"
    );

    let evt = &evaluated[0];
    assert_eq!(
        evt.fields
            .get("is_power_grab")
            .and_then(serde_json::Value::as_bool),
        Some(false)
    );
    assert!(
        evt.fields
            .get("category")
            .map(|v| v.is_null())
            .unwrap_or(true),
        "non-power-grab evaluations must have null category"
    );
    assert_eq!(
        evt.fields
            .get("rotation_counter")
            .and_then(serde_json::Value::as_u64),
        Some(0),
        "rotation_counter must NOT advance for non-power-grab evaluations"
    );
}

#[test]
fn consequence_rotation_advances_in_emitted_events() {
    let (_guard, mut rx) = fresh_subscriber();

    let mut engine = WishConsequenceEngine::new();
    let _ = engine.evaluate("A", "grab 1", true);
    let _ = engine.evaluate("B", "grab 2", true);

    let events = drain_events(&mut rx);
    let evaluated = find_events(&events, "consequence", "wish_evaluated");

    assert_eq!(
        evaluated.len(),
        2,
        "two evaluate() calls must produce two watcher events"
    );

    let categories: Vec<&str> = evaluated
        .iter()
        .filter_map(|e| e.fields.get("category").and_then(serde_json::Value::as_str))
        .collect();
    assert_eq!(
        categories,
        vec!["backfire", "attention"],
        "rotation order must be reflected in successive events"
    );
}

// ===========================================================================
// combatant — broadcast_state_changes (bloodied threshold, Option A rework)
// ===========================================================================
//
// Story 35-10 originally instrumented `Combatant::hp_fraction()` — a pure
// accessor — with a side effect, but `hp_fraction()` had no production
// callers, making the OTEL event unreachable from any live game session.
//
// Rework #3 (Architect decision — Option A): the `combatant.bloodied`
// emission lives in `broadcast_state_changes`, the function dispatched from
// `sidequest-server/src/dispatch/mod.rs:1737` every turn to build the
// PARTY_STATUS message. Emission is gated on `delta.characters_changed()`
// so it fires only when combat math has actually mutated a character this
// turn (transition-at-mutation-site pattern per `disposition::apply_delta`).
//
// This set of tests exercises `broadcast_state_changes` directly with
// fabricated `GameSnapshot` + `StateDelta` fixtures — no source-grepping,
// no `include_str!` wiring loopholes. The behavior under test is exactly
// what dispatch/mod.rs calls at runtime.

#[test]
fn broadcast_state_changes_emits_bloodied_when_friendly_drops_below_half() {
    let (_guard, mut rx) = fresh_subscriber();

    let before_state = snapshot_with_characters(vec![make_friendly("Grog", 30, 30)]);
    let after_state = snapshot_with_characters(vec![make_friendly("Grog", 12, 30)]);
    let delta = compute_delta(
        &state_snapshot(&before_state),
        &state_snapshot(&after_state),
    );
    assert!(
        delta.characters_changed(),
        "test precondition: HP mutation must register as characters_changed",
    );

    let _messages = broadcast_state_changes(&delta, &after_state);

    let events = drain_events(&mut rx);
    let bloodied = find_events(&events, "combatant", "bloodied");

    assert!(
        !bloodied.is_empty(),
        "broadcast_state_changes must emit combatant.bloodied when a friendly \
         character is below 0.5 max_hp AND characters_changed — without this \
         emission the GM panel cannot see combat engage. Got {} other events.",
        events.len()
    );

    let evt = &bloodied[0];
    assert_eq!(
        evt.fields.get("name").and_then(serde_json::Value::as_str),
        Some("Grog"),
        "bloodied event must carry the combatant name",
    );
    assert_eq!(
        evt.fields.get("hp").and_then(serde_json::Value::as_i64),
        Some(12),
        "bloodied event must carry current hp",
    );
    assert_eq!(
        evt.fields.get("max_hp").and_then(serde_json::Value::as_i64),
        Some(30),
        "bloodied event must carry max_hp",
    );
    let hp_frac = evt
        .fields
        .get("hp_fraction")
        .and_then(serde_json::Value::as_f64)
        .expect("hp_fraction field must be a number");
    assert!(
        (hp_frac - 0.4).abs() < 1e-5,
        "hp_fraction should be ~0.4 (12/30), got {hp_frac}",
    );
}

#[test]
fn broadcast_state_changes_does_not_emit_bloodied_at_full_hp() {
    let (_guard, mut rx) = fresh_subscriber();

    // Before/after HP both drive characters_changed=true, but the final
    // "after" state is at full HP. No bloodied event should fire.
    let before_state = snapshot_with_characters(vec![make_friendly("Pike", 20, 30)]);
    let after_state = snapshot_with_characters(vec![make_friendly("Pike", 30, 30)]);
    let delta = compute_delta(
        &state_snapshot(&before_state),
        &state_snapshot(&after_state),
    );
    assert!(delta.characters_changed());

    let _ = broadcast_state_changes(&delta, &after_state);

    let events = drain_events(&mut rx);
    let bloodied = find_events(&events, "combatant", "bloodied");

    assert!(
        bloodied.is_empty(),
        "full-HP friendly must NOT emit combatant.bloodied even when the delta \
         marks characters_changed — a noisy event would flood the GM panel. \
         Got {} bloodied events.",
        bloodied.len()
    );
}

#[test]
fn broadcast_state_changes_does_not_emit_bloodied_at_exactly_half() {
    let (_guard, mut rx) = fresh_subscriber();

    let before_state = snapshot_with_characters(vec![make_friendly("Vex", 30, 30)]);
    let after_state = snapshot_with_characters(vec![make_friendly("Vex", 15, 30)]);
    let delta = compute_delta(
        &state_snapshot(&before_state),
        &state_snapshot(&after_state),
    );
    assert!(delta.characters_changed());

    let _ = broadcast_state_changes(&delta, &after_state);

    let events = drain_events(&mut rx);
    let bloodied = find_events(&events, "combatant", "bloodied");

    assert!(
        bloodied.is_empty(),
        "hp_fraction exactly 0.5 must NOT emit bloodied — the threshold is \
         strict less-than, not less-than-or-equal. Got {} events.",
        bloodied.len()
    );
}

#[test]
fn broadcast_state_changes_does_not_emit_bloodied_when_max_hp_is_zero() {
    let (_guard, mut rx) = fresh_subscriber();

    // Degenerate combatant (max_hp=0). Vary `level` between before and after
    // so the delta still registers characters_changed — the point of this
    // test is that max_hp=0 must short-circuit regardless of delta state.
    let mut before_char = make_friendly("Phantom", 0, 0);
    before_char.core.level = 1;
    let mut after_char = make_friendly("Phantom", 0, 0);
    after_char.core.level = 2;
    let before_state = snapshot_with_characters(vec![before_char]);
    let after_state = snapshot_with_characters(vec![after_char]);
    let delta = compute_delta(
        &state_snapshot(&before_state),
        &state_snapshot(&after_state),
    );
    assert!(delta.characters_changed());

    let _ = broadcast_state_changes(&delta, &after_state);

    let events = drain_events(&mut rx);
    let bloodied = find_events(&events, "combatant", "bloodied");

    assert!(
        bloodied.is_empty(),
        "max_hp=0 degenerate combatant must NOT emit bloodied — there is no \
         meaningful HP state to report. Every uninitialized combatant would \
         flood the channel otherwise."
    );
}

#[test]
fn broadcast_state_changes_does_not_emit_bloodied_when_delta_characters_unchanged() {
    let (_guard, mut rx) = fresh_subscriber();

    // Same state before and after — delta.characters_changed() is false,
    // even though the friendly is in the bloodied range. The emission must
    // be gated on the delta, not on the absolute HP value, so the GM panel
    // only sees an event on the turn combat math actually ran.
    let state = snapshot_with_characters(vec![make_friendly("Grog", 12, 30)]);
    let snap = state_snapshot(&state);
    let delta = compute_delta(&snap, &snap);
    assert!(
        !delta.characters_changed(),
        "test precondition: snapshotting the same state twice must produce \
         an empty characters delta",
    );

    let _ = broadcast_state_changes(&delta, &state);

    let events = drain_events(&mut rx);
    let bloodied = find_events(&events, "combatant", "bloodied");

    assert!(
        bloodied.is_empty(),
        "bloodied emission must be gated on delta.characters_changed() — \
         when characters are unchanged the GM has no new combat info to log. \
         Got {} events.",
        bloodied.len()
    );
}

// ===========================================================================
// party_reconciliation — PartyReconciliation::reconcile
// ===========================================================================
//
// reconcile() returns one of three outcomes. All three emit a watcher event
// so the GM panel can see when the multiplayer resume reconciler engaged
// and what verdict it produced.

#[test]
fn party_reconciliation_no_action_needed_emits_watcher_event() {
    let (_guard, mut rx) = fresh_subscriber();

    let players = vec![
        PlayerLocation {
            player_id: "p1".to_string(),
            player_name: "Alice".to_string(),
            location: "tavern".to_string(),
        },
        PlayerLocation {
            player_id: "p2".to_string(),
            player_name: "Bob".to_string(),
            location: "tavern".to_string(),
        },
    ];
    let result = PartyReconciliation::reconcile(&players, false);
    assert!(matches!(result, ReconciliationResult::NoActionNeeded));

    let events = drain_events(&mut rx);
    let recon = find_events(&events, "party_reconciliation", "reconciled");

    assert!(
        !recon.is_empty(),
        "reconcile() must emit party_reconciliation.reconciled even on \
         no-action-needed; got {} other events",
        events.len()
    );

    let evt = &recon[0];
    assert_eq!(
        evt.fields.get("result").and_then(serde_json::Value::as_str),
        Some("no_action_needed")
    );
    assert_eq!(
        evt.fields
            .get("player_count")
            .and_then(serde_json::Value::as_u64),
        Some(2)
    );
    assert_eq!(
        evt.fields
            .get("moved_count")
            .and_then(serde_json::Value::as_u64),
        Some(0)
    );
    assert!(
        evt.fields
            .get("target_location")
            .map(|v| v.is_null())
            .unwrap_or(true),
        "no-action-needed has no target_location"
    );
}

#[test]
fn party_reconciliation_split_party_allowed_emits_watcher_event() {
    let (_guard, mut rx) = fresh_subscriber();

    let players = vec![
        PlayerLocation {
            player_id: "p1".to_string(),
            player_name: "Alice".to_string(),
            location: "tavern".to_string(),
        },
        PlayerLocation {
            player_id: "p2".to_string(),
            player_name: "Bob".to_string(),
            location: "forest".to_string(),
        },
    ];
    let result = PartyReconciliation::reconcile(&players, true);
    assert!(matches!(result, ReconciliationResult::SplitPartyAllowed));

    let events = drain_events(&mut rx);
    let recon = find_events(&events, "party_reconciliation", "reconciled");

    assert!(
        !recon.is_empty(),
        "reconcile() must emit party_reconciliation.reconciled for split-party-allowed"
    );
    let evt = &recon[0];
    assert_eq!(
        evt.fields.get("result").and_then(serde_json::Value::as_str),
        Some("split_party_allowed")
    );
    assert_eq!(
        evt.fields
            .get("player_count")
            .and_then(serde_json::Value::as_u64),
        Some(2)
    );
    assert_eq!(
        evt.fields
            .get("moved_count")
            .and_then(serde_json::Value::as_u64),
        Some(0),
        "split-party-allowed moves no players"
    );
}

#[test]
fn party_reconciliation_reconciled_emits_watcher_event_with_target_and_moved_count() {
    let (_guard, mut rx) = fresh_subscriber();

    // Three players: two in tavern, one in forest. Majority wins.
    let players = vec![
        PlayerLocation {
            player_id: "p1".to_string(),
            player_name: "Alice".to_string(),
            location: "tavern".to_string(),
        },
        PlayerLocation {
            player_id: "p2".to_string(),
            player_name: "Bob".to_string(),
            location: "tavern".to_string(),
        },
        PlayerLocation {
            player_id: "p3".to_string(),
            player_name: "Carol".to_string(),
            location: "forest".to_string(),
        },
    ];
    let result = PartyReconciliation::reconcile(&players, false);
    assert!(matches!(result, ReconciliationResult::Reconciled { .. }));

    let events = drain_events(&mut rx);
    let recon = find_events(&events, "party_reconciliation", "reconciled");

    assert!(
        !recon.is_empty(),
        "reconcile() must emit party_reconciliation.reconciled for the reconciled outcome"
    );
    let evt = &recon[0];
    assert_eq!(
        evt.fields.get("result").and_then(serde_json::Value::as_str),
        Some("reconciled")
    );
    assert_eq!(
        evt.fields
            .get("player_count")
            .and_then(serde_json::Value::as_u64),
        Some(3)
    );
    assert_eq!(
        evt.fields
            .get("target_location")
            .and_then(serde_json::Value::as_str),
        Some("tavern"),
        "majority location 'tavern' must win"
    );
    assert_eq!(
        evt.fields
            .get("moved_count")
            .and_then(serde_json::Value::as_u64),
        Some(1),
        "exactly one player (Carol) was moved from forest → tavern"
    );
}

// ===========================================================================
// progression — level_to_hp (level scaling)
// ===========================================================================
//
// Pure stat-scaling functions are called constantly during state queries
// (CharacterStatus, snapshot building). Instrumenting every call would
// flood the channel. Instead we instrument the meaningful case: when
// `level > 1`, i.e. when progression is actually applying scaling beyond
// the base. Level 1 is the no-op default.

#[test]
fn progression_level_to_hp_above_level_one_emits_watcher_event() {
    let (_guard, mut rx) = fresh_subscriber();

    let scaled = progression::level_to_hp(10, 5);

    let events = drain_events(&mut rx);
    let scale_events = find_events(&events, "progression", "stat_scaled");

    assert!(
        !scale_events.is_empty(),
        "level_to_hp(base, level>1) must emit progression.stat_scaled; \
         got {} other events",
        events.len()
    );

    let evt = &scale_events[0];
    assert_eq!(
        evt.fields.get("stat").and_then(serde_json::Value::as_str),
        Some("hp"),
        "stat field must identify which scaling function was called"
    );
    assert_eq!(
        evt.fields.get("base").and_then(serde_json::Value::as_i64),
        Some(10)
    );
    assert_eq!(
        evt.fields.get("level").and_then(serde_json::Value::as_u64),
        Some(5)
    );
    assert_eq!(
        evt.fields.get("scaled").and_then(serde_json::Value::as_i64),
        Some(scaled as i64),
        "scaled value in event must match returned value"
    );
}

#[test]
fn progression_level_to_hp_at_level_one_does_not_emit_watcher_event() {
    let (_guard, mut rx) = fresh_subscriber();

    let unscaled = progression::level_to_hp(10, 1);
    assert_eq!(unscaled, 10, "level 1 returns base unchanged");

    let events = drain_events(&mut rx);
    let scale_events = find_events(&events, "progression", "stat_scaled");

    assert!(
        scale_events.is_empty(),
        "level_to_hp(base, 1) must NOT emit progression.stat_scaled — \
         level 1 is the no-op default and would flood the channel on every \
         starter character query"
    );
}

#[test]
fn progression_level_to_damage_above_level_one_emits_watcher_event() {
    let (_guard, mut rx) = fresh_subscriber();

    let scaled = progression::level_to_damage(5, 4);

    let events = drain_events(&mut rx);
    let scale_events = find_events(&events, "progression", "stat_scaled");

    assert!(
        !scale_events.is_empty(),
        "level_to_damage(base, level>1) must emit progression.stat_scaled"
    );

    let evt = &scale_events[0];
    assert_eq!(
        evt.fields.get("stat").and_then(serde_json::Value::as_str),
        Some("damage")
    );
    assert_eq!(
        evt.fields.get("scaled").and_then(serde_json::Value::as_i64),
        Some(scaled as i64)
    );
}

#[test]
fn progression_level_to_defense_above_level_one_emits_watcher_event() {
    let (_guard, mut rx) = fresh_subscriber();

    let scaled = progression::level_to_defense(8, 6);

    let events = drain_events(&mut rx);
    let scale_events = find_events(&events, "progression", "stat_scaled");

    assert!(
        !scale_events.is_empty(),
        "level_to_defense(base, level>1) must emit progression.stat_scaled"
    );

    let evt = &scale_events[0];
    assert_eq!(
        evt.fields.get("stat").and_then(serde_json::Value::as_str),
        Some("defense")
    );
    assert_eq!(
        evt.fields.get("scaled").and_then(serde_json::Value::as_i64),
        Some(scaled as i64)
    );
}

// ===========================================================================
// A5 wiring assertions — production code paths reach these subsystems.
// ===========================================================================
//
// Grep-based checks guard against silent removal of the production
// callers. If any of these assertions fail, the subsystem has been
// orphaned and the OTEL events are no longer reachable from production.

#[test]
fn wiring_consequence_reached_by_dispatch_mod() {
    let src = include_str!("../../sidequest-server/src/dispatch/mod.rs");
    assert!(
        src.contains("WishConsequenceEngine::with_counter"),
        "dispatch/mod.rs must instantiate WishConsequenceEngine — without \
         this call the consequence OTEL events are unreachable from \
         production code."
    );
}

#[test]
fn wiring_party_reconciliation_reached_by_dispatch_connect() {
    let src = include_str!("../../sidequest-server/src/dispatch/connect.rs");
    assert!(
        src.contains("PartyReconciliation::reconcile"),
        "dispatch/connect.rs must call PartyReconciliation::reconcile() — \
         without this call the party_reconciliation OTEL events are \
         unreachable from production code on session resume."
    );
}

#[test]
fn wiring_progression_reached_by_state_mutations() {
    let src = include_str!("../../sidequest-server/src/dispatch/state_mutations.rs");
    assert!(
        src.contains("level_to_hp(") || src.contains("xp_for_level("),
        "dispatch/state_mutations.rs must call progression::level_to_hp() \
         or progression::xp_for_level() — without these calls the \
         progression OTEL events are unreachable from production code."
    );
}

#[test]
fn wiring_broadcast_state_changes_reaches_combatant_bloodied_in_production() {
    // Behavioral wiring test (rework #3 — Option A, Architect decision).
    //
    // Prior attempts used `include_str!` + `src.contains(...)` to source-grep
    // for call sites. Both earlier targets (`Combatant::hp(` and
    // `hp_fraction(`) were vacuous — the first matched unrelated inline
    // accessor calls, the second matched a doc comment. Source-greppy wiring
    // assertions are structurally loophole-prone.
    //
    // This test exercises the real production function:
    // `sidequest_game::state::broadcast_state_changes` is called from
    // `sidequest-server/src/dispatch/mod.rs:1737` on every turn to build the
    // PARTY_STATUS message that ships to clients. If the combatant.bloodied
    // emission is removed or mis-gated inside that function, this test
    // fails — no more grep loopholes, no more "the doc comment kept the
    // assertion green" situations.
    //
    // CLAUDE.md "No half-wired features" + OTEL Observability Principle:
    // the instrumented code MUST be reachable from production code paths.
    let (_guard, mut rx) = fresh_subscriber();

    let before_state = snapshot_with_characters(vec![make_friendly("WiringProbe", 30, 30)]);
    let after_state = snapshot_with_characters(vec![make_friendly("WiringProbe", 10, 30)]);
    let delta = compute_delta(
        &state_snapshot(&before_state),
        &state_snapshot(&after_state),
    );

    let _ = broadcast_state_changes(&delta, &after_state);

    let events = drain_events(&mut rx);
    let bloodied = find_events(&events, "combatant", "bloodied");

    assert!(
        !bloodied.is_empty(),
        "broadcast_state_changes (called every turn from \
         sidequest-server/src/dispatch/mod.rs:1737) must emit combatant.bloodied \
         when a friendly character's HP drops below half. Without this emission \
         the GM panel cannot verify combat math is engaged — CLAUDE.md OTEL \
         Observability Principle + \"No half-wired features\" violation."
    );
}

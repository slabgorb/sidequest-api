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

use sidequest_game::combatant::Combatant;
use sidequest_game::consequence::WishConsequenceEngine;
use sidequest_game::party_reconciliation::{
    PartyReconciliation, PlayerLocation, ReconciliationResult,
};
use sidequest_game::progression;
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
                && e.fields
                    .get("action")
                    .and_then(serde_json::Value::as_str)
                    == Some(action)
        })
        .cloned()
        .collect()
}

/// Minimal Combatant impl for trait-level OTEL tests.
struct TestCombatant {
    name: String,
    hp: i32,
    max_hp: i32,
    level: u32,
    ac: i32,
}

impl Combatant for TestCombatant {
    fn name(&self) -> &str {
        &self.name
    }
    fn hp(&self) -> i32 {
        self.hp
    }
    fn max_hp(&self) -> i32 {
        self.max_hp
    }
    fn level(&self) -> u32 {
        self.level
    }
    fn ac(&self) -> i32 {
        self.ac
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
// combatant — Combatant::hp_fraction (bloodied threshold)
// ===========================================================================
//
// hp_fraction() is the canonical "is this combatant bloodied?" check. Firing
// a watcher event when the result crosses below 0.5 gives the GM panel a
// signal that combat math actually consulted the Combatant trait — without
// flooding the channel on every accessor call.

#[test]
fn combatant_hp_fraction_below_half_emits_bloodied_event() {
    let (_guard, mut rx) = fresh_subscriber();

    let bloodied = TestCombatant {
        name: "Grog".to_string(),
        hp: 12,
        max_hp: 30,
        level: 3,
        ac: 15,
    };
    let frac = bloodied.hp_fraction();
    assert!(frac < 0.5, "test fixture must be in bloodied range");

    let events = drain_events(&mut rx);
    let bloodied_events = find_events(&events, "combatant", "bloodied");

    assert!(
        !bloodied_events.is_empty(),
        "hp_fraction() called on a combatant below 0.5 max_hp must emit \
         combatant.bloodied; got {} other events",
        events.len()
    );

    let evt = &bloodied_events[0];
    assert_eq!(
        evt.fields.get("name").and_then(serde_json::Value::as_str),
        Some("Grog")
    );
    assert_eq!(
        evt.fields.get("hp").and_then(serde_json::Value::as_i64),
        Some(12)
    );
    assert_eq!(
        evt.fields.get("max_hp").and_then(serde_json::Value::as_i64),
        Some(30)
    );
    let hp_frac = evt
        .fields
        .get("hp_fraction")
        .and_then(serde_json::Value::as_f64)
        .expect("hp_fraction field must be a number");
    assert!(
        (hp_frac - 0.4).abs() < 1e-5,
        "hp_fraction should be ~0.4 (12/30), got {hp_frac}"
    );
}

#[test]
fn combatant_hp_fraction_at_full_hp_does_not_emit_bloodied_event() {
    let (_guard, mut rx) = fresh_subscriber();

    let healthy = TestCombatant {
        name: "Pike".to_string(),
        hp: 30,
        max_hp: 30,
        level: 3,
        ac: 15,
    };
    let _ = healthy.hp_fraction();

    let events = drain_events(&mut rx);
    let bloodied_events = find_events(&events, "combatant", "bloodied");

    assert!(
        bloodied_events.is_empty(),
        "hp_fraction() at full HP must NOT emit combatant.bloodied — \
         a noisy event would flood the GM panel; got {} bloodied events",
        bloodied_events.len()
    );
}

#[test]
fn combatant_hp_fraction_at_half_exactly_does_not_emit_bloodied_event() {
    let (_guard, mut rx) = fresh_subscriber();

    let half = TestCombatant {
        name: "Vex".to_string(),
        hp: 15,
        max_hp: 30,
        level: 3,
        ac: 15,
    };
    let frac = half.hp_fraction();
    assert!((frac - 0.5).abs() < f64::EPSILON, "fixture must be exactly 0.5");

    let events = drain_events(&mut rx);
    let bloodied_events = find_events(&events, "combatant", "bloodied");

    assert!(
        bloodied_events.is_empty(),
        "hp_fraction() at exactly 0.5 must NOT emit bloodied — the threshold \
         is strict less-than, not less-than-or-equal"
    );
}

#[test]
fn combatant_hp_fraction_zero_max_hp_does_not_emit_bloodied_event() {
    let (_guard, mut rx) = fresh_subscriber();

    let degenerate = TestCombatant {
        name: "Phantom".to_string(),
        hp: 0,
        max_hp: 0,
        level: 1,
        ac: 0,
    };
    let frac = degenerate.hp_fraction();
    assert_eq!(
        frac, 0.0,
        "zero max_hp must return 0.0 per existing trait contract"
    );

    let events = drain_events(&mut rx);
    let bloodied_events = find_events(&events, "combatant", "bloodied");

    assert!(
        bloodied_events.is_empty(),
        "hp_fraction() with max_hp=0 must NOT emit bloodied — there is no \
         meaningful HP state to report. Otherwise every uninitialized \
         combatant floods the channel."
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
        evt.fields
            .get("result")
            .and_then(serde_json::Value::as_str),
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
        evt.fields
            .get("result")
            .and_then(serde_json::Value::as_str),
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
    assert!(matches!(
        result,
        ReconciliationResult::Reconciled { .. }
    ));

    let events = drain_events(&mut rx);
    let recon = find_events(&events, "party_reconciliation", "reconciled");

    assert!(
        !recon.is_empty(),
        "reconcile() must emit party_reconciliation.reconciled for the reconciled outcome"
    );
    let evt = &recon[0];
    assert_eq!(
        evt.fields
            .get("result")
            .and_then(serde_json::Value::as_str),
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
fn wiring_combatant_hp_fraction_reached_by_state() {
    // Reviewer rework (35-10): the previous assertion grepped for
    // `Combatant::hp(` / `Combatant::max_hp(`, both of which appear in
    // state.rs for unrelated state-build reasons. That gave false confidence:
    // the OTEL event lives in `hp_fraction()`, not in the raw accessors. To
    // actually prove the combatant.bloodied event is reachable from
    // production, this assertion must grep for the literal `hp_fraction(`
    // call. If state.rs (or any state-building production code) does not
    // delegate the bloodied check to `hp_fraction()`, the OTEL event is dead.
    let src = include_str!("../src/state.rs");
    assert!(
        src.contains("hp_fraction("),
        "state.rs must call Combatant::hp_fraction() (not inline the \
         hp/max_hp ratio math) — without this call the combatant.bloodied \
         OTEL event is unreachable from production state code, even though \
         the trait method is defined and tested. CLAUDE.md \"No half-wired \
         features\" requires the instrumented method to have a non-test caller."
    );
}

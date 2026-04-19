//! Story 39-4 — beat-driven Edge delta dispatch wiring.
//!
//! RED: these tests pin the contract for the new beat-driven edge deltas
//! (`BeatDef.edge_delta`, `target_edge_delta`, `resource_deltas`) and for
//! the composure-break auto-resolve that fires when `Edge <= 0` on the
//! acting character or the primary opponent.
//!
//! Why this lives as an integration test (outside `src/`):
//!   The story's AC6 requires a wiring test that drives the edge-delta
//!   dispatch end-to-end. `DispatchContext` is not reachable from outside
//!   the crate and is prohibitively wide to construct in a test — so Dev
//!   must extract the edge-delta logic into a public entry point:
//!
//!       pub fn apply_beat_edge_deltas(
//!           snapshot: &mut GameSnapshot,
//!           beat: &BeatDef,
//!           encounter_type: &str,
//!       ) -> EdgeDeltaOutcome;
//!
//!   `handle_applied_side_effects` then calls this helper. Binding the
//!   seam in a public function is the only way to satisfy both the
//!   wiring rule ("imported, called, reachable from production code
//!   paths") and the no-stub rule ("don't create test-only paths"). The
//!   source-scan test `wiring_handle_applied_side_effects_invokes_edge_delta_helper`
//!   proves the real dispatch call site does call this helper.
//!
//! ACs covered:
//!   AC2 — self-debit: beat with edge_delta=2 debits acting character's
//!         edge.current by 2 and emits `creature.edge_delta`
//!   AC3 — target-debit: beat with target_edge_delta=2 debits primary
//!         opponent's edge.current by 2
//!   AC3b — missing primary opponent on target_edge_delta: fails loudly
//!         (no silent skip) per the "No Silent Fallbacks" rule
//!   AC4 — composure break: driving opponent to 0 sets
//!         `encounter.resolved = true` and emits
//!         `encounter.composure_break`
//!   AC5 — resource_deltas: beat with resource_deltas.voice=-1.0 debits
//!         snapshot.resources["voice"].current by 1
//!   AC6 — wiring test: a beat from inline YAML drives the full path
//!         through the public entry point

use std::collections::HashMap;

use sidequest_game::creature_core::{
    CreatureCore, EdgePool, EdgeThreshold, RecoveryTrigger,
};
use sidequest_game::encounter::{
    EncounterActor, EncounterMetric, MetricDirection, StructuredEncounter,
};
use sidequest_game::npc::Npc;
use sidequest_game::resource_pool::ResourcePool;
use sidequest_game::state::GameSnapshot;
use sidequest_game::{Character, Inventory};
use sidequest_genre::BeatDef;
use sidequest_protocol::NonBlankString;
use sidequest_telemetry::{
    init_global_channel, subscribe_global, WatcherEvent, WatcherEventType,
};

// The public-path imports. These must resolve for this file to compile —
// proving the helper and its outcome are reachable from outside the
// `src/` tree. Dev must add:
//     pub use dispatch::beat::{apply_beat_edge_deltas, EdgeDeltaOutcome};
// at the crate root. Compile failure is the RED signal.
use sidequest_server::{apply_beat_edge_deltas, EdgeDeltaOutcome};

use super::test_helpers::dispatch_source_combined;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn edge_pool(max: i32) -> EdgePool {
    EdgePool {
        current: max,
        max,
        base_max: max,
        recovery_triggers: vec![RecoveryTrigger::OnResolution],
        thresholds: vec![EdgeThreshold {
            at: 1,
            event_id: "edge_strained".to_string(),
            narrator_hint: "You are close to breaking.".to_string(),
        }],
    }
}

fn hero(edge_max: i32) -> Character {
    Character {
        core: CreatureCore {
            name: NonBlankString::new("Hero").unwrap(),
            description: NonBlankString::new("A test hero.").unwrap(),
            personality: NonBlankString::new("Stoic.").unwrap(),
            level: 1,
            xp: 0,
            inventory: Inventory::default(),
            statuses: vec![],
            edge: edge_pool(edge_max),
            acquired_advancements: vec![],
        },
        backstory: NonBlankString::new("From the test ward.").unwrap(),
        narrative_state: "at the testing bench".to_string(),
        hooks: vec![],
        char_class: NonBlankString::new("Fighter").unwrap(),
        race: NonBlankString::new("Human").unwrap(),
        pronouns: "they/them".to_string(),
        stats: HashMap::new(),
        abilities: vec![],
        known_facts: vec![],
        affinities: vec![],
        is_friendly: true,
        resolved_archetype: None,
        archetype_provenance: None,
    }
}

fn combat_encounter_with_opponent(opponent_name: &str) -> StructuredEncounter {
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
        actors: vec![
            EncounterActor {
                name: "Hero".to_string(),
                role: "player".to_string(),
                per_actor_state: HashMap::new(),
            },
            EncounterActor {
                name: opponent_name.to_string(),
                role: "opponent".to_string(),
                per_actor_state: HashMap::new(),
            },
        ],
        outcome: None,
        resolved: false,
        mood_override: None,
        narrator_hints: vec![],
    }
}

fn snapshot_with_hero_and_opponent(hero_edge: i32, opp_edge: i32) -> GameSnapshot {
    let mut snap = GameSnapshot::default();
    snap.characters.push(hero(hero_edge));
    snap.npcs
        .push(Npc::combat_minimal("Goblin", opp_edge, opp_edge, 1));
    snap.encounter = Some(combat_encounter_with_opponent("Goblin"));
    snap
}

fn beat_with(
    id: &str,
    edge_delta: Option<i32>,
    target_edge_delta: Option<i32>,
    resource_deltas: Option<HashMap<String, f64>>,
) -> BeatDef {
    let resource_block = match resource_deltas {
        Some(map) => {
            let mut yaml = String::from("resource_deltas:\n");
            for (k, v) in &map {
                yaml.push_str(&format!("  {}: {}\n", k, v));
            }
            yaml
        }
        None => String::new(),
    };
    let yaml = format!(
        "\nid: {id}\nlabel: \"{id}\"\nmetric_delta: -3\nstat_check: STR\n{edge}{target}{resources}",
        id = id,
        edge = edge_delta
            .map(|d| format!("edge_delta: {d}\n"))
            .unwrap_or_default(),
        target = target_edge_delta
            .map(|d| format!("target_edge_delta: {d}\n"))
            .unwrap_or_default(),
        resources = resource_block,
    );
    serde_yaml::from_str(&yaml).expect("fixture beat must parse")
}

fn find_events(events: &[WatcherEvent], component: &str, event_name: &str) -> Vec<WatcherEvent> {
    events
        .iter()
        .filter(|e| {
            e.component == component
                && e.fields.get("event").and_then(serde_json::Value::as_str) == Some(event_name)
        })
        .cloned()
        .collect()
}

fn drain(rx: &mut tokio::sync::broadcast::Receiver<WatcherEvent>) -> Vec<WatcherEvent> {
    let mut out = Vec::new();
    while let Ok(e) = rx.try_recv() {
        out.push(e);
    }
    out
}

/// Process-wide telemetry lock. The global broadcast channel is shared by
/// every test in this binary (and by `test_support::TELEMETRY_LOCK` on
/// the src side, but that lock is `pub(crate)` and not reachable from
/// integration tests). Holding this guard for the whole
/// subscribe-drive-assert window serialises all tests in this file so
/// they don't clobber each other's events.
static TELEMETRY_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct Scope {
    _guard: std::sync::MutexGuard<'static, ()>,
    rx: tokio::sync::broadcast::Receiver<WatcherEvent>,
}

fn fresh_channel() -> Scope {
    let guard = TELEMETRY_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _ = init_global_channel();
    let mut rx = subscribe_global().expect("global telemetry channel must initialize");
    while rx.try_recv().is_ok() {}
    Scope { _guard: guard, rx }
}

// ---------------------------------------------------------------------------
// AC2 — self-debit
// ---------------------------------------------------------------------------

#[test]
fn self_debit_decreases_acting_character_edge() {
    let mut scope = fresh_channel();
    let mut snap = snapshot_with_hero_and_opponent(10, 10);
    let beat = beat_with("brace", Some(2), None, None);

    let outcome = apply_beat_edge_deltas(&mut snap, &beat, "combat");

    assert_eq!(
        snap.characters[0].core.edge.current, 8,
        "self-debit: beat.edge_delta=2 must decrement acting character's edge.current by 2"
    );
    assert!(
        !outcome.composure_break,
        "composure_break must be false when edge > 0 after debit"
    );

    let events = drain(&mut scope.rx);
    let edge_events = find_events(&events, "creature", "creature.edge_delta");
    assert!(
        !edge_events.is_empty(),
        "self-debit must emit a canonical creature.edge_delta event"
    );
    let ev = &edge_events[0];
    assert!(
        matches!(ev.event_type, WatcherEventType::StateTransition),
        "creature.edge_delta must be a StateTransition event"
    );
    assert_eq!(
        ev.fields.get("source").and_then(|v| v.as_str()),
        Some("beat"),
        "creature.edge_delta must carry source=beat for attribution"
    );
    assert_eq!(
        ev.fields.get("beat_id").and_then(|v| v.as_str()),
        Some("brace")
    );
    assert_eq!(ev.fields.get("delta").and_then(|v| v.as_i64()), Some(-2));
    assert_eq!(
        ev.fields.get("new_current").and_then(|v| v.as_i64()),
        Some(8)
    );
}

// ---------------------------------------------------------------------------
// AC3 — target-debit
// ---------------------------------------------------------------------------

#[test]
fn target_debit_decreases_primary_opponent_edge() {
    let mut scope = fresh_channel();
    let mut snap = snapshot_with_hero_and_opponent(10, 10);
    let beat = beat_with("strike", None, Some(2), None);

    let outcome = apply_beat_edge_deltas(&mut snap, &beat, "combat");

    assert_eq!(
        snap.npcs[0].core.edge.current, 8,
        "target-debit: beat.target_edge_delta=2 must decrement primary opponent's edge.current by 2"
    );
    assert_eq!(
        snap.characters[0].core.edge.current, 10,
        "target-debit must not touch the acting character"
    );
    assert!(
        !outcome.composure_break,
        "no composure break when opponent survives the hit"
    );

    let events = drain(&mut scope.rx);
    let edge_events = find_events(&events, "creature", "creature.edge_delta");
    assert!(
        edge_events
            .iter()
            .any(|e| e.fields.get("new_current").and_then(|v| v.as_i64()) == Some(8)),
        "target-debit must emit creature.edge_delta for the opponent with new_current=8"
    );
}

/// AC3b — no silent fallback when `target_edge_delta` is set but no
/// primary opponent is in the encounter. Project rule: fail loudly.
#[test]
#[should_panic(expected = "primary opponent")]
fn target_debit_without_primary_opponent_panics_loudly() {
    let mut snap = GameSnapshot::default();
    snap.characters.push(hero(10));
    // Encounter with ONLY the player — no opponent at all.
    let mut encounter = combat_encounter_with_opponent("Ghost");
    encounter.actors.retain(|a| a.role != "opponent");
    snap.encounter = Some(encounter);

    let beat = beat_with("strike", None, Some(2), None);
    // MUST panic. A silent skip here would make the edge system appear
    // to work while doing nothing — the anti-pattern CLAUDE.md forbids.
    let _ = apply_beat_edge_deltas(&mut snap, &beat, "combat");
}

// ---------------------------------------------------------------------------
// AC4 — composure break auto-resolves the encounter
// ---------------------------------------------------------------------------

#[test]
fn composure_break_auto_resolves_encounter_when_opponent_hits_zero() {
    let mut scope = fresh_channel();
    // Opponent on the edge — one more hit ends it.
    let mut snap = snapshot_with_hero_and_opponent(10, 2);
    let beat = beat_with("finish", None, Some(2), None);

    let outcome = apply_beat_edge_deltas(&mut snap, &beat, "combat");

    assert_eq!(
        snap.npcs[0].core.edge.current, 0,
        "opponent edge must clamp to 0 after lethal hit"
    );
    assert!(
        outcome.composure_break,
        "outcome.composure_break must be true when opponent Edge <= 0"
    );
    assert!(
        snap.encounter
            .as_ref()
            .expect("encounter must persist")
            .resolved,
        "encounter.resolved must flip to true on composure break"
    );

    let events = drain(&mut scope.rx);
    let breaks = find_events(&events, "encounter", "encounter.composure_break");
    assert_eq!(
        breaks.len(),
        1,
        "exactly one encounter.composure_break event must reach the GM panel"
    );
    assert_eq!(
        breaks[0].fields.get("broken").and_then(|v| v.as_str()),
        Some("Goblin"),
        "composure_break event must name the broken creature"
    );
    assert_eq!(
        breaks[0]
            .fields
            .get("encounter_type")
            .and_then(|v| v.as_str()),
        Some("combat")
    );
}

#[test]
fn self_debit_to_zero_also_resolves_and_emits_break() {
    let mut scope = fresh_channel();
    let mut snap = snapshot_with_hero_and_opponent(2, 10);
    let beat = beat_with("overextend", Some(5), None, None);

    let outcome = apply_beat_edge_deltas(&mut snap, &beat, "combat");

    assert_eq!(
        snap.characters[0].core.edge.current, 0,
        "acting character edge clamps to 0"
    );
    assert!(outcome.composure_break);
    assert!(snap.encounter.as_ref().unwrap().resolved);
    let events = drain(&mut scope.rx);
    assert_eq!(
        find_events(&events, "encounter", "encounter.composure_break").len(),
        1,
        "self-break must also emit encounter.composure_break"
    );
}

// ---------------------------------------------------------------------------
// AC5 — resource_deltas route through ResourcePool
// ---------------------------------------------------------------------------

#[test]
fn resource_deltas_debit_named_resource_pool() {
    let mut snap = snapshot_with_hero_and_opponent(10, 10);
    // Seed a "voice" pool the dispatch must be able to debit.
    snap.resources.insert(
        "voice".to_string(),
        ResourcePool {
            name: "voice".to_string(),
            label: "Voice".to_string(),
            current: 3.0,
            min: 0.0,
            max: 3.0,
            voluntary: true,
            decay_per_turn: 0.0,
            thresholds: vec![],
        },
    );

    let mut deltas = HashMap::new();
    deltas.insert("voice".to_string(), -1.0);
    let beat = beat_with("pact_push", None, None, Some(deltas));

    let _ = apply_beat_edge_deltas(&mut snap, &beat, "combat");

    let after = snap.resources.get("voice").expect("voice pool must persist");
    assert!(
        (after.current - 2.0).abs() < f64::EPSILON,
        "resource_deltas.voice=-1.0 must debit the voice pool by 1.0 (expected 2.0, got {})",
        after.current
    );
}

// ---------------------------------------------------------------------------
// AC6 — wiring: real public entry point, real snapshot, real telemetry
// ---------------------------------------------------------------------------

#[test]
fn wiring_apply_beat_edge_deltas_reachable_via_crate_public_api() {
    let mut scope = fresh_channel();
    let mut snap = snapshot_with_hero_and_opponent(10, 10);
    let beat = beat_with("strike", None, Some(2), None);

    // Drive the helper via the crate's public re-export.
    let outcome: EdgeDeltaOutcome = apply_beat_edge_deltas(&mut snap, &beat, "combat");

    assert_eq!(outcome.target_new_current, Some(8));
    assert!(
        !outcome.composure_break,
        "edge=8 is above the 0 break threshold"
    );

    let events = drain(&mut scope.rx);
    assert!(
        !find_events(&events, "creature", "creature.edge_delta").is_empty(),
        "Wiring: a real BeatDef driven through the public API must emit \
         creature.edge_delta on the global telemetry channel that the GM \
         panel consumes. A dead-but-present helper would fail this test."
    );
}

// ---------------------------------------------------------------------------
// Source-scan wiring — handle_applied_side_effects must actually call
// this helper. The behavioral tests above prove the helper works in
// isolation; this one proves it is wired into the production dispatch
// loop (CLAUDE.md "Verify Wiring, Not Just Existence").
// ---------------------------------------------------------------------------

#[test]
fn wiring_handle_applied_side_effects_invokes_edge_delta_helper() {
    let source = dispatch_source_combined();
    let sig_idx = source
        .find("fn handle_applied_side_effects")
        .expect("handle_applied_side_effects must exist in dispatch tree");
    let body = &source[sig_idx..];
    assert!(
        body.contains("apply_beat_edge_deltas"),
        "handle_applied_side_effects must call apply_beat_edge_deltas — \
         otherwise the public helper is dead-but-present and the beat-driven \
         edge subsystem never fires in production. (Epic 39 wiring gate.)"
    );
}

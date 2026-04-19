//! Story 39-5 — Authored advancement effects + `resolved_beat_for`
//! (dispatch-wiring surface).
//!
//! These tests pin the behaviour promised by story 39-5's ACs 3, 4, 5, 7:
//!
//!   * AC3 — `resolved_beat_for` is a pure view function. Given
//!           `(&Character, &BeatDef, &AdvancementTree)` it returns a
//!           `ResolvedBeat` whose `edge_delta / target_edge_delta /
//!           resource_deltas` reflect the character's `acquired_advancements`.
//!           Unknown tier ids in `acquired_advancements` fail loudly.
//!           `source_effects` names the applied effects for OTEL attribution.
//!
//!   * AC4 — Beat dispatch routes through `resolved_beat_for` so live
//!           advancements affect runtime debits (not just the raw
//!           `beat.edge_delta`). The `creature.edge_delta` span gains an
//!           `advancements_applied` field.
//!
//!   * AC5 — A milestone-grant path (`grant_advancement_tier`) adds the
//!           tier id to `acquired_advancements`, one-shot-applies any
//!           `EdgeMaxBonus` to `core.edge.max`, emits
//!           `advancement.tier_granted`, and fails loudly on unknown tier.
//!
//!   * AC7 — Source-scan wiring test proves the production dispatch tree
//!           imports and calls `resolved_beat_for` — not just that the
//!           function exists as a public symbol. Mirrors the 39-4 pattern
//!           (`wiring_handle_applied_side_effects_invokes_edge_delta_helper`).
//!
//! Red-signal imports — these unresolved paths are the compile failure Dev
//! must satisfy:
//!
//!   * `sidequest-game/src/advancement.rs` (new):
//!       `pub fn resolved_beat_for(&Character, &BeatDef, &AdvancementTree) -> ResolvedBeat`
//!       `pub struct ResolvedBeat { edge_delta, target_edge_delta, resource_deltas, source_effects }`
//!       `pub fn grant_advancement_tier(&mut Character, tier_id: &str, &AdvancementTree) -> Result<AdvancementGrantOutcome, AdvancementGrantError>`
//!       `pub struct AdvancementGrantOutcome { edge_max_delta: i32, applied_effects: Vec<AdvancementEffect> }`
//!       `pub enum AdvancementGrantError` (non_exhaustive; UnknownTierId variant)
//!
//!   * `sidequest-server/src/dispatch/beat.rs`:
//!       `pub fn apply_beat_edge_deltas_resolved(&mut GameSnapshot, &BeatDef, &str, &AdvancementTree) -> EdgeDeltaOutcome`
//!       `handle_applied_side_effects` switches to the resolved entry point
//!       (with an empty `AdvancementTree` when none is loaded, preserving the
//!       39-4 smoke path).
//!
//!   * `sidequest-server/src/lib.rs`:
//!       `pub use dispatch::beat::apply_beat_edge_deltas_resolved;`

use std::collections::HashMap;

use sidequest_game::advancement::{
    grant_advancement_tier, resolved_beat_for, AdvancementGrantError, AdvancementGrantOutcome,
    ResolvedBeat,
};
use sidequest_game::creature_core::{CreatureCore, EdgePool, EdgeThreshold, RecoveryTrigger};
use sidequest_game::encounter::{
    EncounterActor, EncounterMetric, MetricDirection, StructuredEncounter,
};
use sidequest_game::npc::Npc;
use sidequest_game::state::GameSnapshot;
use sidequest_game::{Character, Inventory};
use sidequest_genre::{AdvancementEffect, AdvancementTier, AdvancementTree, BeatDef};
use sidequest_protocol::NonBlankString;
use sidequest_server::apply_beat_edge_deltas_resolved;
use sidequest_telemetry::{init_global_channel, subscribe_global, WatcherEvent, WatcherEventType};

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

fn beat_yaml(id: &str, edge_delta: Option<i32>, target_edge_delta: Option<i32>) -> BeatDef {
    let yaml = format!(
        "id: {id}\nlabel: \"{id}\"\nmetric_delta: -3\nstat_check: STR\n{edge}{target}",
        id = id,
        edge = edge_delta
            .map(|d| format!("edge_delta: {d}\n"))
            .unwrap_or_default(),
        target = target_edge_delta
            .map(|d| format!("target_edge_delta: {d}\n"))
            .unwrap_or_default(),
    );
    serde_yaml::from_str(&yaml).expect("fixture beat must parse")
}

fn tree_with_tier(id: &str, effects: Vec<AdvancementEffect>) -> AdvancementTree {
    AdvancementTree {
        tiers: vec![AdvancementTier {
            id: id.to_string(),
            required_milestone: "test_track".to_string(),
            class_gates: vec![],
            effects,
        }],
    }
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

// ===========================================================================
// AC3 — `resolved_beat_for` behaviour
// ===========================================================================

#[test]
fn resolved_beat_for_with_empty_acquired_returns_beat_unchanged() {
    let hero = hero(10);
    let beat = beat_yaml("strike", Some(1), Some(2));
    let tree = AdvancementTree { tiers: vec![] };

    let resolved: ResolvedBeat = resolved_beat_for(&hero, &beat, &tree);

    assert_eq!(
        resolved.edge_delta, beat.edge_delta,
        "empty acquired_advancements must preserve beat.edge_delta verbatim"
    );
    assert_eq!(
        resolved.target_edge_delta, beat.target_edge_delta,
        "empty acquired_advancements must preserve beat.target_edge_delta verbatim"
    );
    assert!(
        resolved.source_effects.is_empty(),
        "no advancements applied → source_effects must be empty, got {:?}",
        resolved.source_effects
    );
}

#[test]
fn resolved_beat_for_beat_discount_reduces_matching_beat_edge_delta() {
    // A Fighter with acquired tier "iron_1" that discounts committed_blow
    // by 1 → resolved.edge_delta on committed_blow is (original - 1).
    let mut hero = hero(10);
    hero.core.acquired_advancements.push("iron_1".to_string());
    let tree = tree_with_tier(
        "iron_1",
        vec![AdvancementEffect::BeatDiscount {
            beat_id: "committed_blow".to_string(),
            edge_delta_mod: -1,
            resource_mod: None,
        }],
    );
    let beat = beat_yaml("committed_blow", Some(3), None);

    let resolved = resolved_beat_for(&hero, &beat, &tree);

    assert_eq!(
        resolved.edge_delta,
        Some(2),
        "BeatDiscount(edge_delta_mod=-1) on matching beat_id must reduce cost 3 → 2"
    );
    assert_eq!(
        resolved.source_effects.len(),
        1,
        "source_effects must list the BeatDiscount that applied"
    );
    assert!(
        matches!(
            resolved.source_effects[0],
            AdvancementEffect::BeatDiscount { ref beat_id, .. } if beat_id == "committed_blow"
        ),
        "source_effects must name the specific BeatDiscount applied, got {:?}",
        resolved.source_effects[0]
    );
}

#[test]
fn resolved_beat_for_beat_discount_unaffected_on_non_matching_beat() {
    // The discount is for committed_blow; a "strike" beat must be untouched.
    let mut hero = hero(10);
    hero.core.acquired_advancements.push("iron_1".to_string());
    let tree = tree_with_tier(
        "iron_1",
        vec![AdvancementEffect::BeatDiscount {
            beat_id: "committed_blow".to_string(),
            edge_delta_mod: -1,
            resource_mod: None,
        }],
    );
    let beat = beat_yaml("strike", Some(2), None);

    let resolved = resolved_beat_for(&hero, &beat, &tree);

    assert_eq!(
        resolved.edge_delta,
        Some(2),
        "BeatDiscount on committed_blow must NOT affect strike — got edge_delta={:?}",
        resolved.edge_delta
    );
    assert!(
        resolved.source_effects.is_empty(),
        "no effect applied → source_effects must be empty"
    );
}

#[test]
fn resolved_beat_for_beat_discount_resource_mod_reduces_resource_deltas() {
    // A Pact-affinity tier makes push currencies cheaper: resource_mod
    // subtracts from the beat's authored resource_deltas.
    let mut hero = hero(10);
    hero.core.acquired_advancements.push("pact_1".to_string());

    let mut resource_mod = HashMap::new();
    resource_mod.insert("voice".to_string(), -1);
    let tree = tree_with_tier(
        "pact_1",
        vec![AdvancementEffect::BeatDiscount {
            beat_id: "pact_invocation".to_string(),
            edge_delta_mod: 0,
            resource_mod: Some(resource_mod),
        }],
    );

    // Build a beat with resource_deltas: voice=-2.0 (author cost 2 voice).
    let yaml = "id: pact_invocation\n\
                label: \"pact_invocation\"\n\
                metric_delta: -3\n\
                stat_check: CHA\n\
                resource_deltas:\n  voice: -2.0\n";
    let beat: BeatDef = serde_yaml::from_str(yaml).expect("fixture beat must parse");

    let resolved = resolved_beat_for(&hero, &beat, &tree);

    let deltas = resolved
        .resource_deltas
        .as_ref()
        .expect("resolved.resource_deltas must be Some when beat has resource_deltas");
    let voice = deltas
        .get("voice")
        .copied()
        .expect("voice key must survive into resolved resource_deltas");
    // Author cost -2.0 voice, discount -1 voice → effective -1.0.
    // Integer discount applied to floating-point delta per ADR-078; treat
    // the modifier as "add this many units of relief" in the delta's own
    // sign convention. With author delta -2.0 and mod -1 (meaning "one
    // less voice spent"), the resolved delta is -1.0.
    assert!(
        (voice - (-1.0)).abs() < f64::EPSILON,
        "resource_mod(voice=-1) on an author cost of -2.0 must yield resolved -1.0, got {}",
        voice
    );
}

#[test]
fn resolved_beat_for_leverage_bonus_increases_matching_target_edge_delta() {
    let mut hero = hero(10);
    hero.core.acquired_advancements.push("edge_1".to_string());
    let tree = tree_with_tier(
        "edge_1",
        vec![AdvancementEffect::LeverageBonus {
            beat_id: "strike".to_string(),
            target_edge_delta_mod: 1,
        }],
    );
    let beat = beat_yaml("strike", None, Some(2));

    let resolved = resolved_beat_for(&hero, &beat, &tree);

    assert_eq!(
        resolved.target_edge_delta,
        Some(3),
        "LeverageBonus(+1) on matching beat_id must raise target_edge_delta 2 → 3"
    );
    assert_eq!(
        resolved.source_effects.len(),
        1,
        "source_effects must list the applied LeverageBonus"
    );
}

#[test]
fn resolved_beat_for_is_pure_and_does_not_mutate_inputs() {
    // The function takes immutable borrows. Calling it twice with the
    // same inputs must return the same outputs, and the inputs must be
    // unchanged between calls.
    let mut hero = hero(10);
    hero.core.acquired_advancements.push("iron_1".to_string());
    let tree = tree_with_tier(
        "iron_1",
        vec![AdvancementEffect::BeatDiscount {
            beat_id: "committed_blow".to_string(),
            edge_delta_mod: -1,
            resource_mod: None,
        }],
    );
    let beat = beat_yaml("committed_blow", Some(3), None);

    let first = resolved_beat_for(&hero, &beat, &tree);
    let hero_snapshot_edge_current = hero.core.edge.current;
    let hero_snapshot_edge_max = hero.core.edge.max;
    let second = resolved_beat_for(&hero, &beat, &tree);

    assert_eq!(
        first.edge_delta, second.edge_delta,
        "purity: repeated calls must return equal edge_delta"
    );
    assert_eq!(
        first.target_edge_delta, second.target_edge_delta,
        "purity: repeated calls must return equal target_edge_delta"
    );
    assert_eq!(
        hero.core.edge.current, hero_snapshot_edge_current,
        "resolved_beat_for must not mutate character state"
    );
    assert_eq!(
        hero.core.edge.max, hero_snapshot_edge_max,
        "resolved_beat_for must not mutate character state"
    );
}

#[test]
#[should_panic(expected = "unknown advancement tier")]
fn resolved_beat_for_with_unknown_tier_id_panics_loudly() {
    // acquired_advancements holds a tier id that the tree does not
    // contain. This is a content/save bug — silently ignoring it would
    // mask the problem. CLAUDE.md: no silent fallbacks.
    let mut hero = hero(10);
    hero.core.acquired_advancements.push("tier_that_does_not_exist".to_string());
    let tree = AdvancementTree { tiers: vec![] };
    let beat = beat_yaml("strike", Some(1), None);

    let _ = resolved_beat_for(&hero, &beat, &tree);
}

// ===========================================================================
// AC4 — dispatch wiring
// ===========================================================================

#[test]
fn dispatch_applies_resolved_edge_delta_from_acquired_discount_tier() {
    // End-to-end: a hero with an acquired tier that discounts
    // committed_blow by 1 dispatches committed_blow (authored edge_delta=3).
    // The debit must be 2 — proving dispatch went through resolved_beat_for
    // and did NOT use the raw beat.edge_delta.
    let mut scope = fresh_channel();
    let mut snap = snapshot_with_hero_and_opponent(10, 10);
    snap.characters[0]
        .core
        .acquired_advancements
        .push("iron_1".to_string());

    let tree = tree_with_tier(
        "iron_1",
        vec![AdvancementEffect::BeatDiscount {
            beat_id: "committed_blow".to_string(),
            edge_delta_mod: -1,
            resource_mod: None,
        }],
    );
    let beat = beat_yaml("committed_blow", Some(3), None);

    let _ = apply_beat_edge_deltas_resolved(&mut snap, &beat, "combat", &tree);

    assert_eq!(
        snap.characters[0].core.edge.current, 8,
        "dispatch must debit the RESOLVED cost (2), not the raw cost (3); edge went 10 → {} \
         (raw path would have gone to 7)",
        snap.characters[0].core.edge.current
    );

    let events = drain(&mut scope.rx);
    let edge_events = find_events(&events, "creature", "creature.edge_delta");
    assert!(
        !edge_events.is_empty(),
        "dispatch must still emit creature.edge_delta"
    );
    let ev = &edge_events[0];
    assert_eq!(
        ev.fields.get("delta").and_then(|v| v.as_i64()),
        Some(-2),
        "creature.edge_delta.delta must reflect RESOLVED cost (-2), not raw (-3)"
    );
}

#[test]
fn dispatch_edge_delta_span_carries_advancements_applied_field() {
    // OTEL enrichment: when a BeatDiscount applies, the creature.edge_delta
    // span must include an `advancements_applied` field whose value names
    // the tier(s) that modified the debit. GM panel needs this to explain
    // "why did this beat cost 2 instead of 3."
    let mut scope = fresh_channel();
    let mut snap = snapshot_with_hero_and_opponent(10, 10);
    snap.characters[0]
        .core
        .acquired_advancements
        .push("iron_1".to_string());

    let tree = tree_with_tier(
        "iron_1",
        vec![AdvancementEffect::BeatDiscount {
            beat_id: "committed_blow".to_string(),
            edge_delta_mod: -1,
            resource_mod: None,
        }],
    );
    let beat = beat_yaml("committed_blow", Some(3), None);

    let _ = apply_beat_edge_deltas_resolved(&mut snap, &beat, "combat", &tree);

    let events = drain(&mut scope.rx);
    let edge_events = find_events(&events, "creature", "creature.edge_delta");
    assert!(
        !edge_events.is_empty(),
        "creature.edge_delta must fire on dispatch with acquired advancement"
    );
    let applied = edge_events[0]
        .fields
        .get("advancements_applied")
        .expect("creature.edge_delta span must carry advancements_applied field");
    let applied_str = applied
        .as_str()
        .or_else(|| applied.as_array().and_then(|_| Some("")))
        .unwrap_or("");
    let applied_serialised = serde_json::to_string(applied).unwrap_or_default();
    assert!(
        applied_str.contains("iron_1") || applied_serialised.contains("iron_1"),
        "advancements_applied must reference the acquired tier id 'iron_1'; got {}",
        applied_serialised
    );
}

#[test]
fn dispatch_with_empty_tree_matches_raw_beat_delta() {
    // Regression guard — feeding an empty tree MUST be equivalent to the
    // pre-39-5 (raw-beat.edge_delta) behaviour. This is the backward-compat
    // contract for the 39-4 smoke path still working once 39-5 lands.
    let mut snap = snapshot_with_hero_and_opponent(10, 10);
    let empty_tree = AdvancementTree { tiers: vec![] };
    let beat = beat_yaml("brace", Some(2), None);

    let _ = apply_beat_edge_deltas_resolved(&mut snap, &beat, "combat", &empty_tree);

    assert_eq!(
        snap.characters[0].core.edge.current, 8,
        "empty tree → resolved matches raw → debit 10-2=8"
    );
}

// ===========================================================================
// AC5 — milestone grant path
// ===========================================================================

#[test]
fn grant_advancement_tier_pushes_id_to_acquired_advancements() {
    let mut hero = hero(10);
    let tree = tree_with_tier(
        "iron_1",
        vec![AdvancementEffect::BeatDiscount {
            beat_id: "committed_blow".to_string(),
            edge_delta_mod: -1,
            resource_mod: None,
        }],
    );

    grant_advancement_tier(&mut hero, "iron_1", &tree).expect("grant must succeed for known tier");

    assert!(
        hero.core.acquired_advancements.contains(&"iron_1".to_string()),
        "grant must push tier id into acquired_advancements; got {:?}",
        hero.core.acquired_advancements
    );
}

#[test]
fn grant_advancement_tier_applies_edge_max_bonus_one_shot() {
    // EdgeMaxBonus is a one-shot state mutation on grant — NOT a view-time
    // resolution (per epic context). Granting a tier with
    // EdgeMaxBonus(amount=2) must raise core.edge.max by 2. Current stays
    // at its old value (the player must heal to fill the new capacity).
    let mut hero = hero(10);
    let before_max = hero.core.edge.max;
    let before_current = hero.core.edge.current;

    let tree = tree_with_tier(
        "iron_1",
        vec![AdvancementEffect::EdgeMaxBonus { amount: 2 }],
    );

    let outcome: AdvancementGrantOutcome = grant_advancement_tier(&mut hero, "iron_1", &tree)
        .expect("grant must succeed for known tier");

    assert_eq!(
        hero.core.edge.max,
        before_max + 2,
        "EdgeMaxBonus(2) must raise core.edge.max by 2 on grant"
    );
    assert_eq!(
        hero.core.edge.current, before_current,
        "grant must NOT refill current Edge (capacity grows, filling is a separate system)"
    );
    assert_eq!(
        outcome.edge_max_delta, 2,
        "outcome must report the edge_max delta applied"
    );
}

#[test]
fn grant_advancement_tier_emits_tier_granted_otel_span() {
    let mut scope = fresh_channel();
    let mut hero = hero(10);
    let tree = tree_with_tier(
        "iron_1",
        vec![AdvancementEffect::EdgeMaxBonus { amount: 2 }],
    );

    grant_advancement_tier(&mut hero, "iron_1", &tree).expect("grant must succeed");

    let events = drain(&mut scope.rx);
    let granted = find_events(&events, "advancement", "advancement.tier_granted");
    assert!(
        !granted.is_empty(),
        "grant must emit advancement.tier_granted span — GM panel lie-detector requires it"
    );
    let ev = &granted[0];
    assert!(
        matches!(ev.event_type, WatcherEventType::StateTransition),
        "advancement.tier_granted must be a StateTransition event"
    );
    assert_eq!(
        ev.fields.get("tier_id").and_then(|v| v.as_str()),
        Some("iron_1"),
        "advancement.tier_granted must carry tier_id='iron_1'"
    );
}

#[test]
fn grant_advancement_tier_with_unknown_id_returns_error() {
    let mut hero = hero(10);
    let tree = AdvancementTree { tiers: vec![] };

    let err = grant_advancement_tier(&mut hero, "does_not_exist", &tree)
        .expect_err("unknown tier must fail loudly — no silent fallback");

    assert!(
        matches!(err, AdvancementGrantError::UnknownTierId { .. }),
        "error variant must be UnknownTierId for a missing tier, got {:?}",
        err
    );
    assert!(
        hero.core.acquired_advancements.is_empty(),
        "failed grant must NOT leave a phantom id in acquired_advancements"
    );
}

#[test]
fn dispatch_effect_applied_span_emitted_when_resolved_beat_applies_effect() {
    // When dispatch routes through resolved_beat_for and an effect
    // actually applies, an `advancement.effect_applied` span fires naming
    // the effect type and source tier. Paired with `creature.edge_delta`,
    // this lets the GM panel show the full causal chain.
    let mut scope = fresh_channel();
    let mut snap = snapshot_with_hero_and_opponent(10, 10);
    snap.characters[0]
        .core
        .acquired_advancements
        .push("iron_1".to_string());
    let tree = tree_with_tier(
        "iron_1",
        vec![AdvancementEffect::BeatDiscount {
            beat_id: "committed_blow".to_string(),
            edge_delta_mod: -1,
            resource_mod: None,
        }],
    );
    let beat = beat_yaml("committed_blow", Some(3), None);

    let _ = apply_beat_edge_deltas_resolved(&mut snap, &beat, "combat", &tree);

    let events = drain(&mut scope.rx);
    let applied = find_events(&events, "advancement", "advancement.effect_applied");
    assert!(
        !applied.is_empty(),
        "applied BeatDiscount must emit advancement.effect_applied span"
    );
    let ev = &applied[0];
    assert_eq!(
        ev.fields.get("source_tier").and_then(|v| v.as_str()),
        Some("iron_1"),
        "advancement.effect_applied must name source_tier"
    );
    assert_eq!(
        ev.fields.get("beat_id").and_then(|v| v.as_str()),
        Some("committed_blow"),
        "advancement.effect_applied must name the beat_id the effect applied to"
    );
    let effect_type = ev
        .fields
        .get("effect_type")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        effect_type.contains("beat_discount") || effect_type.contains("BeatDiscount"),
        "advancement.effect_applied.effect_type must identify BeatDiscount, got {:?}",
        effect_type
    );
}

// ===========================================================================
// AC7 — wiring tests (source-scan + reachable public API)
// ===========================================================================

#[test]
fn wiring_dispatch_tree_calls_resolved_beat_for() {
    // Source-scan: the production dispatch code MUST call resolved_beat_for.
    // A unit test that stubs behaviour is not enough — CLAUDE.md requires
    // at least one wiring test per test suite proving the component is
    // actually invoked from production paths. Mirrors 39-4's
    // `wiring_handle_applied_side_effects_invokes_edge_delta_helper`.
    let combined = dispatch_source_combined();
    assert!(
        combined.contains("resolved_beat_for"),
        "dispatch tree source must reference resolved_beat_for — no production call site found, \
         meaning 39-5's core view function is defined but unwired"
    );
}

#[test]
fn wiring_apply_beat_edge_deltas_resolved_reachable_via_crate_public_api() {
    // Compile-time wiring check: if this imports resolves, the crate root
    // re-exports the new resolved entry point. If it doesn't, the file
    // fails to compile and the RED signal fires at link time.
    //
    // The referenced `apply_beat_edge_deltas_resolved` MUST be declared
    // with `pub` at its definition site AND re-exported from `lib.rs` via
    // `pub use dispatch::beat::apply_beat_edge_deltas_resolved;`.
    let _fn_ptr: fn(
        &mut GameSnapshot,
        &BeatDef,
        &str,
        &AdvancementTree,
    ) -> sidequest_server::EdgeDeltaOutcome = apply_beat_edge_deltas_resolved;
}

#[test]
fn wiring_grant_advancement_tier_reachable_via_game_crate_public_api() {
    // Similarly pin the grant entry point as a public symbol on
    // sidequest-game's advancement module. Milestone-handler dispatch
    // code (wired by 39-5 in server) imports it from here.
    let _fn_ptr: fn(
        &mut Character,
        &str,
        &AdvancementTree,
    ) -> Result<AdvancementGrantOutcome, AdvancementGrantError> = grant_advancement_tier;
}

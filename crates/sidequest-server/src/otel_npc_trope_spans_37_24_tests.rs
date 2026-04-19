//! Story 37-24: OTEL spans for NPC mechanical turns + stealth/trope engagement
//! outcomes. RED phase tests — assert WatcherEvent emissions at the two
//! dispatch decision points that currently have no OTEL backing.
//!
//! ACs tested:
//! 1. `npc.turn` event emitted when an NPC action resolves, with actor/action/
//!    outcome/mechanical_basis fields.
//! 2. `trope.engagement_outcome` event emitted when a stealth / confrontation /
//!    evasion trope resolves, with trope_id / engagement_kind / outcome /
//!    progression_delta fields.
//! 3. Narrative-only NPC outcomes (no mechanical roll) still emit the span,
//!    with `mechanical_basis = "narrative"` — this is the Illusionism flag the
//!    GM panel uses to surface NPC actions without mechanical backing.
//! 4. Both events use the correct `WatcherEventType::StateTransition` (outcome
//!    changes narrator context; not a pure usage summary).
//! 5. Wiring: the emit functions are callable from the server crate root.
//!
//! Pattern: mirrors `otel_dice_spans_34_11_tests.rs`. Wire-first — Dev must
//! expose `emit_npc_turn` / `emit_trope_engagement_outcome` as pub fns and
//! call them from the real dispatch sites identified in the Architect
//! Assessment (dispatch/mod.rs NpcAction match arm; dispatch/tropes.rs
//! engagement-resolution sites).

use sidequest_telemetry::{WatcherEvent, WatcherEventType};

use crate::test_support::telemetry::{drain_events, fresh_subscriber};

fn find_events<'a>(
    events: &'a [WatcherEvent],
    component: &str,
    event_name: &str,
) -> Vec<&'a WatcherEvent> {
    events
        .iter()
        .filter(|e| {
            e.component == component
                && e.fields.get("event").and_then(serde_json::Value::as_str) == Some(event_name)
        })
        .collect()
}

// ============================================================
// AC-1: npc.turn — required fields
// ============================================================

#[test]
fn npc_turn_emits_watcher_event_with_required_fields() {
    let (_guard, mut rx) = fresh_subscriber();

    crate::emit_npc_turn(
        "Kobold Archer",
        "fires bow at party",
        "success",
        "d20:17 vs dc:12",
    );

    let events = drain_events(&mut rx);
    let npc_events = find_events(&events, "npc", "npc.turn");

    assert_eq!(
        npc_events.len(),
        1,
        "exactly one npc.turn event expected, found {}",
        npc_events.len()
    );

    let e = npc_events[0];
    assert_eq!(e.component, "npc");
    assert_eq!(
        e.fields.get("actor").and_then(|v| v.as_str()),
        Some("Kobold Archer"),
        "actor field required"
    );
    assert_eq!(
        e.fields.get("action").and_then(|v| v.as_str()),
        Some("fires bow at party"),
        "action field required"
    );
    assert_eq!(
        e.fields.get("outcome").and_then(|v| v.as_str()),
        Some("success"),
        "outcome field required"
    );
    assert_eq!(
        e.fields.get("mechanical_basis").and_then(|v| v.as_str()),
        Some("d20:17 vs dc:12"),
        "mechanical_basis field required"
    );
}

// ============================================================
// AC-3: npc.turn with narrative-only outcome + basis (Illusionism flag)
// ============================================================

#[test]
fn npc_turn_narrative_basis_also_emits() {
    let (_guard, mut rx) = fresh_subscriber();

    // NPC acts but narrator did not roll dice AND has no pass/fail
    // determination. Both outcome and mechanical_basis must be "narrative"
    // so the GM panel sees this as an unadjudicated event — never a
    // confirmed mechanical success.
    crate::emit_npc_turn(
        "Innkeeper Mira",
        "pours a second ale",
        "narrative",
        "narrative",
    );

    let events = drain_events(&mut rx);
    let npc_events = find_events(&events, "npc", "npc.turn");

    assert_eq!(npc_events.len(), 1, "narrative-basis turn must also emit");
    assert_eq!(
        npc_events[0]
            .fields
            .get("mechanical_basis")
            .and_then(|v| v.as_str()),
        Some("narrative"),
        "narrative basis must be recorded literally, not elided"
    );
    assert_eq!(
        npc_events[0]
            .fields
            .get("outcome")
            .and_then(|v| v.as_str()),
        Some("narrative"),
        "narrative outcome must be recorded literally — never rewritten to 'success'"
    );
}

// ============================================================
// AC-4: npc.turn event_type is StateTransition
// ============================================================

#[test]
fn npc_turn_uses_state_transition_type() {
    let (_guard, mut rx) = fresh_subscriber();
    crate::emit_npc_turn("Guard", "swings truncheon", "failure", "d20:4 vs dc:14");
    let events = drain_events(&mut rx);
    let npc_events = find_events(&events, "npc", "npc.turn");
    assert_eq!(npc_events.len(), 1);
    assert!(
        matches!(npc_events[0].event_type, WatcherEventType::StateTransition),
        "npc.turn must use StateTransition — NPC action changes narrator state"
    );
}

// ============================================================
// AC-2: trope.engagement_outcome — required fields
// ============================================================

#[test]
fn trope_engagement_outcome_emits_watcher_event_with_required_fields() {
    let (_guard, mut rx) = fresh_subscriber();

    crate::emit_trope_engagement_outcome("sneak_past_guards", "stealth", "success", 0.4);

    let events = drain_events(&mut rx);
    let trope_events = find_events(&events, "trope", "trope.engagement_outcome");

    assert_eq!(
        trope_events.len(),
        1,
        "exactly one trope.engagement_outcome event expected, found {}",
        trope_events.len()
    );

    let e = trope_events[0];
    assert_eq!(e.component, "trope");
    assert_eq!(
        e.fields.get("trope_id").and_then(|v| v.as_str()),
        Some("sneak_past_guards"),
        "trope_id field required"
    );
    assert_eq!(
        e.fields.get("engagement_kind").and_then(|v| v.as_str()),
        Some("stealth"),
        "engagement_kind field required"
    );
    assert_eq!(
        e.fields.get("outcome").and_then(|v| v.as_str()),
        Some("success"),
        "outcome field required"
    );
    let progression = e
        .fields
        .get("progression")
        .and_then(|v| v.as_f64())
        .expect("progression field required and must be numeric");
    assert!(
        (progression - 0.4).abs() < f64::EPSILON,
        "progression must round-trip, got {progression}"
    );
}

// ============================================================
// AC-4: trope.engagement_outcome event_type is StateTransition
// ============================================================

#[test]
fn trope_engagement_outcome_uses_state_transition_type() {
    let (_guard, mut rx) = fresh_subscriber();
    crate::emit_trope_engagement_outcome("duel_at_dawn", "confrontation", "escalation", 0.25);
    let events = drain_events(&mut rx);
    let trope_events = find_events(&events, "trope", "trope.engagement_outcome");
    assert_eq!(trope_events.len(), 1);
    assert!(
        matches!(
            trope_events[0].event_type,
            WatcherEventType::StateTransition
        ),
        "trope.engagement_outcome must use StateTransition"
    );
}

// ============================================================
// AC-2b: engagement_kind covers stealth / confrontation / evasion
// ============================================================

#[test]
fn engagement_kind_covers_stealth_confrontation_evasion() {
    let (_guard, mut rx) = fresh_subscriber();

    crate::emit_trope_engagement_outcome("t1", "stealth", "success", 0.1);
    crate::emit_trope_engagement_outcome("t2", "confrontation", "failure", -0.1);
    crate::emit_trope_engagement_outcome("t3", "evasion", "escalation", 0.3);

    let events = drain_events(&mut rx);
    let trope_events = find_events(&events, "trope", "trope.engagement_outcome");
    assert_eq!(
        trope_events.len(),
        3,
        "all three engagement_kinds must emit, got {}",
        trope_events.len()
    );

    let kinds: Vec<&str> = trope_events
        .iter()
        .filter_map(|e| e.fields.get("engagement_kind").and_then(|v| v.as_str()))
        .collect();
    assert!(kinds.contains(&"stealth"));
    assert!(kinds.contains(&"confrontation"));
    assert!(kinds.contains(&"evasion"));
}

// ============================================================
// AC-5: Wiring — emit functions are pub at crate root
// ============================================================

#[test]
fn emit_functions_are_accessible() {
    // Compile-time signature check. Complements — does not replace — the
    // source-grep wiring tests below that verify the dispatch call sites
    // actually invoke these functions.
    let _f1: fn(&str, &str, &str, &str) = crate::emit_npc_turn;
    let _f2: fn(&str, &str, &str, f64) = crate::emit_trope_engagement_outcome;
}

// ============================================================
// Wiring tests: production dispatch sites actually call the emit fns.
// A compile-time fn-pointer check confirms the pub API exists; these
// tests confirm the wiring has not been deleted. If either dispatch
// call site is removed or renamed, these fail loudly. Reading source
// at test time is deliberate — the dispatch path is too contextual to
// drive end-to-end in a unit test, but the call sites are small and
// stable enough that a textual assertion is honest and verifiable.
// ============================================================

/// Read a file under the sidequest-server crate root, given a path relative
/// to `crates/sidequest-server/`. Uses `CARGO_MANIFEST_DIR` so the lookup
/// works regardless of the directory `cargo test` was invoked from.
fn read_crate_source(relative: &str) -> String {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let path = std::path::Path::new(manifest).join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e))
}

#[test]
fn emit_npc_turn_is_wired_into_scenario_dispatch() {
    let src = read_crate_source("src/dispatch/mod.rs");
    assert!(
        src.contains("crate::emit_npc_turn("),
        "dispatch/mod.rs must invoke crate::emit_npc_turn — wiring missing"
    );
    // The call site lives inside the ScenarioEventType::NpcAction match arm.
    // Requiring both strings in the file guards against the call being moved
    // somewhere irrelevant.
    assert!(
        src.contains("ScenarioEventType::NpcAction"),
        "dispatch/mod.rs must dispatch ScenarioEventType::NpcAction — refactor broke the wiring surface"
    );
}

#[test]
fn emit_trope_engagement_outcome_is_wired_into_tropes_dispatch() {
    let src = read_crate_source("src/dispatch/tropes.rs");
    assert!(
        src.contains("crate::emit_trope_engagement_outcome("),
        "dispatch/tropes.rs must invoke crate::emit_trope_engagement_outcome — wiring missing"
    );
    assert!(
        src.contains("classify_engagement_kind("),
        "dispatch/tropes.rs must call classify_engagement_kind — classifier wiring missing"
    );
}

#[test]
fn classify_engagement_kind_covers_all_three_kinds_including_aliases() {
    // classify_engagement_kind is private to dispatch/tropes.rs, so we assert
    // its tag map via source read rather than a direct call. This verifies
    // the alias folding (combat→confrontation, chase→evasion) the code comments
    // claim and ensures the three primary kinds + two aliases remain present.
    let src = read_crate_source("src/dispatch/tropes.rs");
    for needed in [
        r#"t == "stealth""#,
        r#"t == "confrontation""#,
        r#"t == "combat""#,
        r#"t == "evasion""#,
        r#"t == "chase""#,
    ] {
        assert!(
            src.contains(needed),
            "classify_engagement_kind must retain tag match for {needed} — alias coverage regressed"
        );
    }
}

#[test]
fn non_engagement_trope_produces_no_engagement_span() {
    // Callers MUST skip emission when classify returns None. The test asserts
    // at the source level that the two call sites guard their emit with
    // `if let Some(kind) = classify_engagement_kind(...)`, not a fallback
    // default value like "other".
    let src = read_crate_source("src/dispatch/tropes.rs");
    let occurrences = src.matches("if let Some(kind) = classify_engagement_kind").count();
    assert!(
        occurrences >= 2,
        "expected at least two `if let Some(kind) = classify_engagement_kind` guards \
         (auto-resolve site and fired-beat site); found {occurrences}. \
         A caller emitting unconditionally would smear misleading engagement data onto \
         non-engagement tropes."
    );
}

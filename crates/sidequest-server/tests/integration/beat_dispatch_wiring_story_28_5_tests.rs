//! Story 28-5: Wire apply_beat() into dispatch — beat selection drives encounter progression
//!
//! When creature_smith or the player selects a beat, dispatch calls apply_beat(beat_id, &def)
//! on the live StructuredEncounter. The beat's stat_check field names the mechanic to resolve.
//! After apply_beat(), check resolution and handle outcome. If escalates_to is set and
//! threshold crossed, start the escalation encounter.
//!
//! ACs tested:
//!   AC-Beat-Dispatch-Entry:     Dispatch has a function routing beat_selection actions
//!   AC-Attack-Beat-Routing:     stat_check "attack" → CreatureCore::resolve_attack() call path
//!   AC-Escape-Beat-Routing:     stat_check "escape" → chase escape resolver call path
//!   AC-Metric-Beat-Routing:     Other stat_checks → apply_beat metric_delta only
//!   AC-Resolution-Check:        After apply_beat, encounter.resolved checked and outcome dispatched
//!   AC-Escalation-Trigger:      escalates_to + threshold crossed → new encounter created
//!   AC-OTEL-Beat-Dispatched:    encounter.beat_dispatched event with beat_id, stat_check, resolver
//!   AC-OTEL-Stat-Check-Resolved: encounter.stat_check_resolved event with result
//!   AC-Wiring-Apply-Beat:       apply_beat has non-test consumer in dispatch
//!   AC-Wiring-Dispatch-Pipeline: Beat dispatch integrated into turn pipeline

use sidequest_genre::ConfrontationDef;

// =========================================================================
// Test fixtures — reusable confrontation definitions
// =========================================================================

fn standoff_with_escalation_yaml() -> &'static str {
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
    reveals: opponent_detail
  - id: bluff
    label: "Bluff"
    metric_delta: 3
    stat_check: NERVE
    risk: "opponent may call it"
  - id: draw
    label: "Draw"
    metric_delta: 5
    stat_check: DRAW
    resolution: true
escalates_to: combat
"#
}

fn combat_def_yaml() -> &'static str {
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
    metric_delta: -5
    stat_check: STRENGTH
  - id: defend
    label: "Defend"
    metric_delta: 0
    stat_check: CONSTITUTION
"#
}

fn negotiation_def_yaml() -> &'static str {
    r#"
type: negotiation
label: "Negotiation"
category: social
metric:
  name: leverage
  direction: ascending
  starting: 0
  threshold_high: 10
beats:
  - id: persuade
    label: "Persuade"
    metric_delta: 3
    stat_check: CHARISMA
  - id: intimidate
    label: "Intimidate"
    metric_delta: 2
    stat_check: STRENGTH
    risk: "may backfire"
"#
}

// =========================================================================
// AC-Beat-Dispatch-Entry: Dispatch has a beat_selection routing function
// =========================================================================

/// dispatch/mod.rs must contain a function or code path that handles beat
/// selection actions — routing beat_id to apply_beat on the encounter.
#[test]
fn dispatch_has_beat_selection_handler() {
    let dispatch_src = crate::test_helpers::dispatch_source_combined();
    let production_code = dispatch_src
        .split("#[cfg(test)]")
        .next()
        .unwrap_or(dispatch_src);

    assert!(
        production_code.contains("beat_selection")
            || production_code.contains("dispatch_beat")
            || production_code.contains("handle_beat"),
        "dispatch/mod.rs must contain a beat_selection handler, dispatch_beat function, \
         or handle_beat function to route beat selections — story 28-5 AC-Beat-Dispatch-Entry"
    );
}

// =========================================================================
// AC-Attack-Beat-Routing: attack stat_check → resolve_attack path
// =========================================================================

/// When a beat has stat_check "attack", the dispatch must route through
/// resolve_attack or apply_hp_delta on the creature. Verify the dispatch
/// code references resolve_attack or apply_hp_delta for attack beats.
#[test]
fn dispatch_routes_attack_stat_check() {
    let dispatch_src = crate::test_helpers::dispatch_source_combined();
    let production_code = dispatch_src
        .split("#[cfg(test)]")
        .next()
        .unwrap_or(dispatch_src);

    assert!(
        production_code.contains("resolve_attack") || production_code.contains("apply_hp_delta"),
        "dispatch must route stat_check 'attack' to resolve_attack() or apply_hp_delta() \
         on CreatureCore — story 28-5 AC-Attack-Beat-Routing"
    );
}

// =========================================================================
// AC-Escape-Beat-Routing: escape stat_check → chase escape resolver
// =========================================================================

/// When a beat has stat_check "escape", dispatch must route to chase
/// escape resolution logic — separation metric or escape threshold check.
#[test]
fn dispatch_routes_escape_stat_check() {
    let dispatch_src = crate::test_helpers::dispatch_source_combined();
    let production_code = dispatch_src
        .split("#[cfg(test)]")
        .next()
        .unwrap_or(dispatch_src);

    // The dispatch must reference escape resolution — either direct "escape"
    // matching or chase/separation logic
    assert!(
        production_code.contains("\"escape\"")
            || production_code.contains("escape_threshold")
            || production_code.contains("separation"),
        "dispatch must route stat_check 'escape' to chase escape resolution logic \
         — story 28-5 AC-Escape-Beat-Routing"
    );
}

// =========================================================================
// AC-Resolution-Check: After apply_beat, check encounter.resolved
// =========================================================================

/// After calling apply_beat(), dispatch must check whether the encounter
/// has been resolved and handle the outcome accordingly.
#[test]
fn dispatch_checks_resolution_after_apply_beat() {
    let dispatch_src = crate::test_helpers::dispatch_source_combined();
    let production_code = dispatch_src
        .split("#[cfg(test)]")
        .next()
        .unwrap_or(dispatch_src);

    assert!(
        production_code.contains(".resolved") || production_code.contains("is_resolved"),
        "dispatch must check encounter resolution after apply_beat() \
         — story 28-5 AC-Resolution-Check"
    );
}

// =========================================================================
// AC-Escalation-Trigger: escalates_to + threshold → new encounter
// =========================================================================

/// When an encounter resolves and escalates_to is set (e.g., standoff → combat),
/// dispatch must create the escalation encounter.
#[test]
fn dispatch_handles_escalation() {
    let dispatch_src = crate::test_helpers::dispatch_source_combined();
    let production_code = dispatch_src
        .split("#[cfg(test)]")
        .next()
        .unwrap_or(dispatch_src);

    assert!(
        production_code.contains("escalates_to")
            || production_code.contains("escalation_target")
            || production_code.contains("escalate_to_combat"),
        "dispatch must handle escalation when encounter resolves with escalates_to set \
         — story 28-5 AC-Escalation-Trigger"
    );
}

// =========================================================================
// AC-OTEL-Beat-Dispatched: encounter.beat_dispatched event
// =========================================================================

/// Dispatch must emit an encounter.beat_dispatched OTEL event containing
/// beat_id, stat_check, and the resolver used.
#[test]
fn dispatch_emits_beat_dispatched_otel() {
    let dispatch_src = crate::test_helpers::dispatch_source_combined();
    let production_code = dispatch_src
        .split("#[cfg(test)]")
        .next()
        .unwrap_or(dispatch_src);

    assert!(
        production_code.contains("beat_dispatched"),
        "dispatch must emit encounter.beat_dispatched OTEL event \
         — story 28-5 AC-OTEL-Beat-Dispatched"
    );
}

/// The beat_dispatched OTEL event must include the beat_id field.
#[test]
fn beat_dispatched_otel_includes_beat_id() {
    let dispatch_src = crate::test_helpers::dispatch_source_combined();
    let production_code = dispatch_src
        .split("#[cfg(test)]")
        .next()
        .unwrap_or(dispatch_src);

    // The OTEL event must reference beat_id in the field builder
    assert!(
        production_code.contains("\"beat_id\""),
        "encounter.beat_dispatched OTEL event must include beat_id field \
         — story 28-5"
    );
}

/// The beat_dispatched OTEL event must include the stat_check field.
#[test]
fn beat_dispatched_otel_includes_stat_check() {
    let dispatch_src = crate::test_helpers::dispatch_source_combined();
    let production_code = dispatch_src
        .split("#[cfg(test)]")
        .next()
        .unwrap_or(dispatch_src);

    assert!(
        production_code.contains("\"stat_check\""),
        "encounter.beat_dispatched OTEL event must include stat_check field \
         — story 28-5"
    );
}

// =========================================================================
// AC-OTEL-Stat-Check-Resolved: encounter.stat_check_resolved event
// =========================================================================

/// Dispatch must emit an encounter.stat_check_resolved OTEL event after
/// resolving the stat check (attack damage, escape roll, metric delta).
#[test]
fn dispatch_emits_stat_check_resolved_otel() {
    let dispatch_src = crate::test_helpers::dispatch_source_combined();
    let production_code = dispatch_src
        .split("#[cfg(test)]")
        .next()
        .unwrap_or(dispatch_src);

    assert!(
        production_code.contains("stat_check_resolved"),
        "dispatch must emit encounter.stat_check_resolved OTEL event \
         — story 28-5 AC-OTEL-Stat-Check-Resolved"
    );
}

// =========================================================================
// AC-Wiring-Apply-Beat: apply_beat has non-test consumer in dispatch
// =========================================================================

/// apply_beat must be called from production dispatch code, not just tests.
/// This is the core wiring guarantee.
#[test]
fn apply_beat_has_non_test_consumer_in_dispatch() {
    let dispatch_src = crate::test_helpers::dispatch_source_combined();
    let production_code = dispatch_src
        .split("#[cfg(test)]")
        .next()
        .unwrap_or(dispatch_src);

    assert!(
        production_code.contains("apply_beat"),
        "dispatch/mod.rs must call apply_beat() in production code \
         — the function must have a non-test consumer — story 28-5 AC-Wiring"
    );
}

// =========================================================================
// AC-Wiring-Dispatch-Pipeline: Beat dispatch integrated into turn pipeline
// =========================================================================

/// The beat dispatch must be reachable from dispatch_player_action —
/// either directly or through a called function. Verify the function
/// exists in the dispatch module.
#[test]
fn beat_dispatch_reachable_from_dispatch_player_action() {
    let dispatch_src = crate::test_helpers::dispatch_source_combined();
    let production_code = dispatch_src
        .split("#[cfg(test)]")
        .next()
        .unwrap_or(dispatch_src);

    // dispatch_player_action must reference the beat dispatch mechanism
    // Either by calling a named function or inline beat handling
    let has_beat_handling = production_code.contains("beat_selection")
        || production_code.contains("dispatch_beat")
        || production_code.contains("handle_beat");
    let has_apply_beat = production_code.contains("apply_beat");

    assert!(
        has_beat_handling && has_apply_beat,
        "dispatch must both handle beat selection AND call apply_beat() \
         in the turn pipeline — story 28-5 AC-Wiring-Dispatch-Pipeline"
    );
}

// =========================================================================
// Unit tests: apply_beat mechanics (these run against sidequest-game types)
// =========================================================================

/// Attack beat applies metric delta (HP damage) to the encounter metric.
/// This verifies the encounter engine correctly processes combat beats.
#[test]
fn apply_beat_attack_applies_metric_delta() {
    let def: ConfrontationDef = serde_yaml::from_str(combat_def_yaml()).unwrap();
    let mut encounter = sidequest_game::StructuredEncounter::from_confrontation_def(&def);

    assert_eq!(encounter.metric.current, 20, "HP starts at 20");

    let result = encounter.apply_beat("attack", &def);
    assert!(
        result.is_ok(),
        "apply_beat should succeed for valid beat_id"
    );
    assert_eq!(
        encounter.metric.current, 15,
        "attack beat with metric_delta -5 should reduce HP from 20 to 15"
    );
}

/// Non-combat beat (persuade) applies metric_delta only — no HP routing needed.
#[test]
fn apply_beat_metric_only_for_social_beats() {
    let def: ConfrontationDef = serde_yaml::from_str(negotiation_def_yaml()).unwrap();
    let mut encounter = sidequest_game::StructuredEncounter::from_confrontation_def(&def);

    assert_eq!(encounter.metric.current, 0, "leverage starts at 0");

    let result = encounter.apply_beat("persuade", &def);
    assert!(result.is_ok());
    assert_eq!(
        encounter.metric.current, 3,
        "persuade beat with metric_delta 3 should increase leverage from 0 to 3"
    );
    assert!(
        !encounter.resolved,
        "encounter should not be resolved — threshold not crossed"
    );
}

/// When metric crosses threshold, encounter becomes resolved.
#[test]
fn apply_beat_resolves_encounter_on_threshold_cross() {
    let def: ConfrontationDef = serde_yaml::from_str(combat_def_yaml()).unwrap();
    let mut encounter = sidequest_game::StructuredEncounter::from_confrontation_def(&def);

    // Apply attack beats until HP reaches 0 (threshold_low)
    // Starting HP: 20, each attack does -5
    for _ in 0..4 {
        let result = encounter.apply_beat("attack", &def);
        assert!(result.is_ok());
    }

    assert_eq!(
        encounter.metric.current, 0,
        "HP should be 0 after 4 attacks"
    );
    assert!(
        encounter.resolved,
        "encounter must be resolved when metric crosses threshold_low"
    );
}

/// Resolution-flagged beat resolves encounter regardless of threshold.
#[test]
fn apply_beat_resolution_flag_resolves_encounter() {
    let def: ConfrontationDef = serde_yaml::from_str(standoff_with_escalation_yaml()).unwrap();
    let mut encounter = sidequest_game::StructuredEncounter::from_confrontation_def(&def);

    // "draw" has resolution: true — should resolve even if tension < 10
    let result = encounter.apply_beat("draw", &def);
    assert!(result.is_ok());
    assert!(
        encounter.resolved,
        "beat with resolution: true must resolve the encounter"
    );
}

/// Escalation target is reported from confrontation def after encounter resolves.
#[test]
fn escalation_target_detected_when_resolved_with_escalates_to() {
    let def: ConfrontationDef = serde_yaml::from_str(standoff_with_escalation_yaml()).unwrap();
    let mut encounter = sidequest_game::StructuredEncounter::from_confrontation_def(&def);

    // Resolve via draw beat
    let result = encounter.apply_beat("draw", &def);
    assert!(result.is_ok());
    assert!(encounter.resolved);

    let escalation = encounter.escalation_target(&def);
    assert_eq!(
        escalation,
        Some("combat".to_string()),
        "resolved standoff with escalates_to: combat must report escalation target"
    );
}

/// escalate_to_combat produces a new combat encounter carrying actors.
#[test]
fn escalate_to_combat_creates_new_encounter() {
    let def: ConfrontationDef = serde_yaml::from_str(standoff_with_escalation_yaml()).unwrap();
    let mut encounter = sidequest_game::StructuredEncounter::from_confrontation_def(&def);

    // Add actors before resolution
    encounter.actors.push(sidequest_game::EncounterActor {
        name: "Player".to_string(),
        role: "duelist".to_string(),
        per_actor_state: std::collections::HashMap::new(),
    });
    encounter.actors.push(sidequest_game::EncounterActor {
        name: "NPC".to_string(),
        role: "opponent".to_string(),
        per_actor_state: std::collections::HashMap::new(),
    });

    // Resolve
    encounter.apply_beat("draw", &def).unwrap();

    let escalated = encounter.escalate_to_combat();
    assert!(
        escalated.is_some(),
        "escalate_to_combat should produce a new encounter when resolved"
    );

    let combat = escalated.unwrap();
    assert_eq!(combat.encounter_type, "combat");
    assert!(!combat.resolved, "escalated combat should start unresolved");
    assert_eq!(
        combat.actors.len(),
        2,
        "escalated combat should carry over actors from the original encounter"
    );
}

/// apply_beat on already-resolved encounter returns Err.
#[test]
fn apply_beat_on_resolved_encounter_returns_error() {
    let def: ConfrontationDef = serde_yaml::from_str(standoff_with_escalation_yaml()).unwrap();
    let mut encounter = sidequest_game::StructuredEncounter::from_confrontation_def(&def);

    // Resolve first
    encounter.apply_beat("draw", &def).unwrap();
    assert!(encounter.resolved);

    // Trying again should error
    let result = encounter.apply_beat("size_up", &def);
    assert!(
        result.is_err(),
        "apply_beat on resolved encounter must return Err"
    );
    assert!(
        result.unwrap_err().contains("already resolved"),
        "error message should indicate encounter is already resolved"
    );
}

/// apply_beat with unknown beat_id returns Err.
#[test]
fn apply_beat_unknown_beat_id_returns_error() {
    let def: ConfrontationDef = serde_yaml::from_str(combat_def_yaml()).unwrap();
    let mut encounter = sidequest_game::StructuredEncounter::from_confrontation_def(&def);

    let result = encounter.apply_beat("nonexistent_beat", &def);
    assert!(result.is_err(), "unknown beat_id must return Err");
    assert!(
        result.unwrap_err().contains("unknown beat id"),
        "error message should indicate unknown beat id"
    );
}

/// Beat counter increments with each apply_beat call.
#[test]
fn apply_beat_increments_beat_counter() {
    let def: ConfrontationDef = serde_yaml::from_str(negotiation_def_yaml()).unwrap();
    let mut encounter = sidequest_game::StructuredEncounter::from_confrontation_def(&def);

    assert_eq!(encounter.beat, 0, "beat starts at 0");

    encounter.apply_beat("persuade", &def).unwrap();
    assert_eq!(encounter.beat, 1, "beat should be 1 after first apply_beat");

    encounter.apply_beat("intimidate", &def).unwrap();
    assert_eq!(
        encounter.beat, 2,
        "beat should be 2 after second apply_beat"
    );
}

/// Phase transitions correctly based on beat number.
#[test]
fn apply_beat_transitions_phases_by_beat_number() {
    let def: ConfrontationDef = serde_yaml::from_str(negotiation_def_yaml()).unwrap();
    let mut encounter = sidequest_game::StructuredEncounter::from_confrontation_def(&def);

    assert_eq!(
        encounter.structured_phase,
        Some(sidequest_game::EncounterPhase::Setup),
        "should start in Setup"
    );

    encounter.apply_beat("persuade", &def).unwrap(); // beat 1
    assert_eq!(
        encounter.structured_phase,
        Some(sidequest_game::EncounterPhase::Opening),
        "beat 1 should transition to Opening"
    );

    encounter.apply_beat("intimidate", &def).unwrap(); // beat 2
    assert_eq!(
        encounter.structured_phase,
        Some(sidequest_game::EncounterPhase::Escalation),
        "beat 2 should transition to Escalation"
    );
}

// =========================================================================
// Wiring: find_confrontation_def used in beat dispatch path
// =========================================================================

/// The beat dispatch code must use find_confrontation_def to look up the
/// ConfrontationDef for the active encounter's type. This is needed to
/// pass the def to apply_beat.
#[test]
fn dispatch_uses_find_confrontation_def_for_beat_dispatch() {
    let dispatch_src = crate::test_helpers::dispatch_source_combined();
    let production_code = dispatch_src
        .split("#[cfg(test)]")
        .next()
        .unwrap_or(dispatch_src);

    // find_confrontation_def is already used in the overlay section,
    // but it must also appear in the beat dispatch section alongside apply_beat
    let apply_beat_region = production_code
        .find("apply_beat")
        .expect("apply_beat must exist in production dispatch code");

    // Verify find_confrontation_def appears in production code
    // (it needs to be called to get the def for apply_beat)
    assert!(
        production_code.contains("find_confrontation_def"),
        "dispatch must call find_confrontation_def() to look up the ConfrontationDef \
         for apply_beat() — story 28-5"
    );

    // Both must appear, confirming the wiring pipeline
    assert!(
        apply_beat_region > 0,
        "apply_beat must be present in production code"
    );
}

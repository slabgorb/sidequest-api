//! Story 38-6: Narrator cockpit-POV prompt extension.
//!
//! Wire-first RED tests for the narrator integration of dogfight
//! `SealedLetterLookup` encounters. The engine (38-5) resolves maneuver
//! commits into per-actor descriptor deltas, but the narrator doesn't
//! yet receive those descriptors or narration hints for cockpit-POV
//! rendering.
//!
//! **What this file proves:**
//!
//! 1. `format_encounter_context()` includes `per_actor_state` fields
//!    when actors have populated state (the narrator can see the cockpit
//!    descriptor — bearing, range, aspect, closure, energy, gun_solution).
//! 2. The narration hint from the resolved interaction cell is surfaced
//!    to the narrator prompt (not just consumed by the engine).
//! 3. `SealedLetterOutcome` carries the `narration_hint` from the cell
//!    so downstream consumers can inject it into the narrator prompt.
//! 4. `per_actor_state` rendering respects field presence — the narrator
//!    context omits fields that aren't set, preventing hallucination of
//!    geometry not backed by the engine.
//! 5. Gun solution field drives explicit narrator instruction: when true,
//!    narrator MUST describe a firing opportunity; when false, narrator
//!    MUST NOT mention firing.
//!
//! All tests are expected to FAIL (RED state) until Dev implements:
//!   - `per_actor_state` rendering in `format_encounter_context()`
//!   - `narration_hint` field on `SealedLetterOutcome`
//!   - Gun solution narrator instruction injection

use sidequest_game::encounter::{
    EncounterActor, EncounterMetric, MetricDirection, StructuredEncounter,
};
use sidequest_genre::ConfrontationDef;
use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════
// Fixtures
// ═══════════════════════════════════════════════════════════

/// Build a dogfight confrontation def for test fixtures.
fn dogfight_def_yaml() -> &'static str {
    r#"
type: dogfight
label: "Dogfight"
category: combat
resolution_mode: sealed_letter_lookup
metric:
  name: engagement_control
  direction: bidirectional
  starting: 0
  threshold_high: 100
  threshold_low: -100
beats:
  - id: maneuver_resolve
    label: "Maneuver Resolution"
    metric_delta: 0
    stat_check: PILOTING
interaction_table:
  version: "0.1.0"
  starting_state: merge
  maneuvers_consumed:
    - straight
    - bank
    - loop
    - kill_rotation
  cells:
    - pair: [straight, straight]
      name: "Clean merge"
      shape: "passive vs passive"
      red_view:
        target_bearing: "06"
        target_range: close
        target_aspect: tail_on
        closure: opening
        gun_solution: false
      blue_view:
        target_bearing: "06"
        target_range: close
        target_aspect: tail_on
        closure: opening
        gun_solution: false
      narration_hint: >-
        Both ships rip past each other. Stars rush back in. No shots.
    - pair: [loop, straight]
      name: "Red reverses onto Blue's six"
      shape: "offense vs passive"
      red_view:
        target_bearing: "12"
        target_range: close
        target_aspect: tail_on
        closure: opening
        gun_solution: true
      blue_view:
        target_bearing: "06"
        target_range: close
        target_aspect: head_on
        closure: opening
        gun_solution: false
      narration_hint: >-
        Red pulls a vertical loop and rolls out with Blue's exhaust
        dead centre in the gunsight. Red has one clean shot.
"#
}

/// Build a StructuredEncounter with two actors who have populated
/// per_actor_state (simulating a resolved dogfight turn).
fn encounter_with_per_actor_state() -> StructuredEncounter {
    let mut red_state = HashMap::new();
    red_state.insert(
        "target_bearing".to_string(),
        serde_json::Value::String("12".to_string()),
    );
    red_state.insert(
        "target_range".to_string(),
        serde_json::Value::String("close".to_string()),
    );
    red_state.insert(
        "target_aspect".to_string(),
        serde_json::Value::String("tail_on".to_string()),
    );
    red_state.insert(
        "closure".to_string(),
        serde_json::Value::String("opening".to_string()),
    );
    red_state.insert(
        "gun_solution".to_string(),
        serde_json::Value::Bool(true),
    );
    red_state.insert(
        "viewer_energy".to_string(),
        serde_json::json!(30),
    );

    let mut blue_state = HashMap::new();
    blue_state.insert(
        "target_bearing".to_string(),
        serde_json::Value::String("06".to_string()),
    );
    blue_state.insert(
        "target_range".to_string(),
        serde_json::Value::String("close".to_string()),
    );
    blue_state.insert(
        "target_aspect".to_string(),
        serde_json::Value::String("head_on".to_string()),
    );
    blue_state.insert(
        "closure".to_string(),
        serde_json::Value::String("opening".to_string()),
    );
    blue_state.insert(
        "gun_solution".to_string(),
        serde_json::Value::Bool(false),
    );
    blue_state.insert(
        "viewer_energy".to_string(),
        serde_json::json!(55),
    );

    StructuredEncounter {
        encounter_type: "dogfight".to_string(),
        structured_phase: None,
        beat: 1,
        metric: EncounterMetric {
            name: "engagement_control".to_string(),
            current: 0,
            starting: 0,
            direction: MetricDirection::Bidirectional,
            threshold_high: Some(100),
            threshold_low: Some(-100),
        },
        secondary_stats: None,
        actors: vec![
            EncounterActor {
                name: "Vex Corsair".to_string(),
                role: "red".to_string(),
                per_actor_state: red_state,
            },
            EncounterActor {
                name: "Nova Wing".to_string(),
                role: "blue".to_string(),
                per_actor_state: blue_state,
            },
        ],
    }
}

/// Build a StructuredEncounter with actors that have EMPTY per_actor_state
/// (no dogfight descriptors — normal BeatSelection encounter).
fn encounter_without_per_actor_state() -> StructuredEncounter {
    StructuredEncounter {
        encounter_type: "combat".to_string(),
        structured_phase: None,
        beat: 1,
        metric: EncounterMetric {
            name: "tension".to_string(),
            current: 0,
            starting: 0,
            direction: MetricDirection::Ascending,
            threshold_high: Some(3),
            threshold_low: None,
        },
        secondary_stats: None,
        actors: vec![
            EncounterActor {
                name: "Fighter".to_string(),
                role: "player".to_string(),
                per_actor_state: HashMap::new(),
            },
            EncounterActor {
                name: "Goblin".to_string(),
                role: "enemy".to_string(),
                per_actor_state: HashMap::new(),
            },
        ],
    }
}

// ═══════════════════════════════════════════════════════════
// AC1: format_encounter_context includes per_actor_state
// ═══════════════════════════════════════════════════════════

#[test]
fn encounter_context_includes_per_actor_state_fields() {
    // When per_actor_state is populated (dogfight descriptor),
    // format_encounter_context must include those fields so the
    // narrator can render cockpit-POV.
    let enc = encounter_with_per_actor_state();
    let def: ConfrontationDef = serde_yaml::from_str(dogfight_def_yaml()).unwrap();
    let context = enc.format_encounter_context(&def);

    // The context must contain per-actor descriptor fields.
    assert!(
        context.contains("target_bearing"),
        "encounter context must include target_bearing from per_actor_state.\n\
         Actual context:\n{context}"
    );
    assert!(
        context.contains("target_range"),
        "encounter context must include target_range from per_actor_state.\n\
         Actual context:\n{context}"
    );
    assert!(
        context.contains("gun_solution"),
        "encounter context must include gun_solution from per_actor_state.\n\
         Actual context:\n{context}"
    );
}

#[test]
fn encounter_context_includes_per_actor_state_per_role() {
    // Each actor's per_actor_state must be labeled by role so the
    // narrator knows which pilot sees what.
    let enc = encounter_with_per_actor_state();
    let def: ConfrontationDef = serde_yaml::from_str(dogfight_def_yaml()).unwrap();
    let context = enc.format_encounter_context(&def);

    // Red pilot has gun_solution: true; Blue has gun_solution: false.
    // The context must distinguish these per actor/role.
    assert!(
        context.contains("red") || context.contains("Red") || context.contains("Vex Corsair"),
        "encounter context must identify the red actor's state.\n\
         Actual context:\n{context}"
    );
    // Both actors' bearing must appear (12 for red, 06 for blue).
    assert!(
        context.contains("12") && context.contains("06"),
        "encounter context must include both actors' target_bearing values.\n\
         Red bearing: 12, Blue bearing: 06.\n\
         Actual context:\n{context}"
    );
}

#[test]
fn encounter_context_omits_per_actor_state_when_empty() {
    // For normal encounters (BeatSelection) where per_actor_state is
    // empty, the context should NOT include per-actor descriptor section.
    // This prevents polluting the narrator prompt with empty fields.
    let enc = encounter_without_per_actor_state();
    let def: ConfrontationDef = serde_yaml::from_str(
        r#"
type: combat
label: "Combat"
category: combat
metric:
  name: tension
  direction: ascending
  starting: 0
  threshold_high: 3
beats:
  - id: attack
    label: "Attack"
    metric_delta: 1
    stat_check: COMBAT
"#,
    )
    .unwrap();
    let context = enc.format_encounter_context(&def);

    // Should NOT contain per-actor descriptor fields.
    assert!(
        !context.contains("target_bearing"),
        "encounter context for empty per_actor_state must not include descriptor fields.\n\
         Actual context:\n{context}"
    );
    assert!(
        !context.contains("gun_solution"),
        "encounter context for empty per_actor_state must not include gun_solution.\n\
         Actual context:\n{context}"
    );
}

// ═══════════════════════════════════════════════════════════
// AC2: Gun solution drives explicit narrator instructions
// ═══════════════════════════════════════════════════════════

#[test]
fn encounter_context_includes_gun_solution_instruction_when_true() {
    // When an actor has gun_solution: true, the narrator context
    // must include an explicit instruction that the pilot has a
    // firing opportunity and MUST describe it.
    let enc = encounter_with_per_actor_state();
    let def: ConfrontationDef = serde_yaml::from_str(dogfight_def_yaml()).unwrap();
    let context = enc.format_encounter_context(&def);

    // Red has gun_solution: true — context must instruct the narrator.
    let context_lower = context.to_lowercase();
    assert!(
        context_lower.contains("firing")
            || context_lower.contains("fire")
            || context_lower.contains("shot")
            || context_lower.contains("gun"),
        "encounter context must include firing instruction when gun_solution is true.\n\
         Actual context:\n{context}"
    );
}

#[test]
fn encounter_context_includes_no_fire_instruction_for_false_gun_solution() {
    // When an actor has gun_solution: false, the narrator context
    // must explicitly forbid describing firing for that pilot.
    // This is the SOUL enforcement test — no hallucinated geometry.
    let enc = encounter_with_per_actor_state();
    let def: ConfrontationDef = serde_yaml::from_str(dogfight_def_yaml()).unwrap();
    let context = enc.format_encounter_context(&def);

    // Blue has gun_solution: false — context must include a "do not fire"
    // instruction for that pilot.
    // We check that somewhere in the blue pilot's section there is a
    // prohibition on firing. The exact text is up to Dev, but it must
    // be present.
    assert!(
        context.contains("MUST NOT")
            || context.contains("must not")
            || context.contains("do not fire")
            || context.contains("no shot")
            || context.contains("cannot fire"),
        "encounter context must include explicit no-fire instruction for \
         actors with gun_solution: false (SOUL enforcement).\n\
         Actual context:\n{context}"
    );
}

// ═══════════════════════════════════════════════════════════
// AC3: SealedLetterOutcome carries narration_hint
// ═══════════════════════════════════════════════════════════

#[test]
fn sealed_letter_outcome_has_narration_hint_field() {
    // SealedLetterOutcome must include the narration_hint from the
    // matched interaction cell, so downstream consumers (narrator
    // prompt builder) can inject it as the beat the narrator should hit.
    use sidequest_server::SealedLetterOutcome;

    // Construct an outcome — if narration_hint field doesn't exist,
    // this won't compile (RED compile gate).
    let outcome = SealedLetterOutcome {
        cell_name: "Red reverses onto Blue's six".to_string(),
        red_maneuver: "loop".to_string(),
        blue_maneuver: "straight".to_string(),
        narration_hint: "Red pulls a vertical loop and rolls out with Blue's exhaust dead centre in the gunsight.".to_string(),
    };

    assert_eq!(
        outcome.narration_hint,
        "Red pulls a vertical loop and rolls out with Blue's exhaust dead centre in the gunsight."
    );
}

#[test]
fn resolve_sealed_letter_returns_narration_hint() {
    // The resolution handler must extract narration_hint from the
    // matched cell and return it in the outcome.
    use sidequest_server::resolve_sealed_letter_lookup;

    let def: ConfrontationDef = serde_yaml::from_str(dogfight_def_yaml()).unwrap();
    let table = def
        .interaction_table
        .as_ref()
        .expect("fixture must have interaction_table");

    let mut enc = encounter_with_per_actor_state();
    // Reset per_actor_state to starting state for clean resolution.
    for actor in &mut enc.actors {
        actor.per_actor_state.clear();
    }

    let mut commits = HashMap::new();
    commits.insert("red".to_string(), "loop".to_string());
    commits.insert("blue".to_string(), "straight".to_string());

    let outcome = resolve_sealed_letter_lookup(&mut enc, &commits, table)
        .expect("resolution should succeed for valid pair");

    // The outcome must carry the narration_hint from the cell.
    assert!(
        !outcome.narration_hint.is_empty(),
        "SealedLetterOutcome.narration_hint must not be empty — \
         it carries the cell's narrative beat for the narrator.\n\
         Got outcome: {:?}",
        outcome
    );
    assert!(
        outcome.narration_hint.contains("loop")
            || outcome.narration_hint.contains("gunsight")
            || outcome.narration_hint.contains("exhaust"),
        "narration_hint should contain content from the cell's hint.\n\
         Got: {}",
        outcome.narration_hint
    );
}

// ═══════════════════════════════════════════════════════════
// AC4: Wiring test — per_actor_state rendering is reachable
//      from production code path
// ═══════════════════════════════════════════════════════════

#[test]
fn format_encounter_context_source_renders_per_actor_state() {
    // Wiring test: verify that the production code path in
    // format_encounter_context actually reads per_actor_state.
    // This catches the case where tests pass but the wire isn't
    // connected in the real dispatch path.
    let src = include_str!("../../src/dispatch/prompt.rs");

    // prompt.rs calls enc.format_encounter_context(def) — that's
    // already wired. But we need to verify that format_encounter_context
    // in encounter.rs actually reads per_actor_state.
    let encounter_src =
        include_str!("../../../sidequest-game/src/encounter.rs");
    assert!(
        encounter_src.contains("per_actor_state"),
        "format_encounter_context must reference per_actor_state in \
         its implementation — otherwise the narrator never sees the \
         dogfight descriptor.\n\
         The production prompt.rs calls format_encounter_context, \
         but that function must actually render per_actor_state."
    );

    // Also verify prompt.rs still calls format_encounter_context
    // (guard against accidental removal).
    assert!(
        src.contains("format_encounter_context"),
        "prompt.rs must call format_encounter_context to inject \
         encounter state into the narrator prompt."
    );
}

// ═══════════════════════════════════════════════════════════
// AC2: Closure field rendered for pacing context
// ═══════════════════════════════════════════════════════════

#[test]
fn encounter_context_includes_closure_for_pacing() {
    // The closure field (closing_fast, opening, stable, etc.) tells
    // the narrator about engagement pacing. It must be rendered.
    let enc = encounter_with_per_actor_state();
    let def: ConfrontationDef = serde_yaml::from_str(dogfight_def_yaml()).unwrap();
    let context = enc.format_encounter_context(&def);

    assert!(
        context.contains("opening") || context.contains("closure"),
        "encounter context must include closure field for pacing cues.\n\
         Actual context:\n{context}"
    );
}

#[test]
fn encounter_context_includes_energy_for_resource_awareness() {
    // Energy is the only resource the pilot manages. The narrator
    // must be able to reference it for flavor ("ragged engine note",
    // "fuel running low").
    let enc = encounter_with_per_actor_state();
    let def: ConfrontationDef = serde_yaml::from_str(dogfight_def_yaml()).unwrap();
    let context = enc.format_encounter_context(&def);

    // Red has energy 30, Blue has energy 55. At least one must appear.
    assert!(
        context.contains("30") || context.contains("55") || context.contains("energy"),
        "encounter context must include energy values from per_actor_state.\n\
         Actual context:\n{context}"
    );
}

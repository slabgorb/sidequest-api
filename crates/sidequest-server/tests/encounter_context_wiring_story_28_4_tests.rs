//! Story 28-4: Wire format_encounter_context() into narrator prompt
//!
//! The narrator prompt currently builds encounter context inline (from_combat_state/
//! from_chase_state + push_str) in dispatch/prompt.rs:335-374. This ad-hoc formatting
//! omits beats, stat_checks, and cinematography hints. format_encounter_context()
//! produces all of that — but has zero non-test callers.
//!
//! ACs tested:
//!   AC-Replaces-Inline:   Old from_combat_state/from_chase_state formatting removed from prompt.rs
//!   AC-Calls-FEC:         format_encounter_context() called from dispatch/prompt.rs
//!   AC-Includes-Beats:    Narrator prompt includes available beat options with stat_checks
//!   AC-Includes-Cinema:   Narrator prompt includes camera/pacing hints
//!   AC-OTEL:              encounter.context_injected event with encounter_type, phase, beat_count
//!   AC-Wiring:            format_encounter_context has a non-test consumer in sidequest-server

use sidequest_genre::ConfrontationDef;

// =========================================================================
// Test fixtures
// =========================================================================

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
"#
}

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

// =========================================================================
// AC-Replaces-Inline: Old inline formatting removed from prompt.rs
// =========================================================================

/// The old inline encounter formatting used from_combat_state and from_chase_state
/// to construct a StructuredEncounter, then manually formatted it with push_str.
/// This must be replaced by format_encounter_context(), not duplicated alongside.
#[test]
fn from_combat_state_removed_from_prompt_rs() {
    let prompt_src = include_str!("../src/dispatch/prompt.rs");

    assert!(
        !prompt_src.contains("from_combat_state"),
        "dispatch/prompt.rs must not contain 'from_combat_state' — \
         encounter context now comes from format_encounter_context()"
    );
}

#[test]
fn from_chase_state_removed_from_prompt_rs() {
    let prompt_src = include_str!("../src/dispatch/prompt.rs");

    assert!(
        !prompt_src.contains("from_chase_state"),
        "dispatch/prompt.rs must not contain 'from_chase_state' — \
         encounter context now comes from format_encounter_context()"
    );
}

/// The old inline formatting manually pushed encounter type, beat, metric.
/// Verify the ad-hoc ACTIVE ENCOUNTER string is gone.
#[test]
fn inline_active_encounter_formatting_removed() {
    let prompt_src = include_str!("../src/dispatch/prompt.rs");

    assert!(
        !prompt_src.contains("ACTIVE ENCOUNTER"),
        "dispatch/prompt.rs must not contain 'ACTIVE ENCOUNTER' — \
         format_encounter_context() produces the encounter header"
    );
}

// =========================================================================
// AC-Calls-FEC: format_encounter_context() called from dispatch/prompt.rs
// =========================================================================

/// dispatch/prompt.rs must call format_encounter_context to produce encounter context.
#[test]
fn format_encounter_context_called_in_prompt_rs() {
    let prompt_src = include_str!("../src/dispatch/prompt.rs");

    assert!(
        prompt_src.contains("format_encounter_context"),
        "dispatch/prompt.rs must call format_encounter_context() \
         to produce structured encounter context for the narrator"
    );
}

/// find_confrontation_def must be called in prompt.rs to look up the def
/// needed by format_encounter_context.
#[test]
fn find_confrontation_def_called_in_prompt_rs() {
    let prompt_src = include_str!("../src/dispatch/prompt.rs");

    assert!(
        prompt_src.contains("find_confrontation_def"),
        "dispatch/prompt.rs must call find_confrontation_def() \
         to look up the ConfrontationDef for format_encounter_context()"
    );
}

// =========================================================================
// AC-Includes-Beats: format_encounter_context output includes beats
// =========================================================================

/// format_encounter_context must include "Available:" section with beat labels
/// and stat_checks so the narrator knows what mechanical actions exist.
#[test]
fn format_encounter_context_includes_available_beats() {
    let def: ConfrontationDef = serde_yaml::from_str(standoff_yaml()).unwrap();
    let encounter = build_standoff_encounter();

    let context = encounter.format_encounter_context(&def);

    assert!(
        context.contains("Available:"),
        "Encounter context must contain 'Available:' header for beat options"
    );
    assert!(
        context.contains("Size Up"),
        "Encounter context must list beat label 'Size Up'"
    );
    assert!(
        context.contains("CUNNING"),
        "Encounter context must include stat_check 'CUNNING'"
    );
    assert!(
        context.contains("Bluff"),
        "Encounter context must list beat label 'Bluff'"
    );
    assert!(
        context.contains("Draw"),
        "Encounter context must list beat label 'Draw'"
    );
}

/// Beat details (reveals, risk, resolution) must appear in the context output.
#[test]
fn format_encounter_context_includes_beat_details() {
    let def: ConfrontationDef = serde_yaml::from_str(standoff_yaml()).unwrap();
    let encounter = build_standoff_encounter();

    let context = encounter.format_encounter_context(&def);

    assert!(
        context.contains("reveals opponent_detail"),
        "Size Up beat must show 'reveals opponent_detail'"
    );
    assert!(
        context.contains("risk: opponent may call it"),
        "Bluff beat must show its risk"
    );
    assert!(
        context.contains("resolves encounter"),
        "Draw beat must indicate it resolves the encounter"
    );
}

// =========================================================================
// AC-Includes-Cinema: Narrator prompt includes camera/pacing hints
// =========================================================================

/// format_encounter_context must include cinematography hints (Camera: line)
/// so the narrator knows the narrative intensity level.
#[test]
fn format_encounter_context_includes_cinematography() {
    let def: ConfrontationDef = serde_yaml::from_str(standoff_yaml()).unwrap();
    let encounter = build_standoff_encounter();

    let context = encounter.format_encounter_context(&def);

    assert!(
        context.contains("Camera:"),
        "Encounter context must include 'Camera:' cinematography hint"
    );
    assert!(
        context.contains("Sentences:"),
        "Encounter context must include sentence count guidance"
    );
}

// =========================================================================
// AC-OTEL: encounter.context_injected event in prompt.rs
// =========================================================================

/// The OTEL event must be "context_injected" (not the old "prompt_injection")
/// and must include encounter_type, phase, and beat_count fields.
#[test]
fn otel_context_injected_event_exists_in_prompt_rs() {
    let prompt_src = include_str!("../src/dispatch/prompt.rs");

    assert!(
        prompt_src.contains("context_injected"),
        "dispatch/prompt.rs must emit an encounter.context_injected OTEL event"
    );
}

/// The OTEL event must include beat_count so the GM panel can verify
/// that beats are actually flowing through the narrator prompt.
#[test]
fn otel_event_includes_beat_count_field() {
    let prompt_src = include_str!("../src/dispatch/prompt.rs");

    assert!(
        prompt_src.contains("beat_count"),
        "encounter.context_injected OTEL event must include beat_count field"
    );
}

/// The OTEL event must include encounter_type field.
#[test]
fn otel_event_includes_encounter_type_field() {
    let prompt_src = include_str!("../src/dispatch/prompt.rs");

    // The old event already has encounter_type, but verify it persists
    assert!(
        prompt_src.contains("encounter_type"),
        "encounter.context_injected OTEL event must include encounter_type field"
    );
}

// =========================================================================
// AC-Wiring: format_encounter_context has a non-test consumer
// =========================================================================

/// format_encounter_context must appear in sidequest-server source (not just tests).
/// This catches the deferral-cascade pattern where functions exist but are never
/// called from production code.
#[test]
fn format_encounter_context_has_non_test_consumer_in_server() {
    let prompt_src = include_str!("../src/dispatch/prompt.rs");

    assert!(
        prompt_src.contains("format_encounter_context"),
        "dispatch/prompt.rs must call format_encounter_context() — \
         the function must have a non-test consumer in sidequest-server"
    );
}

/// Verify confrontation_defs are accessible in prompt.rs (via ctx.confrontation_defs).
/// The build_prompt_context function needs access to defs to call find_confrontation_def.
#[test]
fn prompt_rs_uses_confrontation_defs() {
    let prompt_src = include_str!("../src/dispatch/prompt.rs");

    assert!(
        prompt_src.contains("confrontation_defs"),
        "dispatch/prompt.rs must access confrontation_defs \
         to look up the ConfrontationDef for format_encounter_context()"
    );
}

// =========================================================================
// Regression: format_encounter_context output is structurally sound
// =========================================================================

/// The encounter type header must be uppercase in brackets (e.g., [STANDOFF]).
#[test]
fn format_encounter_context_header_format() {
    let def: ConfrontationDef = serde_yaml::from_str(standoff_yaml()).unwrap();
    let encounter = build_standoff_encounter();

    let context = encounter.format_encounter_context(&def);

    assert!(
        context.starts_with("[STANDOFF]"),
        "Encounter context must start with [TYPE] header, got: {}",
        context.lines().next().unwrap_or("(empty)")
    );
}

/// Combat encounter context must also work, not just standoff.
#[test]
fn format_encounter_context_works_for_combat_type() {
    let def: ConfrontationDef = serde_yaml::from_str(combat_yaml()).unwrap();
    let encounter = build_combat_encounter();

    let context = encounter.format_encounter_context(&def);

    assert!(
        context.starts_with("[COMBAT]"),
        "Combat encounter must produce [COMBAT] header"
    );
    assert!(
        context.contains("Attack"),
        "Combat context must list Attack beat"
    );
    assert!(
        context.contains("Defend"),
        "Combat context must list Defend beat"
    );
    assert!(
        context.contains("Camera:"),
        "Combat context must include cinematography"
    );
}

// =========================================================================
// Helpers — build minimal StructuredEncounter for testing
// =========================================================================

fn build_standoff_encounter() -> sidequest_game::StructuredEncounter {
    use sidequest_game::{EncounterMetric, EncounterPhase, MetricDirection, StructuredEncounter};

    StructuredEncounter {
        encounter_type: "standoff".to_string(),
        beat: 3,
        metric: EncounterMetric {
            name: "tension".to_string(),
            current: 7,
            starting: 0,
            direction: MetricDirection::Ascending,
            threshold_high: Some(10),
            threshold_low: None,
        },
        actors: vec![],
        narrator_hints: vec![],
        structured_phase: Some(EncounterPhase::Escalation),
        secondary_stats: None,
        outcome: None,
        resolved: false,
        mood_override: None,
    }
}

fn build_combat_encounter() -> sidequest_game::StructuredEncounter {
    use sidequest_game::{EncounterMetric, EncounterPhase, MetricDirection, StructuredEncounter};

    StructuredEncounter {
        encounter_type: "combat".to_string(),
        beat: 1,
        metric: EncounterMetric {
            name: "hp".to_string(),
            current: 15,
            starting: 20,
            direction: MetricDirection::Descending,
            threshold_high: None,
            threshold_low: Some(0),
        },
        actors: vec![],
        narrator_hints: vec![],
        structured_phase: Some(EncounterPhase::Opening),
        secondary_stats: None,
        outcome: None,
        resolved: false,
        mood_override: None,
    }
}

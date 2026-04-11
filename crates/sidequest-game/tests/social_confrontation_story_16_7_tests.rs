//! Story 16-7: Social confrontation types — negotiation, interrogation, trial
//!
//! RED phase — Social confrontations are genre-agnostic structured encounters for
//! non-combat resolution. The confrontation engine from 16-2/16-3 already handles
//! metrics and beats; this story adds social-specific BeatDef fields and declares
//! three social confrontation templates.
//!
//! Key mappings:
//!   - negotiation → bidirectional leverage metric (0-10), persuade/threaten/concede/walk_away
//!   - interrogation → descending resistance metric (10→0), pressure/rapport/evidence
//!   - trial → ascending conviction metric, cross_examine/present_argument/object/yield
//!
//! New BeatDef fields (not yet on struct):
//!   - effect: Option<String>     — narrative effect on success
//!   - consequence: Option<String> — what happens on walk_away or resolution
//!   - requires: Option<String>   — precondition (e.g., "must have discovered relevant clue")
//!   - narrator_hint: Option<String> — guidance for narrator LLM
//!
//! ACs:
//!   AC1-Bidirectional: Negotiation leverage swings both directions (0-10)
//!   AC2-Descending:    Interrogation resistance decreases toward break (10→0)
//!   AC3-Trial:         Trial conviction ascending with debate-specific beats
//!   AC4-Beats:         All social beats functional
//!   AC5-Risk:          Failed stat checks on risky beats have consequences
//!   AC6-WalkAway:      Player can exit negotiation at any point via walk_away
//!   AC7-Override:      Genre can declare its own variant that replaces the default
//!   AC8-NoRegression:  Existing combat/chase tests unaffected (run separately)
//!   AC9-Integration:   Full negotiation sequence: persuade → concede → threaten → resolution
//!   AC10-OTEL:         OTEL events emitted for beat execution and metric changes

use sidequest_game::encounter::{EncounterPhase, MetricDirection, StructuredEncounter};
use sidequest_genre::ConfrontationDef;

// =========================================================================
// Helpers
// =========================================================================

/// Helper to locate genre packs directory.
fn genre_packs_path() -> std::path::PathBuf {
    if let Ok(path) = std::env::var("GENRE_PACKS_PATH") {
        return std::path::PathBuf::from(path);
    }
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.join("../../../sidequest-content/genre_packs")
}

/// Build a negotiation ConfrontationDef from inline YAML.
/// This tests the full deserialization path including new social beat fields.
fn negotiation_def() -> ConfrontationDef {
    let yaml = r#"
type: negotiation
label: "Tense Negotiation"
category: social
metric:
  name: leverage
  direction: bidirectional
  starting: 5
  threshold_high: 10
  threshold_low: 0
beats:
  - id: persuade
    label: "Make Your Case"
    metric_delta: 2
    stat_check: PRESENCE
    effect: "opponent considers your argument"
    narrator_hint: "Show the NPC weighing the player's words."
  - id: threaten
    label: "Threaten"
    metric_delta: 3
    stat_check: NERVE
    risk: "faction reputation -1 if it fails"
    consequence: "NPC becomes hostile on critical failure"
    narrator_hint: "Escalate tension. The threat should feel dangerous."
  - id: concede_point
    label: "Concede a Point"
    metric_delta: -1
    stat_check: PRESENCE
    effect: "opponent disposition +5, reveals their real goal"
    narrator_hint: "Strategic retreat. Player gives ground to gain information."
  - id: walk_away
    label: "Walk Away"
    metric_delta: 0
    stat_check: NERVE
    resolution: true
    consequence: "deal collapses, reputation intact"
    narrator_hint: "Player leaves the table. No deal."
mood: tension
"#;
    serde_yaml::from_str(yaml).expect("negotiation YAML should parse")
}

/// Build an interrogation ConfrontationDef from inline YAML.
fn interrogation_def() -> ConfrontationDef {
    let yaml = r#"
type: interrogation
label: "Interrogation"
category: social
metric:
  name: resistance
  direction: descending
  starting: 10
  threshold_low: 0
beats:
  - id: pressure
    label: "Apply Pressure"
    metric_delta: -2
    stat_check: NERVE
    risk: "subject shuts down if check fails badly"
    narrator_hint: "Intimidation. The subject squirms."
  - id: rapport
    label: "Build Rapport"
    metric_delta: -1
    stat_check: PRESENCE
    effect: "safe — no risk, slow progress"
    narrator_hint: "Empathy. The subject relaxes slightly."
  - id: evidence
    label: "Present Evidence"
    metric_delta: -3
    stat_check: PERCEPTION
    requires: "must have discovered relevant clue"
    narrator_hint: "Confront with proof. Watch their face change."
mood: tension
"#;
    serde_yaml::from_str(yaml).expect("interrogation YAML should parse")
}

/// Build a trial ConfrontationDef from inline YAML.
fn trial_def() -> ConfrontationDef {
    let yaml = r#"
type: trial
label: "Trial by Tribunal"
category: social
metric:
  name: conviction
  direction: ascending
  starting: 0
  threshold_high: 8
beats:
  - id: cross_examine
    label: "Cross-Examine"
    metric_delta: 2
    stat_check: INTELLECT
    effect: "witness credibility damaged"
    narrator_hint: "Pick apart the testimony. Find contradictions."
  - id: present_argument
    label: "Present Argument"
    metric_delta: 2
    stat_check: PRESENCE
    narrator_hint: "Address the tribunal directly. Be persuasive."
  - id: object
    label: "Object"
    metric_delta: 1
    stat_check: INTELLECT
    risk: "overruled — lose credibility if frivolous"
    narrator_hint: "Challenge procedure or evidence. Must be substantive."
  - id: yield
    label: "Yield the Floor"
    metric_delta: -1
    stat_check: PRESENCE
    effect: "opponent speaks — may reveal weakness"
    narrator_hint: "Strategic silence. Let them overplay their hand."
mood: tension
"#;
    serde_yaml::from_str(yaml).expect("trial YAML should parse")
}

// =========================================================================
// AC1-Bidirectional: Negotiation leverage swings both directions (0-10)
// =========================================================================

/// Negotiation metric starts at 5 with bidirectional direction.
#[test]
fn negotiation_metric_is_bidirectional() {
    let def = negotiation_def();
    let encounter = StructuredEncounter::from_confrontation_def(&def);

    assert_eq!(encounter.metric.name, "leverage");
    assert_eq!(encounter.metric.current, 5);
    assert_eq!(encounter.metric.starting, 5);
    assert_eq!(encounter.metric.direction, MetricDirection::Bidirectional);
    assert_eq!(encounter.metric.threshold_high, Some(10));
    assert_eq!(encounter.metric.threshold_low, Some(0));
}

/// Persuade increases leverage toward victory threshold.
#[test]
fn negotiation_persuade_increases_leverage() {
    let def = negotiation_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    encounter.apply_beat("persuade", &def).unwrap();
    assert_eq!(encounter.metric.current, 7, "persuade adds +2 leverage");
    assert!(!encounter.resolved, "not yet at threshold");
}

/// Concede decreases leverage toward defeat threshold.
#[test]
fn negotiation_concede_decreases_leverage() {
    let def = negotiation_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    encounter.apply_beat("concede_point", &def).unwrap();
    assert_eq!(encounter.metric.current, 4, "concede subtracts 1 leverage");
    assert!(!encounter.resolved);
}

/// Leverage reaching threshold_high (10) resolves as victory.
#[test]
fn negotiation_resolves_at_high_threshold() {
    let def = negotiation_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    // Start at 5, persuade (+2) three times = 11 → clamped/resolved at >=10
    encounter.apply_beat("persuade", &def).unwrap(); // 7
    encounter.apply_beat("threaten", &def).unwrap(); // 10
    assert!(
        encounter.resolved,
        "negotiation should resolve when leverage reaches threshold_high"
    );
    assert_eq!(encounter.structured_phase, Some(EncounterPhase::Resolution));
}

/// Leverage reaching threshold_low (0) resolves as defeat.
#[test]
fn negotiation_resolves_at_low_threshold() {
    let def = negotiation_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    // Start at 5, concede (-1) five times = 0 → resolved
    for _ in 0..5 {
        encounter.apply_beat("concede_point", &def).unwrap();
    }
    assert!(
        encounter.resolved,
        "negotiation should resolve when leverage reaches threshold_low"
    );
}

// =========================================================================
// AC2-Descending: Interrogation resistance decreases toward break (10→0)
// =========================================================================

/// Interrogation metric starts at 10 and descends.
#[test]
fn interrogation_metric_is_descending() {
    let def = interrogation_def();
    let encounter = StructuredEncounter::from_confrontation_def(&def);

    assert_eq!(encounter.metric.name, "resistance");
    assert_eq!(encounter.metric.current, 10);
    assert_eq!(encounter.metric.direction, MetricDirection::Descending);
    assert_eq!(encounter.metric.threshold_low, Some(0));
}

/// Pressure reduces resistance by 2.
#[test]
fn interrogation_pressure_reduces_resistance() {
    let def = interrogation_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    encounter.apply_beat("pressure", &def).unwrap();
    assert_eq!(
        encounter.metric.current, 8,
        "pressure subtracts 2 resistance"
    );
}

/// Rapport reduces resistance slowly (by 1).
#[test]
fn interrogation_rapport_slow_but_safe() {
    let def = interrogation_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    encounter.apply_beat("rapport", &def).unwrap();
    assert_eq!(
        encounter.metric.current, 9,
        "rapport subtracts 1 resistance"
    );
}

/// Evidence is the strongest beat (-3 resistance).
#[test]
fn interrogation_evidence_high_impact() {
    let def = interrogation_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    encounter.apply_beat("evidence", &def).unwrap();
    assert_eq!(
        encounter.metric.current, 7,
        "evidence subtracts 3 resistance"
    );
}

/// Interrogation resolves when resistance reaches 0.
#[test]
fn interrogation_resolves_at_zero_resistance() {
    let def = interrogation_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    // 10 - 2 - 2 - 3 - 2 - 1 = 0
    encounter.apply_beat("pressure", &def).unwrap(); // 8
    encounter.apply_beat("pressure", &def).unwrap(); // 6
    encounter.apply_beat("evidence", &def).unwrap(); // 3
    encounter.apply_beat("pressure", &def).unwrap(); // 1
    encounter.apply_beat("rapport", &def).unwrap(); // 0

    assert!(
        encounter.resolved,
        "interrogation should resolve at zero resistance"
    );
    assert_eq!(encounter.structured_phase, Some(EncounterPhase::Resolution));
}

// =========================================================================
// AC3-Trial: Trial conviction ascending with debate-specific beats
// =========================================================================

/// Trial metric starts at 0 and ascends toward conviction threshold.
#[test]
fn trial_metric_is_ascending() {
    let def = trial_def();
    let encounter = StructuredEncounter::from_confrontation_def(&def);

    assert_eq!(encounter.metric.name, "conviction");
    assert_eq!(encounter.metric.current, 0);
    assert_eq!(encounter.metric.direction, MetricDirection::Ascending);
    assert_eq!(encounter.metric.threshold_high, Some(8));
}

/// Cross-examine adds 2 conviction.
#[test]
fn trial_cross_examine_builds_conviction() {
    let def = trial_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    encounter.apply_beat("cross_examine", &def).unwrap();
    assert_eq!(encounter.metric.current, 2);
}

/// Present argument adds 2 conviction.
#[test]
fn trial_present_argument_builds_conviction() {
    let def = trial_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    encounter.apply_beat("present_argument", &def).unwrap();
    assert_eq!(encounter.metric.current, 2);
}

/// Yield loses 1 conviction but may reveal opponent weakness.
#[test]
fn trial_yield_loses_conviction() {
    let def = trial_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    // Need some conviction first so we don't go negative
    encounter.apply_beat("cross_examine", &def).unwrap(); // 2
    encounter.apply_beat("yield", &def).unwrap(); // 1
    assert_eq!(encounter.metric.current, 1);
}

/// Trial resolves when conviction reaches 8.
#[test]
fn trial_resolves_at_conviction_threshold() {
    let def = trial_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    // 0 + 2 + 2 + 2 + 2 = 8
    encounter.apply_beat("cross_examine", &def).unwrap(); // 2
    encounter.apply_beat("present_argument", &def).unwrap(); // 4
    encounter.apply_beat("cross_examine", &def).unwrap(); // 6
    encounter.apply_beat("present_argument", &def).unwrap(); // 8

    assert!(encounter.resolved, "trial should resolve at conviction 8");
}

// =========================================================================
// AC4-Beats: All social beats functional — new BeatDef fields
// =========================================================================

/// Beats with `effect` field carry narrative effect text.
/// This tests the new `effect` field on BeatDef (does not exist yet → E0609).
#[test]
fn beat_effect_field_present_on_concede() {
    let def = negotiation_def();
    let concede = def.beats.iter().find(|b| b.id == "concede_point").unwrap();
    assert_eq!(
        concede.effect.as_deref(),
        Some("opponent disposition +5, reveals their real goal"),
        "concede_point must carry its narrative effect"
    );
}

/// Beats with `narrator_hint` field guide the LLM narrator.
/// This tests the new `narrator_hint` field on BeatDef (does not exist yet → E0609).
#[test]
fn beat_narrator_hint_field_present_on_persuade() {
    let def = negotiation_def();
    let persuade = def.beats.iter().find(|b| b.id == "persuade").unwrap();
    assert_eq!(
        persuade.narrator_hint.as_deref(),
        Some("Show the NPC weighing the player's words."),
        "persuade must carry a narrator hint"
    );
}

/// Beats with `consequence` field describe what happens on resolution/failure.
/// This tests the new `consequence` field on BeatDef (does not exist yet → E0609).
#[test]
fn beat_consequence_field_present_on_walk_away() {
    let def = negotiation_def();
    let walk_away = def.beats.iter().find(|b| b.id == "walk_away").unwrap();
    assert_eq!(
        walk_away.consequence.as_deref(),
        Some("deal collapses, reputation intact"),
        "walk_away must describe its consequence"
    );
}

/// Beats with `requires` field have preconditions.
/// This tests the new `requires` field on BeatDef (does not exist yet → E0609).
#[test]
fn beat_requires_field_present_on_evidence() {
    let def = interrogation_def();
    let evidence = def.beats.iter().find(|b| b.id == "evidence").unwrap();
    assert_eq!(
        evidence.requires.as_deref(),
        Some("must have discovered relevant clue"),
        "evidence beat must have a precondition"
    );
}

/// All negotiation beats have narrator_hint fields.
#[test]
fn all_negotiation_beats_have_narrator_hints() {
    let def = negotiation_def();
    for beat in &def.beats {
        assert!(
            beat.narrator_hint.is_some(),
            "beat '{}' must have a narrator_hint",
            beat.id
        );
    }
}

/// All trial beats have narrator_hint fields.
#[test]
fn all_trial_beats_have_narrator_hints() {
    let def = trial_def();
    for beat in &def.beats {
        assert!(
            beat.narrator_hint.is_some(),
            "beat '{}' must have a narrator_hint",
            beat.id
        );
    }
}

// =========================================================================
// AC5-Risk: Failed stat checks on risky beats have consequences
// =========================================================================

/// Threaten beat has both risk and consequence fields.
#[test]
fn threaten_has_risk_and_consequence() {
    let def = negotiation_def();
    let threaten = def.beats.iter().find(|b| b.id == "threaten").unwrap();

    assert!(
        threaten.risk.is_some(),
        "threaten must have a risk description"
    );
    assert!(
        threaten.consequence.is_some(),
        "threaten must describe failure consequences"
    );
    assert_eq!(
        threaten.risk.as_deref(),
        Some("faction reputation -1 if it fails")
    );
    assert_eq!(
        threaten.consequence.as_deref(),
        Some("NPC becomes hostile on critical failure")
    );
}

/// Pressure beat has risk (subject shuts down).
#[test]
fn pressure_has_risk() {
    let def = interrogation_def();
    let pressure = def.beats.iter().find(|b| b.id == "pressure").unwrap();

    assert!(pressure.risk.is_some(), "pressure must have risk");
    assert_eq!(
        pressure.risk.as_deref(),
        Some("subject shuts down if check fails badly")
    );
}

/// Object beat in trial has risk (overruled if frivolous).
#[test]
fn object_has_risk() {
    let def = trial_def();
    let object = def.beats.iter().find(|b| b.id == "object").unwrap();

    assert!(object.risk.is_some(), "object must have risk");
    assert_eq!(
        object.risk.as_deref(),
        Some("overruled — lose credibility if frivolous")
    );
}

// =========================================================================
// AC6-WalkAway: Player can exit negotiation at any point via walk_away
// =========================================================================

/// Walk_away immediately resolves the negotiation.
#[test]
fn walk_away_resolves_immediately() {
    let def = negotiation_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    // Walk away on first beat — should resolve
    encounter.apply_beat("walk_away", &def).unwrap();
    assert!(encounter.resolved, "walk_away must resolve the encounter");
    assert_eq!(encounter.structured_phase, Some(EncounterPhase::Resolution));
}

/// Walk_away after multiple beats still resolves.
#[test]
fn walk_away_resolves_mid_negotiation() {
    let def = negotiation_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    encounter.apply_beat("persuade", &def).unwrap();
    encounter.apply_beat("concede_point", &def).unwrap();
    encounter.apply_beat("walk_away", &def).unwrap();

    assert!(encounter.resolved);
    // Leverage should be 5 + 2 - 1 + 0 = 6
    assert_eq!(encounter.metric.current, 6, "metric tracks up to walk_away");
}

/// Cannot apply beats after walk_away resolution.
#[test]
fn cannot_act_after_walk_away() {
    let def = negotiation_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    encounter.apply_beat("walk_away", &def).unwrap();
    let result = encounter.apply_beat("persuade", &def);
    assert!(
        result.is_err(),
        "should not be able to act after resolution"
    );
}

// =========================================================================
// AC7-Override: Genre can declare its own variant
// =========================================================================

/// All genres should have a negotiation confrontation type declared.
#[test]
fn all_genres_have_negotiation() {
    let packs_dir = genre_packs_path();
    let genres = [
        "spaghetti_western",
        "pulp_noir",
        "neon_dystopia",
        "space_opera",
        "road_warrior",
        "victoria",
        "elemental_harmony",
        "low_fantasy",
        "mutant_wasteland",
    ];

    for genre in &genres {
        let pack = sidequest_genre::load_genre_pack(&packs_dir.join(genre))
            .unwrap_or_else(|e| panic!("{} should load: {}", genre, e));

        let negotiation = pack
            .rules
            .confrontations
            .iter()
            .find(|c| c.confrontation_type == "negotiation");

        assert!(
            negotiation.is_some(),
            "{} must declare a 'negotiation' confrontation type",
            genre
        );
    }
}

/// pulp_noir must declare an interrogation confrontation type.
#[test]
fn pulp_noir_has_interrogation() {
    let packs_dir = genre_packs_path();
    let pack = sidequest_genre::load_genre_pack(&packs_dir.join("pulp_noir"))
        .expect("pulp_noir should load");

    let interrogation = pack
        .rules
        .confrontations
        .iter()
        .find(|c| c.confrontation_type == "interrogation");

    assert!(
        interrogation.is_some(),
        "pulp_noir must declare an 'interrogation' confrontation type"
    );
}

/// victoria must declare a trial confrontation type.
#[test]
fn victoria_has_trial() {
    let packs_dir = genre_packs_path();
    let pack = sidequest_genre::load_genre_pack(&packs_dir.join("victoria"))
        .expect("victoria should load");

    let trial = pack
        .rules
        .confrontations
        .iter()
        .find(|c| c.confrontation_type == "trial");

    assert!(
        trial.is_some(),
        "victoria must declare a 'trial' confrontation type"
    );
}

/// Genre-declared interrogation in pulp_noir is social category.
#[test]
fn pulp_noir_interrogation_is_social() {
    let packs_dir = genre_packs_path();
    let pack = sidequest_genre::load_genre_pack(&packs_dir.join("pulp_noir"))
        .expect("pulp_noir should load");

    let interrogation = pack
        .rules
        .confrontations
        .iter()
        .find(|c| c.confrontation_type == "interrogation")
        .expect("interrogation must exist in pulp_noir");

    assert_eq!(
        interrogation.category, "social",
        "interrogation must be social category"
    );
    assert_eq!(
        interrogation.metric.direction, "descending",
        "interrogation resistance descends"
    );
}

/// Genre-declared trial in victoria uses ascending conviction.
#[test]
fn victoria_trial_uses_ascending_conviction() {
    let packs_dir = genre_packs_path();
    let pack = sidequest_genre::load_genre_pack(&packs_dir.join("victoria"))
        .expect("victoria should load");

    let trial = pack
        .rules
        .confrontations
        .iter()
        .find(|c| c.confrontation_type == "trial")
        .expect("trial must exist in victoria");

    assert_eq!(trial.category, "social");
    assert_eq!(trial.metric.name, "conviction");
    assert_eq!(trial.metric.direction, "ascending");
}

// =========================================================================
// AC9-Integration: Full negotiation sequence
// =========================================================================

/// Full negotiation: persuade → concede → threaten → resolution.
#[test]
fn full_negotiation_sequence_to_victory() {
    let def = negotiation_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    // Start: leverage = 5
    assert_eq!(encounter.beat, 0);
    assert_eq!(encounter.encounter_type, "negotiation");

    // Beat 1: Persuade (+2) → leverage = 7
    encounter.apply_beat("persuade", &def).unwrap();
    assert_eq!(encounter.metric.current, 7);
    assert_eq!(encounter.beat, 1);
    assert!(!encounter.resolved);

    // Beat 2: Concede (-1) → leverage = 6
    encounter.apply_beat("concede_point", &def).unwrap();
    assert_eq!(encounter.metric.current, 6);
    assert_eq!(encounter.beat, 2);
    assert!(!encounter.resolved);

    // Beat 3: Threaten (+3) → leverage = 9
    encounter.apply_beat("threaten", &def).unwrap();
    assert_eq!(encounter.metric.current, 9);
    assert_eq!(encounter.beat, 3);
    assert!(!encounter.resolved);

    // Beat 4: Persuade (+2) → leverage = 11 → resolved (>= 10)
    encounter.apply_beat("persuade", &def).unwrap();
    assert_eq!(encounter.metric.current, 11);
    assert!(encounter.resolved, "should resolve at leverage >= 10");
    assert_eq!(encounter.structured_phase, Some(EncounterPhase::Resolution));
}

/// Full negotiation to defeat: concede repeatedly until leverage hits 0.
#[test]
fn full_negotiation_sequence_to_defeat() {
    let def = negotiation_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    // Start: leverage = 5, concede 5 times (-1 each)
    for i in 0..5 {
        encounter.apply_beat("concede_point", &def).unwrap();
        let expected = 5 - (i + 1);
        assert_eq!(
            encounter.metric.current,
            expected,
            "after {} concessions, leverage should be {}",
            i + 1,
            expected
        );
    }
    assert!(encounter.resolved, "should resolve at leverage 0");
}

/// Full interrogation sequence to breakthrough.
#[test]
fn full_interrogation_to_breakthrough() {
    let def = interrogation_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    assert_eq!(encounter.encounter_type, "interrogation");
    assert_eq!(encounter.metric.current, 10);

    // Pressure (-2), rapport (-1), evidence (-3), pressure (-2), pressure (-2) = 0
    encounter.apply_beat("pressure", &def).unwrap(); // 8
    encounter.apply_beat("rapport", &def).unwrap(); // 7
    encounter.apply_beat("evidence", &def).unwrap(); // 4
    encounter.apply_beat("pressure", &def).unwrap(); // 2
    encounter.apply_beat("pressure", &def).unwrap(); // 0

    assert!(encounter.resolved, "interrogation resolves at 0 resistance");
}

/// Full trial sequence to conviction.
#[test]
fn full_trial_to_conviction() {
    let def = trial_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    assert_eq!(encounter.encounter_type, "trial");
    assert_eq!(encounter.metric.current, 0);

    // cross_examine (+2), present_argument (+2), object (+1), cross_examine (+2),
    // present_argument (+2) = 9 → resolved at >= 8
    encounter.apply_beat("cross_examine", &def).unwrap(); // 2
    encounter.apply_beat("present_argument", &def).unwrap(); // 4
    encounter.apply_beat("object", &def).unwrap(); // 5
    encounter.apply_beat("cross_examine", &def).unwrap(); // 7
    assert!(!encounter.resolved, "not yet at threshold 8");
    encounter.apply_beat("present_argument", &def).unwrap(); // 9
    assert!(encounter.resolved, "trial resolves at conviction >= 8");
}

// =========================================================================
// Context formatting
// =========================================================================

/// Negotiation context shows bidirectional thresholds.
#[test]
fn negotiation_context_shows_bidirectional_thresholds() {
    let def = negotiation_def();
    let encounter = StructuredEncounter::from_confrontation_def(&def);
    let context = encounter.format_encounter_context(&def);

    assert!(
        context.contains("[NEGOTIATION]"),
        "header should be NEGOTIATION"
    );
    assert!(
        context.contains("Leverage: 5"),
        "should show leverage value"
    );
    assert!(
        context.contains("low:0") && context.contains("high:10"),
        "should show both thresholds for bidirectional: got {}",
        context
    );
}

/// Interrogation context shows descending metric.
#[test]
fn interrogation_context_shows_descending_metric() {
    let def = interrogation_def();
    let encounter = StructuredEncounter::from_confrontation_def(&def);
    let context = encounter.format_encounter_context(&def);

    assert!(context.contains("[INTERROGATION]"));
    assert!(context.contains("Resistance: 10"));
}

/// Narrator hints from beats appear in formatted context.
/// Tests that narrator_hint field is included in format output.
#[test]
fn context_includes_narrator_hints_from_beats() {
    let def = negotiation_def();
    let encounter = StructuredEncounter::from_confrontation_def(&def);
    let context = encounter.format_encounter_context(&def);

    // format_encounter_context should include narrator_hint for each beat
    assert!(
        context.contains("narrator_hint:") || context.contains("Show the NPC"),
        "context should surface narrator hints from beats: got {}",
        context
    );
}

// =========================================================================
// Encounter type and mood
// =========================================================================

/// Social encounters set mood_override from the confrontation def.
#[test]
fn social_encounter_sets_mood() {
    let def = negotiation_def();
    let encounter = StructuredEncounter::from_confrontation_def(&def);
    assert_eq!(
        encounter.mood_override.as_deref(),
        Some("tension"),
        "negotiation should set mood to tension"
    );
}

/// Unknown beat ID returns error.
#[test]
fn unknown_beat_id_returns_error() {
    let def = negotiation_def();
    let mut encounter = StructuredEncounter::from_confrontation_def(&def);

    let result = encounter.apply_beat("nonexistent_beat", &def);
    assert!(result.is_err(), "unknown beat should return error");
    assert!(
        result.unwrap_err().contains("unknown beat id"),
        "error should mention unknown beat"
    );
}

/// Social encounters start in Setup phase.
#[test]
fn social_encounter_starts_in_setup() {
    let def = negotiation_def();
    let encounter = StructuredEncounter::from_confrontation_def(&def);
    assert_eq!(encounter.structured_phase, Some(EncounterPhase::Setup));
    assert_eq!(encounter.beat, 0);
    assert!(!encounter.resolved);
}

// =========================================================================
// Wiring test: ConfrontationDef with social fields round-trips through serde
// =========================================================================

/// Social beat fields survive serialization round-trip.
/// Tests that new fields (effect, consequence, requires, narrator_hint) are
/// preserved through serialize → deserialize.
#[test]
fn social_beat_fields_survive_serde_roundtrip() {
    let def = negotiation_def();

    // Serialize to JSON (tests Serialize derive)
    let json = serde_json::to_string(&def).expect("should serialize");

    // Deserialize back (tests Deserialize derive)
    let restored: ConfrontationDef =
        serde_json::from_str(&json).expect("should deserialize from JSON");

    let concede = restored
        .beats
        .iter()
        .find(|b| b.id == "concede_point")
        .unwrap();

    assert_eq!(
        concede.effect.as_deref(),
        Some("opponent disposition +5, reveals their real goal"),
        "effect must survive round-trip"
    );

    let threaten = restored.beats.iter().find(|b| b.id == "threaten").unwrap();

    assert_eq!(
        threaten.consequence.as_deref(),
        Some("NPC becomes hostile on critical failure"),
        "consequence must survive round-trip"
    );

    assert!(
        threaten.narrator_hint.is_some(),
        "narrator_hint must survive round-trip"
    );
}

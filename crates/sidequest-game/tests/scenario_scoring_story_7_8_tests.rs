//! Tests for Story 7-8: Scenario scoring — evidence collection metrics,
//! accusation accuracy, deduction quality.
//!
//! Written by TEA (Fezzik) against acceptance criteria.
//! Tests WILL NOT COMPILE until Dev implements ScenarioScore, DeductionQuality,
//! ScenarioGrade, and score_scenario() in sidequest_game::scenario_scoring.

use std::collections::HashMap;

use sidequest_game::accusation::{AccusationResult, EvidenceQuality, EvidenceSummary};
use sidequest_game::clue_activation::{
    ClueGraph, ClueNode, ClueType, ClueVisibility, DiscoveryMethod,
};
use sidequest_game::npc_actions::ScenarioRole;
use sidequest_game::scenario_scoring::{
    score_scenario, DeductionQuality, ScenarioGrade, ScenarioScore, ScenarioScoreInput,
};
use sidequest_game::scenario_state::ScenarioState;

// ===========================================================================
// Test helpers
// ===========================================================================

/// Build a ClueNode with given id and implication list.
fn make_clue(id: &str, implicates: &[&str]) -> ClueNode {
    let mut node = ClueNode::new(
        id.to_string(),
        format!("Test clue {}", id),
        ClueType::Physical,
        DiscoveryMethod::Forensic,
        ClueVisibility::Obvious,
    );
    for &suspect in implicates {
        node.add_implication(suspect.to_string());
    }
    node
}

/// Build a red herring clue that should NOT count toward evidence coverage.
fn make_red_herring(id: &str) -> ClueNode {
    let mut node = ClueNode::new(
        id.to_string(),
        format!("Red herring {}", id),
        ClueType::Physical,
        DiscoveryMethod::Search,
        ClueVisibility::Hidden,
    );
    node.set_red_herring(true);
    node
}

/// Build a minimal ScenarioState with the given clue graph and discovered clues.
fn make_scenario_state(
    clues: Vec<ClueNode>,
    discovered: &[&str],
    npc_roles: HashMap<String, ScenarioRole>,
    guilty_npc: &str,
) -> ScenarioState {
    let clue_graph = ClueGraph::new(clues);
    let adjacency = HashMap::new();
    let mut state = ScenarioState::new(clue_graph, npc_roles, guilty_npc.to_string(), adjacency);
    for &clue_id in discovered {
        state.discover_clue(clue_id.to_string());
    }
    state
}

/// Build a mock AccusationResult with the given correctness and evidence quality.
fn make_accusation_result(is_correct: bool, quality: EvidenceQuality) -> AccusationResult {
    let evidence_score = match quality {
        EvidenceQuality::Circumstantial => 2,
        EvidenceQuality::Strong => 4,
        EvidenceQuality::Airtight => 8,
    };
    AccusationResult {
        quality,
        is_correct,
        evidence: EvidenceSummary {
            implicating_clues: vec!["clue1".to_string()],
            facts_about_accused: vec![],
            corroborating_claims: vec![],
            contradicting_claims: vec![],
            accused_credibility: 0.5,
            contradiction_count: 0,
            evidence_score,
        },
        narrative_prompt: String::new(),
    }
}

/// Standard 3-NPC role map: one guilty, one witness, one innocent.
fn standard_npc_roles() -> HashMap<String, ScenarioRole> {
    let mut roles = HashMap::new();
    roles.insert("guilty_npc".to_string(), ScenarioRole::Guilty);
    roles.insert("witness_npc".to_string(), ScenarioRole::Witness);
    roles.insert("innocent_npc".to_string(), ScenarioRole::Innocent);
    roles
}

/// Standard set of 6 non-red-herring clues implicating "guilty_npc".
fn standard_clues() -> Vec<ClueNode> {
    vec![
        make_clue("clue1", &["guilty_npc"]),
        make_clue("clue2", &["guilty_npc"]),
        make_clue("clue3", &["guilty_npc"]),
        make_clue("clue4", &["guilty_npc"]),
        make_clue("clue5", &["guilty_npc"]),
        make_clue("clue6", &["guilty_npc"]),
    ]
}

/// Build a ScenarioScoreInput with defaults for fields we don't care about.
fn make_input<'a>(
    state: &'a ScenarioState,
    accusation: &'a AccusationResult,
    total_turns: u64,
    npcs_questioned: &'a [String],
) -> ScenarioScoreInput<'a> {
    ScenarioScoreInput {
        scenario_state: state,
        accusation_result: accusation,
        total_turns,
        npcs_questioned,
    }
}

// ===========================================================================
// AC: Evidence coverage — percentage of available clues discovered
// ===========================================================================

#[test]
fn evidence_coverage_full_discovery() {
    let state = make_scenario_state(
        standard_clues(),
        &["clue1", "clue2", "clue3", "clue4", "clue5", "clue6"],
        standard_npc_roles(),
        "guilty_npc",
    );
    let result = make_accusation_result(true, EvidenceQuality::Airtight);
    let input = make_input(&state, &result, 10, &[]);
    let score = score_scenario(&input);

    assert!(
        (score.evidence_coverage() - 1.0).abs() < f64::EPSILON,
        "All 6 of 6 clues discovered should be 100% coverage, got {}",
        score.evidence_coverage()
    );
}

#[test]
fn evidence_coverage_partial_discovery() {
    let state = make_scenario_state(
        standard_clues(),
        &["clue1", "clue3"],
        standard_npc_roles(),
        "guilty_npc",
    );
    let result = make_accusation_result(true, EvidenceQuality::Circumstantial);
    let input = make_input(&state, &result, 8, &[]);
    let score = score_scenario(&input);

    let expected = 2.0 / 6.0;
    assert!(
        (score.evidence_coverage() - expected).abs() < 0.01,
        "2 of 6 clues discovered should be ~33%, got {}",
        score.evidence_coverage()
    );
}

#[test]
fn evidence_coverage_no_discovery() {
    let state = make_scenario_state(standard_clues(), &[], standard_npc_roles(), "guilty_npc");
    let result = make_accusation_result(false, EvidenceQuality::Circumstantial);
    let input = make_input(&state, &result, 5, &[]);
    let score = score_scenario(&input);

    assert!(
        score.evidence_coverage().abs() < f64::EPSILON,
        "No clues discovered should be 0% coverage, got {}",
        score.evidence_coverage()
    );
}

#[test]
fn evidence_coverage_excludes_red_herrings() {
    let mut clues = standard_clues(); // 6 real clues
    clues.push(make_red_herring("herring1"));
    clues.push(make_red_herring("herring2"));
    // 8 total nodes, but only 6 are real clues

    let state = make_scenario_state(
        clues,
        &["clue1", "clue2", "clue3", "herring1"],
        standard_npc_roles(),
        "guilty_npc",
    );
    let result = make_accusation_result(true, EvidenceQuality::Strong);
    let input = make_input(&state, &result, 12, &[]);
    let score = score_scenario(&input);

    // 3 real clues discovered out of 6 real total = 50%
    // herring1 discovered but doesn't count for or against
    let expected = 3.0 / 6.0;
    assert!(
        (score.evidence_coverage() - expected).abs() < 0.01,
        "3 of 6 real clues = 50% (red herrings excluded from denominator), got {}",
        score.evidence_coverage()
    );
}

// ===========================================================================
// AC: Interrogation breadth — percentage of relevant NPCs questioned
// ===========================================================================

#[test]
fn interrogation_breadth_all_questioned() {
    let state = make_scenario_state(
        standard_clues(),
        &["clue1"],
        standard_npc_roles(),
        "guilty_npc",
    );
    let result = make_accusation_result(true, EvidenceQuality::Strong);
    let npcs_questioned = vec![
        "guilty_npc".to_string(),
        "witness_npc".to_string(),
        "innocent_npc".to_string(),
    ];
    let input = make_input(&state, &result, 10, &npcs_questioned);
    let score = score_scenario(&input);

    assert!(
        (score.interrogation_breadth() - 1.0).abs() < f64::EPSILON,
        "3 of 3 NPCs questioned should be 100%, got {}",
        score.interrogation_breadth()
    );
}

#[test]
fn interrogation_breadth_partial() {
    let state = make_scenario_state(
        standard_clues(),
        &["clue1"],
        standard_npc_roles(),
        "guilty_npc",
    );
    let result = make_accusation_result(true, EvidenceQuality::Circumstantial);
    let npcs_questioned = vec!["witness_npc".to_string()];
    let input = make_input(&state, &result, 8, &npcs_questioned);
    let score = score_scenario(&input);

    let expected = 1.0 / 3.0;
    assert!(
        (score.interrogation_breadth() - expected).abs() < 0.01,
        "1 of 3 NPCs questioned should be ~33%, got {}",
        score.interrogation_breadth()
    );
}

#[test]
fn interrogation_breadth_none_questioned() {
    let state = make_scenario_state(standard_clues(), &[], standard_npc_roles(), "guilty_npc");
    let result = make_accusation_result(false, EvidenceQuality::Circumstantial);
    let npcs_questioned: Vec<String> = vec![];
    let input = make_input(&state, &result, 5, &npcs_questioned);
    let score = score_scenario(&input);

    assert!(
        score.interrogation_breadth().abs() < f64::EPSILON,
        "No NPCs questioned should be 0%, got {}",
        score.interrogation_breadth()
    );
}

// ===========================================================================
// AC: Deduction quality — Guesswork / Methodical / Masterful
// ===========================================================================

#[test]
fn deduction_quality_guesswork_from_circumstantial() {
    let state = make_scenario_state(
        standard_clues(),
        &["clue1"],
        standard_npc_roles(),
        "guilty_npc",
    );
    let result = make_accusation_result(true, EvidenceQuality::Circumstantial);
    let input = make_input(&state, &result, 5, &[]);
    let score = score_scenario(&input);

    assert_eq!(
        score.deduction_quality(),
        DeductionQuality::Guesswork,
        "Circumstantial evidence should yield Guesswork deduction"
    );
}

#[test]
fn deduction_quality_methodical_from_strong() {
    let state = make_scenario_state(
        standard_clues(),
        &["clue1", "clue2", "clue3"],
        standard_npc_roles(),
        "guilty_npc",
    );
    let result = make_accusation_result(true, EvidenceQuality::Strong);
    let input = make_input(&state, &result, 10, &[]);
    let score = score_scenario(&input);

    assert_eq!(
        score.deduction_quality(),
        DeductionQuality::Methodical,
        "Strong evidence should yield Methodical deduction"
    );
}

#[test]
fn deduction_quality_masterful_from_airtight() {
    let state = make_scenario_state(
        standard_clues(),
        &["clue1", "clue2", "clue3", "clue4", "clue5", "clue6"],
        standard_npc_roles(),
        "guilty_npc",
    );
    let result = make_accusation_result(true, EvidenceQuality::Airtight);
    let input = make_input(&state, &result, 15, &[]);
    let score = score_scenario(&input);

    assert_eq!(
        score.deduction_quality(),
        DeductionQuality::Masterful,
        "Airtight evidence should yield Masterful deduction"
    );
}

// ===========================================================================
// AC: Grade assignment — Gold / Silver / Bronze / Failed
// ===========================================================================

#[test]
fn grade_gold_requires_correct_airtight_high_coverage() {
    // Gold: correct accusation + airtight evidence + high coverage (>80%)
    let state = make_scenario_state(
        standard_clues(),
        &["clue1", "clue2", "clue3", "clue4", "clue5"],
        standard_npc_roles(),
        "guilty_npc",
    );
    let result = make_accusation_result(true, EvidenceQuality::Airtight);
    let input = make_input(&state, &result, 12, &[]);
    let score = score_scenario(&input);

    assert_eq!(
        score.grade(),
        ScenarioGrade::Gold,
        "Correct + Airtight + 83% coverage should yield Gold"
    );
}

#[test]
fn grade_silver_correct_strong_moderate_coverage() {
    // Silver: correct + strong evidence + moderate coverage
    let state = make_scenario_state(
        standard_clues(),
        &["clue1", "clue2", "clue3"],
        standard_npc_roles(),
        "guilty_npc",
    );
    let result = make_accusation_result(true, EvidenceQuality::Strong);
    let input = make_input(&state, &result, 10, &[]);
    let score = score_scenario(&input);

    assert_eq!(
        score.grade(),
        ScenarioGrade::Silver,
        "Correct + Strong + 50% coverage should yield Silver"
    );
}

#[test]
fn grade_bronze_correct_circumstantial() {
    // Bronze: correct but weak evidence
    let state = make_scenario_state(
        standard_clues(),
        &["clue1"],
        standard_npc_roles(),
        "guilty_npc",
    );
    let result = make_accusation_result(true, EvidenceQuality::Circumstantial);
    let input = make_input(&state, &result, 8, &[]);
    let score = score_scenario(&input);

    assert_eq!(
        score.grade(),
        ScenarioGrade::Bronze,
        "Correct + Circumstantial should yield Bronze at best"
    );
}

// ===========================================================================
// AC: Failed grade — wrong accusation ALWAYS results in Failed
// ===========================================================================

#[test]
fn grade_failed_wrong_accusation_despite_airtight_evidence() {
    // Wrong accusation → Failed, even with airtight evidence
    let state = make_scenario_state(
        standard_clues(),
        &["clue1", "clue2", "clue3", "clue4", "clue5", "clue6"],
        standard_npc_roles(),
        "guilty_npc",
    );
    let result = make_accusation_result(false, EvidenceQuality::Airtight);
    let input = make_input(&state, &result, 20, &[]);
    let score = score_scenario(&input);

    assert_eq!(
        score.grade(),
        ScenarioGrade::Failed,
        "Wrong accusation MUST always be Failed, regardless of evidence quality"
    );
}

#[test]
fn grade_failed_wrong_accusation_circumstantial() {
    let state = make_scenario_state(
        standard_clues(),
        &["clue1"],
        standard_npc_roles(),
        "guilty_npc",
    );
    let result = make_accusation_result(false, EvidenceQuality::Circumstantial);
    let input = make_input(&state, &result, 5, &[]);
    let score = score_scenario(&input);

    assert_eq!(
        score.grade(),
        ScenarioGrade::Failed,
        "Wrong accusation with weak evidence should also be Failed"
    );
}

// ===========================================================================
// AC: Turn tracking — total turns recorded in scorecard
// ===========================================================================

#[test]
fn turn_count_recorded() {
    let state = make_scenario_state(
        standard_clues(),
        &["clue1"],
        standard_npc_roles(),
        "guilty_npc",
    );
    let result = make_accusation_result(true, EvidenceQuality::Strong);
    let input = make_input(&state, &result, 42, &[]);
    let score = score_scenario(&input);

    assert_eq!(
        score.total_turns(),
        42,
        "Score should record the exact turn count passed in"
    );
}

#[test]
fn turn_count_zero_is_valid() {
    let state = make_scenario_state(standard_clues(), &[], standard_npc_roles(), "guilty_npc");
    let result = make_accusation_result(false, EvidenceQuality::Circumstantial);
    let input = make_input(&state, &result, 0, &[]);
    let score = score_scenario(&input);

    assert_eq!(
        score.total_turns(),
        0,
        "Zero turns should be recorded as-is"
    );
}

// ===========================================================================
// AC: Serializable — ScenarioScore serializes for archive inclusion
// ===========================================================================

#[test]
fn scenario_score_serialization_roundtrip() {
    let state = make_scenario_state(
        standard_clues(),
        &["clue1", "clue2", "clue3"],
        standard_npc_roles(),
        "guilty_npc",
    );
    let result = make_accusation_result(true, EvidenceQuality::Strong);
    let input = make_input(&state, &result, 15, &[]);
    let score = score_scenario(&input);

    let json = serde_json::to_string(&score).expect("ScenarioScore should serialize to JSON");
    let restored: ScenarioScore =
        serde_json::from_str(&json).expect("ScenarioScore should deserialize from JSON");

    assert_eq!(score.grade(), restored.grade(), "Grade survives roundtrip");
    assert_eq!(
        score.deduction_quality(),
        restored.deduction_quality(),
        "Deduction quality survives roundtrip"
    );
    assert!(
        (score.evidence_coverage() - restored.evidence_coverage()).abs() < f64::EPSILON,
        "Evidence coverage survives roundtrip"
    );
    assert_eq!(
        score.total_turns(),
        restored.total_turns(),
        "Turn count survives roundtrip"
    );
}

// ===========================================================================
// Rule #2: #[non_exhaustive] on public enums
// ===========================================================================

#[test]
fn deduction_quality_is_non_exhaustive() {
    // If #[non_exhaustive] is missing, this wildcard arm is unreachable
    // and the compiler will warn (or error with deny(unreachable_patterns)).
    let quality = DeductionQuality::Guesswork;
    let label = match quality {
        DeductionQuality::Guesswork => "guesswork",
        DeductionQuality::Methodical => "methodical",
        DeductionQuality::Masterful => "masterful",
        _ => "unknown future variant",
    };
    assert_eq!(label, "guesswork");
}

#[test]
fn scenario_grade_is_non_exhaustive() {
    let grade = ScenarioGrade::Gold;
    let label = match grade {
        ScenarioGrade::Failed => "failed",
        ScenarioGrade::Bronze => "bronze",
        ScenarioGrade::Silver => "silver",
        ScenarioGrade::Gold => "gold",
        _ => "unknown future variant",
    };
    assert_eq!(label, "gold");
}

// ===========================================================================
// Edge case: empty scenario (no clues, no NPCs)
// ===========================================================================

#[test]
fn score_empty_scenario_no_clues() {
    let state = make_scenario_state(vec![], &[], HashMap::new(), "guilty_npc");
    let result = make_accusation_result(false, EvidenceQuality::Circumstantial);
    let input = make_input(&state, &result, 1, &[]);
    let score = score_scenario(&input);

    // No clues in scenario → coverage should be 0.0 (not NaN from 0/0)
    assert!(
        score.evidence_coverage().is_finite(),
        "Evidence coverage must not be NaN or Inf for empty scenario"
    );
    assert_eq!(score.grade(), ScenarioGrade::Failed);
}

// ===========================================================================
// Integration: wiring test — score_scenario is reachable from lib.rs
// ===========================================================================

#[test]
fn scenario_scoring_module_exported_from_lib() {
    // This verifies that the scoring module is publicly accessible from
    // sidequest_game::scenario_scoring — not just an internal module.
    // If this compiles, the module is wired.
    let _: fn(&ScenarioScoreInput) -> ScenarioScore = score_scenario;
}

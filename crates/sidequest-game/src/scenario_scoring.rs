//! Scenario scoring — post-resolution scorecard for whodunit scenarios.
//!
//! Story 7-8: After a scenario resolves via accusation, compute a scorecard
//! measuring evidence coverage, interrogation breadth, deduction quality,
//! and an overall grade. The scorecard is informational only — it does not
//! affect future gameplay.

use serde::{Deserialize, Serialize};

use crate::accusation::{AccusationResult, EvidenceQuality};
use crate::scenario_state::ScenarioState;

/// How thoroughly the player reasoned through the evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum DeductionQuality {
    /// Weak evidence — the player guessed.
    Guesswork,
    /// Moderate evidence — the player followed a trail.
    Methodical,
    /// Overwhelming evidence — the player left no stone unturned.
    Masterful,
}

/// Overall scenario grade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ScenarioGrade {
    /// Wrong accusation — automatic fail regardless of evidence.
    Failed,
    /// Correct accusation with weak evidence.
    Bronze,
    /// Correct accusation with strong evidence.
    Silver,
    /// Correct accusation with airtight evidence and high coverage.
    Gold,
}

/// Input bundle for scenario scoring.
pub struct ScenarioScoreInput<'a> {
    /// The scenario's runtime state (clue graph, discovered clues, NPC roles).
    pub scenario_state: &'a ScenarioState,
    /// The result of the player's accusation.
    pub accusation_result: &'a AccusationResult,
    /// Total turns elapsed during the scenario.
    pub total_turns: u64,
    /// Names of NPCs the player interrogated.
    pub npcs_questioned: &'a [String],
}

/// A scenario scorecard produced after resolution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioScore {
    evidence_coverage: f64,
    interrogation_breadth: f64,
    deduction_quality: DeductionQuality,
    grade: ScenarioGrade,
    total_turns: u64,
}

impl ScenarioScore {
    /// Fraction of non-red-herring clues the player discovered (0.0–1.0).
    pub fn evidence_coverage(&self) -> f64 {
        self.evidence_coverage
    }

    /// Fraction of scenario NPCs the player interrogated (0.0–1.0).
    pub fn interrogation_breadth(&self) -> f64 {
        self.interrogation_breadth
    }

    /// How well the player's deduction was supported by evidence.
    pub fn deduction_quality(&self) -> DeductionQuality {
        self.deduction_quality
    }

    /// Overall grade for the scenario.
    pub fn grade(&self) -> ScenarioGrade {
        self.grade
    }

    /// Total turns elapsed during the scenario.
    pub fn total_turns(&self) -> u64 {
        self.total_turns
    }
}

/// Compute a scenario scorecard from the resolved scenario state and accusation.
pub fn score_scenario(input: &ScenarioScoreInput) -> ScenarioScore {
    let evidence_coverage = compute_evidence_coverage(input.scenario_state);
    let interrogation_breadth =
        compute_interrogation_breadth(input.scenario_state, input.npcs_questioned);
    let deduction_quality = compute_deduction_quality(input.accusation_result);
    let grade = compute_grade(input.accusation_result, evidence_coverage);

    ScenarioScore {
        evidence_coverage,
        interrogation_breadth,
        deduction_quality,
        grade,
        total_turns: input.total_turns,
    }
}

/// Evidence coverage: discovered non-red-herring clues / total non-red-herring clues.
fn compute_evidence_coverage(state: &ScenarioState) -> f64 {
    let all_nodes = state.clue_graph().nodes();

    let real_clue_ids: Vec<&str> = all_nodes
        .iter()
        .filter(|n| !n.is_red_herring())
        .map(|n| n.id())
        .collect();

    let total = real_clue_ids.len();
    if total == 0 {
        return 0.0;
    }

    let discovered = state.discovered_clues();
    let found = real_clue_ids
        .iter()
        .filter(|id| discovered.contains(**id))
        .count();

    found as f64 / total as f64
}

/// Interrogation breadth: NPCs questioned / total NPCs in scenario.
fn compute_interrogation_breadth(state: &ScenarioState, npcs_questioned: &[String]) -> f64 {
    let total_npcs = state.npc_roles().len();
    if total_npcs == 0 {
        return 0.0;
    }

    let npc_names: std::collections::HashSet<&str> =
        state.npc_roles().keys().map(|s| s.as_str()).collect();

    let questioned_count = npcs_questioned
        .iter()
        .filter(|q| npc_names.contains(q.as_str()))
        .count();

    questioned_count as f64 / total_npcs as f64
}

/// Map evidence quality to deduction quality.
fn compute_deduction_quality(accusation: &AccusationResult) -> DeductionQuality {
    match accusation.quality {
        EvidenceQuality::Circumstantial => DeductionQuality::Guesswork,
        EvidenceQuality::Strong => DeductionQuality::Methodical,
        EvidenceQuality::Airtight => DeductionQuality::Masterful,
    }
}

/// Compute grade from accusation correctness and evidence coverage.
///
/// Wrong accusation → always Failed.
/// Correct + Airtight + coverage > 0.8 → Gold.
/// Correct + Strong (or Airtight with low coverage) → Silver.
/// Correct + Circumstantial → Bronze.
fn compute_grade(accusation: &AccusationResult, evidence_coverage: f64) -> ScenarioGrade {
    if !accusation.is_correct {
        return ScenarioGrade::Failed;
    }

    match accusation.quality {
        EvidenceQuality::Airtight if evidence_coverage > 0.8 => ScenarioGrade::Gold,
        EvidenceQuality::Airtight => ScenarioGrade::Silver,
        EvidenceQuality::Strong => ScenarioGrade::Silver,
        EvidenceQuality::Circumstantial => ScenarioGrade::Bronze,
    }
}

//! Wiring tests for Story 35-3: Wire scenario_scoring into /accuse handler.
//!
//! Verifies that:
//! 1. ScenarioState has questioned_npcs tracking with serde(default)
//! 2. record_questioned_npc() populates the set
//! 3. score_scenario is called from dispatch/slash.rs (source-level wiring check)
//! 4. questioned_npcs tracking is wired into dispatch/mod.rs
//! 5. Score summary is appended to accusation narration text

use std::collections::HashMap;

use sidequest_game::accusation::{AccusationResult, EvidenceQuality, EvidenceSummary};
use sidequest_game::clue_activation::{
    ClueGraph, ClueNode, ClueType, ClueVisibility, DiscoveryMethod,
};
use sidequest_game::npc_actions::ScenarioRole;
use sidequest_game::scenario_scoring::{score_scenario, ScenarioScoreInput};
use sidequest_game::scenario_state::ScenarioState;

// ===========================================================================
// Helpers
// ===========================================================================

fn make_clue(id: &str) -> ClueNode {
    ClueNode::new(
        id.to_string(),
        format!("Test clue {}", id),
        ClueType::Physical,
        DiscoveryMethod::Forensic,
        ClueVisibility::Obvious,
    )
}

fn make_scenario() -> ScenarioState {
    let clues = vec![make_clue("c1"), make_clue("c2"), make_clue("c3")];
    let clue_graph = ClueGraph::new(clues);
    let mut npc_roles = HashMap::new();
    npc_roles.insert("Alice".to_string(), ScenarioRole::Guilty);
    npc_roles.insert("Bob".to_string(), ScenarioRole::Witness);
    npc_roles.insert("Carol".to_string(), ScenarioRole::Innocent);
    let adjacency = HashMap::new();
    ScenarioState::new(clue_graph, npc_roles, "Alice".to_string(), adjacency)
}

// ===========================================================================
// AC-1: questioned_npcs field with serde(default)
// ===========================================================================

#[test]
fn ac1_questioned_npcs_starts_empty() {
    let scenario = make_scenario();
    assert!(
        scenario.questioned_npcs().is_empty(),
        "questioned_npcs should be empty on init"
    );
}

#[test]
fn ac1_questioned_npcs_serde_default_compat() {
    // Deserialize a ScenarioState JSON that lacks the questioned_npcs field.
    // serde(default) should populate it as empty HashSet.
    let scenario = make_scenario();
    let json = serde_json::to_string(&scenario).unwrap();

    // Remove questioned_npcs from JSON to simulate old save format
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let mut map = v.as_object().unwrap().clone();
    map.remove("questioned_npcs");
    let trimmed = serde_json::to_string(&map).unwrap();

    let restored: ScenarioState = serde_json::from_str(&trimmed).unwrap();
    assert!(
        restored.questioned_npcs().is_empty(),
        "questioned_npcs should default to empty on deserialization"
    );
}

// ===========================================================================
// AC-2: record_questioned_npc populates the set
// ===========================================================================

#[test]
fn ac2_record_questioned_npc_tracks_unique_names() {
    let mut scenario = make_scenario();

    scenario.record_questioned_npc("Bob".to_string());
    scenario.record_questioned_npc("Carol".to_string());
    scenario.record_questioned_npc("Bob".to_string()); // duplicate

    assert_eq!(scenario.questioned_npcs().len(), 2);
    assert!(scenario.questioned_npcs().contains("Bob"));
    assert!(scenario.questioned_npcs().contains("Carol"));
}

// ===========================================================================
// AC-3: score_scenario works with questioned_npcs data
// ===========================================================================

#[test]
fn ac3_score_scenario_uses_questioned_npcs_for_breadth() {
    let mut scenario = make_scenario();
    scenario.discover_clue("c1".to_string());
    scenario.record_questioned_npc("Bob".to_string());

    let result = AccusationResult {
        is_correct: true,
        quality: EvidenceQuality::Strong,
        narrative_prompt: "Test".to_string(),
        evidence: EvidenceSummary {
            implicating_clues: vec!["c1".to_string()],
            facts_about_accused: vec![],
            corroborating_claims: vec![],
            contradicting_claims: vec![],
            accused_credibility: 0.5,
            contradiction_count: 0,
            evidence_score: 50,
        },
    };

    let questioned: Vec<String> = scenario.questioned_npcs().iter().cloned().collect();
    let input = ScenarioScoreInput {
        scenario_state: &scenario,
        accusation_result: &result,
        total_turns: 10,
        npcs_questioned: &questioned,
    };
    let score = score_scenario(&input);

    // 1 of 3 NPCs questioned = ~33%
    assert!(
        score.interrogation_breadth() > 0.3 && score.interrogation_breadth() < 0.4,
        "Expected ~33% interrogation breadth, got {:.2}",
        score.interrogation_breadth()
    );
}

// ===========================================================================
// AC-4/5: Wiring checks — score_scenario called from slash.rs, questioned_npcs from dispatch
// ===========================================================================

#[test]
fn ac4_slash_rs_calls_score_scenario() {
    let source = include_str!("../src/dispatch/slash.rs");
    assert!(
        source.contains("score_scenario"),
        "dispatch/slash.rs must call score_scenario() after accusation resolution — story 35-3"
    );
}

#[test]
fn ac4_slash_rs_emits_scenario_scored_otel() {
    let source = include_str!("../src/dispatch/slash.rs");
    assert!(
        source.contains("scenario.scored"),
        "dispatch/slash.rs must emit scenario.scored OTEL event — story 35-3"
    );
}

#[test]
fn ac5_slash_rs_appends_score_summary_to_narration() {
    let source = include_str!("../src/dispatch/slash.rs");
    assert!(
        source.contains("score_summary"),
        "dispatch/slash.rs must append score summary to accusation narration — story 35-3"
    );
}

#[test]
fn ac2_dispatch_mod_tracks_questioned_npcs() {
    let source = include_str!("../src/dispatch/mod.rs");
    assert!(
        source.contains("record_questioned_npc"),
        "dispatch/mod.rs must call record_questioned_npc() for scenario NPC tracking — story 35-3"
    );
}

// ===========================================================================
// Wiring test: score_scenario has non-test consumer
// ===========================================================================

#[test]
fn wiring_score_scenario_has_production_consumer() {
    let source = include_str!("../src/dispatch/slash.rs");
    let production_code = source.split("#[cfg(test)]").next().unwrap_or(source);
    assert!(
        production_code.contains("score_scenario"),
        "score_scenario must have a non-test consumer in dispatch/slash.rs"
    );
}

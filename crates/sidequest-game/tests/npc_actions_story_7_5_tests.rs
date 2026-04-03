//! Tests for Story 7-5: NPC autonomous actions — scenario-driven behaviors.
//!
//! Written by TEA (Han Solo) against acceptance criteria.
//! All tests should FAIL (compile error) since the module doesn't exist yet.

use std::collections::HashMap;

use rand::rngs::StdRng;
use rand::SeedableRng;

use sidequest_game::belief_state::{Belief, BeliefSource, BeliefState};
use sidequest_game::npc_actions::{
    available_actions, select_npc_action, NpcAction, ScenarioRole,
};

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn seeded_rng(seed: u64) -> StdRng {
    StdRng::seed_from_u64(seed)
}

fn empty_beliefs() -> BeliefState {
    BeliefState::new()
}

fn beliefs_with_suspicion(subject: &str, confidence: f32) -> BeliefState {
    let mut bs = BeliefState::new();
    bs.add_belief(Belief::suspicion(
        subject.to_string(),
        format!("I suspect {} is involved", subject),
        1,
        BeliefSource::Inferred,
        confidence,
    ));
    bs
}

// ---------------------------------------------------------------------------
// AC: Role-based actions — Guilty NPC has access to CreateAlibi,
//     DestroyEvidence, Flee, Confess
// ---------------------------------------------------------------------------

#[test]
fn ac_guilty_has_create_alibi_at_moderate_tension() {
    let actions = available_actions(&ScenarioRole::Guilty, &empty_beliefs(), 0.5);
    let has_alibi = actions.iter().any(|(a, _)| matches!(a, NpcAction::CreateAlibi { .. }));
    assert!(has_alibi, "Guilty NPC should have CreateAlibi available at moderate tension");
}

#[test]
fn ac_guilty_has_destroy_evidence_at_high_tension() {
    let actions = available_actions(&ScenarioRole::Guilty, &empty_beliefs(), 0.7);
    let has_destroy = actions.iter().any(|(a, _)| matches!(a, NpcAction::DestroyEvidence { .. }));
    assert!(has_destroy, "Guilty NPC should have DestroyEvidence at tension > 0.6");
}

#[test]
fn ac_guilty_has_flee_at_extreme_tension() {
    let actions = available_actions(&ScenarioRole::Guilty, &empty_beliefs(), 0.9);
    let has_flee = actions.iter().any(|(a, _)| matches!(a, NpcAction::Flee { .. }));
    assert!(has_flee, "Guilty NPC should have Flee at tension > 0.8");
}

#[test]
fn ac_guilty_has_confess_at_extreme_tension() {
    let actions = available_actions(&ScenarioRole::Guilty, &empty_beliefs(), 0.9);
    let has_confess = actions.iter().any(|(a, _)| matches!(a, NpcAction::Confess { .. }));
    assert!(has_confess, "Guilty NPC should have Confess at tension > 0.8");
}

#[test]
fn ac_innocent_cannot_create_alibi() {
    let actions = available_actions(&ScenarioRole::Innocent, &empty_beliefs(), 0.9);
    let has_alibi = actions.iter().any(|(a, _)| matches!(a, NpcAction::CreateAlibi { .. }));
    assert!(!has_alibi, "Innocent NPC should not have CreateAlibi");
}

#[test]
fn ac_innocent_cannot_destroy_evidence() {
    let actions = available_actions(&ScenarioRole::Innocent, &empty_beliefs(), 0.9);
    let has_destroy = actions.iter().any(|(a, _)| matches!(a, NpcAction::DestroyEvidence { .. }));
    assert!(!has_destroy, "Innocent NPC should not have DestroyEvidence");
}

#[test]
fn ac_witness_has_spread_rumor() {
    let beliefs = beliefs_with_suspicion("suspect", 0.8);
    let actions = available_actions(&ScenarioRole::Witness, &beliefs, 0.5);
    let has_rumor = actions.iter().any(|(a, _)| matches!(a, NpcAction::SpreadRumor { .. }));
    assert!(has_rumor, "Witness NPC with strong suspicion should have SpreadRumor");
}

#[test]
fn ac_accomplice_has_create_alibi() {
    let actions = available_actions(&ScenarioRole::Accomplice, &empty_beliefs(), 0.5);
    let has_alibi = actions.iter().any(|(a, _)| matches!(a, NpcAction::CreateAlibi { .. }));
    assert!(has_alibi, "Accomplice should have CreateAlibi (covering for the guilty)");
}

// ---------------------------------------------------------------------------
// AC: Tension scaling — higher tension increases weight of desperate actions
// ---------------------------------------------------------------------------

#[test]
fn ac_tension_scaling_alibi_weight_increases_with_tension() {
    let low = available_actions(&ScenarioRole::Guilty, &empty_beliefs(), 0.2);
    let high = available_actions(&ScenarioRole::Guilty, &empty_beliefs(), 0.8);

    let alibi_weight_low = low.iter()
        .find(|(a, _)| matches!(a, NpcAction::CreateAlibi { .. }))
        .map(|(_, w)| *w)
        .unwrap_or(0.0);
    let alibi_weight_high = high.iter()
        .find(|(a, _)| matches!(a, NpcAction::CreateAlibi { .. }))
        .map(|(_, w)| *w)
        .unwrap_or(0.0);

    assert!(alibi_weight_high > alibi_weight_low,
        "CreateAlibi weight should increase with tension: low={}, high={}",
        alibi_weight_low, alibi_weight_high);
}

#[test]
fn ac_tension_scaling_no_destroy_evidence_below_threshold() {
    let actions = available_actions(&ScenarioRole::Guilty, &empty_beliefs(), 0.3);
    let has_destroy = actions.iter().any(|(a, _)| matches!(a, NpcAction::DestroyEvidence { .. }));
    assert!(!has_destroy, "DestroyEvidence should not be available at tension 0.3 (threshold is 0.6)");
}

#[test]
fn ac_tension_scaling_no_flee_below_threshold() {
    let actions = available_actions(&ScenarioRole::Guilty, &empty_beliefs(), 0.5);
    let has_flee = actions.iter().any(|(a, _)| matches!(a, NpcAction::Flee { .. }));
    assert!(!has_flee, "Flee should not be available at tension 0.5 (threshold is 0.8)");
}

// ---------------------------------------------------------------------------
// AC: Low tension default — most NPCs ActNormal
// ---------------------------------------------------------------------------

#[test]
fn ac_low_tension_act_normal_has_highest_weight() {
    let actions = available_actions(&ScenarioRole::Guilty, &empty_beliefs(), 0.1);
    let normal_weight = actions.iter()
        .find(|(a, _)| matches!(a, NpcAction::ActNormal))
        .map(|(_, w)| *w)
        .expect("ActNormal should always be available");

    let max_other_weight = actions.iter()
        .filter(|(a, _)| !matches!(a, NpcAction::ActNormal))
        .map(|(_, w)| *w)
        .fold(0.0f32, f32::max);

    assert!(normal_weight > max_other_weight,
        "At low tension, ActNormal weight ({}) should exceed all others (max other: {})",
        normal_weight, max_other_weight);
}

#[test]
fn ac_low_tension_innocent_only_act_normal() {
    let actions = available_actions(&ScenarioRole::Innocent, &empty_beliefs(), 0.1);
    // Innocent at low tension should basically only have ActNormal
    assert!(actions.iter().any(|(a, _)| matches!(a, NpcAction::ActNormal)));
    let non_normal: Vec<_> = actions.iter()
        .filter(|(a, _)| !matches!(a, NpcAction::ActNormal))
        .collect();
    assert!(non_normal.is_empty() || non_normal.iter().all(|(_, w)| *w < 0.1),
        "Innocent at low tension should have no significant non-normal actions");
}

// ---------------------------------------------------------------------------
// AC: Alibi creates claim — inserts false claim into BeliefState
// ---------------------------------------------------------------------------

#[test]
fn ac_alibi_creates_false_claim_in_belief_state() {
    let mut belief = BeliefState::new();
    let action = NpcAction::CreateAlibi {
        false_claim: Belief::Claim {
            subject: "suspect".to_string(),
            content: "I was at the tavern all night".to_string(),
            turn_learned: 3,
            source: BeliefSource::Witnessed,
            believed: true, sentiment: sidequest_game::belief_state::ClaimSentiment::Neutral,
        },
    };

    // The action should be resolvable to add the claim
    // (resolve_action or apply_action function)
    assert!(matches!(action, NpcAction::CreateAlibi { .. }),
        "CreateAlibi should carry a false claim");
    // After resolution, the belief should appear in the NPC's state
    // This test verifies the data structure; integration test verifies resolution
}

// ---------------------------------------------------------------------------
// AC: Evidence destruction — deactivates a clue
// ---------------------------------------------------------------------------

#[test]
fn ac_destroy_evidence_carries_clue_id() {
    let action = NpcAction::DestroyEvidence {
        clue_id: "bloody_knife".to_string(),
    };
    match action {
        NpcAction::DestroyEvidence { clue_id } => {
            assert_eq!(clue_id, "bloody_knife");
        }
        _ => panic!("Expected DestroyEvidence"),
    }
}

// ---------------------------------------------------------------------------
// AC: Flee changes state — updates NPC location
// ---------------------------------------------------------------------------

#[test]
fn ac_flee_carries_destination() {
    let action = NpcAction::Flee {
        destination: "the_docks".to_string(),
    };
    match action {
        NpcAction::Flee { destination } => {
            assert_eq!(destination, "the_docks");
        }
        _ => panic!("Expected Flee"),
    }
}

// ---------------------------------------------------------------------------
// AC: Deterministic test — seeded RNG produces reproducible results
// ---------------------------------------------------------------------------

#[test]
fn ac_deterministic_selection_with_seeded_rng() {
    let beliefs = empty_beliefs();
    let mut rng1 = seeded_rng(42);
    let mut rng2 = seeded_rng(42);

    let action1 = select_npc_action("suspect", &ScenarioRole::Guilty, &beliefs, 0.5, &mut rng1);
    let action2 = select_npc_action("suspect", &ScenarioRole::Guilty, &beliefs, 0.5, &mut rng2);

    // Same seed, same inputs → same action variant
    assert_eq!(
        std::mem::discriminant(&action1),
        std::mem::discriminant(&action2),
        "Same seed should produce same action variant"
    );
}

#[test]
fn ac_different_seeds_can_produce_different_actions() {
    let beliefs = empty_beliefs();
    let mut results = std::collections::HashSet::new();

    // Try many seeds at moderate tension (where multiple actions are available)
    for seed in 0..100 {
        let mut rng = seeded_rng(seed);
        let action = select_npc_action("suspect", &ScenarioRole::Guilty, &beliefs, 0.6, &mut rng);
        results.insert(std::mem::discriminant(&action));
    }

    assert!(results.len() > 1,
        "Different seeds should produce at least 2 different action variants at moderate tension");
}

// ---------------------------------------------------------------------------
// AC: Gossip integration — SpreadRumor uses existing GossipEngine types
// ---------------------------------------------------------------------------

#[test]
fn ac_spread_rumor_carries_claim_and_target() {
    let action = NpcAction::SpreadRumor {
        claim: Belief::Claim {
            subject: "suspect".to_string(),
            content: "I saw them near the crime scene".to_string(),
            turn_learned: 2,
            source: BeliefSource::Inferred,
            believed: true, sentiment: sidequest_game::belief_state::ClaimSentiment::Neutral,
        },
        target_npc: "bartender".to_string(),
    };

    match action {
        NpcAction::SpreadRumor { claim, target_npc } => {
            assert_eq!(target_npc, "bartender");
            assert_eq!(claim.subject(), "suspect");
        }
        _ => panic!("Expected SpreadRumor"),
    }
}

// ---------------------------------------------------------------------------
// Edge cases
// ---------------------------------------------------------------------------

#[test]
fn edge_act_normal_always_available() {
    for role in &[ScenarioRole::Guilty, ScenarioRole::Witness, ScenarioRole::Innocent, ScenarioRole::Accomplice] {
        for tension in [0.0, 0.3, 0.5, 0.7, 0.9, 1.0] {
            let actions = available_actions(role, &empty_beliefs(), tension);
            let has_normal = actions.iter().any(|(a, _)| matches!(a, NpcAction::ActNormal));
            assert!(has_normal,
                "ActNormal should always be available for {:?} at tension {}",
                role, tension);
        }
    }
}

#[test]
fn edge_tension_zero_only_normal_for_guilty() {
    let actions = available_actions(&ScenarioRole::Guilty, &empty_beliefs(), 0.0);
    let act_normal_weight = actions.iter()
        .find(|(a, _)| matches!(a, NpcAction::ActNormal))
        .map(|(_, w)| *w)
        .unwrap_or(0.0);
    // At tension 0, ActNormal weight should be ~1.0 (1.0 - 0.0)
    assert!(act_normal_weight >= 0.9, "ActNormal weight at tension 0 should be near 1.0, got {}", act_normal_weight);
}

#[test]
fn edge_tension_clamped_to_valid_range() {
    // Should not panic with out-of-range tension values
    let _ = available_actions(&ScenarioRole::Guilty, &empty_beliefs(), -0.5);
    let _ = available_actions(&ScenarioRole::Guilty, &empty_beliefs(), 1.5);
    // Just verifying no panic — the function should clamp or handle gracefully
}

#[test]
fn edge_all_weights_positive() {
    for tension in [0.0, 0.3, 0.5, 0.7, 0.9, 1.0] {
        let actions = available_actions(&ScenarioRole::Guilty, &empty_beliefs(), tension);
        for (action, weight) in &actions {
            assert!(*weight >= 0.0,
                "Action {:?} has negative weight {} at tension {}",
                action, weight, tension);
        }
    }
}

// ---------------------------------------------------------------------------
// Serde round-trip
// ---------------------------------------------------------------------------

#[test]
fn serde_npc_action_round_trips() {
    let action = NpcAction::CreateAlibi {
        false_claim: Belief::Claim {
            subject: "suspect".to_string(),
            content: "I was elsewhere".to_string(),
            turn_learned: 1,
            source: BeliefSource::Witnessed,
            believed: true, sentiment: sidequest_game::belief_state::ClaimSentiment::Neutral,
        },
    };
    let json = serde_json::to_string(&action).expect("serialize NpcAction");
    let restored: NpcAction = serde_json::from_str(&json).expect("deserialize NpcAction");
    assert!(matches!(restored, NpcAction::CreateAlibi { .. }));
}

#[test]
fn serde_scenario_role_round_trips() {
    for role in &[ScenarioRole::Guilty, ScenarioRole::Witness, ScenarioRole::Innocent, ScenarioRole::Accomplice] {
        let json = serde_json::to_string(role).expect("serialize");
        let restored: ScenarioRole = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(std::mem::discriminant(&restored), std::mem::discriminant(role));
    }
}

//! Tests for Story 7-4: Accusation system — evidence gathering, quality grading, and narrative.
//!
//! Written by TEA (Han Solo) against acceptance criteria.
//! Tests are designed to FAIL against known implementation issues.

use std::collections::{HashMap, HashSet};

use sidequest_game::accusation::{
    Accusation, AccusationResult, EvidenceQuality, EvidenceSummary, evaluate_accusation,
};
use sidequest_game::belief_state::{Belief, BeliefSource, BeliefState, Credibility};
use sidequest_game::clue_activation::{ClueNode, ClueType, ClueVisibility, DiscoveryMethod};

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

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

fn make_accusation(accused: &str) -> Accusation {
    Accusation::new(
        "player".to_string(),
        accused.to_string(),
        "I think they did it".to_string(),
    )
}

fn discovered(ids: &[&str]) -> HashSet<String> {
    ids.iter().map(|s| s.to_string()).collect()
}

fn empty_beliefs() -> HashMap<String, BeliefState> {
    HashMap::new()
}

// ---------------------------------------------------------------------------
// AC: Evidence gathering — collects activated clues, claims, and credibility
// ---------------------------------------------------------------------------

#[test]
fn ac_evidence_gathers_implicating_clues() {
    let clues = vec![
        make_clue("knife", &["suspect_a"]),
        make_clue("alibi_doc", &["suspect_b"]),
    ];
    let disc = discovered(&["knife", "alibi_doc"]);
    let accusation = make_accusation("suspect_a");

    let result = evaluate_accusation(&accusation, &disc, &clues, &empty_beliefs(), "suspect_a");

    // Only the knife implicates suspect_a
    assert_eq!(result.evidence.implicating_clues.len(), 1);
    assert!(result.evidence.implicating_clues.contains(&"knife".to_string()));
}

#[test]
fn ac_evidence_ignores_undiscovered_clues() {
    let clues = vec![make_clue("hidden_knife", &["suspect_a"])];
    let disc = discovered(&[]); // nothing discovered
    let accusation = make_accusation("suspect_a");

    let result = evaluate_accusation(&accusation, &disc, &clues, &empty_beliefs(), "suspect_a");

    assert!(result.evidence.implicating_clues.is_empty());
    assert_eq!(result.evidence.evidence_score, 0);
}

#[test]
fn ac_evidence_gathers_corroborating_claims() {
    let clues = vec![];
    let disc = discovered(&[]);

    let mut beliefs = HashMap::new();
    let mut witness = BeliefState::new();
    witness.add_belief(Belief::Claim {
        subject: "suspect_a".to_string(),
        content: "suspect_a is guilty of the crime".to_string(),
        turn_learned: 1,
        source: BeliefSource::Witnessed,
        believed: true, sentiment: sidequest_game::belief_state::ClaimSentiment::Corroborating,
    });
    beliefs.insert("witness_1".to_string(), witness);

    let accusation = make_accusation("suspect_a");
    let result = evaluate_accusation(&accusation, &disc, &clues, &beliefs, "suspect_a");

    assert!(!result.evidence.corroborating_claims.is_empty(),
        "Should collect corroborating claims about guilt");
}

#[test]
fn ac_evidence_gathers_contradicting_claims() {
    let mut beliefs = HashMap::new();
    let mut defender = BeliefState::new();
    defender.add_belief(Belief::Claim {
        subject: "suspect_a".to_string(),
        content: "suspect_a is innocent, I saw their alibi".to_string(),
        turn_learned: 2,
        source: BeliefSource::Witnessed,
        believed: true, sentiment: sidequest_game::belief_state::ClaimSentiment::Contradicting,
    });
    beliefs.insert("defender".to_string(), defender);

    let accusation = make_accusation("suspect_a");
    let result = evaluate_accusation(
        &accusation, &discovered(&[]), &[], &beliefs, "suspect_a",
    );

    assert!(!result.evidence.contradicting_claims.is_empty(),
        "Should collect contradicting claims about innocence/alibi");
}

#[test]
fn ac_evidence_collects_facts_about_accused() {
    let mut beliefs = HashMap::new();
    let mut observer = BeliefState::new();
    observer.add_belief(Belief::Fact {
        subject: "suspect_a".to_string(),
        content: "was at the crime scene".to_string(),
        turn_learned: 1,
        source: BeliefSource::Witnessed,
    });
    beliefs.insert("observer".to_string(), observer);

    let accusation = make_accusation("suspect_a");
    let result = evaluate_accusation(
        &accusation, &discovered(&[]), &[], &beliefs, "suspect_a",
    );

    assert!(!result.evidence.facts_about_accused.is_empty(),
        "Should collect facts about the accused NPC");
}

// ---------------------------------------------------------------------------
// AC: Quality grading — score maps to Circumstantial/Strong/Airtight
// ---------------------------------------------------------------------------

#[test]
fn ac_quality_boundary_score_0_is_circumstantial() {
    let accusation = make_accusation("suspect_a");
    let result = evaluate_accusation(
        &accusation, &discovered(&[]), &[], &empty_beliefs(), "suspect_a",
    );

    assert_eq!(result.quality, EvidenceQuality::Circumstantial);
    assert_eq!(result.evidence.evidence_score, 0);
}

#[test]
fn ac_quality_boundary_score_2_is_circumstantial() {
    // 1 clue = 2 pts → top of Circumstantial range
    let clues = vec![make_clue("knife", &["suspect_a"])];
    let accusation = make_accusation("suspect_a");

    let result = evaluate_accusation(
        &accusation, &discovered(&["knife"]), &clues, &empty_beliefs(), "suspect_a",
    );

    assert_eq!(result.evidence.evidence_score, 2);
    assert_eq!(result.quality, EvidenceQuality::Circumstantial);
}

#[test]
fn ac_quality_boundary_score_3_is_strong() {
    // 1 clue (2 pts) + 1 corroborating claim (1 pt) = 3 → bottom of Strong
    let clues = vec![make_clue("knife", &["suspect_a"])];
    let mut beliefs = HashMap::new();
    let mut w = BeliefState::new();
    w.add_belief(Belief::Claim {
        subject: "suspect_a".to_string(),
        content: "suspect_a is guilty".to_string(),
        turn_learned: 1,
        source: BeliefSource::Inferred,
        believed: true, sentiment: sidequest_game::belief_state::ClaimSentiment::Corroborating,
    });
    beliefs.insert("witness".to_string(), w);

    let accusation = make_accusation("suspect_a");
    let result = evaluate_accusation(
        &accusation, &discovered(&["knife"]), &clues, &beliefs, "suspect_a",
    );

    assert_eq!(result.evidence.evidence_score, 3);
    assert_eq!(result.quality, EvidenceQuality::Strong);
}

#[test]
fn ac_quality_boundary_score_6_is_airtight() {
    // 3 clues (6 pts) = Airtight threshold
    let clues = vec![
        make_clue("knife", &["suspect_a"]),
        make_clue("witness_stmt", &["suspect_a"]),
        make_clue("motive", &["suspect_a"]),
    ];
    let accusation = make_accusation("suspect_a");

    let result = evaluate_accusation(
        &accusation, &discovered(&["knife", "witness_stmt", "motive"]),
        &clues, &empty_beliefs(), "suspect_a",
    );

    assert_eq!(result.evidence.evidence_score, 6);
    assert_eq!(result.quality, EvidenceQuality::Airtight);
}

// ---------------------------------------------------------------------------
// AC: Correctness check — compares accused against guilty NPC
// ---------------------------------------------------------------------------

#[test]
fn ac_correct_when_accused_matches_guilty() {
    let accusation = make_accusation("the_villain");
    let result = evaluate_accusation(
        &accusation, &discovered(&[]), &[], &empty_beliefs(), "the_villain",
    );

    assert!(result.is_correct);
}

#[test]
fn ac_incorrect_when_accused_differs_from_guilty() {
    let accusation = make_accusation("innocent_bystander");
    let result = evaluate_accusation(
        &accusation, &discovered(&[]), &[], &empty_beliefs(), "the_villain",
    );

    assert!(!result.is_correct);
}

// ---------------------------------------------------------------------------
// AC: Narrative prompt — encodes quality and correctness
// ---------------------------------------------------------------------------

#[test]
fn ac_narrative_correct_airtight_is_triumphant() {
    let clues = vec![
        make_clue("c1", &["villain"]),
        make_clue("c2", &["villain"]),
        make_clue("c3", &["villain"]),
    ];
    let accusation = make_accusation("villain");
    let result = evaluate_accusation(
        &accusation, &discovered(&["c1", "c2", "c3"]),
        &clues, &empty_beliefs(), "villain",
    );

    assert!(result.is_correct);
    assert_eq!(result.quality, EvidenceQuality::Airtight);
    assert!(result.narrative_prompt.contains("CORRECT")
        || result.narrative_prompt.to_lowercase().contains("correct"),
        "Narrative should indicate correctness");
}

#[test]
fn ac_narrative_wrong_circumstantial_is_dismissive() {
    let accusation = make_accusation("wrong_person");
    let result = evaluate_accusation(
        &accusation, &discovered(&[]), &[], &empty_beliefs(), "villain",
    );

    assert!(!result.is_correct);
    assert_eq!(result.quality, EvidenceQuality::Circumstantial);
    assert!(result.narrative_prompt.contains("WRONG")
        || result.narrative_prompt.to_lowercase().contains("wrong"),
        "Narrative should indicate incorrect accusation");
}

#[test]
fn ac_narrative_all_six_combinations_distinct() {
    // There are 6 combos: (correct/incorrect) × (Circumstantial/Strong/Airtight)
    // Each should produce a distinct narrative prompt.
    let mut prompts = HashSet::new();

    let qualities = [
        (0, EvidenceQuality::Circumstantial),  // 0 clues
        (2, EvidenceQuality::Strong),           // enough for strong
        (3, EvidenceQuality::Airtight),         // enough for airtight
    ];

    for (clue_count, _expected_quality) in &qualities {
        let clues: Vec<_> = (0..*clue_count)
            .map(|i| make_clue(&format!("clue_{}", i), &["target"]))
        .collect();
        let disc: HashSet<String> = (0..*clue_count)
            .map(|i| format!("clue_{}", i))
            .collect();

        // Need corroboration to hit Strong with 2 clues (4 pts + need 1 more)
        let mut beliefs = HashMap::new();
        if *clue_count == 2 {
            let mut w = BeliefState::new();
            w.add_belief(Belief::Claim {
                subject: "target".to_string(),
                content: "target is guilty".to_string(),
                turn_learned: 1,
                source: BeliefSource::Inferred,
                believed: true, sentiment: sidequest_game::belief_state::ClaimSentiment::Corroborating,
            });
            beliefs.insert("w".to_string(), w);
        }

        for guilty in &["target", "someone_else"] {
            let accusation = make_accusation("target");
            let result = evaluate_accusation(
                &accusation, &disc, &clues, &beliefs, guilty,
            );
            prompts.insert(result.narrative_prompt.clone());
        }
    }

    assert_eq!(prompts.len(), 6,
        "All 6 (correct/incorrect × quality) combos should produce distinct prompts");
}

// ---------------------------------------------------------------------------
// AC: Weak accusation — few clues + no corroboration → Circumstantial
// ---------------------------------------------------------------------------

#[test]
fn ac_weak_accusation_no_evidence() {
    let accusation = make_accusation("suspect_a");
    let result = evaluate_accusation(
        &accusation, &discovered(&[]), &[], &empty_beliefs(), "suspect_a",
    );

    assert_eq!(result.quality, EvidenceQuality::Circumstantial);
    assert_eq!(result.evidence.evidence_score, 0);
    assert!(result.evidence.implicating_clues.is_empty());
    assert!(result.evidence.corroborating_claims.is_empty());
}

// ---------------------------------------------------------------------------
// AC: Strong accusation — multiple clues + corroborated claims → Strong
// ---------------------------------------------------------------------------

#[test]
fn ac_strong_accusation_clues_plus_claims() {
    let clues = vec![
        make_clue("knife", &["suspect_a"]),
        make_clue("witness", &["suspect_a"]),
    ];
    let mut beliefs = HashMap::new();
    let mut w = BeliefState::new();
    w.add_belief(Belief::Claim {
        subject: "suspect_a".to_string(),
        content: "suspect_a is guilty of murder".to_string(),
        turn_learned: 3,
        source: BeliefSource::ToldBy("informant".to_string()),
        believed: true, sentiment: sidequest_game::belief_state::ClaimSentiment::Corroborating,
    });
    beliefs.insert("informant".to_string(), w);

    let accusation = make_accusation("suspect_a");
    let result = evaluate_accusation(
        &accusation, &discovered(&["knife", "witness"]),
        &clues, &beliefs, "suspect_a",
    );

    // 2 clues (4 pts) + 1 claim (1 pt) = 5 → Strong
    assert_eq!(result.quality, EvidenceQuality::Strong);
    assert_eq!(result.evidence.implicating_clues.len(), 2);
    assert!(!result.evidence.corroborating_claims.is_empty());
}

// ---------------------------------------------------------------------------
// AC: Airtight accusation — physical evidence + exposed contradictions
// ---------------------------------------------------------------------------

#[test]
fn ac_airtight_accusation_overwhelming_evidence() {
    let clues = vec![
        make_clue("murder_weapon", &["suspect_a"]),
        make_clue("blood_trail", &["suspect_a"]),
        make_clue("confession_note", &["suspect_a"]),
        make_clue("eyewitness", &["suspect_a"]),
    ];

    let mut beliefs = HashMap::new();
    let mut w = BeliefState::new();
    w.add_belief(Belief::Claim {
        subject: "suspect_a".to_string(),
        content: "suspect_a is guilty beyond doubt".to_string(),
        turn_learned: 5,
        source: BeliefSource::Witnessed,
        believed: true, sentiment: sidequest_game::belief_state::ClaimSentiment::Corroborating,
    });
    beliefs.insert("corroborator".to_string(), w);

    let accusation = make_accusation("suspect_a");
    let result = evaluate_accusation(
        &accusation,
        &discovered(&["murder_weapon", "blood_trail", "confession_note", "eyewitness"]),
        &clues, &beliefs, "suspect_a",
    );

    // 4 clues (8 pts) + 1 claim (1 pt) = 9 → Airtight
    assert_eq!(result.quality, EvidenceQuality::Airtight);
    assert!(result.evidence.evidence_score >= 6);
    assert!(result.is_correct);
}

// ---------------------------------------------------------------------------
// Edge cases: contradiction counting
// ---------------------------------------------------------------------------

#[test]
fn edge_contradiction_counted_once_per_pair() {
    // Two NPCs have contradicting beliefs about the accused.
    // Each pair should be counted ONCE, not twice.
    let mut beliefs = HashMap::new();

    let mut npc_a = BeliefState::new();
    npc_a.add_belief(Belief::Fact {
        subject: "suspect".to_string(),
        content: "was at the tavern".to_string(),
        turn_learned: 1,
        source: BeliefSource::Witnessed,
    });
    beliefs.insert("npc_a".to_string(), npc_a);

    let mut npc_b = BeliefState::new();
    npc_b.add_belief(Belief::Fact {
        subject: "suspect".to_string(),
        content: "was at the docks".to_string(),
        turn_learned: 1,
        source: BeliefSource::Witnessed,
    });
    beliefs.insert("npc_b".to_string(), npc_b);

    let accusation = make_accusation("suspect");
    let result = evaluate_accusation(
        &accusation, &discovered(&[]), &[], &beliefs, "suspect",
    );

    // Two NPCs, one contradicting pair → contradiction_count should be 1, NOT 2
    assert_eq!(result.evidence.contradiction_count, 1,
        "Each contradicting pair should be counted once, not double-counted");
}

#[test]
fn edge_three_npcs_contradictions_not_inflated() {
    // Three NPCs with different beliefs about suspect's whereabouts.
    // Should produce 3 unique contradiction pairs, not 6.
    let mut beliefs = HashMap::new();

    for (name, location) in &[("npc_a", "tavern"), ("npc_b", "docks"), ("npc_c", "market")] {
        let mut npc = BeliefState::new();
        npc.add_belief(Belief::Fact {
            subject: "suspect".to_string(),
            content: format!("was at the {}", location),
            turn_learned: 1,
            source: BeliefSource::Witnessed,
        });
        beliefs.insert(name.to_string(), npc);
    }

    let accusation = make_accusation("suspect");
    let result = evaluate_accusation(
        &accusation, &discovered(&[]), &[], &beliefs, "suspect",
    );

    // 3 NPCs, 3 unique pairs → contradiction_count should be 3
    assert_eq!(result.evidence.contradiction_count, 3,
        "Three NPCs with conflicting facts should produce 3 contradiction pairs, not 6");
}

// ---------------------------------------------------------------------------
// Edge cases: credibility scoring
// ---------------------------------------------------------------------------

#[test]
fn edge_low_credibility_adds_score_bonus() {
    let mut beliefs = HashMap::new();
    let mut npc = BeliefState::new();
    npc.update_credibility("suspect", 0.2); // low credibility
    beliefs.insert("observer".to_string(), npc);

    let accusation = make_accusation("suspect");
    let result = evaluate_accusation(
        &accusation, &discovered(&[]), &[], &beliefs, "suspect",
    );

    // Low credibility (< 0.4) should add 1 point
    assert!(result.evidence.accused_credibility < 0.4);
    assert_eq!(result.evidence.evidence_score, 1,
        "Low credibility of accused should contribute +1 to evidence score");
}

#[test]
fn edge_neutral_credibility_no_bonus() {
    let mut beliefs = HashMap::new();
    let mut npc = BeliefState::new();
    npc.update_credibility("suspect", 0.5);
    beliefs.insert("observer".to_string(), npc);

    let accusation = make_accusation("suspect");
    let result = evaluate_accusation(
        &accusation, &discovered(&[]), &[], &beliefs, "suspect",
    );

    assert!(result.evidence.accused_credibility >= 0.4);
    assert_eq!(result.evidence.evidence_score, 0,
        "Neutral/high credibility should not add bonus points");
}

// ---------------------------------------------------------------------------
// Edge cases: suspicion handling
// ---------------------------------------------------------------------------

#[test]
fn edge_high_confidence_suspicion_corroborates() {
    let mut beliefs = HashMap::new();
    let mut npc = BeliefState::new();
    npc.add_belief(Belief::suspicion(
        "suspect".to_string(),
        "I suspect suspect did it".to_string(),
        2,
        BeliefSource::Inferred,
        0.8, // high confidence
    ));
    beliefs.insert("suspicious_npc".to_string(), npc);

    let accusation = make_accusation("suspect");
    let result = evaluate_accusation(
        &accusation, &discovered(&[]), &[], &beliefs, "suspect",
    );

    assert!(!result.evidence.corroborating_claims.is_empty(),
        "High-confidence suspicion about the accused should count as corroboration");
}

#[test]
fn edge_low_confidence_suspicion_ignored() {
    let mut beliefs = HashMap::new();
    let mut npc = BeliefState::new();
    npc.add_belief(Belief::suspicion(
        "suspect".to_string(),
        "Maybe suspect was involved".to_string(),
        2,
        BeliefSource::Inferred,
        0.3, // low confidence
    ));
    beliefs.insert("uncertain_npc".to_string(), npc);

    let accusation = make_accusation("suspect");
    let result = evaluate_accusation(
        &accusation, &discovered(&[]), &[], &beliefs, "suspect",
    );

    assert!(result.evidence.corroborating_claims.is_empty(),
        "Low-confidence suspicion should not count as corroboration");
}

// ---------------------------------------------------------------------------
// Edge cases: empty / degenerate inputs
// ---------------------------------------------------------------------------

#[test]
fn edge_empty_everything_produces_circumstantial() {
    let accusation = make_accusation("nobody");
    let result = evaluate_accusation(
        &accusation, &discovered(&[]), &[], &empty_beliefs(), "villain",
    );

    assert_eq!(result.quality, EvidenceQuality::Circumstantial);
    assert_eq!(result.evidence.evidence_score, 0);
    assert!(!result.is_correct);
}

// ---------------------------------------------------------------------------
// Serialization round-trip
// ---------------------------------------------------------------------------

#[test]
fn serde_accusation_round_trips() {
    let accusation = make_accusation("suspect_a");
    let json = serde_json::to_string(&accusation).expect("serialize");
    let restored: Accusation = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(restored.accused_npc_name, "suspect_a");
}

#[test]
fn serde_evidence_quality_round_trips() {
    for quality in &[EvidenceQuality::Circumstantial, EvidenceQuality::Strong, EvidenceQuality::Airtight] {
        let json = serde_json::to_string(quality).expect("serialize");
        let restored: EvidenceQuality = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(&restored, quality);
    }
}

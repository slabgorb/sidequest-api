//! Accusation system — player accuses NPC, system evaluates evidence quality.
//!
//! Story 7-4: When a player accuses an NPC, the accusation system gathers
//! all available evidence (clues, corroborated claims, contradictions) and
//! evaluates the quality of the accusation. Results grade to Circumstantial,
//! Strong, or Airtight, allowing the narrator to dramatize appropriately.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::belief_state::{Belief, BeliefState};
use crate::clue_activation::ClueNode;

/// The strength of evidence supporting an accusation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvidenceQuality {
    /// Weak case: 1-2 clues, minimal corroboration.
    Circumstantial,
    /// Moderate case: 3+ clues, corroborated claims, some contradictions.
    Strong,
    /// Overwhelming case: physical evidence, exposed contradictions, confession.
    Airtight,
}

/// An accusation made by the player against an NPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Accusation {
    /// The player character making the accusation.
    pub accuser_name: String,
    /// The NPC being accused.
    pub accused_npc_name: String,
    /// The player's stated reason or theory.
    pub stated_reason: String,
}

impl Accusation {
    /// Create a new accusation.
    pub fn new(accuser_name: String, accused_npc_name: String, stated_reason: String) -> Self {
        Self {
            accuser_name,
            accused_npc_name,
            stated_reason,
        }
    }
}

/// Summary of evidence gathered for an accusation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceSummary {
    /// Clues implicating the accused.
    pub implicating_clues: Vec<String>,
    /// Facts about the accused from various sources.
    pub facts_about_accused: Vec<String>,
    /// Claims that corroborate the accusation.
    pub corroborating_claims: Vec<String>,
    /// Claims that contradict the accusation (supporting innocence).
    pub contradicting_claims: Vec<String>,
    /// Credibility score of the accused (0.0..=1.0).
    pub accused_credibility: f32,
    /// Number of NPCs with knowledge contradicting each other about the accused.
    pub contradiction_count: usize,
    /// Total evidence score (used to determine quality tier).
    pub evidence_score: u32,
}

impl EvidenceSummary {
    /// Compute evidence quality based on raw score.
    ///
    /// Scoring:
    /// - 0-2: Circumstantial
    /// - 3-5: Strong
    /// - 6+: Airtight
    pub fn quality(&self) -> EvidenceQuality {
        match self.evidence_score {
            0..=2 => EvidenceQuality::Circumstantial,
            3..=5 => EvidenceQuality::Strong,
            _ => EvidenceQuality::Airtight,
        }
    }
}

/// The result of evaluating an accusation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccusationResult {
    /// The strength of evidence backing the accusation.
    pub quality: EvidenceQuality,
    /// Whether the accusation correctly identifies the guilty party.
    pub is_correct: bool,
    /// Detailed evidence summary.
    pub evidence: EvidenceSummary,
    /// Narrative prompt for the narrator to dramatize the accusation.
    pub narrative_prompt: String,
}

/// Evaluates the quality of an accusation given scenario evidence.
///
/// Gathers all evidence against the accused (clues, corroborated claims,
/// credibility score, contradictions) and produces an EvidenceQuality rating.
pub fn evaluate_accusation(
    accusation: &Accusation,
    discovered_clues: &HashSet<String>,
    clue_nodes: &[ClueNode],
    npc_beliefs: &HashMap<String, BeliefState>,
    guilty_npc: &str,
) -> AccusationResult {
    let evidence = gather_evidence(accusation, discovered_clues, clue_nodes, npc_beliefs);

    let quality = evidence.quality();
    let is_correct = guilty_npc == accusation.accused_npc_name;

    let narrative_prompt = build_narrative_prompt(is_correct, quality);

    AccusationResult {
        quality,
        is_correct,
        evidence,
        narrative_prompt,
    }
}

/// Gather evidence against the accused from all available sources.
///
/// Returns a summary containing discovered clues implicating the accused,
/// facts and claims from NPC beliefs, contradictions, and credibility score.
fn gather_evidence(
    accusation: &Accusation,
    discovered_clues: &HashSet<String>,
    clue_nodes: &[ClueNode],
    npc_beliefs: &HashMap<String, BeliefState>,
) -> EvidenceSummary {
    let mut implicating_clues = Vec::new();
    let mut facts_about_accused = Vec::new();
    let mut corroborating_claims = Vec::new();
    let mut contradicting_claims = Vec::new();
    let mut contradiction_count = 0;

    // Gather clues implicating the accused
    for node in clue_nodes {
        if discovered_clues.contains(node.id())
            && node.implicates().contains(&accusation.accused_npc_name)
        {
            implicating_clues.push(node.id().to_string());
        }
    }

    // Gather NPC beliefs about the accused
    for (npc_name, beliefs_state) in npc_beliefs {
        let about_accused = beliefs_state.beliefs_about(&accusation.accused_npc_name);

        for belief in about_accused {
            match belief {
                Belief::Fact { content, .. } => {
                    facts_about_accused.push(format!("{}: {}", npc_name, content));
                }
                Belief::Claim {
                    content, sentiment, ..
                } => {
                    use crate::belief_state::ClaimSentiment;
                    match sentiment {
                        ClaimSentiment::Corroborating => {
                            corroborating_claims.push(format!("{}: {}", npc_name, content));
                        }
                        ClaimSentiment::Contradicting => {
                            contradicting_claims.push(format!("{}: {}", npc_name, content));
                        }
                        ClaimSentiment::Neutral => {}
                    }
                }
                Belief::Suspicion {
                    content,
                    confidence,
                    subject,
                    ..
                } => {
                    if subject == &accusation.accused_npc_name && *confidence > 0.6 {
                        corroborating_claims.push(format!("{}: {}", npc_name, content));
                    }
                }
            }
        }
    }

    // Detect contradictions about the accused — unique pairs only
    let npc_names: Vec<&String> = npc_beliefs.keys().collect();
    for i in 0..npc_names.len() {
        for j in (i + 1)..npc_names.len() {
            let beliefs_i = npc_beliefs[npc_names[i]].beliefs_about(&accusation.accused_npc_name);
            let beliefs_j = npc_beliefs[npc_names[j]].beliefs_about(&accusation.accused_npc_name);
            for belief in beliefs_i.iter() {
                for other_belief in beliefs_j.iter() {
                    if belief.content() != other_belief.content()
                        && belief.subject() == other_belief.subject()
                    {
                        contradiction_count += 1;
                    }
                }
            }
        }
    }

    // Get credibility score of the accused
    let mut accused_credibility = 0.5; // Default if no beliefs recorded
                                       // Average credibility across all NPCs' trust in the accused
    let mut total_credibility = 0.0;
    let mut count = 0;
    for (_, beliefs) in npc_beliefs {
        let cred = beliefs.credibility_of(&accusation.accused_npc_name);
        total_credibility += cred.score();
        count += 1;
    }
    if count > 0 {
        accused_credibility = total_credibility / count as f32;
    }

    // Score evidence
    let mut score = 0u32;

    // Each implicating clue is worth 2 points
    score += (implicating_clues.len() as u32) * 2;

    // Each corroborating claim is worth 1 point
    score += corroborating_claims.len() as u32;

    // Each contradiction found is worth 1 point (exposes inconsistency)
    score += contradiction_count as u32;

    // Low credibility of accused is worth 1 point
    if accused_credibility < 0.4 {
        score += 1;
    }

    EvidenceSummary {
        implicating_clues,
        facts_about_accused,
        corroborating_claims,
        contradicting_claims,
        accused_credibility,
        contradiction_count,
        evidence_score: score,
    }
}

/// Build a narrative prompt for the narrator to dramatize the accusation result.
fn build_narrative_prompt(is_correct: bool, quality: EvidenceQuality) -> String {
    match (is_correct, quality) {
        (true, EvidenceQuality::Airtight) => {
            "The accusation is CORRECT and the evidence is AIRTIGHT. \
             The accused has no escape. Dramatize a triumphant, definitive moment of truth. \
             The guilty party stands exposed and condemned."
                .to_string()
        }
        (true, EvidenceQuality::Strong) => {
            "The accusation is CORRECT and the evidence is STRONG. \
             The case is compelling though not ironclad. Dramatize a moment of justified conviction. \
             The accused's guilt is clear but questions remain about details."
                .to_string()
        }
        (true, EvidenceQuality::Circumstantial) => {
            "The accusation is CORRECT but the evidence is CIRCUMSTANTIAL. \
             You got the right person but the evidence is weak. Dramatize a lucky vindication. \
             The accused admits guilt, but it could have gone the other way."
                .to_string()
        }
        (false, EvidenceQuality::Airtight) => {
            "The accusation is WRONG and the evidence against them is AIRTIGHT. \
             You have definitively accused the innocent. Dramatize the horror of miscarriage of justice. \
             The real culprit remains at large."
                .to_string()
        }
        (false, EvidenceQuality::Strong) => {
            "The accusation is WRONG but the evidence seems STRONG. \
             You made a case that appears ironclad, but it's all circumstantial misdirection. \
             Dramatize the accused's righteous defense and the moment of doubt."
                .to_string()
        }
        (false, EvidenceQuality::Circumstantial) => {
            "The accusation is WRONG and the evidence is CIRCUMSTANTIAL. \
             Everyone doubts your theory. Dramatize mockery and dismissal. \
             The accused easily deflects; you have nothing."
                .to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clue_activation::{ClueType, ClueVisibility, DiscoveryMethod};

    fn make_test_clue(id: &str, description: &str, implicates: Vec<String>) -> ClueNode {
        let mut node = ClueNode::new(
            id.to_string(),
            description.to_string(),
            ClueType::Physical,
            DiscoveryMethod::Forensic,
            ClueVisibility::Obvious,
        );
        for suspect in implicates {
            node.add_implication(suspect);
        }
        node
    }

    fn make_test_belief_state() -> HashMap<String, BeliefState> {
        let mut map = HashMap::new();
        let mut suspect_beliefs = BeliefState::new();
        suspect_beliefs.add_belief(Belief::Fact {
            subject: "suspect".to_string(),
            content: "was at the scene".to_string(),
            turn_learned: 1,
            source: crate::belief_state::BeliefSource::Witnessed,
        });
        map.insert("suspect".to_string(), suspect_beliefs);

        let witness_beliefs = BeliefState::new();
        map.insert("witness".to_string(), witness_beliefs);

        map
    }

    fn make_belief_state_with_corroboration() -> HashMap<String, BeliefState> {
        let mut map = make_test_belief_state();
        let mut witness_beliefs = BeliefState::new();
        witness_beliefs.add_belief(Belief::Claim {
            subject: "suspect".to_string(),
            content: "suspect is guilty".to_string(),
            turn_learned: 2,
            source: crate::belief_state::BeliefSource::Inferred,
            believed: true,
            sentiment: crate::belief_state::ClaimSentiment::Corroborating,
        });
        map.insert("witness".to_string(), witness_beliefs);
        map
    }

    #[test]
    fn test_accusation_creation() {
        let accusation = Accusation::new(
            "player".to_string(),
            "suspect".to_string(),
            "They were at the scene".to_string(),
        );

        assert_eq!(accusation.accuser_name, "player");
        assert_eq!(accusation.accused_npc_name, "suspect");
        assert_eq!(accusation.stated_reason, "They were at the scene");
    }

    #[test]
    fn test_circumstantial_accusation() {
        // One clue only = 2 points → Circumstantial
        let clues = vec![make_test_clue(
            "clue1",
            "bloody_knife",
            vec!["suspect".to_string()],
        )];
        let discovered = {
            let mut set = HashSet::new();
            set.insert("clue1".to_string());
            set
        };
        let beliefs = make_test_belief_state();
        let accusation = Accusation::new(
            "player".to_string(),
            "suspect".to_string(),
            "They did it".to_string(),
        );

        let result = evaluate_accusation(&accusation, &discovered, &clues, &beliefs, "suspect");

        assert_eq!(result.quality, EvidenceQuality::Circumstantial);
        assert!(result.is_correct);
        assert!(result.evidence.evidence_score <= 2);
        assert_eq!(result.evidence.implicating_clues.len(), 1);
    }

    #[test]
    fn test_strong_accusation() {
        // Two clues + corroboration = Strong or higher
        let clues = vec![
            make_test_clue("clue1", "bloody_knife", vec!["suspect".to_string()]),
            make_test_clue("clue2", "witness_statement", vec!["suspect".to_string()]),
        ];
        let mut discovered = HashSet::new();
        discovered.insert("clue1".to_string());
        discovered.insert("clue2".to_string());

        let beliefs = make_belief_state_with_corroboration();
        let accusation = Accusation::new(
            "player".to_string(),
            "suspect".to_string(),
            "They did it".to_string(),
        );

        let result = evaluate_accusation(&accusation, &discovered, &clues, &beliefs, "suspect");

        // 2 clues × 2 + 1 claim = 5 points → Strong
        assert!(result.quality != EvidenceQuality::Circumstantial);
        assert!(result.is_correct);
        assert!(result.evidence.evidence_score >= 3);
        assert_eq!(result.evidence.implicating_clues.len(), 2);
    }

    #[test]
    fn test_airtight_accusation() {
        // Many clues and corroboration = Airtight
        let clues = vec![
            make_test_clue("clue1", "bloody_knife", vec!["suspect".to_string()]),
            make_test_clue("clue2", "witness_statement", vec!["suspect".to_string()]),
            make_test_clue("clue3", "motive", vec!["suspect".to_string()]),
            make_test_clue("clue4", "confession", vec!["suspect".to_string()]),
        ];
        let mut discovered = HashSet::new();
        for i in 1..=4 {
            discovered.insert(format!("clue{}", i));
        }

        let beliefs = make_belief_state_with_corroboration();
        let accusation = Accusation::new(
            "player".to_string(),
            "suspect".to_string(),
            "They did it".to_string(),
        );

        let result = evaluate_accusation(&accusation, &discovered, &clues, &beliefs, "suspect");

        assert_eq!(result.quality, EvidenceQuality::Airtight);
        assert!(result.is_correct);
        assert!(result.evidence.evidence_score >= 6);
    }

    #[test]
    fn test_incorrect_accusation() {
        let clues = vec![make_test_clue(
            "clue1",
            "bloody_knife",
            vec!["suspect".to_string()],
        )];
        let discovered = {
            let mut set = HashSet::new();
            set.insert("clue1".to_string());
            set
        };
        let beliefs = make_belief_state_with_corroboration();
        let accusation = Accusation::new(
            "player".to_string(),
            "innocent_person".to_string(),
            "They did it".to_string(),
        );

        let result = evaluate_accusation(&accusation, &discovered, &clues, &beliefs, "suspect");

        assert!(!result.is_correct);
        assert!(result.narrative_prompt.contains("WRONG"));
    }

    #[test]
    fn test_correct_airtight_narrative() {
        let clues = vec![
            make_test_clue("clue1", "bloody_knife", vec!["suspect".to_string()]),
            make_test_clue("clue2", "confession", vec!["suspect".to_string()]),
            make_test_clue("clue3", "witness", vec!["suspect".to_string()]),
            make_test_clue("clue4", "motive", vec!["suspect".to_string()]),
        ];
        let mut discovered = HashSet::new();
        for i in 1..=4 {
            discovered.insert(format!("clue{}", i));
        }

        let beliefs = make_belief_state_with_corroboration();
        let accusation = Accusation::new(
            "player".to_string(),
            "suspect".to_string(),
            "They did it".to_string(),
        );

        let result = evaluate_accusation(&accusation, &discovered, &clues, &beliefs, "suspect");

        assert_eq!(result.quality, EvidenceQuality::Airtight);
        assert!(result.is_correct);
        assert!(result.narrative_prompt.contains("CORRECT"));
        assert!(result.narrative_prompt.contains("AIRTIGHT"));
        assert!(result.narrative_prompt.contains("triumphant"));
    }

    #[test]
    fn test_wrong_airtight_narrative() {
        let clues = vec![
            make_test_clue("clue1", "bloody_knife", vec!["wrong_person".to_string()]),
            make_test_clue("clue2", "witness", vec!["wrong_person".to_string()]),
            make_test_clue("clue3", "motive", vec!["wrong_person".to_string()]),
            make_test_clue("clue4", "confession", vec!["wrong_person".to_string()]),
        ];
        let mut discovered = HashSet::new();
        for i in 1..=4 {
            discovered.insert(format!("clue{}", i));
        }

        let mut beliefs = make_belief_state_with_corroboration();
        // Add beliefs about wrong_person with clues against them
        let mut wrong_beliefs = BeliefState::new();
        wrong_beliefs.add_belief(Belief::Fact {
            subject: "wrong_person".to_string(),
            content: "was at the scene".to_string(),
            turn_learned: 1,
            source: crate::belief_state::BeliefSource::Witnessed,
        });
        beliefs.insert("wrong_person".to_string(), wrong_beliefs);

        let accusation = Accusation::new(
            "player".to_string(),
            "wrong_person".to_string(),
            "They did it".to_string(),
        );

        let result = evaluate_accusation(&accusation, &discovered, &clues, &beliefs, "suspect");

        assert!(!result.is_correct);
        assert_eq!(result.quality, EvidenceQuality::Airtight);
        assert!(result.narrative_prompt.contains("WRONG"));
        assert!(result.narrative_prompt.contains("AIRTIGHT"));
        assert!(result.narrative_prompt.contains("miscarriage of justice"));
    }

    #[test]
    fn test_evidence_summary_quality_tiers() {
        let summary_circumstantial = EvidenceSummary {
            implicating_clues: vec!["clue1".to_string()],
            facts_about_accused: vec![],
            corroborating_claims: vec![],
            contradicting_claims: vec![],
            accused_credibility: 0.5,
            contradiction_count: 0,
            evidence_score: 2,
        };

        let summary_strong = EvidenceSummary {
            implicating_clues: vec!["clue1".to_string(), "clue2".to_string()],
            facts_about_accused: vec![],
            corroborating_claims: vec!["claim1".to_string()],
            contradicting_claims: vec![],
            accused_credibility: 0.3,
            contradiction_count: 1,
            evidence_score: 4,
        };

        let summary_airtight = EvidenceSummary {
            implicating_clues: vec![
                "clue1".to_string(),
                "clue2".to_string(),
                "clue3".to_string(),
            ],
            facts_about_accused: vec![],
            corroborating_claims: vec!["claim1".to_string(), "claim2".to_string()],
            contradicting_claims: vec![],
            accused_credibility: 0.2,
            contradiction_count: 3,
            evidence_score: 10,
        };

        assert_eq!(
            summary_circumstantial.quality(),
            EvidenceQuality::Circumstantial
        );
        assert_eq!(summary_strong.quality(), EvidenceQuality::Strong);
        assert_eq!(summary_airtight.quality(), EvidenceQuality::Airtight);
    }
}

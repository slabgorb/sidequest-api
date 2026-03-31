//! NPC autonomous actions — scenario-driven NPC behaviors.
//!
//! Story 7-5: NPCs take strategic actions between turns based on their role
//! in the scenario, their BeliefState, and the current tension level.
//! Higher tension escalates behavior from acting normal through creating
//! alibis, destroying evidence, and potentially fleeing or confessing.

use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::belief_state::{Belief, BeliefSource, BeliefState};

/// An autonomous action an NPC can take between turns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NpcAction {
    /// Create a false alibi by inserting a fabricated claim.
    CreateAlibi {
        /// The false claim to insert into the NPC's BeliefState.
        false_claim: Belief,
    },
    /// Destroy a piece of evidence to prevent its discovery.
    DestroyEvidence {
        /// The ID of the clue to deactivate.
        clue_id: String,
    },
    /// Flee to a different location.
    Flee {
        /// Where the NPC is fleeing to.
        destination: String,
    },
    /// Confess guilt, optionally to a specific NPC.
    Confess {
        /// The NPC to confess to, or None for a public confession.
        to_npc: Option<String>,
    },
    /// Do nothing suspicious — default low-tension behavior.
    ActNormal,
    /// Spread a rumor (claim) to another NPC.
    SpreadRumor {
        /// The claim to spread.
        claim: Belief,
        /// The NPC to spread the rumor to.
        target_npc: String,
    },
}

/// An NPC's role within a scenario, determining available actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScenarioRole {
    /// The perpetrator — has access to alibi, destroy evidence, flee, confess.
    Guilty,
    /// Saw something — can spread rumors about what they observed.
    Witness,
    /// Not involved — limited to normal behavior.
    Innocent,
    /// Helping the guilty — can create alibis for them.
    Accomplice,
}

/// Select an NPC action using weighted random selection.
///
/// The action set depends on the NPC's role and tension level.
/// Higher tension increases the likelihood of desperate actions.
pub fn select_npc_action(
    _npc_id: &str,
    role: &ScenarioRole,
    belief: &BeliefState,
    tension: f32,
    rng: &mut impl Rng,
) -> NpcAction {
    let options = available_actions(role, belief, tension);
    weighted_select(&options, rng)
}

/// Determine which actions are available and their weights.
///
/// Returns a list of `(action, weight)` pairs. Weights are relative —
/// they don't need to sum to 1.0. Higher tension makes desperate
/// actions more likely while reducing the weight of ActNormal.
pub fn available_actions(
    role: &ScenarioRole,
    belief: &BeliefState,
    tension: f32,
) -> Vec<(NpcAction, f32)> {
    let tension = tension.clamp(0.0, 1.0);
    let mut actions: Vec<(NpcAction, f32)> = Vec::new();

    // ActNormal is always available, weight decreases with tension
    actions.push((NpcAction::ActNormal, (1.0 - tension).max(0.05)));

    match role {
        ScenarioRole::Guilty => {
            // Alibi: available at any tension, weight increases with tension
            actions.push((
                NpcAction::CreateAlibi {
                    false_claim: Belief::Claim {
                        subject: "self".to_string(),
                        content: "I was elsewhere".to_string(),
                        turn_learned: 0,
                        source: BeliefSource::Witnessed,
                        believed: true,
                    },
                },
                0.3 + tension * 0.4,
            ));

            // Destroy evidence: only at high tension (> 0.6)
            if tension > 0.6 {
                actions.push((
                    NpcAction::DestroyEvidence {
                        clue_id: "evidence".to_string(),
                    },
                    tension * 0.5,
                ));
            }

            // Flee and confess: only at extreme tension (> 0.8)
            if tension > 0.8 {
                actions.push((
                    NpcAction::Flee {
                        destination: "unknown".to_string(),
                    },
                    tension * 0.3,
                ));
                actions.push((
                    NpcAction::Confess { to_npc: None },
                    0.1,
                ));
            }
        }
        ScenarioRole::Witness => {
            // Witnesses can spread rumors if they have suspicions
            let has_suspicion = belief.beliefs().iter().any(|b| {
                matches!(b, Belief::Suspicion { confidence, .. } if *confidence > 0.5)
            });
            if has_suspicion {
                // Build a rumor from the strongest suspicion
                let suspicion = belief.beliefs().iter().find(|b| {
                    matches!(b, Belief::Suspicion { confidence, .. } if *confidence > 0.5)
                });
                if let Some(s) = suspicion {
                    actions.push((
                        NpcAction::SpreadRumor {
                            claim: Belief::Claim {
                                subject: s.subject().to_string(),
                                content: s.content().to_string(),
                                turn_learned: 0,
                                source: BeliefSource::Inferred,
                                believed: true,
                            },
                            target_npc: "nearby_npc".to_string(),
                        },
                        0.3 + tension * 0.2,
                    ));
                }
            }
        }
        ScenarioRole::Innocent => {
            // Innocents mostly act normal — no special actions
        }
        ScenarioRole::Accomplice => {
            // Accomplice can create alibis for the guilty party
            actions.push((
                NpcAction::CreateAlibi {
                    false_claim: Belief::Claim {
                        subject: "accomplice_target".to_string(),
                        content: "They were with me".to_string(),
                        turn_learned: 0,
                        source: BeliefSource::Witnessed,
                        believed: true,
                    },
                },
                0.3 + tension * 0.3,
            ));

            // Accomplice can also spread cover stories
            if tension > 0.5 {
                actions.push((
                    NpcAction::SpreadRumor {
                        claim: Belief::Claim {
                            subject: "accomplice_target".to_string(),
                            content: "They are innocent".to_string(),
                            turn_learned: 0,
                            source: BeliefSource::Witnessed,
                            believed: true,
                        },
                        target_npc: "nearby_npc".to_string(),
                    },
                    0.2 + tension * 0.2,
                ));
            }
        }
    }

    actions
}

/// Weighted random selection from a list of (action, weight) pairs.
fn weighted_select(options: &[(NpcAction, f32)], rng: &mut impl Rng) -> NpcAction {
    let total_weight: f32 = options.iter().map(|(_, w)| w).sum();
    if total_weight <= 0.0 {
        return NpcAction::ActNormal;
    }

    let mut roll: f32 = rng.gen::<f32>() * total_weight;
    for (action, weight) in options {
        roll -= weight;
        if roll <= 0.0 {
            return action.clone();
        }
    }

    // Fallback (shouldn't reach here with valid weights)
    options.last().map(|(a, _)| a.clone()).unwrap_or(NpcAction::ActNormal)
}

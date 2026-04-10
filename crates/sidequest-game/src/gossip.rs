//! Gossip propagation — NPCs spread claims between turns.
//!
//! Story 7-2: NPCs share what they know with neighbors, creating chains of
//! information (and misinformation). Contradictions between beliefs decay
//! the credibility of their sources.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sidequest_telemetry::{WatcherEventBuilder, WatcherEventType};

use crate::belief_state::{Belief, BeliefSource, BeliefState};

/// Orchestrates gossip propagation across NPCs using a relationship graph.
pub struct GossipEngine {
    adjacency: HashMap<String, Vec<String>>,
}

/// The decay amount applied to each source's credibility when a contradiction is found.
const CREDIBILITY_DECAY: f32 = 0.1;

impl GossipEngine {
    /// Create a gossip engine with the given NPC adjacency graph.
    /// Keys are NPC names, values are lists of neighbors they can gossip with.
    pub fn new(adjacency: HashMap<String, Vec<String>>) -> Self {
        Self { adjacency }
    }

    /// Get the neighbors of a named NPC. Returns empty slice for unknown NPCs.
    pub fn neighbors(&self, name: &str) -> &[String] {
        self.adjacency.get(name).map_or(&[], |v| v.as_slice())
    }

    /// Propagate beliefs for one turn.
    ///
    /// Each NPC shares their beliefs with adjacent neighbors. Propagated beliefs
    /// become Claims sourced as `ToldBy(gossiper)`. After propagation, contradictions
    /// are detected and credibility is decayed for sources of conflicting claims.
    pub fn propagate_turn(
        &self,
        npcs: &mut HashMap<String, BeliefState>,
        turn: u64,
    ) -> PropagationResult {
        // Phase 1: Snapshot — collect all beliefs to share before mutating
        let mut pending: Vec<(String, Belief)> = Vec::new(); // (recipient, belief)

        let npc_names: Vec<String> = npcs.keys().cloned().collect();
        for gossiper in &npc_names {
            let neighbors = self.neighbors(gossiper);
            if neighbors.is_empty() {
                continue;
            }
            let Some(state) = npcs.get(gossiper) else {
                continue;
            };
            for belief in state.beliefs() {
                for neighbor in neighbors {
                    if !npcs.contains_key(neighbor) {
                        continue;
                    }
                    // Create a Claim attributed to the gossiper
                    let propagated = Belief::Claim {
                        subject: belief.subject().to_string(),
                        content: belief.content().to_string(),
                        turn_learned: turn,
                        source: BeliefSource::ToldBy(gossiper.clone()),
                        believed: true,
                        sentiment: crate::belief_state::ClaimSentiment::Neutral,
                    };
                    pending.push((neighbor.clone(), propagated));
                }
            }
        }

        // Phase 2: Apply — add beliefs, suppressing duplicates
        let mut claims_spread: u32 = 0;
        for (recipient, belief) in pending {
            let Some(state) = npcs.get(&recipient) else {
                continue;
            };
            // Suppress if recipient already knows about this subject+content
            let dominated = state
                .beliefs_about(belief.subject())
                .iter()
                .any(|b| b.content() == belief.content());
            if dominated {
                continue;
            }
            // Safe to add — get mutable ref now
            if let Some(state) = npcs.get_mut(&recipient) {
                state.add_belief(belief);
                claims_spread += 1;
            }
        }

        // Phase 3: Detect contradictions and decay credibility
        let mut contradictions_found: u32 = 0;
        for name in &npc_names {
            if let Some(state) = npcs.get(name) {
                let contradictions = Self::detect_contradictions(state);
                contradictions_found += contradictions.len() as u32;
                if let Some(state) = npcs.get_mut(name) {
                    for c in &contradictions {
                        Self::decay_credibility(state, c);
                    }
                }
            }
        }

        // OTEL: gossip.turn_propagated — GM panel summary of gossip activity
        // for this turn. Captures the aggregate so the GM can distinguish an
        // active rumor mill from a silent scene where nothing propagated.
        WatcherEventBuilder::new("gossip", WatcherEventType::StateTransition)
            .field("action", "turn_propagated")
            .field("turn", turn)
            .field("npc_count", npc_names.len())
            .field("claims_spread", claims_spread)
            .field("contradictions_found", contradictions_found)
            .send();

        PropagationResult {
            claims_spread,
            contradictions_found,
        }
    }

    /// Detect contradictions within a single NPC's beliefs.
    ///
    /// Two beliefs contradict if they share the same subject but have different content.
    pub fn detect_contradictions(beliefs: &BeliefState) -> Vec<Contradiction> {
        let mut contradictions = Vec::new();
        let all = beliefs.beliefs();

        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                if all[i].subject() == all[j].subject() && all[i].content() != all[j].content() {
                    let source_a = Self::extract_source_name(&all[i]);
                    let source_b = Self::extract_source_name(&all[j]);
                    contradictions.push(Contradiction {
                        subject: all[i].subject().to_string(),
                        belief_a_content: all[i].content().to_string(),
                        belief_a_source: source_a,
                        belief_b_content: all[j].content().to_string(),
                        belief_b_source: source_b,
                    });
                }
            }
        }

        contradictions
    }

    /// Decay credibility for both sources named in a contradiction.
    pub fn decay_credibility(state: &mut BeliefState, contradiction: &Contradiction) {
        let cred_a = state.credibility_of(&contradiction.belief_a_source).score();
        state.update_credibility(&contradiction.belief_a_source, cred_a - CREDIBILITY_DECAY);

        let cred_b = state.credibility_of(&contradiction.belief_b_source).score();
        state.update_credibility(&contradiction.belief_b_source, cred_b - CREDIBILITY_DECAY);
    }

    /// Extract the source NPC name from a belief, or "unknown" for Witnessed/Inferred/Overheard.
    fn extract_source_name(belief: &Belief) -> String {
        match belief.source() {
            BeliefSource::ToldBy(name) => name.clone(),
            _ => "unknown".to_string(),
        }
    }
}

/// Describes a contradiction between two beliefs about the same subject.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contradiction {
    /// The subject both beliefs are about.
    pub subject: String,
    /// Content of the first belief.
    pub belief_a_content: String,
    /// Source NPC name for the first belief.
    pub belief_a_source: String,
    /// Content of the second belief.
    pub belief_b_content: String,
    /// Source NPC name for the second belief.
    pub belief_b_source: String,
}

/// Result of a gossip propagation turn.
pub struct PropagationResult {
    /// Number of claims successfully spread to new NPCs.
    pub claims_spread: u32,
    /// Number of contradictions detected across all NPCs.
    pub contradictions_found: u32,
}

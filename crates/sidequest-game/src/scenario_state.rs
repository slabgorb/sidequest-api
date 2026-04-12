//! ScenarioState — runtime state for an active scenario (Epic 7 integration).
//!
//! Story 7-9: Binds a ScenarioPack to a live game session. Owns the ClueGraph,
//! GossipEngine, discovered clues, NPC role assignments, and tension level.
//! Orchestrates between-turn processing: gossip propagation, NPC autonomous
//! actions, and clue availability evaluation.

use std::collections::{HashMap, HashSet};

use rand::SeedableRng;
use serde::{Deserialize, Serialize};

use crate::accusation::{evaluate_accusation, Accusation, AccusationResult};
use crate::belief_state::BeliefState;
use crate::clue_activation::{ClueActivation, ClueGraph};
use crate::gossip::GossipEngine;
use crate::npc::Npc;
use crate::npc_actions::{select_npc_action, NpcAction, ScenarioRole};

/// An event produced by scenario processing (gossip, NPC actions, clue discovery).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioEvent {
    /// What kind of scenario event this is.
    pub event_type: ScenarioEventType,
    /// Human-readable description for narrator context injection.
    pub description: String,
}

/// Categories of scenario events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScenarioEventType {
    /// A clue became discoverable or was discovered.
    ClueDiscovered {
        /// The clue ID that became available.
        clue_id: String,
    },
    /// An NPC took an autonomous action.
    NpcAction {
        /// The NPC who acted.
        npc_name: String,
        /// The action they took.
        action: NpcAction,
    },
    /// Gossip spread between NPCs.
    GossipSpread {
        /// Number of claims that spread this turn.
        claims_spread: u32,
        /// Number of contradictions detected.
        contradictions_found: u32,
    },
    /// A player made an accusation and it was resolved.
    AccusationResolved {
        /// The accusation result.
        result: AccusationResult,
    },
}

/// Runtime state for an active scenario bound to a game session.
///
/// Owns the scenario's clue graph, gossip engine, and tracks which clues
/// have been discovered, which roles NPCs play, and the current tension level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioState {
    /// The clue dependency graph for this scenario.
    clue_graph: ClueGraph,
    /// Which clues have been discovered so far.
    discovered_clues: HashSet<String>,
    /// NPC name → scenario role mapping.
    npc_roles: HashMap<String, ScenarioRole>,
    /// The guilty NPC's name (resolved from assignment matrix).
    guilty_npc: String,
    /// Current tension level (0.0 = calm, 1.0 = maximum pressure).
    tension: f32,
    /// Whether the scenario has been resolved (accusation made).
    resolved: bool,
    /// NPC adjacency graph for gossip propagation.
    adjacency: HashMap<String, Vec<String>>,
    /// Names of NPCs the player has questioned during this scenario.
    #[serde(default)]
    questioned_npcs: HashSet<String>,
}

impl ScenarioState {
    /// Initialize a ScenarioState from a genre pack's ScenarioPack.
    ///
    /// Converts genre-level types to game-level types:
    /// - ClueGraph nodes → game ClueNode with typed enums
    /// - AssignmentMatrix suspects → NPC role assignments (Guilty/Witness/Innocent)
    /// - ScenarioNpc adjacency → gossip propagation graph
    ///
    /// Selects a random guilty NPC from the `can_be_guilty` suspects.
    pub fn from_genre_pack(pack: &sidequest_genre::ScenarioPack) -> Self {
        use rand::seq::IndexedRandom;

        // Convert genre ClueGraph → game ClueGraph
        let game_nodes: Vec<crate::clue_activation::ClueNode> = pack
            .clue_graph
            .nodes
            .iter()
            .map(|gn| {
                let clue_type = match gn.clue_type.to_lowercase().as_str() {
                    "physical" => crate::clue_activation::ClueType::Physical,
                    "testimonial" => crate::clue_activation::ClueType::Testimonial,
                    "behavioral" => crate::clue_activation::ClueType::Behavioral,
                    "deduction" => crate::clue_activation::ClueType::Deduction,
                    _ => crate::clue_activation::ClueType::Physical,
                };
                let discovery = match gn.discovery_method.to_lowercase().as_str() {
                    "forensic" => crate::clue_activation::DiscoveryMethod::Forensic,
                    "interrogate" => crate::clue_activation::DiscoveryMethod::Interrogate,
                    "search" => crate::clue_activation::DiscoveryMethod::Search,
                    "observe" => crate::clue_activation::DiscoveryMethod::Observe,
                    _ => crate::clue_activation::DiscoveryMethod::Search,
                };
                let visibility = match gn.visibility.to_lowercase().as_str() {
                    "obvious" => crate::clue_activation::ClueVisibility::Obvious,
                    "hidden" => crate::clue_activation::ClueVisibility::Hidden,
                    "requires_skill" => crate::clue_activation::ClueVisibility::RequiresSkill,
                    _ => crate::clue_activation::ClueVisibility::Hidden,
                };
                let mut node = crate::clue_activation::ClueNode::new(
                    gn.id.clone(),
                    gn.description.clone(),
                    clue_type,
                    discovery,
                    visibility,
                );
                for req in &gn.requires {
                    node.add_requirement(req.clone());
                }
                for imp in &gn.implicates {
                    node.add_implication(imp.clone());
                }
                node.set_red_herring(gn.red_herring);
                for loc in &gn.locations {
                    node.add_location(loc.clone());
                }
                node
            })
            .collect();
        let clue_graph = ClueGraph::new(game_nodes);

        // Select guilty NPC from assignment matrix
        let guilty_candidates: Vec<&str> = pack
            .assignment_matrix
            .suspects
            .iter()
            .filter(|s| s.can_be_guilty)
            .map(|s| s.id.as_str())
            .collect();
        let mut rng = rand::rng();
        let guilty_npc = guilty_candidates
            .choose(&mut rng)
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                // Fallback: first NPC if no suspects marked can_be_guilty
                pack.npcs
                    .first()
                    .map(|n| n.id.clone())
                    .unwrap_or_else(|| "unknown".to_string())
            });

        // Build NPC role assignments
        let mut npc_roles: HashMap<String, ScenarioRole> = HashMap::new();
        for snpc in &pack.npcs {
            let role = if snpc.id == guilty_npc {
                ScenarioRole::Guilty
            } else {
                // Check if this NPC has initial suspicions → they're a witness
                if !snpc.initial_beliefs.suspicions.is_empty() {
                    ScenarioRole::Witness
                } else {
                    ScenarioRole::Innocent
                }
            };
            npc_roles.insert(snpc.name.clone(), role);
        }

        // Build adjacency graph for gossip (fully connected for now —
        // all NPCs can gossip with all others in the same scenario).
        let npc_names: Vec<String> = pack.npcs.iter().map(|n| n.name.clone()).collect();
        let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();
        for name in &npc_names {
            adjacency.insert(
                name.clone(),
                npc_names.iter().filter(|n| *n != name).cloned().collect(),
            );
        }

        Self::new(clue_graph, npc_roles, guilty_npc, adjacency)
    }

    /// Create a new scenario state from components.
    ///
    /// Called during scenario initialization when a ScenarioPack is bound
    /// to a game session.
    pub fn new(
        clue_graph: ClueGraph,
        npc_roles: HashMap<String, ScenarioRole>,
        guilty_npc: String,
        adjacency: HashMap<String, Vec<String>>,
    ) -> Self {
        Self {
            clue_graph,
            discovered_clues: HashSet::new(),
            npc_roles,
            guilty_npc,
            tension: 0.0,
            resolved: false,
            adjacency,
            questioned_npcs: HashSet::new(),
        }
    }

    /// Current tension level.
    pub fn tension(&self) -> f32 {
        self.tension
    }

    /// Set tension level (clamped to 0.0..=1.0).
    pub fn set_tension(&mut self, tension: f32) {
        self.tension = tension.clamp(0.0, 1.0);
    }

    /// Whether the scenario has been resolved.
    pub fn is_resolved(&self) -> bool {
        self.resolved
    }

    /// The guilty NPC's name.
    pub fn guilty_npc(&self) -> &str {
        &self.guilty_npc
    }

    /// The discovered clues set.
    pub fn discovered_clues(&self) -> &HashSet<String> {
        &self.discovered_clues
    }

    /// The NPC role assignments.
    pub fn npc_roles(&self) -> &HashMap<String, ScenarioRole> {
        &self.npc_roles
    }

    /// The clue graph.
    pub fn clue_graph(&self) -> &ClueGraph {
        &self.clue_graph
    }

    /// The set of NPCs the player has questioned.
    pub fn questioned_npcs(&self) -> &HashSet<String> {
        &self.questioned_npcs
    }

    /// Record that the player questioned a scenario NPC.
    pub fn record_questioned_npc(&mut self, npc_name: String) {
        self.questioned_npcs.insert(npc_name);
    }

    /// Mark a clue as discovered.
    pub fn discover_clue(&mut self, clue_id: String) {
        self.discovered_clues.insert(clue_id);
    }

    /// Process between-turn scenario logic.
    ///
    /// Runs gossip propagation, NPC autonomous actions, and clue availability
    /// checks. Returns a list of scenario events for narrator context injection.
    pub fn process_between_turns(&mut self, npcs: &mut [Npc], turn: u64) -> Vec<ScenarioEvent> {
        if self.resolved {
            return vec![];
        }

        let mut events = Vec::new();

        // Escalate tension slightly each turn
        self.tension = (self.tension + 0.05).clamp(0.0, 1.0);

        // Phase 1: Gossip propagation
        let gossip_engine = GossipEngine::new(self.adjacency.clone());
        let mut belief_map: HashMap<String, BeliefState> = npcs
            .iter()
            .map(|npc| (npc.core.name.to_string(), npc.belief_state.clone()))
            .collect();

        let gossip_result = gossip_engine.propagate_turn(&mut belief_map, turn);

        // Write updated beliefs back to NPCs
        for npc in npcs.iter_mut() {
            if let Some(updated) = belief_map.remove(&npc.core.name.to_string()) {
                npc.belief_state = updated;
            }
        }

        if gossip_result.claims_spread > 0 || gossip_result.contradictions_found > 0 {
            events.push(ScenarioEvent {
                event_type: ScenarioEventType::GossipSpread {
                    claims_spread: gossip_result.claims_spread,
                    contradictions_found: gossip_result.contradictions_found,
                },
                description: format!(
                    "Gossip spread: {} claims propagated, {} contradictions detected.",
                    gossip_result.claims_spread, gossip_result.contradictions_found
                ),
            });
        }

        // Phase 2: NPC autonomous actions
        let mut rng = rand::rngs::StdRng::seed_from_u64(turn);
        for npc in npcs.iter() {
            let npc_name = npc.core.name.to_string();
            if let Some(role) = self.npc_roles.get(&npc_name) {
                let action =
                    select_npc_action(&npc_name, role, &npc.belief_state, self.tension, &mut rng);

                // Only emit events for non-trivial actions
                if !matches!(action, NpcAction::ActNormal) {
                    events.push(ScenarioEvent {
                        event_type: ScenarioEventType::NpcAction {
                            npc_name: npc_name.clone(),
                            action: action.clone(),
                        },
                        description: format_npc_action_description(&npc_name, &action),
                    });
                }

                // Apply action effects
                match &action {
                    NpcAction::DestroyEvidence { clue_id } => {
                        // Remove the clue from discoverable set (not from graph — it existed)
                        self.discovered_clues.remove(clue_id);
                    }
                    NpcAction::Flee { destination } => {
                        // Update NPC location (handled by caller via narration)
                        tracing::info!(
                            npc = %npc_name,
                            destination = %destination,
                            "scenario.npc_fled"
                        );
                    }
                    _ => {}
                }
            }
        }

        // Phase 3: Check for newly discoverable clues
        let activation = ClueActivation::new(&self.clue_graph);
        let available = activation.discoverable_clues(&self.discovered_clues);
        for clue_id in &available {
            events.push(ScenarioEvent {
                event_type: ScenarioEventType::ClueDiscovered {
                    clue_id: clue_id.clone(),
                },
                description: format!("Clue '{}' is now discoverable.", clue_id),
            });
        }

        events
    }

    /// Handle a player accusation.
    ///
    /// Evaluates evidence quality and resolves the scenario.
    pub fn handle_accusation(&mut self, accusation: &Accusation, npcs: &[Npc]) -> AccusationResult {
        let npc_beliefs: HashMap<String, BeliefState> = npcs
            .iter()
            .map(|npc| (npc.core.name.to_string(), npc.belief_state.clone()))
            .collect();

        let result = evaluate_accusation(
            accusation,
            &self.discovered_clues,
            self.clue_graph.nodes(),
            &npc_beliefs,
            &self.guilty_npc,
        );

        self.resolved = true;
        result
    }

    /// Format scenario state as context for narrator prompt injection.
    pub fn format_narrator_context(&self, npcs: &[Npc]) -> String {
        let mut parts = Vec::new();

        parts.push(format!(
            "ACTIVE SCENARIO: Tension level {:.0}%.",
            self.tension * 100.0
        ));

        // Discovered clues summary
        if !self.discovered_clues.is_empty() {
            let clue_list: Vec<&str> = self.discovered_clues.iter().map(|s| s.as_str()).collect();
            parts.push(format!("Discovered clues: {}.", clue_list.join(", ")));
        }

        // Available clues
        let activation = ClueActivation::new(&self.clue_graph);
        let available = activation.discoverable_clues(&self.discovered_clues);
        if !available.is_empty() {
            let available_list: Vec<&str> = available.iter().map(|s| s.as_str()).collect();
            parts.push(format!(
                "Clues currently discoverable: {}.",
                available_list.join(", ")
            ));
        }

        // NPC suspicion levels (from beliefs about the guilty NPC)
        for npc in npcs {
            let npc_name = npc.core.name.to_string();
            let suspicions = npc.belief_state.beliefs_about(&self.guilty_npc);
            if !suspicions.is_empty() {
                parts.push(format!(
                    "{} has {} beliefs about {}.",
                    npc_name,
                    suspicions.len(),
                    self.guilty_npc
                ));
            }
        }

        parts.join(" ")
    }
}

/// Format a human-readable description of an NPC autonomous action.
fn format_npc_action_description(npc_name: &str, action: &NpcAction) -> String {
    match action {
        NpcAction::CreateAlibi { .. } => {
            format!("{} is fabricating an alibi.", npc_name)
        }
        NpcAction::DestroyEvidence { clue_id } => {
            format!("{} destroyed evidence: {}.", npc_name, clue_id)
        }
        NpcAction::Flee { destination } => {
            format!("{} is fleeing to {}.", npc_name, destination)
        }
        NpcAction::Confess { to_npc } => match to_npc {
            Some(target) => format!("{} is confessing to {}.", npc_name, target),
            None => format!("{} is making a public confession.", npc_name),
        },
        NpcAction::ActNormal => format!("{} is acting normally.", npc_name),
        NpcAction::SpreadRumor { target_npc, .. } => {
            format!("{} is spreading a rumor to {}.", npc_name, target_npc)
        }
    }
}

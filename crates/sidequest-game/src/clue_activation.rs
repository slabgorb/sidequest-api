//! Clue activation — semantic trigger evaluation for clue availability.
//!
//! Story 7-3: Stateless evaluator that determines which clues are currently
//! discoverable based on game state (discovered clues, NPC knowledge).
//! ClueActivation does NOT own discovered state — ScenarioState will.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::belief_state::BeliefState;

/// The nature of the evidence a clue represents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClueType {
    /// Tangible object found in the environment.
    Physical,
    /// Statement or confession from an NPC.
    Testimonial,
    /// Observed NPC action or mannerism.
    Behavioral,
    /// Logical conclusion drawn from other clues.
    Deduction,
}

/// How a clue can be discovered by the player.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiscoveryMethod {
    /// Detailed examination of physical evidence.
    Forensic,
    /// Questioning an NPC.
    Interrogate,
    /// Searching a location.
    Search,
    /// Watching or listening.
    Observe,
}

/// Whether a clue is immediately apparent or requires effort to find.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClueVisibility {
    /// Automatically noticed.
    Obvious,
    /// Must be actively sought.
    Hidden,
    /// Requires a specific skill check to detect.
    RequiresSkill,
}

/// A single clue definition within a scenario's clue graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClueNode {
    id: String,
    description: String,
    clue_type: ClueType,
    discovery_method: DiscoveryMethod,
    visibility: ClueVisibility,
    requires: Vec<String>,
    implicates: Vec<String>,
    red_herring: bool,
    locations: Vec<String>,
    requires_npc_knowledge: Option<String>,
}

impl ClueNode {
    /// Create a new clue node with the given core properties.
    pub fn new(
        id: String,
        description: String,
        clue_type: ClueType,
        discovery_method: DiscoveryMethod,
        visibility: ClueVisibility,
    ) -> Self {
        Self {
            id,
            description,
            clue_type,
            discovery_method,
            visibility,
            requires: Vec::new(),
            implicates: Vec::new(),
            red_herring: false,
            locations: Vec::new(),
            requires_npc_knowledge: None,
        }
    }

    /// The unique identifier for this clue.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The clue's description.
    pub fn description(&self) -> &str {
        &self.description
    }

    /// What kind of evidence this clue represents.
    pub fn clue_type(&self) -> &ClueType {
        &self.clue_type
    }

    /// How this clue can be discovered.
    pub fn discovery_method(&self) -> &DiscoveryMethod {
        &self.discovery_method
    }

    /// Whether the clue is obvious, hidden, or requires a skill check.
    pub fn visibility(&self) -> &ClueVisibility {
        &self.visibility
    }

    /// Clue IDs that must be discovered before this clue becomes available.
    pub fn requires(&self) -> &[String] {
        &self.requires
    }

    /// Suspect IDs that this clue points toward.
    pub fn implicates(&self) -> &[String] {
        &self.implicates
    }

    /// Whether this clue is a false lead planted to mislead.
    pub fn is_red_herring(&self) -> bool {
        self.red_herring
    }

    /// Locations where this clue can be found.
    pub fn locations(&self) -> &[String] {
        &self.locations
    }

    /// Add a prerequisite clue ID.
    pub fn add_requirement(&mut self, clue_id: String) {
        self.requires.push(clue_id);
    }

    /// Add a suspect this clue implicates.
    pub fn add_implication(&mut self, suspect_id: String) {
        self.implicates.push(suspect_id);
    }

    /// Mark this clue as a red herring (or not).
    pub fn set_red_herring(&mut self, value: bool) {
        self.red_herring = value;
    }

    /// Add a location where this clue can be found.
    pub fn add_location(&mut self, location: String) {
        self.locations.push(location);
    }

    /// Set the NPC knowledge subject required to unlock this clue.
    pub fn set_requires_npc_knowledge(&mut self, subject: String) {
        self.requires_npc_knowledge = Some(subject);
    }

    /// The NPC knowledge subject required, if any.
    pub fn requires_npc_knowledge(&self) -> Option<&str> {
        self.requires_npc_knowledge.as_deref()
    }
}

/// A directed graph of clue nodes with dependency edges.
///
/// Duplicate IDs are resolved by last-wins semantics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClueGraph {
    nodes: Vec<ClueNode>,
}

impl ClueGraph {
    /// Build a clue graph from a list of nodes.
    /// If duplicate IDs exist, the last definition wins.
    pub fn new(nodes: Vec<ClueNode>) -> Self {
        // Deduplicate by ID, keeping last occurrence
        let mut seen = HashSet::new();
        let mut deduped: Vec<ClueNode> = Vec::new();
        for node in nodes.into_iter().rev() {
            if seen.insert(node.id.clone()) {
                deduped.push(node);
            }
        }
        deduped.reverse();
        Self { nodes: deduped }
    }

    /// All nodes in the graph.
    pub fn nodes(&self) -> &[ClueNode] {
        &self.nodes
    }

    /// Look up a clue node by ID.
    pub fn get(&self, id: &str) -> Option<&ClueNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// Return all clues that implicate a given suspect.
    pub fn clues_implicating(&self, suspect: &str) -> Vec<&ClueNode> {
        self.nodes
            .iter()
            .filter(|n| n.implicates.contains(&suspect.to_string()))
            .collect()
    }

    /// Return all clues marked as red herrings.
    pub fn red_herrings(&self) -> Vec<&ClueNode> {
        self.nodes.iter().filter(|n| n.red_herring).collect()
    }
}

/// Stateless evaluator for clue discoverability.
///
/// Takes a reference to a ClueGraph and evaluates which clues are currently
/// discoverable given a set of already-discovered clue IDs. Does not own
/// any mutable state — ScenarioState tracks what has been discovered.
pub struct ClueActivation<'a> {
    graph: &'a ClueGraph,
}

impl<'a> ClueActivation<'a> {
    /// Create a new activation evaluator for the given graph.
    pub fn new(graph: &'a ClueGraph) -> Self {
        Self { graph }
    }

    /// Determine which clues are currently discoverable.
    ///
    /// A clue is discoverable if:
    /// 1. It has not already been discovered
    /// 2. All of its required clues have been discovered
    pub fn discoverable_clues(&self, discovered: &HashSet<String>) -> HashSet<String> {
        self.graph
            .nodes
            .iter()
            .filter(|node| Self::is_node_discoverable(node, discovered))
            .map(|node| node.id.clone())
            .collect()
    }

    /// Determine which clues are discoverable, factoring in NPC knowledge.
    ///
    /// Same rules as `discoverable_clues`, plus:
    /// - If a clue has `requires_npc_knowledge`, the NPC must have at least
    ///   one belief about that subject.
    pub fn discoverable_clues_with_npc(
        &self,
        discovered: &HashSet<String>,
        npc_beliefs: &BeliefState,
    ) -> HashSet<String> {
        self.graph
            .nodes
            .iter()
            .filter(|node| {
                Self::is_node_discoverable(node, discovered)
                    && Self::npc_knowledge_satisfied(node, npc_beliefs)
            })
            .map(|node| node.id.clone())
            .collect()
    }

    /// Check base discoverability: not already found + all deps met.
    fn is_node_discoverable(node: &ClueNode, discovered: &HashSet<String>) -> bool {
        !discovered.contains(&node.id)
            && node.requires.iter().all(|r| discovered.contains(r))
    }

    /// Check NPC knowledge requirement. Returns true if no requirement or if satisfied.
    fn npc_knowledge_satisfied(node: &ClueNode, npc_beliefs: &BeliefState) -> bool {
        match &node.requires_npc_knowledge {
            Some(subject) => !npc_beliefs.beliefs_about(subject).is_empty(),
            None => true,
        }
    }
}

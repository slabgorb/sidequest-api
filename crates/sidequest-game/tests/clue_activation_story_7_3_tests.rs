//! Story 7-3: Clue activation — semantic trigger evaluation for clue availability
//!
//! RED phase — these tests reference types and methods that don't exist yet.
//! They will fail to compile until Dev implements:
//!   - ClueNode — a single clue definition with id, type, visibility, requires, implicates
//!   - ClueType — Physical / Testimonial / Behavioral / Deduction
//!   - DiscoveryMethod — Forensic / Interrogate / Search / Observe
//!   - ClueVisibility — Obvious / Hidden / RequiresSkill
//!   - ClueGraph — collection of ClueNodes, dependency-aware
//!   - ClueActivation — stateless evaluator: (game state + discovered clues + graph) → discoverable
//!   - NPC knowledge integration — clue availability filtered by BeliefState
//!   - Serde round-trip for all new types
//!
//! ACs tested: Dependency resolution, visibility evaluation, implication queries,
//!             NPC knowledge filtering, red herring detection, edge cases

use std::collections::HashSet;

use sidequest_game::belief_state::{Belief, BeliefSource, BeliefState};
use sidequest_game::clue_activation::{
    ClueActivation, ClueGraph, ClueNode, ClueType, ClueVisibility, DiscoveryMethod,
};

// ============================================================================
// Test helpers
// ============================================================================

/// Build a simple ClueNode with sensible defaults.
fn node(id: &str) -> ClueNode {
    ClueNode::new(
        id.to_string(),
        format!("Description for {id}"),
        ClueType::Physical,
        DiscoveryMethod::Search,
        ClueVisibility::Obvious,
    )
}

/// Build a hidden clue requiring specific prior clues.
fn hidden_node(id: &str, requires: &[&str]) -> ClueNode {
    let mut n = ClueNode::new(
        id.to_string(),
        format!("Hidden clue {id}"),
        ClueType::Physical,
        DiscoveryMethod::Forensic,
        ClueVisibility::Hidden,
    );
    for r in requires {
        n.add_requirement(r.to_string());
    }
    n
}

/// Build a deduction clue that requires multiple prior clues.
fn deduction_node(id: &str, requires: &[&str]) -> ClueNode {
    let mut n = ClueNode::new(
        id.to_string(),
        format!("Deduction: {id}"),
        ClueType::Deduction,
        DiscoveryMethod::Observe,
        ClueVisibility::Hidden,
    );
    for r in requires {
        n.add_requirement(r.to_string());
    }
    n
}

fn discovered(ids: &[&str]) -> HashSet<String> {
    ids.iter().map(|s| s.to_string()).collect()
}

// ============================================================================
// AC: ClueNode construction and field access
// ============================================================================

#[test]
fn clue_node_construction() {
    let n = node("poison_vial");
    assert_eq!(n.id(), "poison_vial");
    assert_eq!(n.clue_type(), &ClueType::Physical);
    assert_eq!(n.discovery_method(), &DiscoveryMethod::Search);
    assert_eq!(n.visibility(), &ClueVisibility::Obvious);
    assert!(
        n.requires().is_empty(),
        "new node should have no requirements"
    );
    assert!(
        n.implicates().is_empty(),
        "new node should have no implications"
    );
    assert!(!n.is_red_herring(), "default should not be red herring");
}

#[test]
fn clue_node_with_requirements() {
    let n = hidden_node("motive_deduction", &["torn_letter", "financial_records"]);
    assert_eq!(n.requires().len(), 2);
    assert!(n.requires().iter().any(|s| s == "torn_letter"));
    assert!(n.requires().iter().any(|s| s == "financial_records"));
}

#[test]
fn clue_node_with_implications() {
    let mut n = node("bloody_knife");
    n.add_implication("suspect_varek".to_string());
    assert_eq!(n.implicates().len(), 1);
    assert!(n.implicates().iter().any(|s| s == "suspect_varek"));
}

#[test]
fn clue_node_red_herring() {
    let mut n = node("red_herring_scarf");
    n.set_red_herring(true);
    assert!(n.is_red_herring());
}

// ============================================================================
// AC: ClueType, DiscoveryMethod, ClueVisibility enums
// ============================================================================

#[test]
fn clue_type_variants_exist() {
    let types = [
        ClueType::Physical,
        ClueType::Testimonial,
        ClueType::Behavioral,
        ClueType::Deduction,
    ];
    // All four variants are distinct
    for (i, a) in types.iter().enumerate() {
        for (j, b) in types.iter().enumerate() {
            if i == j {
                assert_eq!(a, b);
            } else {
                assert_ne!(a, b);
            }
        }
    }
}

#[test]
fn discovery_method_variants_exist() {
    let methods = [
        DiscoveryMethod::Forensic,
        DiscoveryMethod::Interrogate,
        DiscoveryMethod::Search,
        DiscoveryMethod::Observe,
    ];
    for (i, a) in methods.iter().enumerate() {
        for (j, b) in methods.iter().enumerate() {
            if i == j {
                assert_eq!(a, b);
            } else {
                assert_ne!(a, b);
            }
        }
    }
}

#[test]
fn clue_visibility_variants_exist() {
    let vis = [
        ClueVisibility::Obvious,
        ClueVisibility::Hidden,
        ClueVisibility::RequiresSkill,
    ];
    for (i, a) in vis.iter().enumerate() {
        for (j, b) in vis.iter().enumerate() {
            if i == j {
                assert_eq!(a, b);
            } else {
                assert_ne!(a, b);
            }
        }
    }
}

// ============================================================================
// AC: ClueGraph construction and lookup
// ============================================================================

#[test]
fn clue_graph_empty() {
    let graph = ClueGraph::new(vec![]);
    assert!(graph.nodes().is_empty());
    assert!(graph.get("nonexistent").is_none());
}

#[test]
fn clue_graph_lookup_by_id() {
    let graph = ClueGraph::new(vec![node("vial"), node("letter")]);
    assert!(graph.get("vial").is_some());
    assert!(graph.get("letter").is_some());
    assert!(graph.get("missing").is_none());
}

#[test]
fn clue_graph_nodes_count() {
    let graph = ClueGraph::new(vec![node("a"), node("b"), node("c")]);
    assert_eq!(graph.nodes().len(), 3);
}

// ============================================================================
// AC: Dependency resolution — clue available only if requires are met
// ============================================================================

#[test]
fn obvious_clue_no_deps_always_discoverable() {
    let graph = ClueGraph::new(vec![node("obvious_evidence")]);
    let activation = ClueActivation::new(&graph);
    let discovered = discovered(&[]);

    let available = activation.discoverable_clues(&discovered);
    assert!(
        available.iter().any(|s| s == "obvious_evidence"),
        "obvious clue with no deps should always be discoverable"
    );
}

#[test]
fn hidden_clue_blocked_by_unmet_dependency() {
    let graph = ClueGraph::new(vec![
        node("torn_letter"),
        hidden_node("motive", &["torn_letter"]),
    ]);
    let activation = ClueActivation::new(&graph);

    // Nothing discovered yet — motive should be blocked
    let available = activation.discoverable_clues(&discovered(&[]));
    assert!(
        !available.iter().any(|s| s == "motive"),
        "hidden clue should be blocked when dependency not met"
    );
    // torn_letter is obvious, so it IS discoverable
    assert!(available.iter().any(|s| s == "torn_letter"));
}

#[test]
fn hidden_clue_unlocked_by_dependency() {
    let graph = ClueGraph::new(vec![
        node("torn_letter"),
        hidden_node("motive", &["torn_letter"]),
    ]);
    let activation = ClueActivation::new(&graph);

    let available = activation.discoverable_clues(&discovered(&["torn_letter"]));
    assert!(
        available.iter().any(|s| s == "motive"),
        "hidden clue should unlock once dependency is discovered"
    );
}

#[test]
fn multiple_dependencies_all_must_be_met() {
    let graph = ClueGraph::new(vec![
        node("torn_letter"),
        node("financial_records"),
        deduction_node("motive_deduction", &["torn_letter", "financial_records"]),
    ]);
    let activation = ClueActivation::new(&graph);

    // Only one dependency met
    let partial = activation.discoverable_clues(&discovered(&["torn_letter"]));
    assert!(
        !partial.iter().any(|s| s == "motive_deduction"),
        "deduction should be blocked when not all deps are met"
    );

    // Both met
    let full = activation.discoverable_clues(&discovered(&["torn_letter", "financial_records"]));
    assert!(
        full.iter().any(|s| s == "motive_deduction"),
        "deduction should unlock when all deps met"
    );
}

// ============================================================================
// AC: Transitive dependency chains
// ============================================================================

#[test]
fn transitive_dependency_chain() {
    // A → B → C (C requires B, B requires A)
    let graph = ClueGraph::new(vec![
        node("clue_a"),
        hidden_node("clue_b", &["clue_a"]),
        hidden_node("clue_c", &["clue_b"]),
    ]);
    let activation = ClueActivation::new(&graph);

    // Nothing discovered — only A is available
    let step0 = activation.discoverable_clues(&discovered(&[]));
    assert!(step0.iter().any(|s| s == "clue_a"));
    assert!(!step0.iter().any(|s| s == "clue_b"));
    assert!(!step0.iter().any(|s| s == "clue_c"));

    // A discovered — B unlocks, C still blocked
    let step1 = activation.discoverable_clues(&discovered(&["clue_a"]));
    assert!(step1.iter().any(|s| s == "clue_b"));
    assert!(!step1.iter().any(|s| s == "clue_c"));

    // A+B discovered — C unlocks
    let step2 = activation.discoverable_clues(&discovered(&["clue_a", "clue_b"]));
    assert!(step2.iter().any(|s| s == "clue_c"));
}

// ============================================================================
// AC: Already-discovered clues excluded from results
// ============================================================================

#[test]
fn already_discovered_clues_not_in_discoverable() {
    let graph = ClueGraph::new(vec![node("obvious_clue")]);
    let activation = ClueActivation::new(&graph);

    let available = activation.discoverable_clues(&discovered(&["obvious_clue"]));
    assert!(
        !available.iter().any(|s| s == "obvious_clue"),
        "already discovered clues should not appear in discoverable list"
    );
}

// ============================================================================
// AC: Implication queries — which clues implicate a suspect
// ============================================================================

#[test]
fn implicates_query_returns_matching_clues() {
    let mut knife = node("bloody_knife");
    knife.add_implication("suspect_varek".to_string());
    let mut letter = node("torn_letter");
    letter.add_implication("suspect_irina".to_string());
    let mut vial = node("poison_vial");
    vial.add_implication("suspect_varek".to_string());

    let graph = ClueGraph::new(vec![knife, letter, vial]);

    let varek_clues = graph.clues_implicating("suspect_varek");
    assert_eq!(varek_clues.len(), 2);
    let ids: HashSet<&str> = varek_clues.iter().map(|n| n.id()).collect();
    assert!(ids.contains("bloody_knife"));
    assert!(ids.contains("poison_vial"));

    let irina_clues = graph.clues_implicating("suspect_irina");
    assert_eq!(irina_clues.len(), 1);
    assert_eq!(irina_clues[0].id(), "torn_letter");
}

#[test]
fn implicates_query_no_matches_returns_empty() {
    let graph = ClueGraph::new(vec![node("random_evidence")]);
    let results = graph.clues_implicating("nobody");
    assert!(results.is_empty());
}

// ============================================================================
// AC: Red herring identification
// ============================================================================

#[test]
fn red_herrings_identified_in_graph() {
    let mut herring = node("planted_scarf");
    herring.set_red_herring(true);
    herring.add_implication("suspect_irina".to_string());

    let real = node("genuine_clue");

    let graph = ClueGraph::new(vec![herring, real]);

    let herrings = graph.red_herrings();
    assert_eq!(herrings.len(), 1);
    assert_eq!(herrings[0].id(), "planted_scarf");
}

#[test]
fn red_herring_still_discoverable() {
    // Red herrings participate in normal discovery — they're false leads, not invisible
    let mut herring = node("planted_scarf");
    herring.set_red_herring(true);

    let graph = ClueGraph::new(vec![herring]);
    let activation = ClueActivation::new(&graph);

    let available = activation.discoverable_clues(&discovered(&[]));
    assert!(
        available.iter().any(|s| s == "planted_scarf"),
        "red herrings should still be discoverable"
    );
}

// ============================================================================
// AC: NPC knowledge integration — belief-filtered availability
// ============================================================================

#[test]
fn npc_with_relevant_belief_enables_testimonial_clue() {
    let mut testimonial = ClueNode::new(
        "confession_about_poison".to_string(),
        "NPC reveals knowledge of poisoning".to_string(),
        ClueType::Testimonial,
        DiscoveryMethod::Interrogate,
        ClueVisibility::Hidden,
    );
    testimonial.set_requires_npc_knowledge("poisoning".to_string());

    let graph = ClueGraph::new(vec![testimonial]);
    let activation = ClueActivation::new(&graph);

    // NPC has relevant belief about poisoning
    let mut npc_beliefs = BeliefState::new();
    npc_beliefs.add_belief(Belief::Fact {
        subject: "poisoning".to_string(),
        content: "I saw someone add something to the drink".to_string(),
        turn_learned: 1,
        source: BeliefSource::Witnessed,
    });

    let available = activation.discoverable_clues_with_npc(&discovered(&[]), &npc_beliefs);
    assert!(
        available.iter().any(|s| s == "confession_about_poison"),
        "testimonial clue should be available when NPC has relevant knowledge"
    );
}

#[test]
fn npc_without_relevant_belief_blocks_testimonial_clue() {
    let mut testimonial = ClueNode::new(
        "confession_about_poison".to_string(),
        "NPC reveals knowledge of poisoning".to_string(),
        ClueType::Testimonial,
        DiscoveryMethod::Interrogate,
        ClueVisibility::Hidden,
    );
    testimonial.set_requires_npc_knowledge("poisoning".to_string());

    let graph = ClueGraph::new(vec![testimonial]);
    let activation = ClueActivation::new(&graph);

    // NPC has NO beliefs about poisoning
    let npc_beliefs = BeliefState::new();

    let available = activation.discoverable_clues_with_npc(&discovered(&[]), &npc_beliefs);
    assert!(
        !available.iter().any(|s| s == "confession_about_poison"),
        "testimonial clue should be blocked when NPC lacks relevant knowledge"
    );
}

#[test]
fn npc_knowledge_filter_does_not_affect_physical_clues() {
    // Physical clues don't need NPC knowledge — you find them by searching
    let graph = ClueGraph::new(vec![node("physical_evidence")]);
    let activation = ClueActivation::new(&graph);

    let empty_beliefs = BeliefState::new();
    let available = activation.discoverable_clues_with_npc(&discovered(&[]), &empty_beliefs);
    assert!(
        available.iter().any(|s| s == "physical_evidence"),
        "physical clues should not be blocked by NPC knowledge filter"
    );
}

#[test]
fn npc_suspicion_also_enables_testimonial_clue() {
    let mut testimonial = ClueNode::new(
        "suspicion_about_motive".to_string(),
        "NPC suspects financial motive".to_string(),
        ClueType::Testimonial,
        DiscoveryMethod::Interrogate,
        ClueVisibility::Hidden,
    );
    testimonial.set_requires_npc_knowledge("financial_motive".to_string());

    let graph = ClueGraph::new(vec![testimonial]);
    let activation = ClueActivation::new(&graph);

    let mut npc_beliefs = BeliefState::new();
    npc_beliefs.add_belief(Belief::suspicion(
        "financial_motive".to_string(),
        "Something seems off about the books".to_string(),
        2,
        BeliefSource::Inferred,
        0.6,
    ));

    let available = activation.discoverable_clues_with_npc(&discovered(&[]), &npc_beliefs);
    assert!(
        available.iter().any(|s| s == "suspicion_about_motive"),
        "suspicion-level belief should also enable testimonial clue"
    );
}

// ============================================================================
// AC: Combined filters — dependency + NPC knowledge
// ============================================================================

#[test]
fn both_dependency_and_npc_knowledge_required() {
    let mut testimonial = ClueNode::new(
        "deep_confession".to_string(),
        "NPC confesses only after evidence shown".to_string(),
        ClueType::Testimonial,
        DiscoveryMethod::Interrogate,
        ClueVisibility::Hidden,
    );
    testimonial.add_requirement("evidence_shown".to_string());
    testimonial.set_requires_npc_knowledge("the_crime".to_string());

    let graph = ClueGraph::new(vec![node("evidence_shown"), testimonial]);
    let activation = ClueActivation::new(&graph);

    let mut npc_beliefs = BeliefState::new();
    npc_beliefs.add_belief(Belief::Fact {
        subject: "the_crime".to_string(),
        content: "I know what happened".to_string(),
        turn_learned: 1,
        source: BeliefSource::Witnessed,
    });

    // Has NPC knowledge but missing dependency
    let no_dep = activation.discoverable_clues_with_npc(&discovered(&[]), &npc_beliefs);
    assert!(
        !no_dep.iter().any(|s| s == "deep_confession"),
        "should be blocked when dependency not met, even with NPC knowledge"
    );

    // Has dependency but no NPC knowledge
    let no_knowledge = activation
        .discoverable_clues_with_npc(&discovered(&["evidence_shown"]), &BeliefState::new());
    assert!(
        !no_knowledge.iter().any(|s| s == "deep_confession"),
        "should be blocked when NPC lacks knowledge, even with dependency met"
    );

    // Both conditions met
    let both =
        activation.discoverable_clues_with_npc(&discovered(&["evidence_shown"]), &npc_beliefs);
    assert!(
        both.iter().any(|s| s == "deep_confession"),
        "should unlock when both dependency and NPC knowledge are present"
    );
}

// ============================================================================
// AC: Serde round-trip for all new types
// ============================================================================

#[test]
fn clue_node_serde_roundtrip() {
    let mut n = hidden_node("test_clue", &["dep_a"]);
    n.add_implication("suspect_x".to_string());
    n.set_red_herring(true);

    let json = serde_json::to_string(&n).expect("serialize ClueNode");
    let deserialized: ClueNode = serde_json::from_str(&json).expect("deserialize ClueNode");

    assert_eq!(deserialized.id(), "test_clue");
    assert_eq!(deserialized.requires().len(), 1);
    assert!(deserialized.requires().iter().any(|s| s == "dep_a"));
    assert_eq!(deserialized.implicates().len(), 1);
    assert!(deserialized.is_red_herring());
    assert_eq!(deserialized.visibility(), &ClueVisibility::Hidden);
}

#[test]
fn clue_type_serde_roundtrip() {
    for ct in [
        ClueType::Physical,
        ClueType::Testimonial,
        ClueType::Behavioral,
        ClueType::Deduction,
    ] {
        let json = serde_json::to_string(&ct).expect("serialize ClueType");
        let back: ClueType = serde_json::from_str(&json).expect("deserialize ClueType");
        assert_eq!(back, ct);
    }
}

#[test]
fn clue_visibility_serde_roundtrip() {
    for vis in [
        ClueVisibility::Obvious,
        ClueVisibility::Hidden,
        ClueVisibility::RequiresSkill,
    ] {
        let json = serde_json::to_string(&vis).expect("serialize ClueVisibility");
        let back: ClueVisibility = serde_json::from_str(&json).expect("deserialize ClueVisibility");
        assert_eq!(back, vis);
    }
}

#[test]
fn discovery_method_serde_roundtrip() {
    for dm in [
        DiscoveryMethod::Forensic,
        DiscoveryMethod::Interrogate,
        DiscoveryMethod::Search,
        DiscoveryMethod::Observe,
    ] {
        let json = serde_json::to_string(&dm).expect("serialize DiscoveryMethod");
        let back: DiscoveryMethod =
            serde_json::from_str(&json).expect("deserialize DiscoveryMethod");
        assert_eq!(back, dm);
    }
}

#[test]
fn clue_graph_serde_roundtrip() {
    let mut n1 = node("a");
    n1.add_implication("suspect_1".to_string());
    let n2 = hidden_node("b", &["a"]);

    let graph = ClueGraph::new(vec![n1, n2]);
    let json = serde_json::to_string(&graph).expect("serialize ClueGraph");
    let back: ClueGraph = serde_json::from_str(&json).expect("deserialize ClueGraph");

    assert_eq!(back.nodes().len(), 2);
    assert!(back.get("a").is_some());
    assert!(back.get("b").is_some());
    assert_eq!(back.get("b").unwrap().requires().len(), 1);
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn discoverable_clues_on_empty_graph() {
    let graph = ClueGraph::new(vec![]);
    let activation = ClueActivation::new(&graph);
    let available = activation.discoverable_clues(&discovered(&[]));
    assert!(available.is_empty());
}

#[test]
fn all_clues_already_discovered() {
    let graph = ClueGraph::new(vec![node("a"), node("b")]);
    let activation = ClueActivation::new(&graph);
    let available = activation.discoverable_clues(&discovered(&["a", "b"]));
    assert!(available.is_empty(), "no clues left to discover");
}

#[test]
fn discovered_set_contains_unknown_ids_no_panic() {
    let graph = ClueGraph::new(vec![node("real_clue")]);
    let activation = ClueActivation::new(&graph);
    // Discovered set has IDs not in the graph — should not panic
    let available = activation.discoverable_clues(&discovered(&["real_clue", "ghost_clue"]));
    assert!(available.is_empty());
}

#[test]
fn dependency_on_nonexistent_clue_blocks_forever() {
    // A clue that requires a clue not in the graph can never be discovered
    let graph = ClueGraph::new(vec![hidden_node("orphan", &["does_not_exist"])]);
    let activation = ClueActivation::new(&graph);
    let available = activation.discoverable_clues(&discovered(&[]));
    assert!(
        !available.iter().any(|s| s == "orphan"),
        "clue with nonexistent dependency should never be discoverable"
    );
}

#[test]
fn duplicate_node_ids_last_wins() {
    // If the same ID appears twice, graph should handle gracefully
    let mut a1 = node("dupe");
    a1.add_implication("suspect_1".to_string());
    let mut a2 = node("dupe");
    a2.add_implication("suspect_2".to_string());

    let graph = ClueGraph::new(vec![a1, a2]);
    // Should have exactly one node with id "dupe"
    let n = graph.get("dupe").expect("should find the dupe node");
    assert_eq!(
        n.implicates().len(),
        1,
        "duplicate IDs — last definition should win"
    );
    assert!(n.implicates().iter().any(|s| s == "suspect_2"));
}

#[test]
fn behavioral_clue_type_exists() {
    // Behavioral clues — detected by observing NPC actions
    let n = ClueNode::new(
        "nervous_glance".to_string(),
        "NPC keeps glancing at the door".to_string(),
        ClueType::Behavioral,
        DiscoveryMethod::Observe,
        ClueVisibility::RequiresSkill,
    );
    assert_eq!(n.clue_type(), &ClueType::Behavioral);
    assert_eq!(n.visibility(), &ClueVisibility::RequiresSkill);
}

#[test]
fn clue_node_locations_field() {
    // Clues can be associated with specific locations
    let mut n = node("poison_vial");
    n.add_location("dining_car".to_string());
    n.add_location("kitchen".to_string());
    assert_eq!(n.locations().len(), 2);
    assert!(n.locations().iter().any(|s| s == "dining_car"));
    assert!(n.locations().iter().any(|s| s == "kitchen"));
}

#[test]
fn clue_node_default_has_no_locations() {
    let n = node("test");
    assert!(n.locations().is_empty());
}

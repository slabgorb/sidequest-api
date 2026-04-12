//! Story 7-2: Gossip propagation — NPCs spread claims between turns
//!
//! RED phase — these tests reference types and methods that don't exist yet.
//! They will fail to compile until Dev implements:
//!   - GossipEngine struct — orchestrates gossip propagation across NPCs
//!   - GossipEngine::new(adjacency) — construct with NPC relationship graph
//!   - GossipEngine::propagate_turn(npcs, turn) — spread claims for one turn
//!   - GossipEngine::detect_contradictions(beliefs) — find conflicting beliefs
//!   - Contradiction struct — describes two conflicting beliefs
//!   - GossipEngine::decay_credibility(npc, contradiction) — reduce trust
//!   - Serde round-trip for new types
//!
//! ACs tested: Claim sharing, propagation, contradiction detection,
//!             credibility decay, relationship-respecting propagation,
//!             multi-hop propagation, BeliefState integration

use std::collections::HashMap;

use sidequest_game::belief_state::{Belief, BeliefSource, BeliefState};
use sidequest_game::gossip::{Contradiction, GossipEngine};

// ============================================================================
// Test helpers
// ============================================================================

/// Build a named NPC's BeliefState with some initial beliefs.
fn npc_state_with_claim(subject: &str, content: &str, turn: u64, source_npc: &str) -> BeliefState {
    let mut state = BeliefState::new();
    state.add_belief(Belief::Claim {
        subject: subject.to_string(),
        content: content.to_string(),
        turn_learned: turn,
        source: BeliefSource::ToldBy(source_npc.to_string()),
        believed: true,
        sentiment: sidequest_game::belief_state::ClaimSentiment::Neutral,
    });
    state
}

fn npc_state_with_fact(subject: &str, content: &str, turn: u64) -> BeliefState {
    let mut state = BeliefState::new();
    state.add_belief(Belief::Fact {
        subject: subject.to_string(),
        content: content.to_string(),
        turn_learned: turn,
        source: BeliefSource::Witnessed,
    });
    state
}

/// Build an adjacency map: who can gossip with whom.
/// Adjacency is bidirectional — if A can gossip with B, B can gossip with A.
fn adjacency(pairs: &[(&str, &str)]) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for (a, b) in pairs {
        map.entry(a.to_string()).or_default().push(b.to_string());
        map.entry(b.to_string()).or_default().push(a.to_string());
    }
    map
}

// ============================================================================
// AC: GossipEngine construction and basic API
// ============================================================================

#[test]
fn gossip_engine_new_with_adjacency() {
    let adj = adjacency(&[("Alice", "Bob"), ("Bob", "Charlie")]);
    let engine = GossipEngine::new(adj);
    assert!(engine.neighbors("Alice").contains(&"Bob".to_string()));
    assert!(!engine.neighbors("Alice").contains(&"Charlie".to_string()));
}

#[test]
fn gossip_engine_neighbors_returns_empty_for_unknown() {
    let adj = adjacency(&[("Alice", "Bob")]);
    let engine = GossipEngine::new(adj);
    assert!(engine.neighbors("Unknown").is_empty());
}

// ============================================================================
// AC: NPCs share claims with neighbors during turns
// ============================================================================

#[test]
fn propagate_turn_shares_claim_to_neighbor() {
    let adj = adjacency(&[("Alice", "Bob")]);
    let engine = GossipEngine::new(adj);

    let mut npcs: HashMap<String, BeliefState> = HashMap::new();
    npcs.insert(
        "Alice".to_string(),
        npc_state_with_claim("murder", "The butler did it", 1, "Witness"),
    );
    npcs.insert("Bob".to_string(), BeliefState::new());

    let result = engine.propagate_turn(&mut npcs, 2);

    // Bob should now have a claim about "murder" from Alice
    let bob_beliefs = npcs["Bob"].beliefs_about("murder");
    assert_eq!(
        bob_beliefs.len(),
        1,
        "Bob should have received Alice's claim"
    );
    assert_eq!(bob_beliefs[0].content(), "The butler did it");
    assert!(result.claims_spread > 0);
}

#[test]
fn propagate_turn_does_not_share_with_non_neighbor() {
    let adj = adjacency(&[("Alice", "Bob")]); // Charlie not adjacent to Alice
    let engine = GossipEngine::new(adj);

    let mut npcs: HashMap<String, BeliefState> = HashMap::new();
    npcs.insert(
        "Alice".to_string(),
        npc_state_with_claim("murder", "The butler did it", 1, "Witness"),
    );
    npcs.insert("Bob".to_string(), BeliefState::new());
    npcs.insert("Charlie".to_string(), BeliefState::new());

    engine.propagate_turn(&mut npcs, 2);

    let charlie_beliefs = npcs["Charlie"].beliefs_about("murder");
    assert!(
        charlie_beliefs.is_empty(),
        "Charlie should not receive gossip — not adjacent to Alice"
    );
}

#[test]
fn propagate_turn_marks_source_as_told_by() {
    let adj = adjacency(&[("Alice", "Bob")]);
    let engine = GossipEngine::new(adj);

    let mut npcs: HashMap<String, BeliefState> = HashMap::new();
    npcs.insert(
        "Alice".to_string(),
        npc_state_with_fact("weapon", "Dagger in library", 1),
    );
    npcs.insert("Bob".to_string(), BeliefState::new());

    engine.propagate_turn(&mut npcs, 2);

    let bob_beliefs = npcs["Bob"].beliefs_about("weapon");
    assert_eq!(bob_beliefs.len(), 1);
    if let Belief::Claim { source, .. } = bob_beliefs[0] {
        assert!(
            matches!(source, BeliefSource::ToldBy(ref name) if name == "Alice"),
            "propagated belief should be sourced as ToldBy(Alice)"
        );
    } else {
        panic!("propagated belief should be a Claim, not a Fact — Bob didn't witness it");
    }
}

#[test]
fn propagate_turn_uses_current_turn_number() {
    let adj = adjacency(&[("Alice", "Bob")]);
    let engine = GossipEngine::new(adj);

    let mut npcs: HashMap<String, BeliefState> = HashMap::new();
    npcs.insert(
        "Alice".to_string(),
        npc_state_with_fact("weapon", "Dagger", 1),
    );
    npcs.insert("Bob".to_string(), BeliefState::new());

    engine.propagate_turn(&mut npcs, 5);

    let bob_beliefs = npcs["Bob"].beliefs_about("weapon");
    assert_eq!(
        bob_beliefs[0].turn_learned(),
        5,
        "propagated belief should use the propagation turn"
    );
}

// ============================================================================
// AC: Duplicate claim suppression — don't re-share what NPC already knows
// ============================================================================

#[test]
fn propagate_does_not_duplicate_existing_belief() {
    let adj = adjacency(&[("Alice", "Bob")]);
    let engine = GossipEngine::new(adj);

    let mut npcs: HashMap<String, BeliefState> = HashMap::new();
    npcs.insert(
        "Alice".to_string(),
        npc_state_with_fact("weapon", "Dagger in library", 1),
    );
    // Bob already knows about the weapon
    npcs.insert(
        "Bob".to_string(),
        npc_state_with_fact("weapon", "Dagger in library", 1),
    );

    engine.propagate_turn(&mut npcs, 2);

    let bob_beliefs = npcs["Bob"].beliefs_about("weapon");
    assert_eq!(
        bob_beliefs.len(),
        1,
        "Bob already knew about the weapon — should not get a duplicate"
    );
}

// ============================================================================
// AC: Multi-hop propagation (A → B → C across multiple turns)
// ============================================================================

#[test]
fn multi_hop_propagation_across_turns() {
    let adj = adjacency(&[("Alice", "Bob"), ("Bob", "Charlie")]);
    let engine = GossipEngine::new(adj);

    let mut npcs: HashMap<String, BeliefState> = HashMap::new();
    npcs.insert(
        "Alice".to_string(),
        npc_state_with_fact("secret", "Hidden passage exists", 1),
    );
    npcs.insert("Bob".to_string(), BeliefState::new());
    npcs.insert("Charlie".to_string(), BeliefState::new());

    // Turn 2: Alice → Bob
    engine.propagate_turn(&mut npcs, 2);
    assert_eq!(
        npcs["Bob"].beliefs_about("secret").len(),
        1,
        "Bob should know after turn 2"
    );
    assert!(
        npcs["Charlie"].beliefs_about("secret").is_empty(),
        "Charlie should not know yet — not adjacent to Alice"
    );

    // Turn 3: Bob → Charlie (Bob now has the claim and can spread it)
    engine.propagate_turn(&mut npcs, 3);
    assert_eq!(
        npcs["Charlie"].beliefs_about("secret").len(),
        1,
        "Charlie should know after turn 3 via Bob"
    );
}

#[test]
fn multi_hop_preserves_chain_attribution() {
    let adj = adjacency(&[("Alice", "Bob"), ("Bob", "Charlie")]);
    let engine = GossipEngine::new(adj);

    let mut npcs: HashMap<String, BeliefState> = HashMap::new();
    npcs.insert(
        "Alice".to_string(),
        npc_state_with_fact("secret", "Hidden passage", 1),
    );
    npcs.insert("Bob".to_string(), BeliefState::new());
    npcs.insert("Charlie".to_string(), BeliefState::new());

    engine.propagate_turn(&mut npcs, 2); // Alice → Bob
    engine.propagate_turn(&mut npcs, 3); // Bob → Charlie

    // Charlie's source should be Bob (immediate gossiper), not Alice
    let charlie_beliefs = npcs["Charlie"].beliefs_about("secret");
    if let Belief::Claim { source, .. } = charlie_beliefs[0] {
        assert!(
            matches!(source, BeliefSource::ToldBy(ref name) if name == "Bob"),
            "Charlie should attribute the gossip to Bob, not Alice"
        );
    } else {
        panic!("expected Claim variant for gossip chain");
    }
}

// ============================================================================
// AC: Contradiction detection — find conflicting beliefs on same subject
// ============================================================================

#[test]
fn detect_contradictions_finds_conflicting_claims() {
    let mut state = BeliefState::new();
    state.add_belief(Belief::Claim {
        subject: "murder".to_string(),
        content: "The butler did it".to_string(),
        turn_learned: 1,
        source: BeliefSource::ToldBy("Alice".to_string()),
        believed: true,
        sentiment: sidequest_game::belief_state::ClaimSentiment::Neutral,
    });
    state.add_belief(Belief::Claim {
        subject: "murder".to_string(),
        content: "The cook did it".to_string(),
        turn_learned: 3,
        source: BeliefSource::ToldBy("Bob".to_string()),
        believed: true,
        sentiment: sidequest_game::belief_state::ClaimSentiment::Neutral,
    });

    let contradictions = GossipEngine::detect_contradictions(&state);
    assert!(
        !contradictions.is_empty(),
        "two conflicting claims about 'murder' should produce a contradiction"
    );
}

#[test]
fn detect_contradictions_returns_empty_for_consistent_beliefs() {
    let mut state = BeliefState::new();
    state.add_belief(Belief::Fact {
        subject: "weapon".to_string(),
        content: "Dagger in library".to_string(),
        turn_learned: 1,
        source: BeliefSource::Witnessed,
    });
    // Same subject, same content — no contradiction
    state.add_belief(Belief::Claim {
        subject: "weapon".to_string(),
        content: "Dagger in library".to_string(),
        turn_learned: 3,
        source: BeliefSource::ToldBy("Bob".to_string()),
        believed: true,
        sentiment: sidequest_game::belief_state::ClaimSentiment::Neutral,
    });

    let contradictions = GossipEngine::detect_contradictions(&state);
    assert!(
        contradictions.is_empty(),
        "matching beliefs on same subject should not be contradictions"
    );
}

#[test]
fn detect_contradictions_ignores_different_subjects() {
    let mut state = BeliefState::new();
    state.add_belief(Belief::Claim {
        subject: "murder".to_string(),
        content: "The butler did it".to_string(),
        turn_learned: 1,
        source: BeliefSource::ToldBy("Alice".to_string()),
        believed: true,
        sentiment: sidequest_game::belief_state::ClaimSentiment::Neutral,
    });
    state.add_belief(Belief::Claim {
        subject: "theft".to_string(),
        content: "The cook stole it".to_string(),
        turn_learned: 2,
        source: BeliefSource::ToldBy("Bob".to_string()),
        believed: true,
        sentiment: sidequest_game::belief_state::ClaimSentiment::Neutral,
    });

    let contradictions = GossipEngine::detect_contradictions(&state);
    assert!(
        contradictions.is_empty(),
        "beliefs about different subjects are not contradictions"
    );
}

#[test]
fn contradiction_carries_both_beliefs() {
    let mut state = BeliefState::new();
    state.add_belief(Belief::Claim {
        subject: "killer".to_string(),
        content: "The butler".to_string(),
        turn_learned: 1,
        source: BeliefSource::ToldBy("Alice".to_string()),
        believed: true,
        sentiment: sidequest_game::belief_state::ClaimSentiment::Neutral,
    });
    state.add_belief(Belief::Claim {
        subject: "killer".to_string(),
        content: "The cook".to_string(),
        turn_learned: 3,
        source: BeliefSource::ToldBy("Bob".to_string()),
        believed: true,
        sentiment: sidequest_game::belief_state::ClaimSentiment::Neutral,
    });

    let contradictions = GossipEngine::detect_contradictions(&state);
    assert_eq!(contradictions.len(), 1);
    assert_eq!(contradictions[0].subject, "killer");
    assert_ne!(
        contradictions[0].belief_a_content, contradictions[0].belief_b_content,
        "contradiction should reference two different claims"
    );
}

// ============================================================================
// AC: Credibility decay — contradictions reduce trust in sources
// ============================================================================

#[test]
fn decay_credibility_reduces_source_trust() {
    let mut state = BeliefState::new();
    state.update_credibility("Alice", 0.8);
    state.update_credibility("Bob", 0.7);

    let contradiction = Contradiction {
        subject: "killer".to_string(),
        belief_a_content: "The butler".to_string(),
        belief_a_source: "Alice".to_string(),
        belief_b_content: "The cook".to_string(),
        belief_b_source: "Bob".to_string(),
    };

    GossipEngine::decay_credibility(&mut state, &contradiction);

    assert!(
        state.credibility_of("Alice").score() < 0.8,
        "Alice's credibility should decrease after contradiction"
    );
    assert!(
        state.credibility_of("Bob").score() < 0.7,
        "Bob's credibility should decrease after contradiction"
    );
}

#[test]
fn decay_credibility_does_not_go_below_zero() {
    let mut state = BeliefState::new();
    state.update_credibility("Liar", 0.05);

    let contradiction = Contradiction {
        subject: "anything".to_string(),
        belief_a_content: "X".to_string(),
        belief_a_source: "Liar".to_string(),
        belief_b_content: "Y".to_string(),
        belief_b_source: "Other".to_string(),
    };

    GossipEngine::decay_credibility(&mut state, &contradiction);

    assert!(
        state.credibility_of("Liar").score() >= 0.0,
        "credibility must not go below 0.0"
    );
}

// ============================================================================
// AC: PropagationResult — reports what happened during a turn
// ============================================================================

#[test]
fn propagation_result_reports_claims_spread() {
    let adj = adjacency(&[("Alice", "Bob"), ("Alice", "Charlie")]);
    let engine = GossipEngine::new(adj);

    let mut npcs: HashMap<String, BeliefState> = HashMap::new();
    npcs.insert(
        "Alice".to_string(),
        npc_state_with_fact("secret", "Hidden door", 1),
    );
    npcs.insert("Bob".to_string(), BeliefState::new());
    npcs.insert("Charlie".to_string(), BeliefState::new());

    let result = engine.propagate_turn(&mut npcs, 2);

    assert_eq!(
        result.claims_spread, 2,
        "Alice should spread to both Bob and Charlie"
    );
}

#[test]
fn propagation_result_reports_contradictions_found() {
    let adj = adjacency(&[("Alice", "Bob")]);
    let engine = GossipEngine::new(adj);

    let mut npcs: HashMap<String, BeliefState> = HashMap::new();
    // Alice thinks butler did it
    npcs.insert(
        "Alice".to_string(),
        npc_state_with_claim("killer", "The butler", 1, "Witness"),
    );
    // Bob thinks cook did it
    npcs.insert(
        "Bob".to_string(),
        npc_state_with_claim("killer", "The cook", 1, "Other"),
    );

    let result = engine.propagate_turn(&mut npcs, 2);

    assert!(
        result.contradictions_found > 0,
        "contradictions should be detected during propagation"
    );
}

// ============================================================================
// AC: Serde persistence — GossipEngine adjacency round-trips
// ============================================================================

#[test]
fn contradiction_serde_round_trip() {
    let c = Contradiction {
        subject: "killer".to_string(),
        belief_a_content: "The butler".to_string(),
        belief_a_source: "Alice".to_string(),
        belief_b_content: "The cook".to_string(),
        belief_b_source: "Bob".to_string(),
    };
    let json = serde_json::to_string(&c).expect("serialize contradiction");
    let restored: Contradiction = serde_json::from_str(&json).expect("deserialize contradiction");
    assert_eq!(restored.subject, "killer");
    assert_eq!(restored.belief_a_source, "Alice");
    assert_eq!(restored.belief_b_source, "Bob");
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn propagate_with_no_npcs_is_noop() {
    let adj = adjacency(&[]);
    let engine = GossipEngine::new(adj);
    let mut npcs: HashMap<String, BeliefState> = HashMap::new();

    let result = engine.propagate_turn(&mut npcs, 1);
    assert_eq!(result.claims_spread, 0);
    assert_eq!(result.contradictions_found, 0);
}

#[test]
fn propagate_with_isolated_npc_spreads_nothing() {
    let adj = adjacency(&[]); // No connections
    let engine = GossipEngine::new(adj);

    let mut npcs: HashMap<String, BeliefState> = HashMap::new();
    npcs.insert(
        "Hermit".to_string(),
        npc_state_with_fact("secret", "I know things", 1),
    );

    let result = engine.propagate_turn(&mut npcs, 2);
    assert_eq!(
        result.claims_spread, 0,
        "isolated NPC should not spread gossip"
    );
}

#[test]
fn detect_contradictions_on_empty_beliefs_returns_empty() {
    let state = BeliefState::new();
    let contradictions = GossipEngine::detect_contradictions(&state);
    assert!(contradictions.is_empty());
}

#[test]
fn bidirectional_gossip_both_npcs_share() {
    let adj = adjacency(&[("Alice", "Bob")]);
    let engine = GossipEngine::new(adj);

    let mut npcs: HashMap<String, BeliefState> = HashMap::new();
    npcs.insert(
        "Alice".to_string(),
        npc_state_with_fact("secret_a", "Alice knows A", 1),
    );
    npcs.insert(
        "Bob".to_string(),
        npc_state_with_fact("secret_b", "Bob knows B", 1),
    );

    engine.propagate_turn(&mut npcs, 2);

    assert_eq!(
        npcs["Bob"].beliefs_about("secret_a").len(),
        1,
        "Bob should receive Alice's gossip"
    );
    assert_eq!(
        npcs["Alice"].beliefs_about("secret_b").len(),
        1,
        "Alice should receive Bob's gossip"
    );
}

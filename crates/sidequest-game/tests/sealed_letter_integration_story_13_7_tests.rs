//! Integration tests for Story 13-7: Sealed letter end-to-end flow.
//!
//! Verifies the complete sealed-letter turn system with 4 players:
//!   - Barrier collects all actions simultaneously
//!   - SealedRoundContext composed from barrier output + genre initiative rules
//!   - Claim election: exactly one handler runs narrator, others retrieve shared result
//!   - Turn counter increments correctly
//!   - Second turn carries over state
//!
//! These are INTEGRATION tests — they compose multiple subsystems (barrier,
//! multiplayer, sealed_round) into realistic 4-player flows, unlike the unit
//! tests in 13-8, 13-11, and 13-14 which test components in isolation.

use std::collections::HashMap;

use sidequest_game::barrier::{TurnBarrier, TurnBarrierConfig};
use sidequest_game::multiplayer::MultiplayerSession;
use sidequest_game::sealed_round::build_sealed_round_context;
use sidequest_genre::InitiativeRule;

mod common;
use common::make_character;

// ===========================================================================
// Fixtures — 4-player session with character names and stats
// ===========================================================================

/// Create a 4-player session with named characters (not player IDs).
fn four_player_session() -> MultiplayerSession {
    let mut players = HashMap::new();
    players.insert("alice".to_string(), make_character("Alice"));
    players.insert("bob".to_string(), make_character("Bob"));
    players.insert("carol".to_string(), make_character("Carol"));
    players.insert("dave".to_string(), make_character("Dave"));
    MultiplayerSession::new(players)
}

/// Create a barrier wrapping a 4-player session (no timeout).
fn four_player_barrier() -> TurnBarrier {
    TurnBarrier::new(four_player_session(), TurnBarrierConfig::disabled())
}

/// Initiative rules for combat encounters (DEX-based).
fn combat_initiative_rules() -> HashMap<String, InitiativeRule> {
    let mut rules = HashMap::new();
    rules.insert(
        "combat".to_string(),
        InitiativeRule {
            primary_stat: "DEX".to_string(),
            description: "Reflexes and speed determine who acts first".to_string(),
        },
    );
    rules
}

/// Per-character DEX stats for initiative ordering.
fn player_dex_stats() -> HashMap<String, HashMap<String, i32>> {
    let mut stats = HashMap::new();
    stats.insert("Alice".to_string(), {
        let mut s = HashMap::new();
        s.insert("DEX".to_string(), 14);
        s
    });
    stats.insert("Bob".to_string(), {
        let mut s = HashMap::new();
        s.insert("DEX".to_string(), 16);
        s
    });
    stats.insert("Carol".to_string(), {
        let mut s = HashMap::new();
        s.insert("DEX".to_string(), 10);
        s
    });
    stats.insert("Dave".to_string(), {
        let mut s = HashMap::new();
        s.insert("DEX".to_string(), 12);
        s
    });
    stats
}

// ===========================================================================
// AC-1: 4 simultaneous clients connect to a multiplayer session
// ===========================================================================

#[test]
fn four_player_session_has_correct_player_count() {
    let session = four_player_session();
    assert_eq!(session.player_count(), 4, "Session must have 4 players");
}

#[test]
fn four_player_barrier_tracks_all_players() {
    let barrier = four_player_barrier();
    assert_eq!(barrier.player_count(), 4, "Barrier must track 4 players");
}

// ===========================================================================
// AC-2 + AC-3: All submit actions, barrier waits, then resolves
// ===========================================================================

#[tokio::test]
async fn barrier_does_not_resolve_until_all_four_submit() {
    let barrier = four_player_barrier();

    // Submit 3 of 4 — barrier should NOT be met
    barrier.submit_action("alice", "I attack the goblin");
    barrier.submit_action("bob", "I sneak around");
    barrier.submit_action("carol", "I heal the party");

    // named_actions should only have 3
    let actions = barrier.named_actions();
    assert_eq!(actions.len(), 3, "Only 3 actions submitted — barrier should not have resolved");

    // Now submit the 4th
    barrier.submit_action("dave", "I cast fireball");

    // Barrier should now be met — wait_for_turn resolves immediately
    let result = barrier.wait_for_turn().await;
    assert!(
        result.claimed_resolution,
        "First wait_for_turn after barrier met should claim resolution"
    );
    assert!(
        !result.timed_out,
        "Should not time out — all 4 submitted"
    );
    assert!(
        result.missing_players.is_empty(),
        "No missing players when all 4 submit"
    );
}

// ===========================================================================
// AC-4: ACTION_REVEAL — named_actions keyed by character name
// ===========================================================================

#[tokio::test]
async fn named_actions_keyed_by_character_name_not_player_id() {
    let barrier = four_player_barrier();

    barrier.submit_action("alice", "I attack");
    barrier.submit_action("bob", "I defend");
    barrier.submit_action("carol", "I heal");
    barrier.submit_action("dave", "I cast");

    let actions = barrier.named_actions();
    assert_eq!(actions.len(), 4, "All 4 actions present");

    // Keys must be character names, not player IDs
    assert!(actions.contains_key("Alice"), "Key should be character name 'Alice', not player id 'alice'");
    assert!(actions.contains_key("Bob"), "Key should be character name 'Bob'");
    assert!(actions.contains_key("Carol"), "Key should be character name 'Carol'");
    assert!(actions.contains_key("Dave"), "Key should be character name 'Dave'");

    // Verify action text is preserved
    assert_eq!(actions.get("Alice").map(|s| s.as_str()), Some("I attack"));
    assert_eq!(actions.get("Bob").map(|s| s.as_str()), Some("I defend"));
    assert_eq!(actions.get("Carol").map(|s| s.as_str()), Some("I heal"));
    assert_eq!(actions.get("Dave").map(|s| s.as_str()), Some("I cast"));
}

// ===========================================================================
// AC-5: Single narrator call — SealedRoundContext from barrier output
// ===========================================================================

#[tokio::test]
async fn sealed_round_context_from_barrier_actions_includes_all_four() {
    let barrier = four_player_barrier();

    barrier.submit_action("alice", "I attack the goblin");
    barrier.submit_action("bob", "I sneak around the side");
    barrier.submit_action("carol", "I cast cure light wounds");
    barrier.submit_action("dave", "I fire bolt at the goblin");

    // Resolve the barrier
    let _result = barrier.wait_for_turn().await;

    // Build SealedRoundContext from barrier's named_actions
    let actions = barrier.named_actions();
    let ctx = build_sealed_round_context(
        &actions,
        "combat",
        &combat_initiative_rules(),
        &player_dex_stats(),
    );

    assert_eq!(ctx.player_count(), 4, "SealedRoundContext must have 4 players");
    assert_eq!(ctx.encounter_type(), "combat");
    assert_eq!(ctx.action_count(), 4);
}

#[tokio::test]
async fn sealed_round_prompt_from_barrier_contains_all_actions_and_initiative() {
    let barrier = four_player_barrier();

    barrier.submit_action("alice", "I attack the goblin");
    barrier.submit_action("bob", "I sneak around the side");
    barrier.submit_action("carol", "I cast cure light wounds");
    barrier.submit_action("dave", "I fire bolt at the goblin");

    let _result = barrier.wait_for_turn().await;

    let actions = barrier.named_actions();
    let ctx = build_sealed_round_context(
        &actions,
        "combat",
        &combat_initiative_rules(),
        &player_dex_stats(),
    );
    let prompt = ctx.to_prompt_section();

    // All 4 character names + actions in the prompt
    assert!(prompt.contains("Alice"), "Prompt must contain Alice");
    assert!(prompt.contains("Bob"), "Prompt must contain Bob");
    assert!(prompt.contains("Carol"), "Prompt must contain Carol");
    assert!(prompt.contains("Dave"), "Prompt must contain Dave");

    assert!(prompt.contains("I attack the goblin"), "Prompt must contain Alice's action");
    assert!(prompt.contains("I sneak around the side"), "Prompt must contain Bob's action");
    assert!(prompt.contains("I cast cure light wounds"), "Prompt must contain Carol's action");
    assert!(prompt.contains("I fire bolt at the goblin"), "Prompt must contain Dave's action");

    // Initiative context present
    assert!(prompt.contains("DEX"), "Prompt must include DEX initiative stat");
    assert!(prompt.contains("combat"), "Prompt must include encounter type");
    assert!(prompt.contains("initiative"), "Prompt must mention initiative");

    // Per-player DEX values
    assert!(prompt.contains("16"), "Prompt must include Bob's DEX (16)");
    assert!(prompt.contains("14"), "Prompt must include Alice's DEX (14)");

    // Simultaneous framing
    assert!(
        prompt.contains("simultaneous") || prompt.contains("Simultaneous"),
        "Prompt must state actions were simultaneous"
    );
}

// ===========================================================================
// AC-5 continued: Claim election — one narrator call, shared narration
// ===========================================================================

#[tokio::test]
async fn four_player_claim_election_exactly_one_winner() {
    let barrier = four_player_barrier();

    barrier.submit_action("alice", "attack");
    barrier.submit_action("bob", "defend");
    barrier.submit_action("carol", "heal");
    barrier.submit_action("dave", "cast");

    // 4 concurrent wait_for_turn calls — exactly 1 should claim
    let b1 = barrier.clone();
    let b2 = barrier.clone();
    let b3 = barrier.clone();
    let b4 = barrier.clone();

    let (r1, r2, r3, r4) = tokio::join!(
        b1.wait_for_turn(),
        b2.wait_for_turn(),
        b3.wait_for_turn(),
        b4.wait_for_turn(),
    );

    let claimed_count = [
        r1.claimed_resolution,
        r2.claimed_resolution,
        r3.claimed_resolution,
        r4.claimed_resolution,
    ]
    .iter()
    .filter(|&&c| c)
    .count();

    assert_eq!(
        claimed_count, 1,
        "Exactly one of 4 concurrent handlers should claim resolution"
    );
}

#[tokio::test]
async fn claiming_handler_stores_narration_others_retrieve() {
    let barrier = four_player_barrier();

    barrier.submit_action("alice", "attack");
    barrier.submit_action("bob", "defend");
    barrier.submit_action("carol", "heal");
    barrier.submit_action("dave", "cast");

    let result = barrier.wait_for_turn().await;
    assert!(result.claimed_resolution, "First caller claims");

    // Claiming handler "runs narrator" and stores result
    let narration = "The battle erupts! Bob strikes first with lightning reflexes...".to_string();
    barrier.store_resolution_narration(narration.clone());

    // Non-claiming handlers retrieve the shared narration
    let retrieved = barrier.get_resolution_narration();
    assert_eq!(
        retrieved.as_deref(),
        Some("The battle erupts! Bob strikes first with lightning reflexes..."),
        "Non-claiming handlers must get the stored narration"
    );
}

// ===========================================================================
// AC-7: Turn counter increments correctly
// ===========================================================================

#[tokio::test]
async fn turn_counter_increments_after_resolution() {
    let barrier = four_player_barrier();
    assert_eq!(barrier.turn_number(), 1, "Turn starts at 1");

    barrier.submit_action("alice", "attack");
    barrier.submit_action("bob", "defend");
    barrier.submit_action("carol", "heal");
    barrier.submit_action("dave", "cast");

    let _result = barrier.wait_for_turn().await;
    assert_eq!(barrier.turn_number(), 2, "Turn should be 2 after first resolution");
}

// ===========================================================================
// Multi-turn flow: Turn 2 carries over state correctly
// ===========================================================================

#[tokio::test]
async fn two_consecutive_turns_both_resolve_correctly() {
    let barrier = four_player_barrier();

    // --- Turn 1 ---
    barrier.submit_action("alice", "I attack");
    barrier.submit_action("bob", "I defend");
    barrier.submit_action("carol", "I heal");
    barrier.submit_action("dave", "I cast");

    let r1 = barrier.wait_for_turn().await;
    assert!(r1.claimed_resolution, "Turn 1 should resolve");
    assert_eq!(barrier.turn_number(), 2, "After turn 1 → turn 2");

    // Build sealed round context for turn 1
    let actions_t1 = barrier.named_actions();
    let ctx_t1 = build_sealed_round_context(
        &actions_t1,
        "combat",
        &combat_initiative_rules(),
        &player_dex_stats(),
    );
    assert_eq!(ctx_t1.player_count(), 4, "Turn 1 should have 4 actions");

    // --- Turn 2: different actions ---
    barrier.submit_action("alice", "I search the room");
    barrier.submit_action("bob", "I loot the goblin");
    barrier.submit_action("carol", "I pray for guidance");
    barrier.submit_action("dave", "I study the runes");

    let r2 = barrier.wait_for_turn().await;
    assert!(r2.claimed_resolution, "Turn 2 should resolve");
    assert_eq!(barrier.turn_number(), 3, "After turn 2 → turn 3");

    // Turn 2 actions should be the new ones
    let actions_t2 = barrier.named_actions();
    let ctx_t2 = build_sealed_round_context(
        &actions_t2,
        "combat",
        &combat_initiative_rules(),
        &player_dex_stats(),
    );
    let prompt_t2 = ctx_t2.to_prompt_section();
    assert!(
        prompt_t2.contains("I search the room"),
        "Turn 2 prompt should have turn 2 actions, not turn 1"
    );
    assert!(
        !prompt_t2.contains("I attack"),
        "Turn 2 prompt must not contain turn 1 actions"
    );
}

// ===========================================================================
// Wiring test: barrier → named_actions → SealedRoundContext pipeline
// ===========================================================================

#[tokio::test]
async fn full_pipeline_barrier_to_sealed_round_to_prompt() {
    // This is the WIRING test: verifies the complete data path from
    // barrier submission through to narrator prompt composition.
    let barrier = four_player_barrier();

    // Step 1: Submit actions through barrier
    barrier.submit_action("alice", "I charge the dragon");
    barrier.submit_action("bob", "I aim my crossbow");
    barrier.submit_action("carol", "I invoke divine shield");
    barrier.submit_action("dave", "I teleport behind the dragon");

    // Step 2: Barrier resolves
    let result = barrier.wait_for_turn().await;
    assert!(result.claimed_resolution, "Barrier must resolve");

    // Step 3: Extract named_actions from barrier
    let actions = barrier.named_actions();
    assert_eq!(actions.len(), 4, "Barrier must produce 4 named actions");

    // Step 4: Build SealedRoundContext
    let ctx = build_sealed_round_context(
        &actions,
        "combat",
        &combat_initiative_rules(),
        &player_dex_stats(),
    );

    // Step 5: Generate prompt section
    let prompt = ctx.to_prompt_section();

    // Step 6: Verify prompt is complete and usable by narrator
    // All actions present
    assert!(prompt.contains("I charge the dragon"));
    assert!(prompt.contains("I aim my crossbow"));
    assert!(prompt.contains("I invoke divine shield"));
    assert!(prompt.contains("I teleport behind the dragon"));

    // Initiative context present
    assert!(prompt.contains("DEX"));
    assert!(prompt.contains("Reflexes and speed"));

    // Framing directives present
    assert!(
        prompt.to_lowercase().contains("third-person")
            || prompt.to_lowercase().contains("omniscient"),
        "Prompt must direct narrator to use third-person omniscient"
    );
    assert!(
        prompt.to_lowercase().contains("simultaneous"),
        "Prompt must state actions were simultaneous"
    );

    // Per-player stat values present (for initiative ordering)
    assert!(prompt.contains("Bob") && prompt.contains("16"), "Bob's DEX 16 must be in prompt");
    assert!(prompt.contains("Alice") && prompt.contains("14"), "Alice's DEX 14 must be in prompt");
}

// ===========================================================================
// Edge: partial submission + second turn
// ===========================================================================

#[tokio::test]
async fn barrier_with_partial_then_complete_still_resolves() {
    let barrier = four_player_barrier();

    // Only 2 submit initially
    barrier.submit_action("alice", "I wait");
    barrier.submit_action("bob", "I also wait");

    // Verify not resolved
    let actions = barrier.named_actions();
    assert_eq!(actions.len(), 2, "Only 2 of 4 submitted");

    // Remaining 2 submit
    barrier.submit_action("carol", "I finally act");
    barrier.submit_action("dave", "Me too");

    // Now barrier should resolve
    let result = barrier.wait_for_turn().await;
    assert!(result.claimed_resolution);
    assert_eq!(barrier.turn_number(), 2);
}

//! RED tests for Story 13-8: Fix barrier resolution — single narrator call per turn.
//!
//! Two critical bugs in the barrier turn resolution path:
//!   Bug 1: `named_actions()` read from wrong MultiplayerSession — combined action always empty
//!   Bug 2: All N handlers resume from `wait_for_turn()` and each calls narrator independently
//!
//! These tests verify:
//!   AC-1: Narrator called exactly once per barrier resolution
//!   AC-2: Narrator receives correct PARTY ACTIONS from barrier's internal session
//!   AC-3: All handlers receive the same narration via broadcast/shared result
//!   AC-4: No duplicate writes to world state
//!   AC-5: All multiplayer tests pass (existing + new)
//!
//! Key files:
//!   - sidequest-game/src/barrier.rs — TurnBarrier, resolution coordination
//!   - sidequest-game/src/multiplayer.rs — MultiplayerSession, named_actions()

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use sidequest_game::barrier::{TurnBarrier, TurnBarrierConfig};
use sidequest_game::character::Character;
use sidequest_game::creature_core::CreatureCore;
use sidequest_game::inventory::Inventory;
use sidequest_game::multiplayer::MultiplayerSession;
use sidequest_protocol::NonBlankString;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_character(name: &str) -> Character {
    Character {
        core: CreatureCore {
            name: NonBlankString::new(name).unwrap(),
            description: NonBlankString::new("A brave adventurer").unwrap(),
            personality: NonBlankString::new("Bold and curious").unwrap(),
            level: 1,
            hp: 20,
            max_hp: 20,
            ac: 12,
            xp: 0,
            inventory: Inventory::default(),
            statuses: vec![],
        },
        backstory: NonBlankString::new("Grew up on the frontier").unwrap(),
        narrative_state: String::new(),
        hooks: vec![],
        char_class: NonBlankString::new("Fighter").unwrap(),
        race: NonBlankString::new("Human").unwrap(),
        pronouns: String::new(),
        stats: HashMap::new(),
        abilities: vec![],
        known_facts: vec![],
        affinities: vec![],
        is_friendly: true,
    }
}

fn two_player_session() -> MultiplayerSession {
    let mut players = HashMap::new();
    players.insert("player-1".to_string(), make_character("Thorn"));
    players.insert("player-2".to_string(), make_character("Elara"));
    MultiplayerSession::new(players)
}

fn four_player_session() -> MultiplayerSession {
    let mut players = HashMap::new();
    players.insert("player-1".to_string(), make_character("Thorn"));
    players.insert("player-2".to_string(), make_character("Elara"));
    players.insert("player-3".to_string(), make_character("Brak"));
    players.insert("player-4".to_string(), make_character("Lyra"));
    MultiplayerSession::new(players)
}

// ---------------------------------------------------------------------------
// AC-2: TurnBarrierResult.narration contains submitted actions (Bug 1 fix)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn barrier_result_narration_contains_submitted_actions() {
    let session = two_player_session();
    let barrier = TurnBarrier::new(session, TurnBarrierConfig::new(Duration::from_secs(5)));

    barrier.submit_action("player-1", "I search the merchant's cart");
    barrier.submit_action("player-2", "I keep watch for guards");

    let result = barrier.wait_for_turn().await;

    // TurnBarrierResult.narration must contain the actual submitted actions
    assert!(!result.narration.is_empty(), "narration map should not be empty after barrier resolution");

    // Verify both players' actions are present (keyed by player_id)
    assert!(
        result.narration.contains_key("player-1"),
        "narration should contain player-1's action"
    );
    assert!(
        result.narration.contains_key("player-2"),
        "narration should contain player-2's action"
    );
}

#[tokio::test]
async fn barrier_result_narration_values_match_submitted_text() {
    let session = two_player_session();
    let barrier = TurnBarrier::new(session, TurnBarrierConfig::new(Duration::from_secs(5)));

    barrier.submit_action("player-1", "I search the merchant's cart");
    barrier.submit_action("player-2", "I keep watch for guards");

    let result = barrier.wait_for_turn().await;

    // Narration values should contain the actual action text
    let p1_narration = result.narration.get("player-1").expect("player-1 narration missing");
    let p2_narration = result.narration.get("player-2").expect("player-2 narration missing");

    assert!(
        p1_narration.contains("search") || p1_narration.contains("merchant"),
        "player-1 narration should reference their submitted action, got: {}",
        p1_narration
    );
    assert!(
        p2_narration.contains("watch") || p2_narration.contains("guard"),
        "player-2 narration should reference their submitted action, got: {}",
        p2_narration
    );
}

// ---------------------------------------------------------------------------
// AC-2: Barrier exposes named_actions from its INTERNAL session (Bug 1 root cause)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn barrier_named_actions_returns_submitted_actions() {
    // This test verifies that TurnBarrier exposes named_actions() from its
    // internal MultiplayerSession — the Bug 1 fix requires either exposing
    // this or using TurnBarrierResult.narration instead.
    let session = two_player_session();
    let barrier = TurnBarrier::new(session, TurnBarrierConfig::new(Duration::from_secs(5)));

    barrier.submit_action("player-1", "I draw my sword");
    barrier.submit_action("player-2", "I cast a shield spell");

    // The barrier must provide access to the actions submitted to its internal session.
    // Currently there is no public method for this — this test will fail until one is added,
    // OR until the handler is fixed to use TurnBarrierResult.narration instead.
    let named = barrier.named_actions();

    assert_eq!(named.len(), 2, "should have 2 named actions");
    // Actions should be keyed by character name, not player_id
    assert!(named.contains_key("Thorn"), "should contain Thorn's action");
    assert!(named.contains_key("Elara"), "should contain Elara's action");
}

// ---------------------------------------------------------------------------
// AC-1: Resolution lock — only one handler should resolve per turn (Bug 2 fix)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn barrier_try_claim_resolution_returns_true_only_once() {
    // After barrier resolves, only one handler should "claim" the resolution
    // to run the narrator. All others should receive the result via broadcast.
    // This requires a new method: try_claim_resolution() -> bool
    let session = two_player_session();
    let barrier = TurnBarrier::new(session, TurnBarrierConfig::new(Duration::from_secs(5)));

    barrier.submit_action("player-1", "I search");
    barrier.submit_action("player-2", "I wait");

    let result = barrier.wait_for_turn().await;
    assert!(!result.timed_out);

    // First claim should succeed
    let claimed = barrier.try_claim_resolution();
    assert!(claimed, "first handler should claim resolution");

    // Second claim should fail — someone else already claimed it
    let claimed_again = barrier.try_claim_resolution();
    assert!(!claimed_again, "second handler should NOT claim resolution");
}

#[tokio::test]
async fn barrier_concurrent_handlers_only_one_claims() {
    // Simulate N handlers resuming concurrently after barrier resolves.
    // Only one should successfully claim resolution.
    let session = four_player_session();
    let barrier = TurnBarrier::new(session, TurnBarrierConfig::new(Duration::from_secs(5)));

    // Submit all 4 actions
    barrier.submit_action("player-1", "I search");
    barrier.submit_action("player-2", "I wait");
    barrier.submit_action("player-3", "I hide");
    barrier.submit_action("player-4", "I scout");

    // All 4 handlers try to claim concurrently
    let claim_count = Arc::new(AtomicU32::new(0));
    let mut handles = vec![];

    for _ in 0..4 {
        let b = barrier.clone();
        let count = claim_count.clone();
        handles.push(tokio::spawn(async move {
            let _result = b.wait_for_turn().await;
            if b.try_claim_resolution() {
                count.fetch_add(1, Ordering::SeqCst);
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    assert_eq!(
        claim_count.load(Ordering::SeqCst),
        1,
        "exactly one handler should claim resolution, not {}",
        claim_count.load(Ordering::SeqCst)
    );
}

// ---------------------------------------------------------------------------
// AC-3: Resolution result stored for non-claiming handlers
// ---------------------------------------------------------------------------

#[tokio::test]
async fn barrier_stores_narration_result_for_non_claimers() {
    // After the claiming handler runs the narrator and stores the result,
    // non-claiming handlers should be able to retrieve it.
    // This requires a new method: get_resolution_narration() -> Option<String>
    let session = two_player_session();
    let barrier = TurnBarrier::new(session, TurnBarrierConfig::new(Duration::from_secs(5)));

    barrier.submit_action("player-1", "I search");
    barrier.submit_action("player-2", "I wait");

    let _result = barrier.wait_for_turn().await;

    // First handler claims and "runs narrator"
    assert!(barrier.try_claim_resolution());

    // Store the narration result (simulating what the claiming handler does)
    let narration_text = "The party searches while keeping watch. Thorn finds a hidden compartment.";
    barrier.store_resolution_narration(narration_text.to_string());

    // Non-claiming handler retrieves it
    let stored = barrier.get_resolution_narration();
    assert!(stored.is_some(), "stored narration should be available");
    assert_eq!(
        stored.unwrap(),
        narration_text,
        "non-claimer should get the same narration text"
    );
}

// ---------------------------------------------------------------------------
// AC-4: Turn counter advances exactly once per resolution
// ---------------------------------------------------------------------------

#[tokio::test]
async fn barrier_turn_counter_increments_once_per_resolution() {
    let session = two_player_session();
    let barrier = TurnBarrier::new(session, TurnBarrierConfig::new(Duration::from_secs(5)));

    let initial_turn = barrier.turn_number();

    barrier.submit_action("player-1", "I search");
    barrier.submit_action("player-2", "I wait");

    let _result = barrier.wait_for_turn().await;

    let post_turn = barrier.turn_number();
    assert_eq!(
        post_turn,
        initial_turn + 1,
        "turn should increment exactly once, from {} to {}, got {}",
        initial_turn,
        initial_turn + 1,
        post_turn
    );
}

// ---------------------------------------------------------------------------
// AC-2: Four-player barrier includes all four actions
// ---------------------------------------------------------------------------

#[tokio::test]
async fn four_player_barrier_result_contains_all_actions() {
    let session = four_player_session();
    let barrier = TurnBarrier::new(session, TurnBarrierConfig::new(Duration::from_secs(5)));

    barrier.submit_action("player-1", "I charge forward");
    barrier.submit_action("player-2", "I cast fireball");
    barrier.submit_action("player-3", "I flank left");
    barrier.submit_action("player-4", "I heal the party");

    let result = barrier.wait_for_turn().await;

    assert_eq!(
        result.narration.len(),
        4,
        "narration should have entries for all 4 players, got {}",
        result.narration.len()
    );
    assert!(!result.timed_out, "should not have timed out");
    assert!(result.missing_players.is_empty(), "no players should be missing");
}

// ---------------------------------------------------------------------------
// Timeout path: timed_out flag is true when not all submit
// ---------------------------------------------------------------------------

#[tokio::test]
async fn barrier_timeout_sets_timed_out_flag() {
    let session = two_player_session();
    // Very short timeout to trigger quickly
    let barrier = TurnBarrier::new(session, TurnBarrierConfig::new(Duration::from_millis(50)));

    // Only player-1 submits
    barrier.submit_action("player-1", "I search");

    let result = barrier.wait_for_turn().await;

    assert!(result.timed_out, "should have timed out with only 1 of 2 players");
    assert!(
        !result.missing_players.is_empty(),
        "should report missing players"
    );
    assert!(
        result.missing_players.contains(&"player-2".to_string()),
        "player-2 should be in missing list"
    );
}

#[tokio::test]
async fn barrier_timeout_fills_missing_with_hesitates() {
    let session = two_player_session();
    let barrier = TurnBarrier::new(session, TurnBarrierConfig::new(Duration::from_millis(50)));

    barrier.submit_action("player-1", "I search");

    let result = barrier.wait_for_turn().await;

    // Player-2 should have a "hesitates" action filled in
    let p2_narration = result.narration.get("player-2");
    assert!(
        p2_narration.is_some(),
        "timed-out player should still have a narration entry"
    );
    assert!(
        p2_narration.unwrap().contains("hesitate"),
        "timed-out player's narration should contain 'hesitate', got: {}",
        p2_narration.unwrap()
    );
}

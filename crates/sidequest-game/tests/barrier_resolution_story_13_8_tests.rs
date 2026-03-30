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
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU32, Ordering};
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
    // The claim election happens atomically inside wait_for_turn(),
    // so the first task to resolve() wins. After resolution completes,
    // the returned TurnBarrierResult contains claimed_resolution field.
    let session = two_player_session();
    let barrier = TurnBarrier::new(session, TurnBarrierConfig::new(Duration::from_secs(5)));

    // First turn: both players submit
    barrier.submit_action("player-1", "I search");
    barrier.submit_action("player-2", "I wait");

    let result = barrier.wait_for_turn().await;
    assert!(!result.timed_out, "barrier should resolve without timeout");
    assert!(result.claimed_resolution, "first task should claim resolution");

    // Second turn: verify that claim state is reset for the next turn
    // Submit new actions
    barrier.submit_action("player-1", "I advance");
    barrier.submit_action("player-2", "I follow");

    let result2 = barrier.wait_for_turn().await;
    assert!(!result2.timed_out, "second barrier should resolve without timeout");
    // This task should also claim for the second turn (since it's the only one calling wait_for_turn)
    assert!(result2.claimed_resolution, "first task of second turn should also claim resolution");
}

#[tokio::test]
async fn barrier_concurrent_handlers_only_one_claims() {
    // Simulate N handlers resuming concurrently after barrier resolves.
    // Only one should successfully claim resolution.
    // To ensure true concurrency, spawn the tasks first (which enter wait_for_turn)
    // and THEN submit the actions, causing all to wake up at roughly the same time.
    let session = four_player_session();
    let barrier = TurnBarrier::new(session, TurnBarrierConfig::new(Duration::from_secs(30)));

    // Spawn all 4 handlers FIRST (they'll wait for barrier)
    let claim_count = Arc::new(AtomicU32::new(0));
    let mut handles = vec![];

    for id in 0..4 {
        let b = barrier.clone();
        let count = claim_count.clone();
        handles.push(tokio::spawn(async move {
            let result = b.wait_for_turn().await;
            eprintln!("Task {} - claimed_resolution: {}", id, result.claimed_resolution);
            if result.claimed_resolution {
                count.fetch_add(1, Ordering::SeqCst);
            }
        }));
    }

    // Give tasks time to enter wait_for_turn() and block on the select
    tokio::time::sleep(Duration::from_millis(10)).await;

    // NOW submit all 4 actions concurrently, waking up all handlers at once
    barrier.submit_action("player-1", "I search");
    barrier.submit_action("player-2", "I wait");
    barrier.submit_action("player-3", "I hide");
    barrier.submit_action("player-4", "I scout");

    // Wait for all tasks to complete
    for h in handles {
        h.await.unwrap();
    }

    let final_count = claim_count.load(Ordering::SeqCst);
    eprintln!("Final claim count: {}", final_count);
    assert_eq!(
        final_count,
        1,
        "exactly one handler should claim resolution, not {}",
        final_count
    );
}

// ---------------------------------------------------------------------------
// AC-3: Resolution result stored for non-claiming handlers
// ---------------------------------------------------------------------------

#[tokio::test]
async fn barrier_stores_narration_result_for_non_claimers() {
    // Simulate the real flow: multiple handlers wake up from wait_for_turn() concurrently.
    // The claiming handler stores narration text. Non-claiming handlers retrieve it.
    let session = two_player_session();
    let barrier = TurnBarrier::new(session, TurnBarrierConfig::new(Duration::from_secs(5)));

    // Pre-fill both players' actions
    barrier.submit_action("player-1", "I search");
    barrier.submit_action("player-2", "I wait");

    let stored_text = Arc::new(Mutex::new(None));

    // Spawn two tasks to simulate concurrent handlers
    let mut handles = vec![];
    for player_id in &["player-1", "player-2"] {
        let b = barrier.clone();
        let text_ref = stored_text.clone();
        let _pid = player_id.to_string();
        
        handles.push(tokio::spawn(async move {
            let result = b.wait_for_turn().await;
            
            if result.claimed_resolution {
                // This handler won — run narrator and store result
                let narration = "The party searches while keeping watch. Thorn finds a hidden compartment.";
                b.store_resolution_narration(narration.to_string());
                *text_ref.lock().unwrap() = Some(narration.to_string());
            } else {
                // Non-claiming handler should be able to retrieve stored narration
                if let Some(narration) = b.get_resolution_narration() {
                    *text_ref.lock().unwrap() = Some(narration);
                }
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    let stored = stored_text.lock().unwrap();
    assert!(stored.is_some(), "stored narration should be available");
    assert_eq!(
        stored.as_ref().unwrap().as_str(),
        "The party searches while keeping watch. Thorn finds a hidden compartment.",
        "handler should have stored/retrieved correct narration text"
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

// ===========================================================================
// RED TESTS — Story 13-8 coordination gaps in dispatch integration
//
// These tests expose the real bugs: the barrier primitives above all work,
// but the dispatch handler pattern fails because:
//   Bug 1: dispatch reads ss.multiplayer.named_actions() (wrong session)
//   Bug 2: all N handlers call narrator (no claimed_resolution gating)
//
// The fix requires adding a wait mechanism so non-claiming handlers block
// until the claimer stores the narration result.
//
// NOTE: Uses multi_thread runtime + tokio::sync::Barrier to force real
// concurrency. On single-threaded runtime, cooperative scheduling hides
// the race because tasks serialize naturally.
// ===========================================================================

// ---------------------------------------------------------------------------
// Bug 2: Non-claiming handlers cannot reliably retrieve narration
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_handlers_with_narrator_delay_all_receive_narration() {
    // Simulates the real dispatch pattern: the claiming handler calls the
    // narrator (which takes time), while non-claiming handlers need the
    // result immediately. With the current non-blocking get_resolution_narration(),
    // non-claimers race and get None.
    //
    // The tokio::sync::Barrier ensures ALL handlers have returned from
    // wait_for_turn() before ANY of them proceed to the if/else block.
    // This guarantees the race window: the claimer starts sleeping (narrator)
    // while non-claimers simultaneously call get_resolution_narration().
    //
    // Fix: TurnBarrier needs a blocking wait_for_resolution_narration() method
    // (or equivalent signaling mechanism) so non-claimers can wait.

    let session = four_player_session();
    let barrier = TurnBarrier::new(session, TurnBarrierConfig::new(Duration::from_secs(30)));

    // Pre-submit all actions so barrier_met is true immediately
    barrier.submit_action("player-1", "I charge forward");
    barrier.submit_action("player-2", "I cast fireball");
    barrier.submit_action("player-3", "I flank left");
    barrier.submit_action("player-4", "I heal the party");

    let received: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    // Sync barrier ensures all 4 handlers return from wait_for_turn() before proceeding
    let sync_barrier = Arc::new(tokio::sync::Barrier::new(4));
    let mut handles = vec![];

    for _ in 0..4 {
        let b = barrier.clone();
        let recv = received.clone();
        let sync = sync_barrier.clone();
        handles.push(tokio::spawn(async move {
            let result = b.wait_for_turn().await;

            // All handlers synchronize here — guarantees they all returned
            // from wait_for_turn() before any proceeds to if/else
            sync.wait().await;

            let narration = if result.claimed_resolution {
                // Simulate narrator call — this takes real time in production
                // (Claude CLI subprocess, typically 2-10 seconds)
                tokio::time::sleep(Duration::from_millis(100)).await;
                let text = "The party acts in concert.".to_string();
                b.store_resolution_narration(text.clone());
                text
            } else {
                // Non-claiming handler must retrieve the narration.
                // Currently get_resolution_narration() is non-blocking and returns
                // None if the claimer hasn't stored yet. This MUST become blocking
                // or use a wait mechanism (e.g., wait_for_resolution_narration()).
                b.get_resolution_narration()
                    .expect("non-claiming handler must receive stored narration (race condition!)")
            };

            recv.lock().unwrap().push(narration);
        }));
    }

    for h in handles {
        h.await.expect("handler task should not panic");
    }

    let all = received.lock().unwrap();
    assert_eq!(all.len(), 4, "all 4 handlers must receive narration");
    for n in all.iter() {
        assert_eq!(
            n, "The party acts in concert.",
            "all handlers must receive identical narration text"
        );
    }
}

// ---------------------------------------------------------------------------
// Bug 2 (multi-turn stress): Narrator called exactly once per turn
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn five_turns_narrator_called_exactly_once_per_turn() {
    // Over 5 successive barrier turns with 4 concurrent handlers each,
    // the narrator (simulated by an atomic counter) must be called exactly
    // once per turn — not 4 times.
    //
    // This is AC-1 and AC-4: no duplicate narrator calls, no duplicate
    // world state writes.

    let session = four_player_session();
    let barrier = TurnBarrier::new(session, TurnBarrierConfig::new(Duration::from_secs(30)));

    let narrator_calls = Arc::new(AtomicU32::new(0));
    let all_narrations: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    for turn in 0..5u32 {
        // Submit all 4 actions BEFORE spawning handlers so barrier_met is
        // true immediately — no notify/timeout dependency
        barrier.submit_action("player-1", &format!("action-{}-1", turn));
        barrier.submit_action("player-2", &format!("action-{}-2", turn));
        barrier.submit_action("player-3", &format!("action-{}-3", turn));
        barrier.submit_action("player-4", &format!("action-{}-4", turn));

        let sync_barrier = Arc::new(tokio::sync::Barrier::new(4));
        let mut handles = vec![];

        for handler_id in 0..4 {
            let b = barrier.clone();
            let calls = narrator_calls.clone();
            let narrs = all_narrations.clone();
            let sync = sync_barrier.clone();

            handles.push(tokio::spawn(async move {
                let result = b.wait_for_turn().await;

                // Sync all handlers before proceeding
                sync.wait().await;

                let narration = if result.claimed_resolution {
                    // "Narrator" — increment counter, simulate delay
                    calls.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    let text = format!("Turn {} narration", turn);
                    b.store_resolution_narration(text.clone());
                    text
                } else {
                    // Non-claimer must wait for narration
                    b.get_resolution_narration()
                        .unwrap_or_else(|| {
                            panic!(
                                "Handler {} in turn {} could not retrieve narration (race condition)",
                                handler_id, turn
                            )
                        })
                };

                narrs.lock().unwrap().push(narration);
            }));
        }

        for h in handles {
            h.await.expect("handler task should not panic");
        }
    }

    let total_calls = narrator_calls.load(Ordering::SeqCst);
    assert_eq!(
        total_calls, 5,
        "narrator must be called exactly once per turn (5 turns), got {}",
        total_calls
    );

    let all = all_narrations.lock().unwrap();
    assert_eq!(
        all.len(),
        20,
        "all 20 handler invocations (4 handlers × 5 turns) must receive narration"
    );
}

// ---------------------------------------------------------------------------
// Bug 1: Named actions available after resolution (for combined prompt)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn barrier_named_actions_available_for_prompt_after_resolution() {
    // After barrier resolves, the claiming handler needs to build the
    // "PARTY ACTIONS:" block for the narrator prompt. The actions must
    // come from the barrier's internal session (via named_actions()),
    // NOT from SharedGameSession.multiplayer (which is a separate empty session).
    //
    // This test verifies that named_actions() returns character-keyed data
    // AFTER resolution — dispatch.rs must use this instead of ss.multiplayer.

    let session = four_player_session();
    let barrier = TurnBarrier::new(session, TurnBarrierConfig::new(Duration::from_secs(5)));

    barrier.submit_action("player-1", "I charge forward");
    barrier.submit_action("player-2", "I cast fireball");
    barrier.submit_action("player-3", "I flank left");
    barrier.submit_action("player-4", "I heal the party");

    let result = barrier.wait_for_turn().await;
    assert!(result.claimed_resolution, "single waiter should claim");

    // After resolution, named_actions() must still return the actions
    // (falls back to last_resolved_actions). This is the data dispatch.rs
    // should use to build the combined prompt.
    let named = barrier.named_actions();
    assert_eq!(named.len(), 4, "should have all 4 character actions after resolution");

    // Verify character name keys (not player IDs)
    assert!(named.contains_key("Thorn"), "Thorn's action should be present");
    assert!(named.contains_key("Elara"), "Elara's action should be present");
    assert!(named.contains_key("Brak"), "Brak's action should be present");
    assert!(named.contains_key("Lyra"), "Lyra's action should be present");

    // Build combined prompt (same format dispatch.rs uses)
    let combined: String = {
        let mut entries: Vec<_> = named.iter().collect();
        entries.sort_by_key(|(name, _)| name.clone());
        entries
            .iter()
            .map(|(name, act)| format!("{}: {}", name, act))
            .collect::<Vec<_>>()
            .join("\n")
    };

    assert!(combined.contains("Thorn: I charge forward"), "combined prompt must include Thorn");
    assert!(combined.contains("Elara: I cast fireball"), "combined prompt must include Elara");
    assert!(combined.contains("Brak: I flank left"), "combined prompt must include Brak");
    assert!(combined.contains("Lyra: I heal the party"), "combined prompt must include Lyra");
}

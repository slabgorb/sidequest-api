//! RED tests for Story 8-3: Adaptive action batching.
//!
//! The collection window scales by player count:
//! - 2-3 players → 3s timeout
//! - 4+ players → 5s timeout
//!
//! `AdaptiveTimeout` wraps `TurnBarrierConfig` logic so the barrier timeout
//! adjusts automatically when players join or leave. Can be used standalone
//! or plugged into `TurnBarrier`.

use std::collections::HashMap;
use std::time::Duration;

use sidequest_game::barrier::AdaptiveTimeout;
use sidequest_game::barrier::{TurnBarrier, TurnBarrierConfig};
use sidequest_game::character::Character;
use sidequest_game::multiplayer::MultiplayerSession;

mod common;
use common::make_character;

fn session_with_n_players(n: usize) -> MultiplayerSession {
    let players: HashMap<String, Character> = (0..n)
        .map(|i| (format!("player-{i}"), make_character(&format!("Hero {i}"))))
        .collect();
    MultiplayerSession::new(players)
}

// ===========================================================================
// 1. AdaptiveTimeout — default tier thresholds
// ===========================================================================

#[test]
fn default_tiers_two_players_3s() {
    let adaptive = AdaptiveTimeout::default();
    assert_eq!(adaptive.timeout_for(2), Duration::from_secs(3));
}

#[test]
fn default_tiers_three_players_3s() {
    let adaptive = AdaptiveTimeout::default();
    assert_eq!(adaptive.timeout_for(3), Duration::from_secs(3));
}

#[test]
fn default_tiers_four_players_5s() {
    let adaptive = AdaptiveTimeout::default();
    assert_eq!(adaptive.timeout_for(4), Duration::from_secs(5));
}

#[test]
fn default_tiers_six_players_5s() {
    let adaptive = AdaptiveTimeout::default();
    assert_eq!(adaptive.timeout_for(6), Duration::from_secs(5));
}

// ===========================================================================
// 2. Edge cases — solo player and boundaries
// ===========================================================================

#[test]
fn solo_player_uses_smallest_tier() {
    let adaptive = AdaptiveTimeout::default();
    // 1 player: should use the lowest tier (3s)
    assert_eq!(adaptive.timeout_for(1), Duration::from_secs(3));
}

#[test]
fn exact_boundary_three_to_four() {
    let adaptive = AdaptiveTimeout::default();
    assert_eq!(adaptive.timeout_for(3), Duration::from_secs(3));
    assert_eq!(adaptive.timeout_for(4), Duration::from_secs(5));
}

#[test]
fn zero_players_uses_smallest_tier() {
    let adaptive = AdaptiveTimeout::default();
    // Degenerate case: should not panic, use lowest tier
    assert_eq!(adaptive.timeout_for(0), Duration::from_secs(3));
}

// ===========================================================================
// 3. Custom tier configuration
// ===========================================================================

#[test]
fn custom_two_tier() {
    // 1-2 players → 2s, 3+ → 10s
    let adaptive =
        AdaptiveTimeout::with_tiers(vec![(3, Duration::from_secs(10))], Duration::from_secs(2));
    assert_eq!(adaptive.timeout_for(1), Duration::from_secs(2));
    assert_eq!(adaptive.timeout_for(2), Duration::from_secs(2));
    assert_eq!(adaptive.timeout_for(3), Duration::from_secs(10));
    assert_eq!(adaptive.timeout_for(5), Duration::from_secs(10));
}

#[test]
fn custom_three_tier() {
    // 1-2 → 2s, 3-4 → 5s, 5+ → 8s
    let adaptive = AdaptiveTimeout::with_tiers(
        vec![(3, Duration::from_secs(5)), (5, Duration::from_secs(8))],
        Duration::from_secs(2),
    );
    assert_eq!(adaptive.timeout_for(2), Duration::from_secs(2));
    assert_eq!(adaptive.timeout_for(3), Duration::from_secs(5));
    assert_eq!(adaptive.timeout_for(4), Duration::from_secs(5));
    assert_eq!(adaptive.timeout_for(5), Duration::from_secs(8));
    assert_eq!(adaptive.timeout_for(10), Duration::from_secs(8));
}

#[test]
fn empty_tiers_always_returns_base() {
    let adaptive = AdaptiveTimeout::with_tiers(vec![], Duration::from_secs(7));
    assert_eq!(adaptive.timeout_for(1), Duration::from_secs(7));
    assert_eq!(adaptive.timeout_for(100), Duration::from_secs(7));
}

// ===========================================================================
// 4. Conversion to TurnBarrierConfig
// ===========================================================================

#[test]
fn adaptive_produces_config_for_player_count() {
    let adaptive = AdaptiveTimeout::default();
    let config: TurnBarrierConfig = adaptive.config_for(3);
    assert_eq!(config.timeout(), Duration::from_secs(3));
    assert!(config.is_enabled());
}

#[test]
fn adaptive_produces_config_for_large_party() {
    let adaptive = AdaptiveTimeout::default();
    let config: TurnBarrierConfig = adaptive.config_for(5);
    assert_eq!(config.timeout(), Duration::from_secs(5));
    assert!(config.is_enabled());
}

// ===========================================================================
// 5. Integration with TurnBarrier — adaptive config applied
// ===========================================================================

#[test]
fn barrier_with_adaptive_timeout_two_players() {
    let session = session_with_n_players(2);
    let adaptive = AdaptiveTimeout::default();
    let barrier = TurnBarrier::with_adaptive(session, adaptive);
    // Should have picked the 2-3 player tier (3s)
    assert_eq!(barrier.config().timeout(), Duration::from_secs(3));
}

#[test]
fn barrier_with_adaptive_timeout_four_players() {
    let session = session_with_n_players(4);
    let adaptive = AdaptiveTimeout::default();
    let barrier = TurnBarrier::with_adaptive(session, adaptive);
    // Should have picked the 4+ player tier (5s)
    assert_eq!(barrier.config().timeout(), Duration::from_secs(5));
}

// ===========================================================================
// 6. Timeout adjusts when players join/leave (crossing tier boundary)
// ===========================================================================

#[test]
fn timeout_increases_when_player_joins_crossing_boundary() {
    let session = session_with_n_players(3);
    let adaptive = AdaptiveTimeout::default();
    let barrier = TurnBarrier::with_adaptive(session, adaptive);

    // 3 players → 3s
    assert_eq!(barrier.config().timeout(), Duration::from_secs(3));

    // Add a 4th player — crosses into 4+ tier
    barrier
        .add_player("player-new".to_string(), make_character("Newcomer"))
        .unwrap();

    // Should have auto-adjusted to 5s
    assert_eq!(barrier.config().timeout(), Duration::from_secs(5));
}

#[test]
fn timeout_decreases_when_player_leaves_crossing_boundary() {
    let session = session_with_n_players(4);
    let adaptive = AdaptiveTimeout::default();
    let barrier = TurnBarrier::with_adaptive(session, adaptive);

    // 4 players → 5s
    assert_eq!(barrier.config().timeout(), Duration::from_secs(5));

    // Remove a player — drops to 3 players
    barrier.remove_player("player-0").unwrap();

    // Should have auto-adjusted to 3s
    assert_eq!(barrier.config().timeout(), Duration::from_secs(3));
}

#[test]
fn timeout_stays_same_within_tier() {
    let session = session_with_n_players(4);
    let adaptive = AdaptiveTimeout::default();
    let barrier = TurnBarrier::with_adaptive(session, adaptive);

    assert_eq!(barrier.config().timeout(), Duration::from_secs(5));

    // Add a 5th player — still in 4+ tier
    barrier
        .add_player("player-new".to_string(), make_character("Extra"))
        .unwrap();

    // Should remain 5s
    assert_eq!(barrier.config().timeout(), Duration::from_secs(5));
}

// ===========================================================================
// 7. Async integration — timeout actually fires at adaptive duration
// ===========================================================================

#[tokio::test]
async fn adaptive_timeout_fires_at_3s_for_small_party() {
    tokio::time::pause();

    let session = session_with_n_players(2);
    let adaptive = AdaptiveTimeout::default();
    let barrier = TurnBarrier::with_adaptive(session, adaptive);

    let b = barrier.clone();
    let wait_handle = tokio::spawn(async move { b.wait_for_turn().await });

    // Submit for player-0 only
    tokio::time::advance(Duration::from_millis(10)).await;
    barrier.submit_action("player-0", "attack");

    // Advance 3s + margin — should have timed out
    tokio::time::advance(Duration::from_secs(4)).await;

    let result = wait_handle.await.unwrap();
    assert!(result.timed_out);
    assert_eq!(result.missing_players, vec!["player-1".to_string()]);
}

#[tokio::test]
async fn adaptive_timeout_fires_at_5s_for_large_party() {
    tokio::time::pause();

    let session = session_with_n_players(4);
    let adaptive = AdaptiveTimeout::default();
    let barrier = TurnBarrier::with_adaptive(session, adaptive);

    let b = barrier.clone();
    let wait_handle = tokio::spawn(async move { b.wait_for_turn().await });

    // Submit for player-0 only
    tokio::time::advance(Duration::from_millis(10)).await;
    barrier.submit_action("player-0", "attack");

    // At 4s the 5s timeout shouldn't have fired yet
    tokio::time::advance(Duration::from_secs(4)).await;
    assert!(
        !wait_handle.is_finished(),
        "5s timeout should not fire at 4s"
    );

    // At 6s it should have
    tokio::time::advance(Duration::from_secs(2)).await;

    let result = wait_handle.await.unwrap();
    assert!(result.timed_out);
}

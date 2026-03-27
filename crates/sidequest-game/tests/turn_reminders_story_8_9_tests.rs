//! Story 8-9: Turn reminders — notify idle players after configurable timeout.
//!
//! Tests cover: ReminderConfig creation and defaults, reminder eligibility
//! computation, threshold math, per-player targeting, suppression after
//! submission, and edge cases.

#![allow(deprecated)] // Tests exercise the deprecated ReminderConfig::new() API

use std::collections::HashMap;
use std::time::Duration;

use sidequest_game::character::Character;
use sidequest_game::creature_core::CreatureCore;
use sidequest_game::inventory::Inventory;
use sidequest_game::multiplayer::MultiplayerSession;
use sidequest_game::turn_reminder::{ReminderConfig, ReminderResult};
use sidequest_protocol::NonBlankString;

// ===========================================================================
// Test helpers
// ===========================================================================

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
            inventory: Inventory::default(),
            statuses: vec![],
        },
        backstory: NonBlankString::new("Grew up on the frontier").unwrap(),
        narrative_state: String::new(),
        hooks: vec![],
        char_class: NonBlankString::new("Fighter").unwrap(),
        race: NonBlankString::new("Human").unwrap(),
        stats: HashMap::new(),
        abilities: vec![],
        is_friendly: true,
    }
}

fn two_player_session() -> MultiplayerSession {
    let mut players = HashMap::new();
    players.insert("player-1".to_string(), make_character("Thorn"));
    players.insert("player-2".to_string(), make_character("Elara"));
    MultiplayerSession::new(players)
}

// ===========================================================================
// Section 1: ReminderConfig — construction and defaults
// ===========================================================================

#[test]
fn default_config_has_reasonable_threshold() {
    let config = ReminderConfig::default();
    let threshold = config.threshold();
    // Default threshold should be between 0.0 and 1.0 (fraction of barrier timeout)
    assert!(
        (0.0..=1.0).contains(&threshold),
        "default threshold should be in [0.0, 1.0], got: {}",
        threshold
    );
}

#[test]
fn default_config_has_non_empty_message() {
    let config = ReminderConfig::default();
    assert!(
        !config.message().is_empty(),
        "default reminder message should not be empty"
    );
}

#[test]
fn custom_config_stores_threshold_and_message() {
    let config = ReminderConfig::new(0.75, "Hurry up, adventurer!".to_string());
    assert!((config.threshold() - 0.75).abs() < f64::EPSILON);
    assert_eq!(config.message(), "Hurry up, adventurer!");
}

// ===========================================================================
// Section 2: Reminder delay calculation — AC: configurable threshold
// ===========================================================================

#[test]
fn reminder_delay_is_fraction_of_barrier_timeout() {
    let config = ReminderConfig::new(0.6, "hurry".to_string());
    let barrier_timeout = Duration::from_secs(30);
    let delay = config.reminder_delay(barrier_timeout);
    // 0.6 * 30s = 18s
    assert_eq!(delay, Duration::from_secs(18));
}

#[test]
fn reminder_delay_with_custom_threshold() {
    let config = ReminderConfig::new(0.5, "msg".to_string());
    let barrier_timeout = Duration::from_secs(60);
    let delay = config.reminder_delay(barrier_timeout);
    // 0.5 * 60s = 30s
    assert_eq!(delay, Duration::from_secs(30));
}

#[test]
fn reminder_delay_with_zero_threshold_is_instant() {
    let config = ReminderConfig::new(0.0, "msg".to_string());
    let barrier_timeout = Duration::from_secs(30);
    let delay = config.reminder_delay(barrier_timeout);
    assert_eq!(delay, Duration::from_secs(0));
}

#[test]
fn reminder_delay_with_full_threshold_equals_barrier() {
    let config = ReminderConfig::new(1.0, "msg".to_string());
    let barrier_timeout = Duration::from_secs(30);
    let delay = config.reminder_delay(barrier_timeout);
    assert_eq!(delay, Duration::from_secs(30));
}

// ===========================================================================
// Section 3: Reminder eligibility — AC: only idle players
// ===========================================================================

#[test]
fn idle_players_identified_when_none_submitted() {
    let session = two_player_session();
    let config = ReminderConfig::default();
    let result = ReminderResult::check(&session, &config);

    // Both players are idle
    assert_eq!(result.idle_players().len(), 2);
    assert!(result.idle_players().contains(&"player-1".to_string()));
    assert!(result.idle_players().contains(&"player-2".to_string()));
}

#[test]
fn submitted_player_excluded_from_reminders() {
    let mut session = two_player_session();
    session.submit_action("player-1", "I search the room");

    let config = ReminderConfig::default();
    let result = ReminderResult::check(&session, &config);

    // Only player-2 is idle
    assert_eq!(result.idle_players().len(), 1);
    assert!(result.idle_players().contains(&"player-2".to_string()));
    assert!(!result.idle_players().contains(&"player-1".to_string()));
}

#[test]
fn no_idle_players_when_all_submitted() {
    let mut session = two_player_session();
    session.submit_action("player-1", "attack");
    // Turn resolves after player-2 submits, but let's check before that
    // by using record_action which doesn't auto-resolve
    let mut session2 = two_player_session();
    session2.record_action("player-1", "attack");
    session2.record_action("player-2", "defend");

    let config = ReminderConfig::default();
    let result = ReminderResult::check(&session2, &config);

    assert!(
        result.idle_players().is_empty(),
        "no reminders when all players submitted"
    );
}

// ===========================================================================
// Section 4: Per-player targeting — AC: per-player, not global
// ===========================================================================

#[test]
fn reminder_contains_message_text() {
    let session = two_player_session();
    let config = ReminderConfig::new(0.6, "The party awaits your decision...".to_string());
    let result = ReminderResult::check(&session, &config);

    assert_eq!(result.message(), "The party awaits your decision...");
}

#[test]
fn reminder_result_is_per_player() {
    let mut session = two_player_session();
    session.record_action("player-1", "attack");

    let config = ReminderConfig::default();
    let result = ReminderResult::check(&session, &config);

    // Should target only player-2
    let idle = result.idle_players();
    assert_eq!(idle.len(), 1);
    assert_eq!(idle[0], "player-2");
}

// ===========================================================================
// Section 5: Suppression after submission — AC: timer resets
// ===========================================================================

#[test]
fn reminder_not_needed_after_action_submission() {
    let mut session = two_player_session();
    session.record_action("player-1", "attack");
    session.record_action("player-2", "defend");

    let config = ReminderConfig::default();
    let result = ReminderResult::check(&session, &config);

    assert!(!result.should_send());
}

#[test]
fn reminder_needed_when_players_idle() {
    let session = two_player_session();
    let config = ReminderConfig::default();
    let result = ReminderResult::check(&session, &config);

    assert!(result.should_send());
}

// ===========================================================================
// Section 6: Genre voice — AC: message overridable per genre
// ===========================================================================

#[test]
fn genre_specific_message_used_in_reminder() {
    let config = ReminderConfig::new(
        0.6,
        "The wasteland grows impatient, wanderer...".to_string(),
    );
    assert_eq!(
        config.message(),
        "The wasteland grows impatient, wanderer..."
    );
}

#[test]
fn default_message_is_genre_neutral() {
    let config = ReminderConfig::default();
    let msg = config.message();
    // Default should be a reasonable genre-neutral prompt
    assert!(
        msg.len() > 5,
        "default message should be meaningful, got: {}",
        msg
    );
}

// ===========================================================================
// Section 7: ReminderResult struct
// ===========================================================================

#[test]
fn reminder_result_is_debug() {
    let session = two_player_session();
    let config = ReminderConfig::default();
    let result = ReminderResult::check(&session, &config);
    let debug = format!("{:?}", result);
    assert!(debug.contains("ReminderResult"));
}

#[test]
fn reminder_config_is_debug() {
    let config = ReminderConfig::default();
    let debug = format!("{:?}", config);
    assert!(debug.contains("ReminderConfig"));
}

#[test]
fn reminder_config_clone_is_independent() {
    let config = ReminderConfig::new(0.7, "test".to_string());
    let cloned = config.clone();
    assert!((cloned.threshold() - config.threshold()).abs() < f64::EPSILON);
    assert_eq!(cloned.message(), config.message());
}

// ===========================================================================
// Section 8: Edge cases
// ===========================================================================

#[test]
fn single_player_session_can_get_reminder() {
    let mut players = HashMap::new();
    players.insert("solo".to_string(), make_character("Lone Wolf"));
    let session = MultiplayerSession::new(players);

    let config = ReminderConfig::default();
    let result = ReminderResult::check(&session, &config);

    assert_eq!(result.idle_players().len(), 1);
    assert!(result.idle_players().contains(&"solo".to_string()));
}

#[test]
fn three_player_session_partial_submissions() {
    let mut players = HashMap::new();
    players.insert("p1".to_string(), make_character("A"));
    players.insert("p2".to_string(), make_character("B"));
    players.insert("p3".to_string(), make_character("C"));
    let mut session = MultiplayerSession::new(players);
    session.record_action("p1", "attack");

    let config = ReminderConfig::default();
    let result = ReminderResult::check(&session, &config);

    assert_eq!(result.idle_players().len(), 2);
    assert!(!result.idle_players().contains(&"p1".to_string()));
}

#[test]
fn reminder_config_with_very_small_threshold() {
    let config = ReminderConfig::new(0.01, "quick!".to_string());
    let delay = config.reminder_delay(Duration::from_secs(100));
    assert_eq!(delay, Duration::from_secs(1));
}

#[test]
fn reminder_config_threshold_clamped_or_validated() {
    // Threshold above 1.0 should either be clamped or rejected
    // The implementation decides — test that it doesn't produce weird behavior
    let config = ReminderConfig::new(1.5, "msg".to_string());
    let delay = config.reminder_delay(Duration::from_secs(10));
    // Should be at most the barrier timeout (10s) or 15s if unclamped
    // Either is acceptable — just ensure it doesn't panic
    assert!(delay.as_secs() <= 15);
}

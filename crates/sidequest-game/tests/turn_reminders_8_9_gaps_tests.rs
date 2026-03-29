//! Story 8-9 RED tests — gaps in turn reminder implementation.
//!
//! These tests cover functionality that does NOT yet exist:
//! - Validated constructor (ReminderConfig::try_new → Result)
//! - FreePlay mode awareness (check_with_mode)
//! - ReminderError enum for validation failures
//! - Async run_reminder with cancellation
//!
//! All tests here are expected to FAIL (compile error or assertion failure).
//! Dev must implement the missing APIs to make them pass.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use sidequest_game::character::Character;
use sidequest_game::creature_core::CreatureCore;
use sidequest_game::inventory::Inventory;
use sidequest_game::multiplayer::MultiplayerSession;
use sidequest_game::turn_mode::TurnMode;
use sidequest_game::turn_reminder::{ReminderConfig, ReminderError, ReminderResult};
use sidequest_protocol::NonBlankString;

use tokio::sync::RwLock;

// ===========================================================================
// Test helpers (duplicated from existing test file — game crate has no
// test-utils module yet)
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

fn three_player_session() -> MultiplayerSession {
    let mut players = HashMap::new();
    players.insert("p1".to_string(), make_character("Aric"));
    players.insert("p2".to_string(), make_character("Bryn"));
    players.insert("p3".to_string(), make_character("Cael"));
    MultiplayerSession::new(players)
}

// ===========================================================================
// Section 1: Validated constructor — Rule #5 (trust boundary validation)
//
// ReminderConfig::try_new() must validate:
//   - threshold ∈ [0.0, 1.0]
//   - message is non-empty
//   - threshold is not NaN or infinity
// ===========================================================================

#[test]
fn try_new_accepts_valid_threshold_and_message() {
    let config = ReminderConfig::try_new(0.6, "The party awaits...".to_string());
    assert!(config.is_ok());
    let config = config.unwrap();
    assert!((config.threshold() - 0.6).abs() < f64::EPSILON);
    assert_eq!(config.message(), "The party awaits...");
}

#[test]
fn try_new_accepts_zero_threshold() {
    let config = ReminderConfig::try_new(0.0, "instant reminder".to_string());
    assert!(config.is_ok());
}

#[test]
fn try_new_accepts_threshold_one() {
    let config = ReminderConfig::try_new(1.0, "at barrier timeout".to_string());
    assert!(config.is_ok());
}

#[test]
fn try_new_rejects_negative_threshold() {
    let result = ReminderConfig::try_new(-0.1, "msg".to_string());
    assert!(result.is_err());
}

#[test]
fn try_new_rejects_threshold_above_one() {
    let result = ReminderConfig::try_new(1.5, "msg".to_string());
    assert!(result.is_err());
}

#[test]
fn try_new_rejects_nan_threshold() {
    let result = ReminderConfig::try_new(f64::NAN, "msg".to_string());
    assert!(result.is_err());
}

#[test]
fn try_new_rejects_infinity_threshold() {
    let result = ReminderConfig::try_new(f64::INFINITY, "msg".to_string());
    assert!(result.is_err());
}

#[test]
fn try_new_rejects_negative_infinity_threshold() {
    let result = ReminderConfig::try_new(f64::NEG_INFINITY, "msg".to_string());
    assert!(result.is_err());
}

#[test]
fn try_new_rejects_empty_message() {
    let result = ReminderConfig::try_new(0.6, String::new());
    assert!(result.is_err());
}

#[test]
fn try_new_rejects_whitespace_only_message() {
    let result = ReminderConfig::try_new(0.6, "   ".to_string());
    assert!(result.is_err());
}

#[test]
fn reminder_error_is_debug_and_display() {
    let result = ReminderConfig::try_new(-1.0, "msg".to_string());
    let err = result.unwrap_err();
    let debug = format!("{:?}", err);
    let display = format!("{}", err);
    assert!(!debug.is_empty());
    assert!(!display.is_empty());
}

// ===========================================================================
// Section 2: FreePlay mode awareness — AC: "FreePlay skip"
//
// Reminders should NOT fire in FreePlay mode since there is no barrier.
// ReminderResult::check_with_mode() takes a TurnMode parameter.
// ===========================================================================

#[test]
fn no_reminders_in_freeplay_mode() {
    let session = two_player_session();
    let config = ReminderConfig::default();
    let result = ReminderResult::check_with_mode(&session, &config, &TurnMode::FreePlay);

    assert!(
        result.idle_players().is_empty(),
        "FreePlay mode should suppress all reminders"
    );
    assert!(!result.should_send());
}

#[test]
fn reminders_fire_in_structured_mode() {
    let session = two_player_session();
    let config = ReminderConfig::default();
    let result = ReminderResult::check_with_mode(&session, &config, &TurnMode::Structured);

    assert_eq!(result.idle_players().len(), 2);
    assert!(result.should_send());
}

#[test]
fn reminders_fire_in_cinematic_mode() {
    let session = two_player_session();
    let config = ReminderConfig::default();
    let result = ReminderResult::check_with_mode(
        &session,
        &config,
        &TurnMode::Cinematic {
            prompt: Some("Choose wisely...".to_string()),
        },
    );

    assert_eq!(result.idle_players().len(), 2);
    assert!(result.should_send());
}

#[test]
fn freeplay_skip_even_with_all_players_idle() {
    let session = three_player_session();
    let config = ReminderConfig::default();
    let result = ReminderResult::check_with_mode(&session, &config, &TurnMode::FreePlay);

    assert!(
        result.idle_players().is_empty(),
        "even 3 idle players should get no reminder in FreePlay"
    );
}

#[test]
fn structured_mode_only_targets_idle_players() {
    let mut session = two_player_session();
    session.record_action("player-1", "I attack the troll");

    let config = ReminderConfig::default();
    let result = ReminderResult::check_with_mode(&session, &config, &TurnMode::Structured);

    assert_eq!(result.idle_players().len(), 1);
    assert_eq!(result.idle_players()[0], "player-2");
}

// ===========================================================================
// Section 3: Async reminder execution — AC: "Cancelled on resolve"
//
// run_reminder() is an async function that:
// 1. Sleeps for config.reminder_delay(barrier_timeout)
// 2. Checks which players are idle
// 3. Returns the idle player list (caller sends messages)
//
// It must be cancellation-safe — dropping the future mid-sleep is fine.
// ===========================================================================

#[tokio::test]
async fn run_reminder_identifies_idle_after_delay() {
    let session = Arc::new(RwLock::new(two_player_session()));
    let config = ReminderConfig::default();
    let barrier_timeout = Duration::from_millis(100);

    let result =
        ReminderResult::run_reminder(barrier_timeout, &config, &session, &TurnMode::Structured)
            .await;

    // After delay, both players still idle
    assert_eq!(result.idle_players().len(), 2);
    assert!(result.should_send());
}

#[tokio::test]
async fn run_reminder_returns_empty_when_all_submitted() {
    let session = Arc::new(RwLock::new(two_player_session()));

    // Both players submit before reminder fires
    {
        let mut s = session.write().await;
        s.record_action("player-1", "attack");
        s.record_action("player-2", "defend");
    }

    let config = ReminderConfig::default();
    let barrier_timeout = Duration::from_millis(100);

    let result =
        ReminderResult::run_reminder(barrier_timeout, &config, &session, &TurnMode::Structured)
            .await;

    assert!(result.idle_players().is_empty());
    assert!(!result.should_send());
}

#[tokio::test]
async fn run_reminder_cancelled_when_barrier_resolves() {
    use tokio::select;
    use tokio::time::sleep;

    let session = Arc::new(RwLock::new(two_player_session()));
    let config = ReminderConfig::default(); // 0.6 threshold
    let barrier_timeout = Duration::from_secs(10); // reminder at 6s

    // Barrier resolves after 50ms — well before the 6s reminder delay
    let barrier_resolve = sleep(Duration::from_millis(50));

    let reminder_future =
        ReminderResult::run_reminder(barrier_timeout, &config, &session, &TurnMode::Structured);

    // select! drops the loser — reminder should be safely cancelled
    let fired = select! {
        result = reminder_future => Some(result),
        _ = barrier_resolve => None,
    };

    assert!(
        fired.is_none(),
        "reminder should have been cancelled by barrier resolve"
    );
}

#[tokio::test]
async fn run_reminder_skips_freeplay() {
    let session = Arc::new(RwLock::new(two_player_session()));
    let config = ReminderConfig::default();
    let barrier_timeout = Duration::from_millis(50);

    let result =
        ReminderResult::run_reminder(barrier_timeout, &config, &session, &TurnMode::FreePlay).await;

    assert!(
        result.idle_players().is_empty(),
        "run_reminder should return empty in FreePlay mode"
    );
}

#[tokio::test]
async fn run_reminder_partial_submission_targets_only_idle() {
    let session = Arc::new(RwLock::new(three_player_session()));

    // p1 submits before reminder fires
    {
        let mut s = session.write().await;
        s.record_action("p1", "I scout ahead");
    }

    let config = ReminderConfig::default();
    let barrier_timeout = Duration::from_millis(100);

    let result =
        ReminderResult::run_reminder(barrier_timeout, &config, &session, &TurnMode::Structured)
            .await;

    // Only p2 and p3 are idle
    assert_eq!(result.idle_players().len(), 2);
    assert!(!result.idle_players().contains(&"p1".to_string()));
}

// ===========================================================================
// Section 4: Genre voice integration — AC: "Genre voice"
//
// ReminderConfig message should be overridable from genre pack YAML.
// The try_new constructor validates the message isn't empty.
// ===========================================================================

#[test]
fn genre_specific_message_via_try_new() {
    let config = ReminderConfig::try_new(
        0.6,
        "The wasteland grows impatient, wanderer...".to_string(),
    );
    assert!(config.is_ok());
    assert_eq!(
        config.unwrap().message(),
        "The wasteland grows impatient, wanderer..."
    );
}

#[test]
fn elemental_harmony_genre_message() {
    let config = ReminderConfig::try_new(
        0.6,
        "The elements shift restlessly. Your party awaits your decision.".to_string(),
    );
    assert!(config.is_ok());
}

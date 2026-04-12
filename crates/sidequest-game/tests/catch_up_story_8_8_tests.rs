//! Story 8-8: Catch-up narration — generate arrival snapshot for mid-session
//! joining players.
//!
//! Tests cover: TurnSummary creation, CatchUpGenerator prompt construction,
//! fallback on failure, targeted delivery, perception filtering, and turn
//! summary maintenance.

use sidequest_game::catch_up::{
    CatchUpError, CatchUpGenerator, CatchUpResult, GenerationStrategy, TurnSummary,
};

mod common;
use common::make_character;

fn sample_turn_summaries(count: usize) -> Vec<TurnSummary> {
    (1..=count)
        .map(|i| TurnSummary::new(i as u32, format!("Turn {i}: the party advances deeper")))
        .collect()
}

/// A test strategy that always succeeds with a canned narration.
struct AlwaysSucceedStrategy {
    response: String,
}

impl AlwaysSucceedStrategy {
    fn new(response: &str) -> Self {
        Self {
            response: response.to_string(),
        }
    }
}

impl GenerationStrategy for AlwaysSucceedStrategy {
    fn generate(&self, _prompt: &str) -> Result<String, CatchUpError> {
        Ok(self.response.clone())
    }
}

/// A test strategy that always fails.
struct AlwaysFailStrategy;

impl GenerationStrategy for AlwaysFailStrategy {
    fn generate(&self, _prompt: &str) -> Result<String, CatchUpError> {
        Err(CatchUpError::GenerationFailed(
            "Claude unavailable".to_string(),
        ))
    }
}

// ===========================================================================
// Section 1: TurnSummary struct
// ===========================================================================

#[test]
fn turn_summary_stores_turn_number_and_text() {
    let ts = TurnSummary::new(3, "The party entered the ruins".to_string());
    assert_eq!(ts.turn_number(), 3);
    assert_eq!(ts.summary(), "The party entered the ruins");
}

#[test]
fn turn_summary_debug_is_derived() {
    let ts = TurnSummary::new(1, "test".to_string());
    let debug = format!("{:?}", ts);
    assert!(debug.contains("TurnSummary"));
}

#[test]
fn turn_summary_clone_is_independent() {
    let ts = TurnSummary::new(1, "original".to_string());
    let cloned = ts.clone();
    assert_eq!(cloned.turn_number(), ts.turn_number());
    assert_eq!(cloned.summary(), ts.summary());
}

#[test]
fn turn_summary_fields_are_private() {
    // Verify TurnSummary uses getters — cannot directly access fields.
    // This is a compile-time check: if fields were public, we'd access
    // them directly. The test exercises the getter API instead.
    let ts = TurnSummary::new(5, "event".to_string());
    let _n: u32 = ts.turn_number();
    let _s: &str = ts.summary();
}

// ===========================================================================
// Section 2: CatchUpError enum
// ===========================================================================

#[test]
fn catch_up_error_generation_failed_variant() {
    let err = CatchUpError::GenerationFailed("timeout".to_string());
    let msg = format!("{}", err);
    assert!(
        msg.contains("timeout"),
        "error message should contain cause"
    );
}

#[test]
fn catch_up_error_no_history_variant() {
    let err = CatchUpError::NoHistory;
    let msg = format!("{}", err);
    assert!(!msg.is_empty(), "error should have a display message");
}

#[test]
fn catch_up_error_is_non_exhaustive() {
    // Compile-time: #[non_exhaustive] means external crates need a wildcard.
    // We verify the variants we know about exist and are constructable.
    let _e1 = CatchUpError::GenerationFailed("test".to_string());
    let _e2 = CatchUpError::NoHistory;
}

#[test]
fn catch_up_error_is_debug() {
    let err = CatchUpError::GenerationFailed("test".to_string());
    let debug = format!("{:?}", err);
    assert!(debug.contains("GenerationFailed"));
}

// ===========================================================================
// Section 3: CatchUpGenerator — construction and basic usage
// ===========================================================================

#[test]
fn generator_can_be_constructed_with_strategy() {
    let strategy = AlwaysSucceedStrategy::new("Welcome!");
    let _gen = CatchUpGenerator::new(Box::new(strategy));
}

#[test]
fn generator_uses_strategy_for_generation() {
    let strategy = AlwaysSucceedStrategy::new("You arrive at the tavern.");
    let gen = CatchUpGenerator::new(Box::new(strategy));

    let character = make_character("Thorn");
    let summaries = sample_turn_summaries(3);

    let result = gen
        .generate_catch_up(
            &character,
            &summaries,
            "The Rusty Dagger Tavern",
            "dark fantasy",
        )
        .unwrap();

    assert_eq!(result.narration(), "You arrive at the tavern.");
}

// ===========================================================================
// Section 4: Prompt construction — AC-2 (key events, characters, location)
// ===========================================================================

#[test]
fn prompt_includes_character_name() {
    // Use a strategy that echoes back the prompt to verify contents
    let strategy = AlwaysSucceedStrategy::new("narration");
    let _gen = CatchUpGenerator::new(Box::new(strategy));
    let character = make_character("Elara Brightwood");
    let summaries = sample_turn_summaries(2);

    // The generator must build a prompt containing the character name.
    // We verify this through build_prompt which should be a public helper.
    let prompt =
        CatchUpGenerator::build_prompt(&character, &summaries, "Dark Forest", "dark fantasy");
    assert!(
        prompt.contains("Elara Brightwood"),
        "prompt should contain character name, got: {}",
        prompt
    );
}

#[test]
fn prompt_includes_location() {
    let character = make_character("Thorn");
    let summaries = sample_turn_summaries(2);

    let prompt =
        CatchUpGenerator::build_prompt(&character, &summaries, "Dark Forest", "dark fantasy");
    assert!(
        prompt.contains("Dark Forest"),
        "prompt should contain location, got: {}",
        prompt
    );
}

#[test]
fn prompt_includes_genre_voice() {
    let character = make_character("Thorn");
    let summaries = sample_turn_summaries(1);

    let prompt = CatchUpGenerator::build_prompt(&character, &summaries, "Castle", "dark fantasy");
    assert!(
        prompt.contains("dark fantasy"),
        "prompt should contain genre voice, got: {}",
        prompt
    );
}

#[test]
fn prompt_includes_recent_events() {
    let character = make_character("Thorn");
    let summaries = vec![
        TurnSummary::new(1, "The dragon attacked the village".to_string()),
        TurnSummary::new(2, "Elara healed the wounded".to_string()),
    ];

    let prompt =
        CatchUpGenerator::build_prompt(&character, &summaries, "Village Square", "high fantasy");
    assert!(
        prompt.contains("dragon attacked"),
        "prompt should contain recent events, got: {}",
        prompt
    );
}

// ===========================================================================
// Section 5: Last-5-turns window — Story context AC-2
// ===========================================================================

#[test]
fn format_recent_takes_last_5_turns() {
    let summaries = sample_turn_summaries(10);
    let formatted = CatchUpGenerator::format_recent(&summaries);

    // Should contain turns 6-10 (last 5), not turns 1-5
    assert!(formatted.contains("Turn 10"));
    assert!(formatted.contains("Turn 6"));
    assert!(!formatted.contains("Turn 5"));
}

#[test]
fn format_recent_with_fewer_than_5_includes_all() {
    let summaries = sample_turn_summaries(3);
    let formatted = CatchUpGenerator::format_recent(&summaries);

    assert!(formatted.contains("Turn 1"));
    assert!(formatted.contains("Turn 2"));
    assert!(formatted.contains("Turn 3"));
}

#[test]
fn format_recent_empty_returns_empty_or_no_events() {
    let formatted = CatchUpGenerator::format_recent(&[]);
    // Empty summaries should produce empty or "no events" string
    assert!(
        formatted.is_empty() || formatted.contains("no") || formatted.contains("No"),
        "empty summaries should produce empty or descriptive string, got: {}",
        formatted
    );
}

// ===========================================================================
// Section 6: Graceful fallback — Story context AC-5
// ===========================================================================

#[test]
fn generation_failure_returns_fallback_narration() {
    let strategy = AlwaysFailStrategy;
    let gen = CatchUpGenerator::new(Box::new(strategy));
    let character = make_character("Thorn");
    let summaries = sample_turn_summaries(3);

    let result = gen.generate_catch_up_with_fallback(
        &character,
        &summaries,
        "The Rusty Dagger Tavern",
        "dark fantasy",
    );

    // Should NOT be an error — fallback kicks in
    assert!(result.is_ok(), "fallback should handle generation failure");

    let catch_up = result.unwrap();
    assert!(
        catch_up.is_fallback(),
        "result should be marked as fallback"
    );
    // Fallback should include at least the location
    assert!(
        catch_up.narration().contains("Rusty Dagger Tavern"),
        "fallback should mention location, got: {}",
        catch_up.narration()
    );
}

#[test]
fn fallback_includes_character_name() {
    let strategy = AlwaysFailStrategy;
    let gen = CatchUpGenerator::new(Box::new(strategy));
    let character = make_character("Elara");
    let summaries = sample_turn_summaries(1);

    let result = gen
        .generate_catch_up_with_fallback(&character, &summaries, "Town Square", "fantasy")
        .unwrap();

    assert!(result.is_fallback());
    assert!(
        result.narration().contains("Elara"),
        "fallback should mention character name, got: {}",
        result.narration()
    );
}

#[test]
fn successful_generation_is_not_marked_as_fallback() {
    let strategy = AlwaysSucceedStrategy::new("The torchlight flickers as you enter.");
    let gen = CatchUpGenerator::new(Box::new(strategy));
    let character = make_character("Thorn");
    let summaries = sample_turn_summaries(2);

    let result = gen
        .generate_catch_up_with_fallback(&character, &summaries, "Dungeon", "dark fantasy")
        .unwrap();

    assert!(
        !result.is_fallback(),
        "successful generation should not be fallback"
    );
    assert_eq!(result.narration(), "The torchlight flickers as you enter.");
}

// ===========================================================================
// Section 7: CatchUpResult struct
// ===========================================================================

#[test]
fn catch_up_result_generated_variant() {
    let result = CatchUpResult::generated("You arrive at the scene.".to_string());
    assert_eq!(result.narration(), "You arrive at the scene.");
    assert!(!result.is_fallback());
}

#[test]
fn catch_up_result_fallback_variant() {
    let result = CatchUpResult::fallback("You find yourself in the town.".to_string());
    assert_eq!(result.narration(), "You find yourself in the town.");
    assert!(result.is_fallback());
}

#[test]
fn catch_up_result_is_debug() {
    let result = CatchUpResult::generated("test".to_string());
    let debug = format!("{:?}", result);
    assert!(
        debug.contains("CatchUpResult") || debug.contains("Generated") || debug.contains("test")
    );
}

// ===========================================================================
// Section 8: Targeted delivery — Story context AC-3 / Session AC-4
// ===========================================================================

#[test]
fn catch_up_result_has_target_player_id() {
    let result =
        CatchUpResult::generated("narration".to_string()).for_player("player-3".to_string());

    assert_eq!(result.target_player_id(), Some("player-3"));
}

#[test]
fn catch_up_result_without_target_is_none() {
    let result = CatchUpResult::generated("narration".to_string());
    assert_eq!(result.target_player_id(), None);
}

// ===========================================================================
// Section 9: Join notification — Story context AC-4
// ===========================================================================

#[test]
fn join_notification_for_existing_players() {
    let notification = CatchUpGenerator::join_notification("Elara");
    assert!(
        notification.contains("Elara"),
        "notification should mention joining player, got: {}",
        notification
    );
    // Should be a brief notification, not a full narration
    assert!(
        notification.len() < 200,
        "join notification should be brief"
    );
}

// ===========================================================================
// Section 10: No history edge case
// ===========================================================================

#[test]
fn generate_with_empty_summaries_returns_error_or_minimal() {
    let strategy = AlwaysSucceedStrategy::new("Welcome!");
    let gen = CatchUpGenerator::new(Box::new(strategy));
    let character = make_character("Thorn");

    // With no turn summaries, generate should either return NoHistory error
    // or a minimal result. The spec says "recent context" implies some history.
    let result = gen.generate_catch_up(&character, &[], "Town", "fantasy");

    // Either an error (NoHistory) or a valid result is acceptable
    // The key is it doesn't panic
    match result {
        Ok(r) => assert!(!r.narration().is_empty()),
        Err(CatchUpError::NoHistory) => {} // acceptable
        Err(e) => panic!("unexpected error: {}", e),
    }
}

// ===========================================================================
// Section 11: GenerationStrategy trait — rule #2 non_exhaustive on enums
// ===========================================================================

#[test]
fn generation_strategy_trait_is_object_safe() {
    // Must be able to create Box<dyn GenerationStrategy> for DI
    let strategy: Box<dyn GenerationStrategy> = Box::new(AlwaysSucceedStrategy::new("test"));
    let result = strategy.generate("prompt");
    assert!(result.is_ok());
}

// ===========================================================================
// Section 12: Edge cases and rule enforcement
// ===========================================================================

#[test]
fn turn_summary_with_empty_summary_text() {
    // Turn summary with empty text — should work (no NonBlankString requirement
    // on summaries, they're internal data)
    let ts = TurnSummary::new(1, String::new());
    assert_eq!(ts.summary(), "");
}

#[test]
fn turn_summary_with_turn_zero() {
    let ts = TurnSummary::new(0, "before the game".to_string());
    assert_eq!(ts.turn_number(), 0);
}

#[test]
fn generator_with_long_summaries_truncates_to_5() {
    let summaries = sample_turn_summaries(100);
    let formatted = CatchUpGenerator::format_recent(&summaries);

    // Should only contain turns 96-100
    assert!(formatted.contains("Turn 100"));
    assert!(formatted.contains("Turn 96"));
    assert!(!formatted.contains("Turn 95"));
}

#[test]
fn catch_up_error_is_std_error() {
    let err: Box<dyn std::error::Error> =
        Box::new(CatchUpError::GenerationFailed("test".to_string()));
    assert!(!err.to_string().is_empty());
}

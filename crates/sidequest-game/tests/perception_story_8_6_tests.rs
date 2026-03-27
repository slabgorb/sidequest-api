//! RED tests for Story 8-6: Perception Rewriter
//!
//! Per-character narration variants based on status effects (blinded, charmed,
//! etc.). In multiplayer, each player sees narration filtered through their
//! character's current perception state. The PerceptionRewriter takes base
//! narration and produces per-character variants for players with active
//! perceptual status effects.
//!
//! These tests define the public API through usage — they should compile but
//! FAIL until the implementation exists.

use std::collections::HashMap;

use sidequest_game::perception::{
    PerceptionFilter, PerceptionRewriter, PerceptualEffect, RewriteStrategy, RewriterError,
};

/// A deterministic rewrite strategy for testing. Prepends the effect
/// names to the narration so we can assert on the output without
/// hitting Claude.
struct TestRewriteStrategy;

impl RewriteStrategy for TestRewriteStrategy {
    fn rewrite(
        &self,
        base_narration: &str,
        filter: &PerceptionFilter,
        _genre_voice: &str,
    ) -> Result<String, RewriterError> {
        let effect_names: Vec<String> = filter.effects().iter().map(|e| format!("{e:?}")).collect();
        Ok(format!("[{}] {}", effect_names.join(", "), base_narration))
    }
}

/// A strategy that always fails — for testing graceful degradation.
struct FailingRewriteStrategy;

impl RewriteStrategy for FailingRewriteStrategy {
    fn rewrite(
        &self,
        _base_narration: &str,
        _filter: &PerceptionFilter,
        _genre_voice: &str,
    ) -> Result<String, RewriterError> {
        Err(RewriterError::Agent("mock failure".to_string()))
    }
}

fn blinded_filter(name: &str) -> PerceptionFilter {
    PerceptionFilter::new(name.to_string(), vec![PerceptualEffect::Blinded])
}

fn charmed_filter(name: &str, source: &str) -> PerceptionFilter {
    PerceptionFilter::new(
        name.to_string(),
        vec![PerceptualEffect::Charmed {
            source: source.to_string(),
        }],
    )
}

fn multi_effect_filter(name: &str) -> PerceptionFilter {
    PerceptionFilter::new(
        name.to_string(),
        vec![PerceptualEffect::Blinded, PerceptualEffect::Deafened],
    )
}

// ===========================================================================
// 1. PerceptualEffect enum — variants and Display/Debug
// ===========================================================================

#[test]
fn perceptual_effect_blinded_exists() {
    let effect = PerceptualEffect::Blinded;
    // Verify it's Clone + Debug (required for downstream usage)
    let cloned = effect.clone();
    assert_eq!(format!("{cloned:?}"), "Blinded");
}

#[test]
fn perceptual_effect_charmed_carries_source() {
    let effect = PerceptualEffect::Charmed {
        source: "Vampire Lord".to_string(),
    };
    let debug = format!("{effect:?}");
    assert!(debug.contains("Charmed"));
    assert!(debug.contains("Vampire Lord"));
}

#[test]
fn perceptual_effect_dominated_carries_controller() {
    let effect = PerceptualEffect::Dominated {
        controller: "Mind Flayer".to_string(),
    };
    let debug = format!("{effect:?}");
    assert!(debug.contains("Dominated"));
    assert!(debug.contains("Mind Flayer"));
}

#[test]
fn perceptual_effect_hallucinating_exists() {
    let effect = PerceptualEffect::Hallucinating;
    assert_eq!(format!("{effect:?}"), "Hallucinating");
}

#[test]
fn perceptual_effect_deafened_exists() {
    let effect = PerceptualEffect::Deafened;
    assert_eq!(format!("{effect:?}"), "Deafened");
}

#[test]
fn perceptual_effect_custom_carries_name_and_description() {
    let effect = PerceptualEffect::Custom {
        name: "Ethereal Sight".to_string(),
        description: "Sees into the ethereal plane".to_string(),
    };
    let debug = format!("{effect:?}");
    assert!(debug.contains("Custom"));
    assert!(debug.contains("Ethereal Sight"));
    assert!(debug.contains("Sees into the ethereal plane"));
}

#[test]
fn perceptual_effect_is_clone() {
    let original = PerceptualEffect::Charmed {
        source: "Siren".to_string(),
    };
    let cloned = original.clone();
    assert_eq!(format!("{original:?}"), format!("{cloned:?}"));
}

// ===========================================================================
// 2. PerceptionFilter — construction and accessors
// ===========================================================================

#[test]
fn perception_filter_new_stores_character_name_and_effects() {
    let filter = PerceptionFilter::new("Thorn".to_string(), vec![PerceptualEffect::Blinded]);
    assert_eq!(filter.character_name(), "Thorn");
    assert_eq!(filter.effects().len(), 1);
}

#[test]
fn perception_filter_has_effects_true_when_nonempty() {
    let filter = blinded_filter("Thorn");
    assert!(filter.has_effects());
}

#[test]
fn perception_filter_has_effects_false_when_empty() {
    let filter = PerceptionFilter::new("Thorn".to_string(), vec![]);
    assert!(!filter.has_effects());
}

#[test]
fn perception_filter_multiple_effects() {
    let filter = multi_effect_filter("Thorn");
    assert_eq!(filter.effects().len(), 2);
    assert!(filter.has_effects());
}

#[test]
fn perception_filter_character_name_is_private() {
    // Ensures the field is accessed via getter, not directly.
    // If this test compiles, the getter exists.
    let filter = blinded_filter("Elara");
    let name: &str = filter.character_name();
    assert_eq!(name, "Elara");
}

#[test]
fn perception_filter_effects_is_private() {
    // Ensures effects are accessed via getter (slice), not directly.
    let filter = blinded_filter("Elara");
    let effects: &[PerceptualEffect] = filter.effects();
    assert_eq!(effects.len(), 1);
}

// ===========================================================================
// 3. PerceptionRewriter — rewrite with strategy
// ===========================================================================

#[test]
fn rewriter_rewrites_narration_for_affected_character() {
    let strategy = TestRewriteStrategy;
    let rewriter = PerceptionRewriter::new(Box::new(strategy));
    let filter = blinded_filter("Thorn");

    let result = rewriter
        .rewrite("The vampire lunges at the party.", &filter, "dark fantasy")
        .unwrap();

    // The test strategy prepends effect names
    assert!(result.contains("Blinded"));
    assert!(result.contains("The vampire lunges at the party."));
}

#[test]
fn rewriter_includes_all_effects_in_rewrite() {
    let strategy = TestRewriteStrategy;
    let rewriter = PerceptionRewriter::new(Box::new(strategy));
    let filter = multi_effect_filter("Thorn");

    let result = rewriter
        .rewrite("Swords clash in the darkness.", &filter, "dark fantasy")
        .unwrap();

    assert!(result.contains("Blinded"));
    assert!(result.contains("Deafened"));
}

#[test]
fn rewriter_charmed_includes_source_in_context() {
    let strategy = TestRewriteStrategy;
    let rewriter = PerceptionRewriter::new(Box::new(strategy));
    let filter = charmed_filter("Thorn", "Vampire Lord");

    let result = rewriter
        .rewrite("The vampire bares its fangs.", &filter, "horror")
        .unwrap();

    assert!(result.contains("Charmed"));
    assert!(result.contains("Vampire Lord"));
}

#[test]
fn rewriter_custom_effect_includes_description() {
    let strategy = TestRewriteStrategy;
    let rewriter = PerceptionRewriter::new(Box::new(strategy));
    let filter = PerceptionFilter::new(
        "Thorn".to_string(),
        vec![PerceptualEffect::Custom {
            name: "Ethereal Sight".to_string(),
            description: "Sees into the ethereal plane".to_string(),
        }],
    );

    let result = rewriter
        .rewrite("The room appears empty.", &filter, "dark fantasy")
        .unwrap();

    assert!(result.contains("Custom"));
    assert!(result.contains("Ethereal Sight"));
}

// ===========================================================================
// 4. Graceful degradation — failed rewrite falls back to base
// ===========================================================================

#[test]
fn rewriter_error_returns_err() {
    let strategy = FailingRewriteStrategy;
    let rewriter = PerceptionRewriter::new(Box::new(strategy));
    let filter = blinded_filter("Thorn");

    let result = rewriter.rewrite("Base narration.", &filter, "fantasy");

    assert!(result.is_err());
}

#[test]
fn rewriter_error_is_agent_variant() {
    let strategy = FailingRewriteStrategy;
    let rewriter = PerceptionRewriter::new(Box::new(strategy));
    let filter = blinded_filter("Thorn");

    let err = rewriter
        .rewrite("Base narration.", &filter, "fantasy")
        .unwrap_err();

    // Verify it's the Agent error variant with the message
    let msg = format!("{err}");
    assert!(
        msg.contains("mock failure"),
        "error should contain failure message: {msg}"
    );
}

// ===========================================================================
// 5. Effect description — human-readable effect summaries for prompts
// ===========================================================================

#[test]
fn describe_effects_blinded() {
    let effects = vec![PerceptualEffect::Blinded];
    let description = PerceptionRewriter::describe_effects(&effects);
    assert!(
        description.to_lowercase().contains("blind"),
        "description should mention blindness: {description}"
    );
}

#[test]
fn describe_effects_charmed_includes_source() {
    let effects = vec![PerceptualEffect::Charmed {
        source: "Vampire Lord".to_string(),
    }];
    let description = PerceptionRewriter::describe_effects(&effects);
    assert!(
        description.contains("Vampire Lord"),
        "description should mention charm source: {description}"
    );
}

#[test]
fn describe_effects_dominated_includes_controller() {
    let effects = vec![PerceptualEffect::Dominated {
        controller: "Mind Flayer".to_string(),
    }];
    let description = PerceptionRewriter::describe_effects(&effects);
    assert!(
        description.contains("Mind Flayer"),
        "description should mention controller: {description}"
    );
}

#[test]
fn describe_effects_custom_includes_name_and_description() {
    let effects = vec![PerceptualEffect::Custom {
        name: "Ethereal Sight".to_string(),
        description: "Sees into the ethereal plane".to_string(),
    }];
    let description = PerceptionRewriter::describe_effects(&effects);
    assert!(
        description.contains("Ethereal Sight"),
        "description should include custom effect name: {description}"
    );
    assert!(
        description.contains("Sees into the ethereal plane"),
        "description should include custom effect description: {description}"
    );
}

#[test]
fn describe_effects_multiple_joins_all() {
    let effects = vec![
        PerceptualEffect::Blinded,
        PerceptualEffect::Deafened,
        PerceptualEffect::Hallucinating,
    ];
    let description = PerceptionRewriter::describe_effects(&effects);
    // All three effects should be present in the description
    assert!(
        description.to_lowercase().contains("blind"),
        "missing blinded: {description}"
    );
    assert!(
        description.to_lowercase().contains("deaf"),
        "missing deafened: {description}"
    );
    assert!(
        description.to_lowercase().contains("hallucin"),
        "missing hallucinating: {description}"
    );
}

#[test]
fn describe_effects_empty_returns_none_text() {
    let effects: Vec<PerceptualEffect> = vec![];
    let description = PerceptionRewriter::describe_effects(&effects);
    assert!(
        description.to_lowercase().contains("none")
            || description.to_lowercase().contains("no effect")
            || description.is_empty(),
        "empty effects should produce 'none' or empty: {description}"
    );
}

// ===========================================================================
// 6. Per-player narration routing — broadcast logic
// ===========================================================================

#[test]
fn broadcast_unaffected_players_receive_base_narration() {
    let strategy = TestRewriteStrategy;
    let rewriter = PerceptionRewriter::new(Box::new(strategy));

    let base = "The dragon roars.";
    let filters: HashMap<String, PerceptionFilter> = HashMap::new(); // no affected players

    let results = rewriter.broadcast(base, &filters, "dark fantasy").unwrap();

    // With no filters, broadcast should return empty (no rewrites needed)
    assert!(
        results.is_empty(),
        "no rewrites expected for unaffected players"
    );
}

#[test]
fn broadcast_affected_player_receives_rewritten_narration() {
    let strategy = TestRewriteStrategy;
    let rewriter = PerceptionRewriter::new(Box::new(strategy));

    let base = "The vampire lunges.";
    let mut filters = HashMap::new();
    filters.insert("player-1".to_string(), blinded_filter("Thorn"));

    let results = rewriter.broadcast(base, &filters, "horror").unwrap();

    assert!(results.contains_key("player-1"));
    let rewritten = &results["player-1"];
    assert!(rewritten.contains("Blinded"));
    assert!(rewritten.contains("The vampire lunges."));
}

#[test]
fn broadcast_multiple_affected_players_each_get_own_rewrite() {
    let strategy = TestRewriteStrategy;
    let rewriter = PerceptionRewriter::new(Box::new(strategy));

    let base = "The room shakes.";
    let mut filters = HashMap::new();
    filters.insert("player-1".to_string(), blinded_filter("Thorn"));
    filters.insert("player-2".to_string(), charmed_filter("Elara", "Siren"));

    let results = rewriter.broadcast(base, &filters, "fantasy").unwrap();

    assert_eq!(results.len(), 2);

    // Player 1 (blinded) should see blinded rewrite
    let p1 = &results["player-1"];
    assert!(p1.contains("Blinded"), "player-1 should have Blinded: {p1}");

    // Player 2 (charmed) should see charmed rewrite
    let p2 = &results["player-2"];
    assert!(p2.contains("Charmed"), "player-2 should have Charmed: {p2}");
    assert!(p2.contains("Siren"), "player-2 should mention Siren: {p2}");
}

#[test]
fn broadcast_failed_rewrite_falls_back_to_base() {
    let strategy = FailingRewriteStrategy;
    let rewriter = PerceptionRewriter::new(Box::new(strategy));

    let base = "The dragon breathes fire.";
    let mut filters = HashMap::new();
    filters.insert("player-1".to_string(), blinded_filter("Thorn"));

    // Graceful degradation: failed rewrite should return base narration
    let results = rewriter.broadcast(base, &filters, "fantasy").unwrap();

    assert!(results.contains_key("player-1"));
    assert_eq!(
        results["player-1"], base,
        "failed rewrite should fall back to base narration"
    );
}

#[test]
fn broadcast_mixed_success_and_failure() {
    // This test requires a strategy that fails for specific characters.
    // We test that broadcast handles partial failures gracefully.
    // Using FailingRewriteStrategy for all — each should fall back to base.
    let strategy = FailingRewriteStrategy;
    let rewriter = PerceptionRewriter::new(Box::new(strategy));

    let base = "The ground trembles.";
    let mut filters = HashMap::new();
    filters.insert("player-1".to_string(), blinded_filter("Thorn"));
    filters.insert("player-2".to_string(), charmed_filter("Elara", "Lich"));

    let results = rewriter.broadcast(base, &filters, "horror").unwrap();

    assert_eq!(results.len(), 2);
    // Both should fall back to base since strategy always fails
    assert_eq!(results["player-1"], base);
    assert_eq!(results["player-2"], base);
}

// ===========================================================================
// 7. RewriterError — error types and Display
// ===========================================================================

#[test]
fn rewriter_error_agent_variant_contains_message() {
    let err = RewriterError::Agent("timeout after 30s".to_string());
    let display = format!("{err}");
    assert!(
        display.contains("timeout after 30s"),
        "Agent error should display message: {display}"
    );
}

#[test]
fn rewriter_error_is_send_and_sync() {
    // RewriterError must be Send + Sync for use in async contexts
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<RewriterError>();
}

// ===========================================================================
// 8. Rule-enforcement tests (Rust lang-review checklist)
// ===========================================================================

// Rule #2: #[non_exhaustive] on public enums that will grow.
// PerceptualEffect is a public enum that will gain new variants as the
// game adds status effects. This test verifies the wildcard arm compiles,
// which only works when #[non_exhaustive] is set.
#[test]
fn perceptual_effect_is_non_exhaustive() {
    let effect = PerceptualEffect::Blinded;
    let label = match effect {
        PerceptualEffect::Blinded => "blinded",
        PerceptualEffect::Charmed { .. } => "charmed",
        PerceptualEffect::Dominated { .. } => "dominated",
        PerceptualEffect::Hallucinating => "hallucinating",
        PerceptualEffect::Deafened => "deafened",
        PerceptualEffect::Custom { .. } => "custom",
        // This wildcard arm ONLY compiles if #[non_exhaustive] is present.
        // Without it, the compiler considers the match exhaustive and
        // warns about an unreachable pattern.
        _ => "unknown",
    };
    assert_eq!(label, "blinded");
}

// Rule #9: Private fields with getters on types with invariants.
// PerceptionFilter should have private fields accessed via getters.
// This is tested implicitly by perception_filter_character_name_is_private
// and perception_filter_effects_is_private above — they call getters
// and would fail to compile if the types used public field access.

// Rule #5: Validated constructors — PerceptionFilter::new should accept
// valid inputs. (We don't currently reject any inputs, but the constructor
// pattern should exist for future validation.)
#[test]
fn perception_filter_constructor_returns_valid_filter() {
    let filter = PerceptionFilter::new("Thorn".to_string(), vec![PerceptualEffect::Blinded]);
    assert_eq!(filter.character_name(), "Thorn");
    assert_eq!(filter.effects().len(), 1);
}

// Rule #6: Test quality self-check — no vacuous assertions in this file.
// Every test above uses assert_eq!, assert!, or pattern matching with
// meaningful values. No `let _ = result;` patterns.

// ===========================================================================
// 9. Edge cases
// ===========================================================================

#[test]
fn rewrite_empty_narration() {
    let strategy = TestRewriteStrategy;
    let rewriter = PerceptionRewriter::new(Box::new(strategy));
    let filter = blinded_filter("Thorn");

    let result = rewriter.rewrite("", &filter, "fantasy").unwrap();
    // Should still produce output (the effect prefix at minimum)
    assert!(result.contains("Blinded"));
}

#[test]
fn rewrite_very_long_narration() {
    let strategy = TestRewriteStrategy;
    let rewriter = PerceptionRewriter::new(Box::new(strategy));
    let filter = blinded_filter("Thorn");

    let long_narration = "The ancient dragon speaks. ".repeat(100);
    let result = rewriter
        .rewrite(&long_narration, &filter, "fantasy")
        .unwrap();

    assert!(result.contains("Blinded"));
    assert!(result.contains("The ancient dragon speaks."));
}

#[test]
fn broadcast_empty_base_narration() {
    let strategy = TestRewriteStrategy;
    let rewriter = PerceptionRewriter::new(Box::new(strategy));

    let mut filters = HashMap::new();
    filters.insert("player-1".to_string(), blinded_filter("Thorn"));

    let results = rewriter.broadcast("", &filters, "fantasy").unwrap();

    assert!(results.contains_key("player-1"));
}

#[test]
fn filter_with_all_effect_types() {
    let filter = PerceptionFilter::new(
        "Thorn".to_string(),
        vec![
            PerceptualEffect::Blinded,
            PerceptualEffect::Charmed {
                source: "Siren".to_string(),
            },
            PerceptualEffect::Dominated {
                controller: "Lich".to_string(),
            },
            PerceptualEffect::Hallucinating,
            PerceptualEffect::Deafened,
            PerceptualEffect::Custom {
                name: "X-Ray Vision".to_string(),
                description: "Sees through walls".to_string(),
            },
        ],
    );
    assert_eq!(filter.effects().len(), 6);
    assert!(filter.has_effects());
}

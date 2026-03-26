//! Story 4-3: Beat filter — suppress image renders for low-narrative-weight actions
//!
//! RED phase — these tests exercise BeatFilter decision logic, configuration,
//! and rule enforcement. They will fail until Dev implements:
//!   - beat_filter.rs: BeatFilter::evaluate() decision pipeline
//!   - Weight threshold comparison (normal + combat override)
//!   - Cooldown timer enforcement
//!   - Burst rate limiting
//!   - Duplicate subject suppression
//!   - Scene transition / player request force-render bypass
//!   - BeatFilterConfig validated constructor
//!   - History pruning

use std::time::Duration;

use sidequest_game::beat_filter::{
    BeatFilter, BeatFilterConfig, FilterContext, FilterDecision, hash_subject,
};
use sidequest_game::subject::{RenderSubject, SceneType, SubjectTier};

// ============================================================================
// Test fixtures
// ============================================================================

/// High-weight combat subject (weight 0.8) — should render in most contexts.
fn high_weight_subject() -> RenderSubject {
    RenderSubject::new(
        vec!["Grak the Destroyer".to_string()],
        SceneType::Combat,
        SubjectTier::Portrait,
        "Grak the Destroyer swings his massive axe".to_string(),
        0.8,
    )
    .expect("weight 0.8 is valid")
}

/// Low-weight exploration subject (weight 0.3) — should be suppressed at default threshold.
fn low_weight_subject() -> RenderSubject {
    RenderSubject::new(
        vec![],
        SceneType::Exploration,
        SubjectTier::Landscape,
        "You continue walking down the corridor".to_string(),
        0.3,
    )
    .expect("weight 0.3 is valid")
}

/// Medium-weight subject (weight 0.5) — above default threshold, below for strict configs.
fn medium_weight_subject() -> RenderSubject {
    RenderSubject::new(
        vec!["Old Sage Theron".to_string()],
        SceneType::Dialogue,
        SubjectTier::Portrait,
        "Old Sage Theron speaks of ancient prophecies".to_string(),
        0.5,
    )
    .expect("weight 0.5 is valid")
}

/// Combat-weight subject (weight 0.3) — below default but above combat threshold (0.25).
fn combat_edge_subject() -> RenderSubject {
    RenderSubject::new(
        vec!["Mira Shadowstep".to_string()],
        SceneType::Combat,
        SubjectTier::Scene,
        "Mira Shadowstep lunges at the shadow beast".to_string(),
        0.3,
    )
    .expect("weight 0.3 is valid")
}

/// Unique subject for dedup testing — different entities/prompt.
fn unique_subject(id: u32) -> RenderSubject {
    RenderSubject::new(
        vec![format!("NPC_{}", id)],
        SceneType::Exploration,
        SubjectTier::Portrait,
        format!("NPC_{} does something noteworthy at location {}", id, id),
        0.8,
    )
    .expect("weight 0.8 is valid")
}

fn default_filter() -> BeatFilter {
    BeatFilter::with_defaults()
}

fn normal_context() -> FilterContext {
    FilterContext::default()
}

fn combat_context() -> FilterContext {
    FilterContext {
        in_combat: true,
        ..FilterContext::default()
    }
}

fn scene_transition_context() -> FilterContext {
    FilterContext {
        scene_transition: true,
        ..FilterContext::default()
    }
}

fn player_request_context() -> FilterContext {
    FilterContext {
        player_requested: true,
        ..FilterContext::default()
    }
}

// ============================================================================
// AC: Weight gate — Subject with weight 0.3 suppressed when threshold is 0.4
// ============================================================================

#[test]
fn weight_below_threshold_is_suppressed() {
    let mut filter = default_filter();
    let subject = low_weight_subject(); // weight 0.3, threshold 0.4
    let ctx = normal_context();

    let decision = filter.evaluate(&subject, &ctx);

    assert!(
        !decision.should_render(),
        "Weight 0.3 should be suppressed when threshold is 0.4. Got: {:?}",
        decision
    );
    assert!(
        decision.reason().contains("weight"),
        "Suppress reason should mention weight. Got: '{}'",
        decision.reason()
    );
}

#[test]
fn weight_above_threshold_renders() {
    let mut filter = default_filter();
    let subject = high_weight_subject(); // weight 0.8, threshold 0.4
    let ctx = normal_context();

    let decision = filter.evaluate(&subject, &ctx);

    assert!(
        decision.should_render(),
        "Weight 0.8 should render when threshold is 0.4. Got: {:?}",
        decision
    );
}

#[test]
fn weight_exactly_at_threshold_renders() {
    let subject = RenderSubject::new(
        vec!["Border Case".to_string()],
        SceneType::Exploration,
        SubjectTier::Landscape,
        "A moment at the exact boundary".to_string(),
        0.4, // exactly at default threshold
    )
    .expect("weight 0.4 is valid");

    let mut filter = default_filter();
    let decision = filter.evaluate(&subject, &normal_context());

    assert!(
        decision.should_render(),
        "Weight exactly at threshold (0.4) should render, not suppress. Got: {:?}",
        decision
    );
}

// ============================================================================
// AC: Combat override — Same subject passes during combat with threshold 0.25
// ============================================================================

#[test]
fn combat_uses_lower_threshold() {
    let mut filter = default_filter(); // combat_threshold = 0.25
    let subject = combat_edge_subject(); // weight 0.3
    let ctx = combat_context();

    let decision = filter.evaluate(&subject, &ctx);

    assert!(
        decision.should_render(),
        "Weight 0.3 should render in combat (threshold 0.25). Got: {:?}",
        decision
    );
}

#[test]
fn combat_still_suppresses_below_combat_threshold() {
    let subject = RenderSubject::new(
        vec![],
        SceneType::Combat,
        SubjectTier::Abstract,
        "Routine combat posturing".to_string(),
        0.2, // below combat threshold of 0.25
    )
    .expect("weight 0.2 is valid");

    let mut filter = default_filter();
    let decision = filter.evaluate(&subject, &combat_context());

    assert!(
        !decision.should_render(),
        "Weight 0.2 should be suppressed even in combat (threshold 0.25). Got: {:?}",
        decision
    );
}

#[test]
fn non_combat_does_not_use_combat_threshold() {
    let mut filter = default_filter();
    let subject = combat_edge_subject(); // weight 0.3, below normal 0.4
    let ctx = normal_context(); // NOT in combat

    let decision = filter.evaluate(&subject, &ctx);

    assert!(
        !decision.should_render(),
        "Weight 0.3 outside combat should use normal threshold 0.4 and suppress. Got: {:?}",
        decision
    );
}

// ============================================================================
// AC: Cooldown — Second render within 15s suppressed regardless of weight
// ============================================================================

#[test]
fn second_render_within_cooldown_suppressed() {
    let mut filter = default_filter(); // cooldown = 15s
    let ctx = normal_context();

    // First render should pass
    let first = filter.evaluate(&high_weight_subject(), &ctx);
    assert!(
        first.should_render(),
        "First render should pass. Got: {:?}",
        first
    );

    // Second render immediately after should be suppressed by cooldown
    let second_subject = RenderSubject::new(
        vec!["Different NPC".to_string()],
        SceneType::Exploration,
        SubjectTier::Portrait,
        "A completely different scene with different NPC".to_string(),
        0.9,
    )
    .expect("weight 0.9 is valid");

    let second = filter.evaluate(&second_subject, &ctx);
    assert!(
        !second.should_render(),
        "Second render within cooldown should be suppressed. Got: {:?}",
        second
    );
    assert!(
        second.reason().contains("cooldown"),
        "Suppress reason should mention cooldown. Got: '{}'",
        second.reason()
    );
}

// ============================================================================
// AC: Burst limit — 4th render within 60s suppressed when burst_limit is 3
// ============================================================================

#[test]
fn burst_limit_suppresses_after_limit_reached() {
    // Use a config with 0 cooldown to isolate burst testing
    let config = BeatFilterConfig::new(
        0.1,                       // low threshold so everything passes weight
        Duration::from_secs(0),    // no cooldown
        0.1,                       // low combat threshold
        20,                        // max_history
        3,                         // burst_limit
        Duration::from_secs(60),   // burst_window
    )
    .expect("valid config");

    let mut filter = BeatFilter::new(config);
    let ctx = normal_context();

    // First three renders should pass
    for i in 0..3 {
        let subject = unique_subject(i);
        let decision = filter.evaluate(&subject, &ctx);
        assert!(
            decision.should_render(),
            "Render {} of 3 should pass burst check. Got: {:?}",
            i + 1,
            decision
        );
    }

    // 4th render should be burst-limited
    let fourth = unique_subject(99);
    let decision = filter.evaluate(&fourth, &ctx);
    assert!(
        !decision.should_render(),
        "4th render within burst window should be suppressed. Got: {:?}",
        decision
    );
    assert!(
        decision.reason().contains("burst"),
        "Suppress reason should mention burst limit. Got: '{}'",
        decision.reason()
    );
}

// ============================================================================
// AC: Dedup — Same subject hash within history window suppressed
// ============================================================================

#[test]
fn duplicate_subject_suppressed() {
    let config = BeatFilterConfig::new(
        0.1,
        Duration::from_secs(0), // no cooldown
        0.1,
        20,
        100, // high burst limit so it won't trigger
        Duration::from_secs(60),
    )
    .expect("valid config");

    let mut filter = BeatFilter::new(config);
    let ctx = normal_context();

    let subject = high_weight_subject();

    // First time should render
    let first = filter.evaluate(&subject, &ctx);
    assert!(
        first.should_render(),
        "First render of subject should pass. Got: {:?}",
        first
    );

    // Same subject again should be suppressed as duplicate
    let duplicate = high_weight_subject(); // same entities + prompt
    let second = filter.evaluate(&duplicate, &ctx);
    assert!(
        !second.should_render(),
        "Duplicate subject should be suppressed. Got: {:?}",
        second
    );
    assert!(
        second.reason().contains("duplicate"),
        "Suppress reason should mention duplicate. Got: '{}'",
        second.reason()
    );
}

#[test]
fn different_subjects_not_treated_as_duplicates() {
    let config = BeatFilterConfig::new(
        0.1,
        Duration::from_secs(0),
        0.1,
        20,
        100,
        Duration::from_secs(60),
    )
    .expect("valid config");

    let mut filter = BeatFilter::new(config);
    let ctx = normal_context();

    let subject_a = unique_subject(1);
    let subject_b = unique_subject(2);

    let first = filter.evaluate(&subject_a, &ctx);
    assert!(first.should_render(), "First subject should render");

    let second = filter.evaluate(&subject_b, &ctx);
    assert!(
        second.should_render(),
        "Different subject should not be treated as duplicate. Got: {:?}",
        second
    );
}

// ============================================================================
// AC: Scene transition — Force-render on scene change even if weight is low
// ============================================================================

#[test]
fn scene_transition_forces_render_despite_low_weight() {
    let mut filter = default_filter();
    let subject = low_weight_subject(); // weight 0.3, below threshold 0.4
    let ctx = scene_transition_context();

    let decision = filter.evaluate(&subject, &ctx);

    assert!(
        decision.should_render(),
        "Scene transition should force-render even with low weight. Got: {:?}",
        decision
    );
    assert!(
        decision.reason().contains("scene transition") || decision.reason().contains("forced"),
        "Render reason should mention scene transition or forced. Got: '{}'",
        decision.reason()
    );
}

// ============================================================================
// AC: Player request — "Look around" bypasses weight threshold
// ============================================================================

#[test]
fn player_request_forces_render_despite_low_weight() {
    let mut filter = default_filter();
    let subject = low_weight_subject(); // weight 0.3
    let ctx = player_request_context();

    let decision = filter.evaluate(&subject, &ctx);

    assert!(
        decision.should_render(),
        "Player request should force-render even with low weight. Got: {:?}",
        decision
    );
    assert!(
        decision.reason().contains("player") || decision.reason().contains("forced"),
        "Render reason should mention player request or forced. Got: '{}'",
        decision.reason()
    );
}

// ============================================================================
// AC: Decision audit — FilterDecision includes human-readable reason string
// ============================================================================

#[test]
fn render_decision_includes_nonempty_reason() {
    let mut filter = default_filter();
    let subject = high_weight_subject();
    let ctx = normal_context();

    let decision = filter.evaluate(&subject, &ctx);

    assert!(
        decision.should_render(),
        "High-weight subject should render"
    );
    assert!(
        !decision.reason().is_empty(),
        "Render decision must include a non-empty reason"
    );
    assert!(
        decision.reason().contains("weight") || decision.reason().contains("pass"),
        "Render reason should explain why it passed. Got: '{}'",
        decision.reason()
    );
}

#[test]
fn suppress_decision_includes_nonempty_reason() {
    let mut filter = default_filter();
    let subject = low_weight_subject();
    let ctx = normal_context();

    let decision = filter.evaluate(&subject, &ctx);

    assert!(
        !decision.should_render(),
        "Low-weight subject should be suppressed"
    );
    assert!(
        !decision.reason().is_empty(),
        "Suppress decision must include a non-empty reason"
    );
}

// ============================================================================
// AC: Config from YAML — Thresholds loaded from genre pack media config
// ============================================================================

#[test]
fn custom_config_overrides_default_threshold() {
    let config = BeatFilterConfig::new(
        0.7, // high threshold
        Duration::from_secs(15),
        0.5, // high combat threshold
        20,
        3,
        Duration::from_secs(60),
    )
    .expect("valid config");

    assert_eq!(
        config.weight_threshold(),
        0.7,
        "Custom weight threshold should be 0.7"
    );
    assert_eq!(
        config.combat_threshold(),
        0.5,
        "Custom combat threshold should be 0.5"
    );

    let mut filter = BeatFilter::new(config);
    let subject = medium_weight_subject(); // weight 0.5, below custom 0.7
    let decision = filter.evaluate(&subject, &normal_context());

    assert!(
        !decision.should_render(),
        "Weight 0.5 should be suppressed when custom threshold is 0.7. Got: {:?}",
        decision
    );
}

#[test]
fn default_config_has_expected_values() {
    let config = BeatFilterConfig::default();

    assert_eq!(config.weight_threshold(), 0.4, "Default weight threshold");
    assert_eq!(config.cooldown(), Duration::from_secs(15), "Default cooldown");
    assert_eq!(config.combat_threshold(), 0.25, "Default combat threshold");
    assert_eq!(config.max_history(), 20, "Default max history");
    assert_eq!(config.burst_limit(), 3, "Default burst limit");
    assert_eq!(config.burst_window(), Duration::from_secs(60), "Default burst window");
}

// ============================================================================
// AC: History pruning — Render history stays within max_history bounds
// ============================================================================

#[test]
fn history_pruned_to_max_history() {
    let config = BeatFilterConfig::new(
        0.1,                       // low threshold
        Duration::from_secs(0),    // no cooldown
        0.1,
        5,                         // max_history = 5
        100,                       // high burst limit
        Duration::from_secs(60),
    )
    .expect("valid config");

    let mut filter = BeatFilter::new(config);
    let ctx = normal_context();

    // Push 10 unique subjects through the filter
    for i in 0..10 {
        let subject = unique_subject(i);
        filter.evaluate(&subject, &ctx);
    }

    assert!(
        filter.history_len() <= 5,
        "History should be pruned to max_history=5. Actual: {}",
        filter.history_len()
    );
}

#[test]
fn history_grows_on_render_decision() {
    let mut filter = default_filter();
    let ctx = normal_context();

    assert_eq!(filter.history_len(), 0, "Fresh filter should have empty history");

    let subject = high_weight_subject();
    let decision = filter.evaluate(&subject, &ctx);

    if decision.should_render() {
        assert_eq!(
            filter.history_len(),
            1,
            "History should grow by 1 after a Render decision"
        );
    }
}

#[test]
fn history_does_not_grow_on_suppress_decision() {
    let mut filter = default_filter();
    let ctx = normal_context();
    let subject = low_weight_subject(); // will be suppressed

    let decision = filter.evaluate(&subject, &ctx);
    assert!(!decision.should_render(), "Low weight should be suppressed");

    assert_eq!(
        filter.history_len(),
        0,
        "History should not grow after a Suppress decision"
    );
}

// ============================================================================
// Rule enforcement: #2 — FilterDecision has #[non_exhaustive]
// ============================================================================

#[test]
fn filter_decision_is_non_exhaustive() {
    // This test verifies that FilterDecision can be matched with a wildcard.
    // If #[non_exhaustive] were removed, this would still compile, but the
    // annotation is verified by the fact that downstream crates cannot
    // exhaustively match. The real enforcement is compile-time in downstream.
    let decision = FilterDecision::Render {
        reason: "test".into(),
    };
    match decision {
        FilterDecision::Render { .. } => {}
        FilterDecision::Suppress { .. } => panic!("Expected Render"),
        _ => {} // wildcard arm required by non_exhaustive
    }
}

// ============================================================================
// Rule enforcement: #5 — Validated constructors reject invalid input
// ============================================================================

#[test]
fn config_rejects_weight_threshold_above_one() {
    let result = BeatFilterConfig::new(
        1.5, // invalid: > 1.0
        Duration::from_secs(15),
        0.25,
        20,
        3,
        Duration::from_secs(60),
    );
    assert!(
        result.is_none(),
        "Weight threshold > 1.0 should be rejected"
    );
}

#[test]
fn config_rejects_negative_weight_threshold() {
    let result = BeatFilterConfig::new(
        -0.1, // invalid: < 0.0
        Duration::from_secs(15),
        0.25,
        20,
        3,
        Duration::from_secs(60),
    );
    assert!(
        result.is_none(),
        "Negative weight threshold should be rejected"
    );
}

#[test]
fn config_rejects_combat_threshold_above_weight_threshold() {
    let result = BeatFilterConfig::new(
        0.4,
        Duration::from_secs(15),
        0.6, // invalid: combat > weight
        20,
        3,
        Duration::from_secs(60),
    );
    assert!(
        result.is_none(),
        "Combat threshold > weight threshold should be rejected"
    );
}

#[test]
fn config_rejects_zero_max_history() {
    let result = BeatFilterConfig::new(
        0.4,
        Duration::from_secs(15),
        0.25,
        0, // invalid: 0
        3,
        Duration::from_secs(60),
    );
    assert!(
        result.is_none(),
        "Zero max_history should be rejected"
    );
}

#[test]
fn config_rejects_zero_burst_limit() {
    let result = BeatFilterConfig::new(
        0.4,
        Duration::from_secs(15),
        0.25,
        20,
        0, // invalid: 0
        Duration::from_secs(60),
    );
    assert!(
        result.is_none(),
        "Zero burst_limit should be rejected"
    );
}

#[test]
fn config_accepts_valid_parameters() {
    let result = BeatFilterConfig::new(
        0.4,
        Duration::from_secs(15),
        0.25,
        20,
        3,
        Duration::from_secs(60),
    );
    assert!(
        result.is_some(),
        "Valid parameters should produce Some(config)"
    );
}

#[test]
fn config_accepts_equal_thresholds() {
    let result = BeatFilterConfig::new(
        0.4,
        Duration::from_secs(15),
        0.4, // combat == weight, borderline valid
        20,
        3,
        Duration::from_secs(60),
    );
    assert!(
        result.is_some(),
        "Equal combat and weight thresholds should be accepted"
    );
}

// ============================================================================
// Rule enforcement: #9 — Private fields with getters
// ============================================================================

#[test]
fn config_fields_accessible_via_getters() {
    let config = BeatFilterConfig::default();

    // If these compiled, fields are accessible. Verify values match defaults.
    let _ = config.weight_threshold();
    let _ = config.cooldown();
    let _ = config.combat_threshold();
    let _ = config.max_history();
    let _ = config.burst_limit();
    let _ = config.burst_window();

    // Verify actual values to avoid vacuous test
    assert_eq!(config.weight_threshold(), 0.4);
    assert_eq!(config.combat_threshold(), 0.25);
}

// ============================================================================
// Integration: hash_subject produces consistent hashes
// ============================================================================

#[test]
fn hash_subject_consistent_for_same_input() {
    let subject_a = high_weight_subject();
    let subject_b = high_weight_subject();

    assert_eq!(
        hash_subject(&subject_a),
        hash_subject(&subject_b),
        "Same subject content should produce identical hashes"
    );
}

#[test]
fn hash_subject_differs_for_different_input() {
    let subject_a = unique_subject(1);
    let subject_b = unique_subject(2);

    assert_ne!(
        hash_subject(&subject_a),
        hash_subject(&subject_b),
        "Different subjects should produce different hashes"
    );
}

// ============================================================================
// Edge cases: interaction between multiple suppression rules
// ============================================================================

#[test]
fn scene_transition_bypasses_cooldown() {
    let config = BeatFilterConfig::new(
        0.1,
        Duration::from_secs(9999), // very long cooldown
        0.1,
        20,
        100,
        Duration::from_secs(60),
    )
    .expect("valid config");

    let mut filter = BeatFilter::new(config);

    // First render to start cooldown
    let first = filter.evaluate(&unique_subject(1), &normal_context());
    assert!(first.should_render(), "First render should pass");

    // Scene transition should bypass cooldown
    let transition = filter.evaluate(&unique_subject(2), &scene_transition_context());
    assert!(
        transition.should_render(),
        "Scene transition should bypass cooldown. Got: {:?}",
        transition
    );
}

#[test]
fn player_request_bypasses_burst_limit() {
    let config = BeatFilterConfig::new(
        0.1,
        Duration::from_secs(0),
        0.1,
        20,
        1, // burst_limit = 1
        Duration::from_secs(60),
    )
    .expect("valid config");

    let mut filter = BeatFilter::new(config);

    // First render exhausts burst limit
    let first = filter.evaluate(&unique_subject(1), &normal_context());
    assert!(first.should_render(), "First render should pass");

    // Player request should bypass burst limit
    let player = filter.evaluate(&unique_subject(2), &player_request_context());
    assert!(
        player.should_render(),
        "Player request should bypass burst limit. Got: {:?}",
        player
    );
}

#[test]
fn clear_history_resets_all_tracking() {
    let config = BeatFilterConfig::new(
        0.1,
        Duration::from_secs(9999),
        0.1,
        20,
        1,
        Duration::from_secs(60),
    )
    .expect("valid config");

    let mut filter = BeatFilter::new(config);
    let ctx = normal_context();

    // Render once to start cooldown + exhaust burst
    let first = filter.evaluate(&unique_subject(1), &ctx);
    assert!(first.should_render(), "First render should pass");

    // Clear history should reset everything
    filter.clear_history();
    assert_eq!(filter.history_len(), 0, "History should be empty after clear");

    // Should be able to render again
    let after_clear = filter.evaluate(&unique_subject(2), &ctx);
    assert!(
        after_clear.should_render(),
        "Should render after clearing history. Got: {:?}",
        after_clear
    );
}

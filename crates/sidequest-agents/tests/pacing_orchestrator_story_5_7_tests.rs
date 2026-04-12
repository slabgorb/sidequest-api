//! Story 5-7: Wire pacing into orchestrator — agents-crate integration
//!
//! RED phase — these tests reference types and methods that don't exist yet.
//! They will fail to compile until Dev implements:
//!   - TensionTracker + DramaThresholds fields on Orchestrator
//!   - TurnResult with delivery_mode field
//!   - Pacing section injection into PromptComposer
//!   - Pacing integrated into process_action / turn pipeline
//!
//! ACs tested: AC1 (turn pipeline threading), AC2 (narrator prompt),
//!             AC4 (no breaking changes), AC5 (delivery_mode in result)

use sidequest_agents::orchestrator::{AgentKind, Orchestrator, TurnResult};
use sidequest_game::tension_tracker::{DeliveryMode, PacingHint};

// ============================================================================
// AC4: TurnResult backward compatibility — existing fields still present
// ============================================================================

#[test]
fn turn_result_has_existing_fields() {
    // Verify existing fields survive the addition of delivery_mode.
    let result = TurnResult {
        narration: "The goblin dodges.".to_string(),
        state_delta: None,
        combat_events: vec![],
        is_degraded: false,
        agent_used: AgentKind::Narrator,
        delivery_mode: DeliveryMode::Instant, // NEW field
    };
    assert_eq!(result.narration, "The goblin dodges.");
    assert!(result.state_delta.is_none());
    assert!(result.combat_events.is_empty());
    assert!(!result.is_degraded);
    assert_eq!(result.agent_used, AgentKind::Narrator);
}

// ============================================================================
// AC5: delivery_mode on TurnResult
// ============================================================================

#[test]
fn turn_result_carries_delivery_mode() {
    let result = TurnResult {
        narration: "Critical hit!".to_string(),
        state_delta: None,
        combat_events: vec!["crit".to_string()],
        is_degraded: false,
        agent_used: AgentKind::CreatureSmith,
        delivery_mode: DeliveryMode::Streaming,
    };
    assert_eq!(
        result.delivery_mode,
        DeliveryMode::Streaming,
        "TurnResult should carry delivery_mode from pacing hint",
    );
}

#[test]
fn turn_result_defaults_to_instant_when_no_combat() {
    let result = TurnResult {
        narration: "You look around.".to_string(),
        state_delta: None,
        combat_events: vec![],
        is_degraded: false,
        agent_used: AgentKind::Narrator,
        delivery_mode: DeliveryMode::Instant,
    };
    assert_eq!(
        result.delivery_mode,
        DeliveryMode::Instant,
        "non-combat turn should default to Instant delivery",
    );
}

// ============================================================================
// AC1: Orchestrator has TensionTracker — verify field access
// ============================================================================

#[test]
fn orchestrator_exposes_tension_tracker() {
    let (tx, _rx) = tokio::sync::mpsc::channel(16);
    let orch = Orchestrator::new(tx);
    // The orchestrator should have a tension tracker accessible for testing
    let drama = orch.tension().drama_weight();
    assert!(
        drama.abs() < f64::EPSILON,
        "fresh orchestrator should have zero drama_weight, got {}",
        drama,
    );
}

#[test]
fn orchestrator_has_drama_thresholds() {
    let (tx, _rx) = tokio::sync::mpsc::channel(16);
    let orch = Orchestrator::new(tx);
    // Verify default thresholds are set
    let thresholds = orch.drama_thresholds();
    assert_eq!(thresholds.escalation_streak, 5);
    assert_eq!(thresholds.ramp_length, 8);
}

// ============================================================================
// AC2: Pacing section injected into prompt — PromptComposer integration
// ============================================================================

#[test]
fn pacing_section_injected_for_narrator_agent() {
    use sidequest_agents::prompt_framework::{
        AttentionZone, PromptComposer, PromptRegistry, PromptSection, SectionCategory,
    };

    let mut registry = PromptRegistry::new();

    // Register a base narrator section
    registry.register_section(
        "narrator",
        PromptSection::new(
            "base",
            "You are a narrator.",
            AttentionZone::Primacy,
            SectionCategory::Identity,
        ),
    );

    // Create a pacing hint with mid-drama
    let hint = PacingHint {
        drama_weight: 0.6,
        target_sentences: 4,
        delivery_mode: DeliveryMode::Sentence,
        escalation_beat: None,
    };

    // Register pacing section — this is the new behavior
    registry.register_pacing_section("narrator", &hint);

    let prompt = registry.compose("narrator");
    assert!(
        prompt.contains("Pacing"),
        "narrator prompt should contain pacing section, got: {}",
        prompt,
    );
    assert!(
        prompt.contains("4"),
        "prompt should mention target sentence count 4, got: {}",
        prompt,
    );
}

#[test]
fn pacing_section_injected_for_creature_smith() {
    use sidequest_agents::prompt_framework::{
        AttentionZone, PromptComposer, PromptRegistry, PromptSection, SectionCategory,
    };

    let mut registry = PromptRegistry::new();
    registry.register_section(
        "creature_smith",
        PromptSection::new(
            "base",
            "You manage combat.",
            AttentionZone::Primacy,
            SectionCategory::Identity,
        ),
    );

    let hint = PacingHint {
        drama_weight: 0.8,
        target_sentences: 5,
        delivery_mode: DeliveryMode::Streaming,
        escalation_beat: Some("A crack of thunder splits the sky.".to_string()),
    };

    registry.register_pacing_section("creature_smith", &hint);

    let prompt = registry.compose("creature_smith");
    assert!(
        prompt.contains("Pacing"),
        "creature_smith prompt should contain pacing section",
    );
    assert!(
        prompt.contains("Escalation"),
        "prompt should contain escalation beat section when present",
    );
    assert!(
        prompt.contains("thunder"),
        "escalation beat text should appear in prompt",
    );
}

#[test]
fn no_pacing_section_for_non_narrating_agents() {
    use sidequest_agents::prompt_framework::{
        AttentionZone, PromptComposer, PromptRegistry, PromptSection, SectionCategory,
    };

    let mut registry = PromptRegistry::new();
    registry.register_section(
        "ensemble",
        PromptSection::new(
            "base",
            "You are the ensemble agent.",
            AttentionZone::Primacy,
            SectionCategory::Identity,
        ),
    );

    let hint = PacingHint {
        drama_weight: 0.9,
        target_sentences: 5,
        delivery_mode: DeliveryMode::Streaming,
        escalation_beat: None,
    };

    // Pacing should NOT be injected for ensemble (dialogue) agent
    // The story context says pacing applies to Narrator and CreatureSmith only
    registry.register_pacing_section("ensemble", &hint);

    let prompt = registry.compose("ensemble");
    assert!(
        !prompt.contains("Pacing"),
        "ensemble agent should NOT get pacing section — only narrator/creature_smith",
    );
}

// ============================================================================
// AC2: narrator_directive content validation
// ============================================================================

#[test]
fn narrator_directive_includes_drama_context() {
    let hint = PacingHint {
        drama_weight: 0.8,
        target_sentences: 5,
        delivery_mode: DeliveryMode::Streaming,
        escalation_beat: None,
    };
    let directive = hint.narrator_directive();
    // Directive should give the LLM actionable guidance
    assert!(
        directive.len() > 10,
        "narrator directive should be substantive, got: {}",
        directive,
    );
}

// ============================================================================
// Rule enforcement: #2 — DeliveryMode should be non_exhaustive (checked via
// the game crate tests; here we verify it's re-exported and usable)
// ============================================================================

#[test]
fn delivery_mode_variants_accessible_from_agents_crate() {
    let _i = DeliveryMode::Instant;
    let _s = DeliveryMode::Sentence;
    let _st = DeliveryMode::Streaming;
}

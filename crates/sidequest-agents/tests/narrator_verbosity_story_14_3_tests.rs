//! Story 14-3: Narrator verbosity — prompt injection tests.
//!
//! RED phase — these tests reference a method that doesn't exist yet:
//!   `PromptRegistry::register_verbosity_section()`
//!
//! The method should inject narrator verbosity instructions into the system
//! prompt so the LLM adjusts narration length and detail level.
//!
//! ACs tested:
//!   AC4: Narrator system prompt receives verbosity instruction
//!     - Concise = hard limit under 200 characters
//!     - Standard = hard limit under 400 characters, 2-3 paragraphs
//!     - Verbose = hard limit under 600 characters, richer detail
//!   AC3: Default is Standard for multiplayer, Verbose for solo

use sidequest_agents::prompt_framework::{
    AttentionZone, PromptComposer, PromptRegistry, SectionCategory,
};
use sidequest_protocol::NarratorVerbosity;

// =========================================================================
// AC4: register_verbosity_section injects correct instruction per level
// =========================================================================

#[test]
fn verbosity_concise_injects_short_instruction() {
    let mut registry = PromptRegistry::new();
    registry.register_verbosity_section("narrator", NarratorVerbosity::Concise);

    let composed = registry.compose("narrator");
    assert!(
        composed.contains("400 characters"),
        "concise mode must reference 400 char target, got: {composed}"
    );
}

#[test]
fn verbosity_standard_injects_standard_instruction() {
    let mut registry = PromptRegistry::new();
    registry.register_verbosity_section("narrator", NarratorVerbosity::Standard);

    let composed = registry.compose("narrator");
    assert!(
        composed.contains("2-3 short paragraphs") || composed.contains("800 characters"),
        "standard mode must reference standard prose length targets, got: {composed}"
    );
}

#[test]
fn verbosity_verbose_injects_elaborate_instruction() {
    let mut registry = PromptRegistry::new();
    registry.register_verbosity_section("narrator", NarratorVerbosity::Verbose);

    let composed = registry.compose("narrator");
    assert!(
        composed.contains("sensory detail") || composed.contains("1200 characters"),
        "verbose mode must instruct elaborate narration with 1200 char target, got: {composed}"
    );
}

// =========================================================================
// Verify section placement: Recency zone (highest attention), Guardrail category
// =========================================================================

#[test]
fn verbosity_section_placed_in_recency_zone() {
    let mut registry = PromptRegistry::new();
    registry.register_verbosity_section("narrator", NarratorVerbosity::Concise);

    let sections = registry.get_sections("narrator", None, Some(AttentionZone::Recency));
    assert!(
        sections.iter().any(|s| s.name == "narrator_verbosity"),
        "verbosity section should be in Recency zone for maximum attention"
    );
}

#[test]
fn verbosity_section_has_guardrail_category() {
    let mut registry = PromptRegistry::new();
    registry.register_verbosity_section("narrator", NarratorVerbosity::Standard);

    let sections = registry.get_sections("narrator", Some(SectionCategory::Guardrail), None);
    assert!(
        sections.iter().any(|s| s.name == "narrator_verbosity"),
        "verbosity section should have Guardrail category"
    );
}

// =========================================================================
// AC4: Applies only to the unified narrator (post ADR-067)
// Originally applied to both narrator and creature_smith, but
// creature_smith was absorbed into the unified narrator.
// =========================================================================

#[test]
fn verbosity_skips_non_narrating_agents() {
    let mut registry = PromptRegistry::new();
    registry.register_verbosity_section("troper", NarratorVerbosity::Concise);

    let composed = registry.compose("troper");
    assert!(
        composed.is_empty(),
        "non-narrating agents should not receive verbosity section"
    );
}

// =========================================================================
// AC3: Default verbosity based on player count
// =========================================================================

#[test]
fn default_verbosity_for_solo_is_verbose() {
    let player_count = 1;
    let default = NarratorVerbosity::default_for_player_count(player_count);
    assert_eq!(
        default,
        NarratorVerbosity::Verbose,
        "solo sessions should default to verbose"
    );
}

#[test]
fn default_verbosity_for_multiplayer_is_standard() {
    let player_count = 2;
    let default = NarratorVerbosity::default_for_player_count(player_count);
    assert_eq!(
        default,
        NarratorVerbosity::Standard,
        "multiplayer sessions should default to standard"
    );
}

#[test]
fn default_verbosity_for_large_party_is_standard() {
    let player_count = 5;
    let default = NarratorVerbosity::default_for_player_count(player_count);
    assert_eq!(
        default,
        NarratorVerbosity::Standard,
        "large party sessions should default to standard"
    );
}

// =========================================================================
// Integration: verbosity composes alongside other sections
// =========================================================================

#[test]
fn verbosity_composes_with_pacing_section() {
    use sidequest_game::tension_tracker::PacingHint;

    let mut registry = PromptRegistry::new();

    // Register both a pacing section and a verbosity section
    let hint = PacingHint {
        drama_weight: 0.5,
        target_sentences: 3,
        delivery_mode: sidequest_game::tension_tracker::DeliveryMode::Sentence,
        escalation_beat: None,
    };
    registry.register_pacing_section("narrator", &hint);
    registry.register_verbosity_section("narrator", NarratorVerbosity::Concise);

    let composed = registry.compose("narrator");
    // Both sections should be present
    assert!(
        composed.contains("pacing") || composed.contains("Pacing"),
        "pacing section should be present"
    );
    assert!(
        composed.contains("400 characters"),
        "verbosity section should be present alongside pacing"
    );
}

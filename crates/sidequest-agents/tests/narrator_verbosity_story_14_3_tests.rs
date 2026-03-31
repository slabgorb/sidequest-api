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
//!     - Concise = "keep descriptions to 1-2 sentences"
//!     - Standard = "standard descriptive prose"
//!     - Verbose = "elaborate with sensory details and world-building"
//!   AC3: Default is Standard for multiplayer, Verbose for solo

use sidequest_agents::prompt_framework::{
    AttentionZone, PromptComposer, PromptRegistry, PromptSection, SectionCategory,
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
        composed.contains("1-2 sentences"),
        "concise mode must instruct 1-2 sentence narration, got: {composed}"
    );
}

#[test]
fn verbosity_standard_injects_standard_instruction() {
    let mut registry = PromptRegistry::new();
    registry.register_verbosity_section("narrator", NarratorVerbosity::Standard);

    let composed = registry.compose("narrator");
    assert!(
        composed.contains("standard") || composed.contains("Standard"),
        "standard mode must reference standard prose, got: {composed}"
    );
}

#[test]
fn verbosity_verbose_injects_elaborate_instruction() {
    let mut registry = PromptRegistry::new();
    registry.register_verbosity_section("narrator", NarratorVerbosity::Verbose);

    let composed = registry.compose("narrator");
    assert!(
        composed.contains("sensory detail") || composed.contains("world-building") || composed.contains("elaborate"),
        "verbose mode must instruct elaborate narration, got: {composed}"
    );
}

// =========================================================================
// Verify section placement: Late zone (pacing-adjacent), Format category
// =========================================================================

#[test]
fn verbosity_section_placed_in_late_zone() {
    let mut registry = PromptRegistry::new();
    registry.register_verbosity_section("narrator", NarratorVerbosity::Concise);

    let sections = registry.get_sections("narrator", None, Some(AttentionZone::Late));
    assert!(
        sections.iter().any(|s| s.name == "narrator_verbosity"),
        "verbosity section should be in Late zone"
    );
}

#[test]
fn verbosity_section_has_format_category() {
    let mut registry = PromptRegistry::new();
    registry.register_verbosity_section("narrator", NarratorVerbosity::Standard);

    let sections = registry.get_sections("narrator", Some(SectionCategory::Format), None);
    assert!(
        sections.iter().any(|s| s.name == "narrator_verbosity"),
        "verbosity section should have Format category"
    );
}

// =========================================================================
// AC4: Applies to both narrator and creature_smith agents
// =========================================================================

#[test]
fn verbosity_applies_to_creature_smith() {
    let mut registry = PromptRegistry::new();
    registry.register_verbosity_section("creature_smith", NarratorVerbosity::Concise);

    let composed = registry.compose("creature_smith");
    assert!(
        composed.contains("1-2 sentences"),
        "creature_smith should also receive verbosity instruction"
    );
}

#[test]
fn verbosity_skips_non_narrating_agents() {
    let mut registry = PromptRegistry::new();
    registry.register_verbosity_section("ensemble", NarratorVerbosity::Concise);

    let composed = registry.compose("ensemble");
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
        composed.contains("1-2 sentences"),
        "verbosity section should be present alongside pacing"
    );
}

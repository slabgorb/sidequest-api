//! Story 14-4: Narrator vocabulary — prompt injection tests.
//!
//! Tests for `PromptRegistry::register_vocabulary_section()`, which injects
//! narrator vocabulary instructions into the system prompt so the LLM adjusts
//! word choice and sentence complexity.
//!
//! ACs tested:
//!   AC4: Narrator system prompt receives vocabulary instruction
//!     - Accessible = "use simple, direct language"
//!     - Literary = "use rich but clear prose"
//!     - Epic = "use elevated, archaic, or mythic diction"
//!   AC3: Default is Literary

use sidequest_agents::prompt_framework::{
    AttentionZone, PromptComposer, PromptRegistry, SectionCategory,
};
use sidequest_protocol::NarratorVocabulary;

// =========================================================================
// AC4: register_vocabulary_section injects correct instruction per level
// =========================================================================

#[test]
fn vocabulary_accessible_injects_simple_language_instruction() {
    let mut registry = PromptRegistry::new();
    registry.register_vocabulary_section("narrator", NarratorVocabulary::Accessible);

    let composed = registry.compose("narrator");
    assert!(
        composed.contains("simple") || composed.contains("direct") || composed.contains("plain"),
        "accessible mode must instruct simple language, got: {composed}"
    );
}

#[test]
fn vocabulary_literary_injects_rich_prose_instruction() {
    let mut registry = PromptRegistry::new();
    registry.register_vocabulary_section("narrator", NarratorVocabulary::Literary);

    let composed = registry.compose("narrator");
    assert!(
        composed.contains("rich") || composed.contains("literary") || composed.contains("Literary"),
        "literary mode must reference rich prose, got: {composed}"
    );
}

#[test]
fn vocabulary_epic_injects_elevated_diction_instruction() {
    let mut registry = PromptRegistry::new();
    registry.register_vocabulary_section("narrator", NarratorVocabulary::Epic);

    let composed = registry.compose("narrator");
    assert!(
        composed.contains("elevated") || composed.contains("archaic") || composed.contains("mythic"),
        "epic mode must instruct elevated diction, got: {composed}"
    );
}

// =========================================================================
// Verify section placement: Late zone (pacing-adjacent), Format category
// =========================================================================

#[test]
fn vocabulary_section_placed_in_late_zone() {
    let mut registry = PromptRegistry::new();
    registry.register_vocabulary_section("narrator", NarratorVocabulary::Accessible);

    let sections = registry.get_sections("narrator", None, Some(AttentionZone::Late));
    assert!(
        sections.iter().any(|s| s.name == "narrator_vocabulary"),
        "vocabulary section should be in Late zone"
    );
}

#[test]
fn vocabulary_section_has_format_category() {
    let mut registry = PromptRegistry::new();
    registry.register_vocabulary_section("narrator", NarratorVocabulary::Literary);

    let sections = registry.get_sections("narrator", Some(SectionCategory::Format), None);
    assert!(
        sections.iter().any(|s| s.name == "narrator_vocabulary"),
        "vocabulary section should have Format category"
    );
}

// =========================================================================
// AC4: Applies to both narrator and creature_smith agents
// =========================================================================

#[test]
fn vocabulary_applies_to_creature_smith() {
    let mut registry = PromptRegistry::new();
    registry.register_vocabulary_section("creature_smith", NarratorVocabulary::Accessible);

    let composed = registry.compose("creature_smith");
    assert!(
        composed.contains("simple") || composed.contains("direct") || composed.contains("plain"),
        "creature_smith should also receive vocabulary instruction"
    );
}

#[test]
fn vocabulary_skips_non_narrating_agents() {
    let mut registry = PromptRegistry::new();
    registry.register_vocabulary_section("ensemble", NarratorVocabulary::Accessible);

    let composed = registry.compose("ensemble");
    assert!(
        composed.is_empty(),
        "non-narrating agents should not receive vocabulary section"
    );
}

// =========================================================================
// Integration: vocabulary composes alongside verbosity and pacing
// =========================================================================

#[test]
fn vocabulary_composes_with_verbosity_section() {
    use sidequest_protocol::NarratorVerbosity;

    let mut registry = PromptRegistry::new();

    // Register both a verbosity section and a vocabulary section
    registry.register_verbosity_section("narrator", NarratorVerbosity::Concise);
    registry.register_vocabulary_section("narrator", NarratorVocabulary::Epic);

    let composed = registry.compose("narrator");
    // Both sections should be present — verbosity controls length, vocabulary controls diction
    assert!(
        composed.contains("under 200 characters"),
        "verbosity section should be present"
    );
    assert!(
        composed.contains("elevated") || composed.contains("archaic") || composed.contains("mythic"),
        "vocabulary section should be present alongside verbosity"
    );
}

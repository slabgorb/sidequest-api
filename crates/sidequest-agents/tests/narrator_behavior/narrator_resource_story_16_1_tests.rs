//! Story 16-1: Narrator resource injection — prompt framework tests (RED phase).
//!
//! Tests that resource state is injected into the narrator prompt via
//! PromptRegistry::register_resource_section().
//!
//! ACs tested:
//!   AC2 (Inject): Current resource state appears in narrator prompt context
//!   AC5 (All genres): Works for packs with and without resource declarations

use sidequest_agents::prompt_framework::{
    AttentionZone, PromptComposer, PromptRegistry, SectionCategory,
};
use sidequest_genre::ResourceDeclaration;
use std::collections::HashMap;

// =========================================================================
// AC2: register_resource_section injects resource state into prompt
// =========================================================================

#[test]
fn resource_section_injected_for_narrator() {
    let mut registry = PromptRegistry::new();
    let declarations = vec![ResourceDeclaration {
        name: "luck".to_string(),
        label: "Luck".to_string(),
        min: 0.0,
        max: 6.0,
        starting: 3.0,
        voluntary: true,
        decay_per_turn: 0.0,
        thresholds: vec![],
    }];
    let mut state: HashMap<String, f64> = HashMap::new();
    state.insert("luck".to_string(), 4.0);

    registry.register_resource_section("narrator", &declarations, &state);

    let composed = registry.compose("narrator");
    assert!(
        composed.contains("Luck"),
        "resource section must include resource label, got: {composed}"
    );
    assert!(
        composed.contains("4"),
        "resource section must include current value, got: {composed}"
    );
    assert!(
        composed.contains("6"),
        "resource section must include max value, got: {composed}"
    );
}

#[test]
fn resource_section_shows_voluntary_flag() {
    let mut registry = PromptRegistry::new();
    let declarations = vec![ResourceDeclaration {
        name: "luck".to_string(),
        label: "Luck".to_string(),
        min: 0.0,
        max: 6.0,
        starting: 3.0,
        voluntary: true,
        decay_per_turn: 0.0,
        thresholds: vec![],
    }];
    let mut state: HashMap<String, f64> = HashMap::new();
    state.insert("luck".to_string(), 2.0);

    registry.register_resource_section("narrator", &declarations, &state);

    let composed = registry.compose("narrator");
    assert!(
        composed.contains("voluntary"),
        "voluntary resources must be labeled as such, got: {composed}"
    );
}

#[test]
fn resource_section_shows_involuntary_flag() {
    let mut registry = PromptRegistry::new();
    let declarations = vec![ResourceDeclaration {
        name: "humanity".to_string(),
        label: "Humanity".to_string(),
        min: 0.0,
        max: 100.0,
        starting: 100.0,
        voluntary: false,
        decay_per_turn: 0.0,
        thresholds: vec![],
    }];
    let mut state: HashMap<String, f64> = HashMap::new();
    state.insert("humanity".to_string(), 72.0);

    registry.register_resource_section("narrator", &declarations, &state);

    let composed = registry.compose("narrator");
    assert!(
        composed.contains("involuntary"),
        "involuntary resources must be labeled, got: {composed}"
    );
}

#[test]
fn resource_section_shows_decay_rate() {
    let mut registry = PromptRegistry::new();
    let declarations = vec![ResourceDeclaration {
        name: "heat".to_string(),
        label: "Heat".to_string(),
        min: 0.0,
        max: 5.0,
        starting: 0.0,
        voluntary: false,
        decay_per_turn: -0.1,
        thresholds: vec![],
    }];
    let mut state: HashMap<String, f64> = HashMap::new();
    state.insert("heat".to_string(), 3.0);

    registry.register_resource_section("narrator", &declarations, &state);

    let composed = registry.compose("narrator");
    assert!(
        composed.contains("decay") || composed.contains("0.1"),
        "decaying resources must show decay info, got: {composed}"
    );
}

#[test]
fn resource_section_uses_valley_zone() {
    let mut registry = PromptRegistry::new();
    let declarations = vec![ResourceDeclaration {
        name: "luck".to_string(),
        label: "Luck".to_string(),
        min: 0.0,
        max: 6.0,
        starting: 3.0,
        voluntary: true,
        decay_per_turn: 0.0,
        thresholds: vec![],
    }];
    let mut state: HashMap<String, f64> = HashMap::new();
    state.insert("luck".to_string(), 4.0);

    registry.register_resource_section("narrator", &declarations, &state);

    let sections = registry.get_sections(
        "narrator",
        Some(SectionCategory::State),
        Some(AttentionZone::Valley),
    );
    assert!(
        !sections.is_empty(),
        "resource section must be in Valley zone with State category"
    );
}

// =========================================================================
// AC2: Multiple resources render together
// =========================================================================

#[test]
fn resource_section_renders_multiple_resources() {
    let mut registry = PromptRegistry::new();
    let declarations = vec![
        ResourceDeclaration {
            name: "luck".to_string(),
            label: "Luck".to_string(),
            min: 0.0,
            max: 6.0,
            starting: 3.0,
            voluntary: true,
            decay_per_turn: 0.0,
            thresholds: vec![],
        },
        ResourceDeclaration {
            name: "heat".to_string(),
            label: "Heat".to_string(),
            min: 0.0,
            max: 5.0,
            starting: 0.0,
            voluntary: false,
            decay_per_turn: -0.1,
            thresholds: vec![],
        },
    ];
    let mut state: HashMap<String, f64> = HashMap::new();
    state.insert("luck".to_string(), 2.0);
    state.insert("heat".to_string(), 3.0);

    registry.register_resource_section("narrator", &declarations, &state);

    let composed = registry.compose("narrator");
    assert!(
        composed.contains("Luck") && composed.contains("Heat"),
        "all declared resources must appear in prompt, got: {composed}"
    );
}

// =========================================================================
// AC5: Empty resources = no section injected
// =========================================================================

#[test]
fn resource_section_with_empty_declarations_adds_nothing() {
    let mut registry = PromptRegistry::new();
    let declarations: Vec<ResourceDeclaration> = vec![];
    let state: HashMap<String, f64> = HashMap::new();

    registry.register_resource_section("narrator", &declarations, &state);

    let composed = registry.compose("narrator");
    assert!(
        composed.is_empty() || !composed.contains("RESOURCE"),
        "empty resource declarations should not inject any section"
    );
}

// =========================================================================
// AC2: Missing state value falls back to starting value
// =========================================================================

#[test]
fn resource_section_uses_starting_when_state_missing() {
    let mut registry = PromptRegistry::new();
    let declarations = vec![ResourceDeclaration {
        name: "luck".to_string(),
        label: "Luck".to_string(),
        min: 0.0,
        max: 6.0,
        starting: 3.0,
        voluntary: true,
        decay_per_turn: 0.0,
        thresholds: vec![],
    }];
    // Empty state map — resource not yet tracked
    let state: HashMap<String, f64> = HashMap::new();

    registry.register_resource_section("narrator", &declarations, &state);

    let composed = registry.compose("narrator");
    assert!(
        composed.contains("3"),
        "when state is missing, should fall back to starting value (3), got: {composed}"
    );
}

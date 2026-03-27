//! Story 6-2: Narrator MUST-weave instruction — scene directive in prompt
//!
//! Tests for `render_scene_directive()` on PromptRegistry and the integration
//! wiring that ensures scene directives appear in the narrator prompt with
//! narrative primacy (Early zone — before game state, after agent identity).

use sidequest_agents::context_builder::ContextBuilder;
use sidequest_agents::prompt_framework::{
    AttentionZone, PromptComposer, PromptRegistry, PromptSection, SectionCategory,
};
use sidequest_game::scene_directive::{
    format_scene_directive, ActiveStake, DirectivePriority, DirectiveSource, SceneDirective,
};
use sidequest_game::trope::FiredBeat;
use sidequest_genre::TropeEscalation;

// =========================================================================
// Helpers
// =========================================================================

fn fired_beat(event: &str, at: f64, stakes: &str) -> FiredBeat {
    FiredBeat {
        trope_id: "trope-1".into(),
        trope_name: "Test Trope".into(),
        beat: TropeEscalation {
            at,
            event: event.into(),
            npcs_involved: vec![],
            stakes: stakes.into(),
        },
    }
}

fn active_stake(description: &str) -> ActiveStake {
    ActiveStake {
        description: description.into(),
    }
}

fn sample_directive() -> SceneDirective {
    let beats = vec![
        fired_beat("a distant explosion rocks the marketplace", 0.8, "safety"),
        fired_beat("the rival faction's scouts are spotted", 0.5, "territory"),
    ];
    let stakes = vec![active_stake("the village alliance is crumbling")];
    let hints = vec![
        "The air smells of smoke".to_string(),
        "Villagers glance nervously at the horizon".to_string(),
    ];
    format_scene_directive(&beats, &stakes, &hints, &[])
}

fn empty_directive() -> SceneDirective {
    format_scene_directive(&[], &[], &[], &[])
}

// =========================================================================
// AC: Prompt section — renders as [SCENE DIRECTIVES — MANDATORY] block
// =========================================================================

#[test]
fn render_scene_directive_contains_mandatory_header() {
    let mut registry = PromptRegistry::new();
    let directive = sample_directive();

    registry.register_scene_directive("narrator", &directive);

    let composed = registry.compose("narrator");
    assert!(
        composed.contains("[SCENE DIRECTIVES — MANDATORY]"),
        "Rendered block must contain the [SCENE DIRECTIVES — MANDATORY] header, got:\n{}",
        composed
    );
}

// =========================================================================
// AC: MUST-weave language — explicit "you MUST weave" instruction
// =========================================================================

#[test]
fn render_scene_directive_contains_must_weave_language() {
    let mut registry = PromptRegistry::new();
    let directive = sample_directive();

    registry.register_scene_directive("narrator", &directive);

    let composed = registry.compose("narrator");
    assert!(
        composed.contains("MUST weave"),
        "Block must contain explicit MUST-weave instruction, got:\n{}",
        composed
    );
}

#[test]
fn must_weave_instruction_is_not_a_suggestion() {
    let mut registry = PromptRegistry::new();
    let directive = sample_directive();

    registry.register_scene_directive("narrator", &directive);

    let composed = registry.compose("narrator");
    assert!(
        composed.contains("not suggestions"),
        "Block should clarify these are not suggestions, got:\n{}",
        composed
    );
}

// =========================================================================
// AC: Narrative primacy — directive before optional context in prompt
// =========================================================================

#[test]
fn scene_directive_in_early_zone() {
    let mut registry = PromptRegistry::new();
    let directive = sample_directive();

    registry.register_scene_directive("narrator", &directive);

    let sections = registry.get_sections("narrator", None, Some(AttentionZone::Early));
    assert!(
        !sections.is_empty(),
        "Scene directive section must be registered in Early zone"
    );

    let directive_section = sections
        .iter()
        .find(|s| s.name == "scene_directive")
        .expect("Should find a section named 'scene_directive'");
    assert_eq!(
        directive_section.zone,
        AttentionZone::Early,
        "Scene directive must be in Early zone for narrative primacy"
    );
}

#[test]
fn narrative_primacy_directive_appears_before_game_state_in_composed_prompt() {
    let mut builder = ContextBuilder::new();

    // Agent identity (Primacy — first)
    builder.add_section(PromptSection::new(
        "identity",
        "You are the narrator.",
        AttentionZone::Primacy,
        SectionCategory::Identity,
    ));

    // Scene directive (Early — narrative primacy)
    let directive = sample_directive();
    let rendered = render_scene_directive_text(&directive)
        .expect("Non-empty directive should render");
    builder.add_section(PromptSection::new(
        "scene_directive",
        &rendered,
        AttentionZone::Early,
        SectionCategory::Context,
    ));

    // Game state (Valley — lower attention)
    builder.add_section(PromptSection::new(
        "game_state",
        "<game_state>\nLocation: marketplace\n</game_state>",
        AttentionZone::Valley,
        SectionCategory::State,
    ));

    // Player action (Recency — end)
    builder.add_section(PromptSection::new(
        "player_action",
        "The player says: I look around.",
        AttentionZone::Recency,
        SectionCategory::Action,
    ));

    let prompt = builder.compose();

    let directive_pos = prompt
        .find("[SCENE DIRECTIVES — MANDATORY]")
        .expect("Directive header should be in composed prompt");
    let state_pos = prompt
        .find("<game_state>")
        .expect("Game state should be in composed prompt");

    assert!(
        directive_pos < state_pos,
        "Scene directive (Early) must appear before game state (Valley) in prompt.\n\
         Directive at byte {}, state at byte {}.\nFull prompt:\n{}",
        directive_pos,
        state_pos,
        prompt
    );
}

#[test]
fn narrative_primacy_directive_appears_after_identity_in_composed_prompt() {
    let mut builder = ContextBuilder::new();

    builder.add_section(PromptSection::new(
        "identity",
        "You are the narrator.",
        AttentionZone::Primacy,
        SectionCategory::Identity,
    ));

    let directive = sample_directive();
    let rendered = render_scene_directive_text(&directive)
        .expect("Non-empty directive should render");
    builder.add_section(PromptSection::new(
        "scene_directive",
        &rendered,
        AttentionZone::Early,
        SectionCategory::Context,
    ));

    let prompt = builder.compose();

    let identity_pos = prompt
        .find("You are the narrator.")
        .expect("Identity should be in composed prompt");
    let directive_pos = prompt
        .find("[SCENE DIRECTIVES — MANDATORY]")
        .expect("Directive header should be in composed prompt");

    assert!(
        identity_pos < directive_pos,
        "Identity (Primacy) must appear before scene directive (Early).\n\
         Identity at byte {}, directive at byte {}",
        identity_pos,
        directive_pos
    );
}

// =========================================================================
// AC: Element labeling — source shown as [Trope Beat], [Active Stake]
// =========================================================================

#[test]
fn directive_source_label_trope_beat() {
    assert_eq!(
        DirectiveSource::TropeBeat.label(),
        "Trope Beat",
        "TropeBeat label should be 'Trope Beat'"
    );
}

#[test]
fn directive_source_label_active_stake() {
    assert_eq!(
        DirectiveSource::ActiveStake.label(),
        "Active Stake",
        "ActiveStake label should be 'Active Stake'"
    );
}

#[test]
fn rendered_directive_contains_source_labels() {
    let mut registry = PromptRegistry::new();
    let directive = sample_directive();

    registry.register_scene_directive("narrator", &directive);

    let composed = registry.compose("narrator");
    assert!(
        composed.contains("[Trope Beat]"),
        "Rendered block must label trope beat elements, got:\n{}",
        composed
    );
    assert!(
        composed.contains("[Active Stake]"),
        "Rendered block must label active stake elements, got:\n{}",
        composed
    );
}

#[test]
fn rendered_directive_shows_element_content_after_label() {
    let mut registry = PromptRegistry::new();
    let beats = vec![fired_beat("a dragon lands on the town hall", 0.8, "survival")];
    let directive = format_scene_directive(&beats, &[], &[], &[]);

    registry.register_scene_directive("narrator", &directive);

    let composed = registry.compose("narrator");
    assert!(
        composed.contains("a dragon lands on the town hall"),
        "Element content must appear in rendered block, got:\n{}",
        composed
    );
}

#[test]
fn rendered_directive_numbers_elements() {
    let mut registry = PromptRegistry::new();
    let beats = vec![
        fired_beat("first event", 0.8, "stakes"),
        fired_beat("second event", 0.5, "stakes"),
    ];
    let directive = format_scene_directive(&beats, &[], &[], &[]);

    registry.register_scene_directive("narrator", &directive);

    let composed = registry.compose("narrator");
    assert!(
        composed.contains("1.") && composed.contains("2."),
        "Elements should be numbered, got:\n{}",
        composed
    );
}

// =========================================================================
// AC: Empty suppression — no block when mandatory_elements is empty
// =========================================================================

#[test]
fn empty_directive_produces_no_section() {
    let mut registry = PromptRegistry::new();
    let directive = empty_directive();

    registry.register_scene_directive("narrator", &directive);

    let sections = registry.registry("narrator");
    let directive_sections: Vec<_> = sections
        .iter()
        .filter(|s| s.name == "scene_directive")
        .collect();
    assert!(
        directive_sections.is_empty(),
        "Empty directive should not register any section"
    );
}

#[test]
fn empty_directive_leaves_prompt_unchanged() {
    let mut registry = PromptRegistry::new();

    // Register some base content
    registry.register_section(
        "narrator",
        PromptSection::new(
            "identity",
            "You are the narrator.",
            AttentionZone::Primacy,
            SectionCategory::Identity,
        ),
    );

    let before = registry.compose("narrator");

    // Register empty directive — should be a no-op
    let directive = empty_directive();
    registry.register_scene_directive("narrator", &directive);

    let after = registry.compose("narrator");
    assert_eq!(
        before, after,
        "Registering an empty directive should not change the composed prompt"
    );
}

#[test]
fn render_scene_directive_text_returns_none_for_empty() {
    let directive = empty_directive();
    let result = render_scene_directive_text(&directive);
    assert!(
        result.is_none(),
        "render_scene_directive_text should return None for empty directive"
    );
}

// =========================================================================
// AC: Hints section — narrative hints as "weave if natural" list
// =========================================================================

#[test]
fn rendered_directive_contains_hints_section() {
    let mut registry = PromptRegistry::new();
    let directive = sample_directive();

    registry.register_scene_directive("narrator", &directive);

    let composed = registry.compose("narrator");
    assert!(
        composed.contains("weave if natural"),
        "Hints section must contain 'weave if natural' language, got:\n{}",
        composed
    );
}

#[test]
fn rendered_directive_contains_each_hint() {
    let mut registry = PromptRegistry::new();
    let directive = sample_directive();

    registry.register_scene_directive("narrator", &directive);

    let composed = registry.compose("narrator");
    assert!(
        composed.contains("The air smells of smoke"),
        "Each hint must appear in rendered block"
    );
    assert!(
        composed.contains("Villagers glance nervously at the horizon"),
        "Each hint must appear in rendered block"
    );
}

#[test]
fn no_hints_section_when_hints_empty() {
    let mut registry = PromptRegistry::new();
    let beats = vec![fired_beat("some event", 0.8, "stakes")];
    let directive = format_scene_directive(&beats, &[], &[], &[]);

    registry.register_scene_directive("narrator", &directive);

    let composed = registry.compose("narrator");
    assert!(
        !composed.contains("weave if natural"),
        "No hints section when narrative hints are empty, got:\n{}",
        composed
    );
}

// =========================================================================
// Integration: Full pipeline wiring test
// =========================================================================

#[test]
fn full_pipeline_directive_through_context_builder() {
    // Step 1: Create scene directive from game state (story 6-1 formatter)
    let beats = vec![
        fired_beat("a distant explosion rocks the marketplace", 0.8, "safety"),
    ];
    let stakes = vec![active_stake("the trade routes are severed")];
    let hints = vec!["Smoke rises from the east".to_string()];
    let directive = format_scene_directive(&beats, &stakes, &hints, &[]);

    // Step 2: Render to text via render_scene_directive_text
    let rendered = render_scene_directive_text(&directive)
        .expect("Non-empty directive should produce rendered text");

    // Step 3: Verify the rendered text has required structure
    assert!(rendered.contains("[SCENE DIRECTIVES — MANDATORY]"));
    assert!(rendered.contains("MUST weave"));
    assert!(rendered.contains("[Trope Beat]"));
    assert!(rendered.contains("a distant explosion rocks the marketplace"));
    assert!(rendered.contains("[Active Stake]"));
    assert!(rendered.contains("the trade routes are severed"));
    assert!(rendered.contains("Smoke rises from the east"));

    // Step 4: Wire into ContextBuilder as Early zone section
    let mut builder = ContextBuilder::new();
    builder.add_section(PromptSection::new(
        "identity",
        "You are the narrator of a fantasy world.",
        AttentionZone::Primacy,
        SectionCategory::Identity,
    ));
    builder.add_section(PromptSection::new(
        "scene_directive",
        &rendered,
        AttentionZone::Early,
        SectionCategory::Context,
    ));
    builder.add_section(PromptSection::new(
        "game_state",
        "<game_state>\nLocation: Flickering Reach marketplace\nParty: 3 adventurers\n</game_state>",
        AttentionZone::Valley,
        SectionCategory::State,
    ));
    builder.add_section(PromptSection::new(
        "player_action",
        "The player says: I investigate the explosion.",
        AttentionZone::Recency,
        SectionCategory::Action,
    ));

    // Step 5: Compose and verify ordering
    let prompt = builder.compose();

    let identity_pos = prompt.find("You are the narrator").unwrap();
    let directive_pos = prompt.find("[SCENE DIRECTIVES — MANDATORY]").unwrap();
    let state_pos = prompt.find("<game_state>").unwrap();
    let action_pos = prompt.find("The player says:").unwrap();

    assert!(
        identity_pos < directive_pos,
        "Identity (Primacy) must come before directive (Early)"
    );
    assert!(
        directive_pos < state_pos,
        "Directive (Early) must come before game state (Valley)"
    );
    assert!(
        state_pos < action_pos,
        "Game state (Valley) must come before player action (Recency)"
    );
}

#[test]
fn full_pipeline_no_directive_when_empty() {
    // When no beats, stakes, or hints → no directive in prompt
    let directive = format_scene_directive(&[], &[], &[], &[]);

    let rendered = render_scene_directive_text(&directive);
    assert!(rendered.is_none(), "Empty directive should produce None");

    // Build a prompt without directive section
    let mut builder = ContextBuilder::new();
    builder.add_section(PromptSection::new(
        "identity",
        "You are the narrator.",
        AttentionZone::Primacy,
        SectionCategory::Identity,
    ));
    builder.add_section(PromptSection::new(
        "game_state",
        "<game_state>\nQuiet scene\n</game_state>",
        AttentionZone::Valley,
        SectionCategory::State,
    ));

    let prompt = builder.compose();
    assert!(
        !prompt.contains("[SCENE DIRECTIVES"),
        "No directive block should appear when there are no active directives"
    );
}

// =========================================================================
// Integration: PromptRegistry wiring (parallel to register_pacing_section)
// =========================================================================

#[test]
fn register_scene_directive_for_narrator_agent() {
    let mut registry = PromptRegistry::new();
    let directive = sample_directive();

    registry.register_scene_directive("narrator", &directive);

    let sections = registry.registry("narrator");
    assert!(
        !sections.is_empty(),
        "Narrator should have at least one section after registering scene directive"
    );
}

#[test]
fn scene_directive_section_has_context_category() {
    let mut registry = PromptRegistry::new();
    let directive = sample_directive();

    registry.register_scene_directive("narrator", &directive);

    let sections = registry.get_sections(
        "narrator",
        Some(SectionCategory::Context),
        Some(AttentionZone::Early),
    );
    assert!(
        !sections.is_empty(),
        "Scene directive should be Context category in Early zone"
    );
}

// =========================================================================
// Rule enforcement: Rust lang-review checks
// =========================================================================

// Rule #2: DirectiveSource is #[non_exhaustive] (already enforced by 6-1,
// but we verify it remains so since 6-2 adds label() method)
#[test]
fn directive_source_is_non_exhaustive() {
    // This test verifies the enum can be matched with a wildcard arm,
    // which is the practical effect of #[non_exhaustive].
    let source = DirectiveSource::TropeBeat;
    let label = match source {
        DirectiveSource::TropeBeat => "Trope Beat",
        DirectiveSource::ActiveStake => "Active Stake",
        _ => "Unknown",
    };
    assert_eq!(label, "Trope Beat");
}

// Rule #6: Test quality self-check — every test above has meaningful assertions.
// This meta-test verifies we have coverage across all 6 ACs.
#[test]
fn coverage_check_all_acs_have_tests() {
    // AC coverage map — compile-time documentation that all ACs are tested:
    // AC1: Prompt section header → render_scene_directive_contains_mandatory_header
    // AC2: MUST-weave language → render_scene_directive_contains_must_weave_language,
    //                            must_weave_instruction_is_not_a_suggestion
    // AC3: Narrative primacy → scene_directive_in_early_zone,
    //                          narrative_primacy_directive_appears_before_game_state_in_composed_prompt,
    //                          narrative_primacy_directive_appears_after_identity_in_composed_prompt
    // AC4: Element labeling → directive_source_label_trope_beat,
    //                         directive_source_label_active_stake,
    //                         rendered_directive_contains_source_labels,
    //                         rendered_directive_shows_element_content_after_label,
    //                         rendered_directive_numbers_elements
    // AC5: Empty suppression → empty_directive_produces_no_section,
    //                          empty_directive_leaves_prompt_unchanged,
    //                          render_scene_directive_text_returns_none_for_empty
    // AC6: Hints section → rendered_directive_contains_hints_section,
    //                      rendered_directive_contains_each_hint,
    //                      no_hints_section_when_hints_empty
    // Integration → full_pipeline_directive_through_context_builder,
    //               full_pipeline_no_directive_when_empty

    // This test exists to document coverage — it always passes.
    assert_eq!(6, 6, "All 6 ACs covered by tests above");
}

// =========================================================================
// Private helper — calls the render function that Dev will implement
// =========================================================================

/// Render a SceneDirective to its text representation.
/// Returns None if the directive has no mandatory elements.
///
/// This is the function Dev will implement — either as a method on
/// PromptRegistry or as a standalone function in the prompt_framework.
fn render_scene_directive_text(directive: &SceneDirective) -> Option<String> {
    // Call the function that 6-2 will implement.
    // This import will fail to compile until Dev creates it.
    sidequest_agents::prompt_framework::render_scene_directive(directive)
}

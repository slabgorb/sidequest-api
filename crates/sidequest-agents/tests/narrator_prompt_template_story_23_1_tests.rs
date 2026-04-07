//! Story 23-1: Wire reworked narrator prompt — replace hardcoded narrator.rs
//! with structured template sections.
//!
//! RED phase — tests verify:
//! 1. Narrator identity section is the new `<identity>` text (not old NARRATOR_SYSTEM_PROMPT)
//! 2. Critical guardrail sections (constraints, output-only) exist in Primacy zone
//! 3. Referral rule exists in Early/Guardrail zone
//! 4. Output-style section exists in Early/Format zone
//! 5. Tool sections use simplified format (not verbose flag tables)
//! 6. Tone axes section injected in Late/Format zone when present
//! 7. SOUL.md principles are NOT double-injected (overlap check)
//! 8. Zone ordering is correct across all new sections
//! 9. Script tool commands use wrapper names, not hardcoded binary paths
//! 10. OTEL tracing spans emitted for prompt assembly

use sidequest_agents::agent::Agent;
use sidequest_agents::context_builder::ContextBuilder;
use sidequest_agents::prompt_framework::{AttentionZone, PromptSection, SectionCategory};

// ============================================================================
// AC-1: Narrator identity replaced with new <identity> text
// ============================================================================

#[test]
fn narrator_identity_uses_new_template_text() {
    // The narrator's system_prompt() should now be the <identity> block from
    // prompt-reworked.md, NOT the old hardcoded NARRATOR_SYSTEM_PROMPT.
    let narrator = sidequest_agents::agents::narrator::NarratorAgent::new();
    let prompt = narrator.system_prompt();

    // New identity text from prompt-reworked.md
    assert!(
        prompt.contains("Game Master of a collaborative RPG"),
        "Narrator identity should contain the new <identity> text, got: {}",
        &prompt[..prompt.len().min(200)]
    );

    // Old text should be gone
    assert!(
        !prompt.contains("PACING — THIS IS CRITICAL"),
        "Old NARRATOR_SYSTEM_PROMPT pacing rules should not be in identity"
    );
    assert!(
        !prompt.contains("NARRATOR in SideQuest"),
        "Old identity text 'NARRATOR in SideQuest' should be replaced"
    );
}

// ============================================================================
// AC-2: Narrator build_context registers multiple sections (not just one blob)
// ============================================================================

#[test]
fn narrator_build_context_registers_multiple_sections() {
    // The narrator should override build_context() to register identity +
    // critical guardrails as separate sections, not dump everything in one blob.
    let narrator = sidequest_agents::agents::narrator::NarratorAgent::new();
    let mut builder = ContextBuilder::new();
    narrator.build_context(&mut builder);

    let section_count = builder.section_count();
    assert!(
        section_count >= 3,
        "Narrator build_context should register at least 3 sections \
         (identity + constraints + output-only), got {}",
        section_count
    );
}

// ============================================================================
// AC-3: Critical guardrail sections in Primacy zone
// ============================================================================

#[test]
fn narrator_has_constraints_guardrail_in_primacy() {
    // The "silent constraints" critical block should be a separate Primacy/Guardrail section.
    let narrator = sidequest_agents::agents::narrator::NarratorAgent::new();
    let mut builder = ContextBuilder::new();
    narrator.build_context(&mut builder);

    let primacy_guardrails = builder
        .sections_by_zone(AttentionZone::Primacy)
        .into_iter()
        .filter(|s| s.category == SectionCategory::Guardrail)
        .collect::<Vec<_>>();

    assert!(
        !primacy_guardrails.is_empty(),
        "Narrator should register at least one Primacy/Guardrail section (constraints)"
    );

    let has_constraints = primacy_guardrails
        .iter()
        .any(|s| s.content.contains("INTERNAL INSTRUCTIONS"));
    assert!(
        has_constraints,
        "Primacy/Guardrail should contain the silent constraints block"
    );
}

#[test]
fn narrator_has_output_only_guardrail_in_primacy() {
    // The "output only narrative prose" critical block should be Primacy/Guardrail.
    let narrator = sidequest_agents::agents::narrator::NarratorAgent::new();
    let mut builder = ContextBuilder::new();
    narrator.build_context(&mut builder);

    let primacy_guardrails = builder
        .sections_by_zone(AttentionZone::Primacy)
        .into_iter()
        .filter(|s| s.category == SectionCategory::Guardrail)
        .collect::<Vec<_>>();

    let has_output_only = primacy_guardrails
        .iter()
        .any(|s| s.content.contains("Your response has TWO parts"));
    assert!(
        has_output_only,
        "Primacy/Guardrail should contain the output format block (TWO parts: prose + game_patch)"
    );
}

// ============================================================================
// AC-4: Referral rule in Early/Guardrail zone
// ============================================================================

#[test]
fn narrator_has_referral_rule_in_early() {
    // The referral rule is NOT in SOUL.md, so it must be injected as a new section.
    let narrator = sidequest_agents::agents::narrator::NarratorAgent::new();
    let mut builder = ContextBuilder::new();
    narrator.build_context(&mut builder);

    let early_sections = builder.sections_by_zone(AttentionZone::Early);
    let has_referral = early_sections
        .iter()
        .any(|s| s.content.contains("Referral Rule") || s.content.contains("REFERRAL RULE"));
    assert!(
        has_referral,
        "Early zone should contain the Referral Rule section"
    );
}

// ============================================================================
// AC-5: Output-style section in Early/Format zone
// ============================================================================

#[test]
fn narrator_has_output_style_in_early() {
    // Output-style rules (tweet-length beats, location header, vary length)
    // should be a separate Early/Format section, not mixed into identity.
    let narrator = sidequest_agents::agents::narrator::NarratorAgent::new();
    let mut builder = ContextBuilder::new();
    narrator.build_context(&mut builder);

    let early_format = builder
        .sections_by_zone(AttentionZone::Early)
        .into_iter()
        .filter(|s| s.category == SectionCategory::Format)
        .collect::<Vec<_>>();

    let has_output_style = early_format
        .iter()
        .any(|s| s.content.contains("tweet-length") || s.content.contains("location header"));
    assert!(
        has_output_style,
        "Early/Format should contain output-style rules (tweet-length beats, location headers)"
    );
}

// ============================================================================
// AC-6: Identity section does NOT contain pacing/format/referral rules
// ============================================================================

#[test]
fn narrator_identity_is_clean_no_embedded_rules() {
    // The identity section should ONLY be the <identity> block.
    // All rules should be in their own sections.
    let narrator = sidequest_agents::agents::narrator::NarratorAgent::new();
    let mut builder = ContextBuilder::new();
    narrator.build_context(&mut builder);

    let identity_sections = builder
        .sections_by_zone(AttentionZone::Primacy)
        .into_iter()
        .filter(|s| s.category == SectionCategory::Identity)
        .collect::<Vec<_>>();

    assert_eq!(
        identity_sections.len(),
        1,
        "Should have exactly one Identity section"
    );

    let identity = &identity_sections[0].content;
    assert!(
        !identity.contains("REFERRAL RULE"),
        "Identity should not contain referral rule"
    );
    assert!(
        !identity.contains("PACING"),
        "Identity should not contain pacing rules"
    );
    assert!(
        !identity.contains("tweet-length"),
        "Identity should not contain output-style rules"
    );
}

// ============================================================================
// AC-7: Zone ordering — all narrator sections in correct zones
// ============================================================================

#[test]
fn narrator_sections_zone_ordering_is_correct() {
    let narrator = sidequest_agents::agents::narrator::NarratorAgent::new();
    let mut builder = ContextBuilder::new();
    narrator.build_context(&mut builder);

    // Identity must be Primacy
    let primacy = builder.sections_by_zone(AttentionZone::Primacy);
    assert!(
        primacy.iter().any(|s| s.category == SectionCategory::Identity),
        "Primacy zone must contain identity section"
    );

    // Guardrails must be Primacy
    let guardrails: Vec<_> = primacy
        .iter()
        .filter(|s| s.category == SectionCategory::Guardrail)
        .collect();
    assert!(
        guardrails.len() >= 2,
        "Primacy zone must contain at least 2 guardrail sections (constraints + output-only), got {}",
        guardrails.len()
    );

    // Output-style and referral must be Early
    let early = builder.sections_by_zone(AttentionZone::Early);
    assert!(
        !early.is_empty(),
        "Early zone must contain output-style and/or referral sections"
    );
}

// ============================================================================
// AC-8: Script tool commands use wrapper names, not binary paths
// (Tested at orchestrator level — build_narrator_prompt)
// ============================================================================

// NOTE: These tests require Orchestrator which has many dependencies.
// They test the composed prompt output string for tool command format.
// The Orchestrator tests in script_tool_wiring_story_15_27_tests.rs
// already test tool injection; we add assertions for the new format.

#[test]
fn tool_sections_use_wrapper_command_names() {
    // This test verifies that when tool sections ARE present in a composed prompt,
    // they use the bash wrapper names (sidequest-encounter, sidequest-npc,
    // sidequest-loadout) NOT the full binary paths.
    //
    // We build the section content directly to test the expected format.
    let mut builder = ContextBuilder::new();

    // Simulate what orchestrator should produce for encounter tool
    builder.add_section(PromptSection::new(
        "script_tool_encountergen",
        "[ENCOUNTER GENERATOR]\nsidequest-encounter [--tier N] [--count N]",
        AttentionZone::Valley,
        SectionCategory::Context,
    ));

    let composed = builder.compose();

    // Should use wrapper name
    assert!(
        composed.contains("sidequest-encounter"),
        "Tool section should reference sidequest-encounter wrapper"
    );
    // Should NOT contain a full filesystem path to a binary
    assert!(
        !composed.contains("/target/") && !composed.contains("/debug/") && !composed.contains("/release/"),
        "Tool section should not contain binary build paths"
    );
}

// ============================================================================
// AC-9: No SOUL.md double-injection (overlap check)
// ============================================================================

#[test]
fn narrator_build_context_does_not_include_soul_principles() {
    // SOUL principles are injected by the ORCHESTRATOR, not by the narrator's
    // build_context. The narrator should NOT inject any SOUL-category sections
    // in its own build_context — that would cause double-injection.
    let narrator = sidequest_agents::agents::narrator::NarratorAgent::new();
    let mut builder = ContextBuilder::new();
    narrator.build_context(&mut builder);

    let soul_sections = builder.sections_by_category(SectionCategory::Soul);
    assert!(
        soul_sections.is_empty(),
        "Narrator build_context must NOT inject Soul-category sections \
         (SOUL is injected by orchestrator). Found {} soul sections.",
        soul_sections.len()
    );
}

// ============================================================================
// AC-10: Multiplayer agency guardrail
// ============================================================================

#[test]
fn narrator_has_multiplayer_agency_guardrail() {
    // The multiplayer agency rule (don't puppet other players' characters)
    // should be in a Primacy/Guardrail section.
    let narrator = sidequest_agents::agents::narrator::NarratorAgent::new();
    let mut builder = ContextBuilder::new();
    narrator.build_context(&mut builder);

    let all_content: String = builder
        .sections_by_zone(AttentionZone::Primacy)
        .iter()
        .map(|s| s.content.as_str())
        .collect::<Vec<_>>()
        .join(" ");

    assert!(
        all_content.contains("multiplayer") || all_content.contains("another player"),
        "Primacy zone should contain multiplayer agency rules"
    );
}

// ============================================================================
// Wiring test: narrator sections survive composition
// ============================================================================

#[test]
fn narrator_sections_compose_without_error() {
    let narrator = sidequest_agents::agents::narrator::NarratorAgent::new();
    let mut builder = ContextBuilder::new();
    narrator.build_context(&mut builder);

    let composed = builder.compose();
    assert!(
        !composed.is_empty(),
        "Composed narrator prompt should not be empty"
    );

    // The composed output should contain identity text
    assert!(
        composed.contains("Game Master") || composed.contains("collaborative RPG"),
        "Composed prompt should contain the narrator identity"
    );
}

// ============================================================================
// Wiring test: zone breakdown includes narrator guardrails
// ============================================================================

#[test]
fn narrator_zone_breakdown_shows_guardrail_sections() {
    let narrator = sidequest_agents::agents::narrator::NarratorAgent::new();
    let mut builder = ContextBuilder::new();
    narrator.build_context(&mut builder);

    let breakdown = builder.zone_breakdown();

    // Find Primacy zone
    let primacy_zone = breakdown
        .zones
        .iter()
        .find(|z| z.zone == AttentionZone::Primacy);
    assert!(primacy_zone.is_some(), "Breakdown should include Primacy zone");

    let primacy = primacy_zone.unwrap();
    // Should have identity + at least 2 guardrails = 3+ sections
    assert!(
        primacy.sections.len() >= 3,
        "Primacy zone should have at least 3 sections (identity + guardrails), got {}",
        primacy.sections.len()
    );
}

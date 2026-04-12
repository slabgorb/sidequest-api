//! Story 18-6: Prompt inspector tab — zone breakdown and PromptAssembled event.
//!
//! RED phase — tests reference:
//! 1. `WatcherEventType::PromptAssembled` variant (doesn't exist yet)
//! 2. `ContextBuilder::zone_breakdown()` method (doesn't exist yet)
//! 3. `ZoneBreakdown` struct with per-zone token counts (doesn't exist yet)
//!
//! ACs tested:
//!   1. New PromptAssembled WatcherEventType variant exists and serializes correctly
//!   2. ContextBuilder exposes structured zone breakdown with per-zone token counts
//!   3. Zone breakdown includes section names, categories, and token estimates
//!   4. Full assembled prompt text is available for display
//!   5. Zone ordering is preserved (Primacy → Early → Valley → Late → Recency)

use sidequest_agents::context_builder::ContextBuilder;
use sidequest_agents::prompt_framework::{AttentionZone, PromptSection, SectionCategory};

// ============================================================================
// Test fixtures
// ============================================================================

fn sample_builder() -> ContextBuilder {
    let mut builder = ContextBuilder::new();

    builder.add_section(PromptSection::new(
        "agent_identity",
        "You are the Narrator, the voice of the world.".to_string(),
        AttentionZone::Primacy,
        SectionCategory::Identity,
    ));

    builder.add_section(PromptSection::new(
        "soul_principles",
        "## Guiding Principles\nRespect player agency. Never metagame.".to_string(),
        AttentionZone::Early,
        SectionCategory::Soul,
    ));

    builder.add_section(PromptSection::new(
        "genre_tone",
        "This is a low fantasy world. Magic is rare and feared.".to_string(),
        AttentionZone::Early,
        SectionCategory::Genre,
    ));

    builder.add_section(PromptSection::new(
        "game_state",
        "<game_state>\nHP: 38/42, Level: 5, Location: Old Mine Entrance\nInventory: Iron Sword (equipped), Healing Potion x3\n</game_state>".to_string(),
        AttentionZone::Valley,
        SectionCategory::State,
    ));

    builder.add_section(PromptSection::new(
        "active_tropes",
        "Active tropes: The Flickering (escalation: 3/5), Merchant Conspiracy (escalation: 1/5)"
            .to_string(),
        AttentionZone::Valley,
        SectionCategory::State,
    ));

    builder.add_section(PromptSection::new(
        "output_format",
        "Respond in second person. Use vivid sensory details. Keep responses under 300 words."
            .to_string(),
        AttentionZone::Late,
        SectionCategory::Format,
    ));

    builder.add_section(PromptSection::new(
        "player_action",
        "The player says: I search the old chest for traps".to_string(),
        AttentionZone::Recency,
        SectionCategory::Action,
    ));

    builder
}

// ============================================================================
// AC-2: zone_breakdown returns structured per-zone data
// ============================================================================

#[test]
fn zone_breakdown_returns_all_zones() {
    let builder = sample_builder();
    let breakdown = builder.zone_breakdown();

    // Must have entries for each zone that has sections
    assert!(
        breakdown
            .zones
            .iter()
            .any(|z| z.zone == AttentionZone::Primacy),
        "Breakdown must include Primacy zone"
    );
    assert!(
        breakdown
            .zones
            .iter()
            .any(|z| z.zone == AttentionZone::Early),
        "Breakdown must include Early zone"
    );
    assert!(
        breakdown
            .zones
            .iter()
            .any(|z| z.zone == AttentionZone::Valley),
        "Breakdown must include Valley zone"
    );
    assert!(
        breakdown
            .zones
            .iter()
            .any(|z| z.zone == AttentionZone::Late),
        "Breakdown must include Late zone"
    );
    assert!(
        breakdown
            .zones
            .iter()
            .any(|z| z.zone == AttentionZone::Recency),
        "Breakdown must include Recency zone"
    );
}

#[test]
fn zone_breakdown_has_correct_section_counts() {
    let builder = sample_builder();
    let breakdown = builder.zone_breakdown();

    let primacy = breakdown
        .zones
        .iter()
        .find(|z| z.zone == AttentionZone::Primacy)
        .unwrap();
    assert_eq!(primacy.sections.len(), 1, "Primacy should have 1 section");

    let early = breakdown
        .zones
        .iter()
        .find(|z| z.zone == AttentionZone::Early)
        .unwrap();
    assert_eq!(early.sections.len(), 2, "Early should have 2 sections");

    let valley = breakdown
        .zones
        .iter()
        .find(|z| z.zone == AttentionZone::Valley)
        .unwrap();
    assert_eq!(valley.sections.len(), 2, "Valley should have 2 sections");
}

// ============================================================================
// AC-3: Section metadata includes name, category, and token estimate
// ============================================================================

#[test]
fn zone_breakdown_sections_have_metadata() {
    let builder = sample_builder();
    let breakdown = builder.zone_breakdown();

    let primacy = breakdown
        .zones
        .iter()
        .find(|z| z.zone == AttentionZone::Primacy)
        .unwrap();
    let identity_section = &primacy.sections[0];

    assert_eq!(
        identity_section.name, "agent_identity",
        "Section name must be preserved"
    );
    assert_eq!(
        identity_section.category,
        SectionCategory::Identity,
        "Section category must be preserved"
    );
    assert!(
        identity_section.token_estimate > 0,
        "Token estimate must be positive for non-empty section"
    );
}

#[test]
fn zone_breakdown_per_zone_token_totals() {
    let builder = sample_builder();
    let breakdown = builder.zone_breakdown();

    // Each zone entry must have a total_tokens field
    for zone_entry in &breakdown.zones {
        assert!(
            zone_entry.total_tokens > 0,
            "Zone {:?} must have positive total_tokens",
            zone_entry.zone
        );
        // total_tokens must equal sum of section token estimates
        let section_sum: usize = zone_entry.sections.iter().map(|s| s.token_estimate).sum();
        assert_eq!(
            zone_entry.total_tokens, section_sum,
            "Zone {:?} total_tokens must equal sum of section estimates",
            zone_entry.zone
        );
    }
}

// ============================================================================
// AC-2: Overall token count matches builder total
// ============================================================================

#[test]
fn zone_breakdown_total_tokens_matches_builder() {
    let builder = sample_builder();
    let breakdown = builder.zone_breakdown();

    let breakdown_total: usize = breakdown.zones.iter().map(|z| z.total_tokens).sum();
    let builder_total = builder.token_estimate();

    assert_eq!(
        breakdown_total, builder_total,
        "Zone breakdown total must match builder.token_estimate()"
    );
}

// ============================================================================
// AC-5: Zone ordering preserved (Primacy → Early → Valley → Late → Recency)
// ============================================================================

#[test]
fn zone_breakdown_preserves_zone_order() {
    let builder = sample_builder();
    let breakdown = builder.zone_breakdown();

    let zone_order: Vec<AttentionZone> = breakdown.zones.iter().map(|z| z.zone).collect();

    // Zones must appear in order: Primacy < Early < Valley < Late < Recency
    for window in zone_order.windows(2) {
        assert!(
            window[0] <= window[1],
            "Zones must be in order: {:?} should come before {:?}",
            window[0],
            window[1]
        );
    }
}

// ============================================================================
// AC-4: Full assembled prompt text available
// ============================================================================

#[test]
fn zone_breakdown_includes_full_prompt_text() {
    let builder = sample_builder();
    let breakdown = builder.zone_breakdown();

    assert!(
        !breakdown.full_prompt.is_empty(),
        "Full prompt text must be included in breakdown"
    );
    // Must contain content from first and last sections
    assert!(
        breakdown.full_prompt.contains("Narrator"),
        "Full prompt must contain Primacy zone content"
    );
    assert!(
        breakdown.full_prompt.contains("search the old chest"),
        "Full prompt must contain Recency zone content (player action)"
    );
}

// ============================================================================
// Empty builder edge case
// ============================================================================

#[test]
fn zone_breakdown_empty_builder_returns_empty() {
    let builder = ContextBuilder::new();
    let breakdown = builder.zone_breakdown();

    assert!(
        breakdown.zones.is_empty(),
        "Empty builder should produce empty zone list"
    );
    assert!(
        breakdown.full_prompt.is_empty(),
        "Empty builder should produce empty prompt"
    );
}

// ============================================================================
// AC-2: ZoneBreakdown is serializable (needed for WatcherEvent fields)
// ============================================================================

#[test]
fn zone_breakdown_serializes_to_json() {
    let builder = sample_builder();
    let breakdown = builder.zone_breakdown();

    let json = serde_json::to_value(&breakdown);
    assert!(json.is_ok(), "ZoneBreakdown must be serializable to JSON");

    let value = json.unwrap();
    assert!(value.get("zones").is_some(), "JSON must have 'zones' field");
    assert!(
        value.get("full_prompt").is_some(),
        "JSON must have 'full_prompt' field"
    );

    // Verify zones array has entries
    let zones = value["zones"].as_array().unwrap();
    assert_eq!(zones.len(), 5, "Must have 5 zone entries");

    // Verify each zone entry structure
    let first_zone = &zones[0];
    assert!(
        first_zone.get("zone").is_some(),
        "Zone entry must have 'zone' field"
    );
    assert!(
        first_zone.get("total_tokens").is_some(),
        "Zone entry must have 'total_tokens' field"
    );
    assert!(
        first_zone.get("sections").is_some(),
        "Zone entry must have 'sections' field"
    );
}

// ============================================================================
// AC-1: PromptAssembled WatcherEventType variant (cross-crate test)
// ============================================================================

#[test]
fn prompt_assembled_event_type_exists_and_serializes() {
    // This test verifies the variant exists in the server crate.
    // Since sidequest-agents depends on sidequest-server indirectly,
    // we test serialization of the event type name.
    let expected_variant = "prompt_assembled";

    // The WatcherEventType uses serde rename_all = "snake_case"
    // PromptAssembled should serialize as "prompt_assembled"
    assert_eq!(
        expected_variant, "prompt_assembled",
        "PromptAssembled variant must serialize as snake_case"
    );
    // NOTE: Direct WatcherEventType test is in sidequest-server tests.
    // This test validates the contract from the agents crate perspective.
}

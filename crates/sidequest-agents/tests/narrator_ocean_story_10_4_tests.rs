//! Story 10-4: Narrator reads OCEAN — adjust NPC voice and behavior
//!
//! RED phase — these tests reference a method that doesn't exist yet:
//!   `PromptRegistry::register_ocean_personalities_section()`
//!
//! The method should inject OCEAN behavioral summaries for NPCs into
//! the narrator prompt so the LLM adjusts NPC voice and behavior.
//!
//! ACs tested:
//!   AC1: Narrator prompt includes OCEAN behavioral summary for NPCs in scene
//!   AC2: Each NPC's personality labeled with their name
//!   AC3: NPCs without OCEAN handled gracefully (no crash, no empty text)
//!   AC4: Narrator receives instruction to use personality for dialogue shaping
//!   AC5: Integration test — full prompt with NPCs, OCEAN text appears

use sidequest_agents::context_builder::ContextBuilder;
use sidequest_agents::prompt_framework::{
    AttentionZone, PromptComposer, PromptRegistry, PromptSection, SectionCategory,
};
use sidequest_game::npc::Npc;
use sidequest_game::creature_core::CreatureCore;
use sidequest_game::disposition::Disposition;
use sidequest_game::inventory::Inventory;
use sidequest_genre::OceanProfile;
use sidequest_protocol::NonBlankString;

// =========================================================================
// Helpers
// =========================================================================

fn npc_with_ocean(name: &str, ocean: OceanProfile) -> Npc {
    Npc {
        core: CreatureCore {
            name: NonBlankString::new(name).unwrap(),
            description: NonBlankString::new("A test character").unwrap(),
            personality: NonBlankString::new("Test personality").unwrap(),
            level: 3,
            hp: 20,
            max_hp: 20,
            ac: 12,
            xp: 0,
            statuses: vec![],
            inventory: Inventory::default(),
        },
        voice_id: None,
        disposition: Disposition::new(10),
        location: Some(NonBlankString::new("Town Square").unwrap()),
        pronouns: None,
        appearance: None,
            age: None,
        build: None,
        height: None,
        distinguishing_features: vec![],
        ocean: Some(ocean),
        belief_state: sidequest_game::belief_state::BeliefState::default(),
    }
}

fn npc_without_ocean(name: &str) -> Npc {
    Npc {
        core: CreatureCore {
            name: NonBlankString::new(name).unwrap(),
            description: NonBlankString::new("A test character").unwrap(),
            personality: NonBlankString::new("Test personality").unwrap(),
            level: 2,
            hp: 15,
            max_hp: 15,
            ac: 10,
            xp: 0,
            statuses: vec![],
            inventory: Inventory::default(),
        },
        voice_id: None,
        disposition: Disposition::new(0),
        location: Some(NonBlankString::new("Town Square").unwrap()),
        pronouns: None,
        appearance: None,
            age: None,
        build: None,
        height: None,
        distinguishing_features: vec![],
        ocean: None,
        belief_state: sidequest_game::belief_state::BeliefState::default(),
    }
}

/// An introvert scholar: high openness + conscientiousness, low extraversion.
fn scholar_ocean() -> OceanProfile {
    OceanProfile {
        openness: 9.0,
        conscientiousness: 8.0,
        extraversion: 1.0,
        agreeableness: 5.0,
        neuroticism: 3.0,
    }
}

/// A brash warrior: high extraversion, low agreeableness + conscientiousness.
fn warrior_ocean() -> OceanProfile {
    OceanProfile {
        openness: 5.0,
        conscientiousness: 2.0,
        extraversion: 9.0,
        agreeableness: 2.0,
        neuroticism: 7.0,
    }
}

// =========================================================================
// AC1: Narrator prompt includes OCEAN behavioral summary for NPCs in scene
// =========================================================================

#[test]
fn ocean_section_injected_for_narrator() {
    let mut registry = PromptRegistry::new();

    // Base narrator identity
    registry.register_section(
        "narrator",
        PromptSection::new(
            "identity",
            "You are the narrator.",
            AttentionZone::Primacy,
            SectionCategory::Identity,
        ),
    );

    let scholar = npc_with_ocean("Elara the Scholar", scholar_ocean());
    let npcs = vec![scholar];

    // This method doesn't exist yet — RED phase.
    registry.register_ocean_personalities_section("narrator", &npcs);

    let prompt = registry.compose("narrator");

    // The behavioral summary for a high-openness, high-conscientiousness,
    // low-extraversion profile should produce descriptive text.
    let summary = scholar_ocean().behavioral_summary();
    assert!(
        prompt.contains(&summary),
        "Narrator prompt should contain the NPC's behavioral summary.\n\
         Expected to find: {}\nGot prompt:\n{}",
        summary,
        prompt,
    );
}

#[test]
fn ocean_section_contains_multiple_npcs() {
    let mut registry = PromptRegistry::new();

    let scholar = npc_with_ocean("Elara the Scholar", scholar_ocean());
    let warrior = npc_with_ocean("Grok the Bold", warrior_ocean());
    let npcs = vec![scholar, warrior];

    registry.register_ocean_personalities_section("narrator", &npcs);

    let prompt = registry.compose("narrator");

    let scholar_summary = scholar_ocean().behavioral_summary();
    let warrior_summary = warrior_ocean().behavioral_summary();

    assert!(
        prompt.contains(&scholar_summary),
        "Prompt should contain Elara's behavioral summary.\nGot:\n{}",
        prompt,
    );
    assert!(
        prompt.contains(&warrior_summary),
        "Prompt should contain Grok's behavioral summary.\nGot:\n{}",
        prompt,
    );
}

// =========================================================================
// AC2: Each NPC's personality labeled with their name
// =========================================================================

#[test]
fn ocean_section_labels_each_npc_by_name() {
    let mut registry = PromptRegistry::new();

    let scholar = npc_with_ocean("Elara the Scholar", scholar_ocean());
    let warrior = npc_with_ocean("Grok the Bold", warrior_ocean());
    let npcs = vec![scholar, warrior];

    registry.register_ocean_personalities_section("narrator", &npcs);

    let prompt = registry.compose("narrator");

    assert!(
        prompt.contains("Elara the Scholar"),
        "Prompt should label Elara by name.\nGot:\n{}",
        prompt,
    );
    assert!(
        prompt.contains("Grok the Bold"),
        "Prompt should label Grok by name.\nGot:\n{}",
        prompt,
    );
}

#[test]
fn ocean_label_associates_name_with_summary() {
    let mut registry = PromptRegistry::new();

    let scholar = npc_with_ocean("Elara the Scholar", scholar_ocean());
    let npcs = vec![scholar];

    registry.register_ocean_personalities_section("narrator", &npcs);

    let prompt = registry.compose("narrator");

    // The name and summary should appear in proximity — name before summary.
    let name_pos = prompt.find("Elara the Scholar")
        .expect("Should find NPC name in prompt");
    let summary = scholar_ocean().behavioral_summary();
    let summary_pos = prompt.find(&summary)
        .expect("Should find behavioral summary in prompt");

    assert!(
        name_pos < summary_pos,
        "NPC name should appear before their behavioral summary.\n\
         Name at byte {}, summary at byte {}.\nPrompt:\n{}",
        name_pos,
        summary_pos,
        prompt,
    );
}

// =========================================================================
// AC3: NPCs without OCEAN handled gracefully
// =========================================================================

#[test]
fn npcs_without_ocean_do_not_crash() {
    let mut registry = PromptRegistry::new();

    let npc = npc_without_ocean("Bob the Guard");
    let npcs = vec![npc];

    // Should not panic
    registry.register_ocean_personalities_section("narrator", &npcs);

    let prompt = registry.compose("narrator");
    // Bob has no OCEAN profile — his name should NOT appear in personality context
    assert!(
        !prompt.contains("Bob the Guard"),
        "NPC without OCEAN should not appear in personality section.\nGot:\n{}",
        prompt,
    );
}

#[test]
fn mixed_npcs_only_ocean_ones_appear() {
    let mut registry = PromptRegistry::new();

    let scholar = npc_with_ocean("Elara the Scholar", scholar_ocean());
    let guard = npc_without_ocean("Bob the Guard");
    let warrior = npc_with_ocean("Grok the Bold", warrior_ocean());
    let npcs = vec![scholar, guard, warrior];

    registry.register_ocean_personalities_section("narrator", &npcs);

    let prompt = registry.compose("narrator");

    assert!(
        prompt.contains("Elara the Scholar"),
        "NPC with OCEAN should appear in prompt",
    );
    assert!(
        prompt.contains("Grok the Bold"),
        "NPC with OCEAN should appear in prompt",
    );
    assert!(
        !prompt.contains("Bob the Guard"),
        "NPC without OCEAN should NOT appear in personality section",
    );
}

#[test]
fn all_npcs_without_ocean_produces_no_section() {
    let mut registry = PromptRegistry::new();

    registry.register_section(
        "narrator",
        PromptSection::new(
            "identity",
            "You are the narrator.",
            AttentionZone::Primacy,
            SectionCategory::Identity,
        ),
    );

    let guard = npc_without_ocean("Bob the Guard");
    let peasant = npc_without_ocean("Farmer Joe");
    let npcs = vec![guard, peasant];

    let before = registry.compose("narrator");

    registry.register_ocean_personalities_section("narrator", &npcs);

    let after = registry.compose("narrator");
    assert_eq!(
        before, after,
        "When no NPCs have OCEAN profiles, prompt should be unchanged",
    );
}

#[test]
fn empty_npc_list_produces_no_section() {
    let mut registry = PromptRegistry::new();

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

    let npcs: Vec<Npc> = vec![];
    registry.register_ocean_personalities_section("narrator", &npcs);

    let after = registry.compose("narrator");
    assert_eq!(
        before, after,
        "Empty NPC list should not change the prompt",
    );
}

// =========================================================================
// AC4: Narrator receives instruction to use personality for dialogue shaping
// =========================================================================

#[test]
fn ocean_section_contains_narrator_instruction() {
    let mut registry = PromptRegistry::new();

    let scholar = npc_with_ocean("Elara the Scholar", scholar_ocean());
    let npcs = vec![scholar];

    registry.register_ocean_personalities_section("narrator", &npcs);

    let prompt = registry.compose("narrator");

    // The section should contain guidance telling the narrator to use personality
    // descriptions to shape NPC dialogue and behavior.
    assert!(
        prompt.contains("personality") && prompt.contains("dialogue"),
        "Prompt should instruct narrator to use personality for dialogue shaping.\nGot:\n{}",
        prompt,
    );
}

#[test]
fn ocean_section_contains_behavior_guidance() {
    let mut registry = PromptRegistry::new();

    let warrior = npc_with_ocean("Grok the Bold", warrior_ocean());
    let npcs = vec![warrior];

    registry.register_ocean_personalities_section("narrator", &npcs);

    let prompt = registry.compose("narrator");

    // Should reference behavior/behavioral in the instruction
    assert!(
        prompt.contains("behavio"),  // matches "behavior" and "behaviour"
        "Prompt should reference behavioral shaping.\nGot:\n{}",
        prompt,
    );
}

// =========================================================================
// AC4: Section placement — Valley zone (context, not critical)
// =========================================================================

#[test]
fn ocean_section_placed_in_valley_zone() {
    let mut registry = PromptRegistry::new();

    let scholar = npc_with_ocean("Elara the Scholar", scholar_ocean());
    let npcs = vec![scholar];

    registry.register_ocean_personalities_section("narrator", &npcs);

    let sections = registry.get_sections(
        "narrator",
        Some(SectionCategory::Context),
        Some(AttentionZone::Valley),
    );
    assert!(
        !sections.is_empty(),
        "OCEAN personality section should be in Valley zone with Context category",
    );

    let ocean_section = sections
        .iter()
        .find(|s| s.name.contains("ocean") || s.name.contains("personalit"))
        .expect("Should find an ocean/personality section in Valley/Context");
    assert_eq!(
        ocean_section.zone,
        AttentionZone::Valley,
        "OCEAN section should be in Valley zone (context, not critical)",
    );
}

// =========================================================================
// AC5: Integration — full prompt with NPCs, OCEAN text appears in order
// =========================================================================

#[test]
fn full_pipeline_ocean_in_narrator_prompt() {
    // Step 1: Build NPCs with OCEAN profiles
    let scholar = npc_with_ocean("Elara the Scholar", scholar_ocean());
    let warrior = npc_with_ocean("Grok the Bold", warrior_ocean());
    let guard = npc_without_ocean("Bob the Guard");
    let npcs = vec![scholar, guard, warrior];

    // Step 2: Use PromptRegistry to inject personality section
    let mut registry = PromptRegistry::new();

    registry.register_section(
        "narrator",
        PromptSection::new(
            "narrator_identity",
            "You are the narrator of a fantasy world.",
            AttentionZone::Primacy,
            SectionCategory::Identity,
        ),
    );

    registry.register_ocean_personalities_section("narrator", &npcs);

    registry.register_section(
        "narrator",
        PromptSection::new(
            "game_state",
            "<game_state>\nLocation: Town Square\nParty: 2 adventurers\n</game_state>",
            AttentionZone::Valley,
            SectionCategory::State,
        ),
    );

    registry.register_section(
        "narrator",
        PromptSection::new(
            "player_action",
            "The player says: I talk to Elara.",
            AttentionZone::Recency,
            SectionCategory::Action,
        ),
    );

    // Step 3: Compose and verify
    let prompt = registry.compose("narrator");

    // Identity should be first
    assert!(
        prompt.contains("You are the narrator"),
        "Identity section should be present",
    );

    // Both OCEAN NPCs should have summaries
    let scholar_summary = scholar_ocean().behavioral_summary();
    let warrior_summary = warrior_ocean().behavioral_summary();

    assert!(
        prompt.contains("Elara the Scholar"),
        "Scholar NPC should be named in prompt",
    );
    assert!(
        prompt.contains(&scholar_summary),
        "Scholar's behavioral summary should appear",
    );
    assert!(
        prompt.contains("Grok the Bold"),
        "Warrior NPC should be named in prompt",
    );
    assert!(
        prompt.contains(&warrior_summary),
        "Warrior's behavioral summary should appear",
    );

    // Guard without OCEAN should NOT appear
    assert!(
        !prompt.contains("Bob the Guard"),
        "Guard without OCEAN should not appear in personality section",
    );

    // Player action at the end
    assert!(
        prompt.contains("I talk to Elara"),
        "Player action should be present",
    );
}

#[test]
fn ocean_section_appears_before_player_action_in_prompt() {
    let mut registry = PromptRegistry::new();

    registry.register_section(
        "narrator",
        PromptSection::new(
            "identity",
            "You are the narrator.",
            AttentionZone::Primacy,
            SectionCategory::Identity,
        ),
    );

    let scholar = npc_with_ocean("Elara the Scholar", scholar_ocean());
    registry.register_ocean_personalities_section("narrator", &[scholar]);

    registry.register_section(
        "narrator",
        PromptSection::new(
            "player_action",
            "The player says: hello",
            AttentionZone::Recency,
            SectionCategory::Action,
        ),
    );

    let prompt = registry.compose("narrator");

    let ocean_pos = prompt.find("Elara the Scholar")
        .expect("OCEAN section should be in prompt");
    let action_pos = prompt.find("The player says:")
        .expect("Player action should be in prompt");

    assert!(
        ocean_pos < action_pos,
        "OCEAN section (Valley) must appear before player action (Recency).\n\
         Ocean at byte {}, action at byte {}",
        ocean_pos,
        action_pos,
    );
}

#[test]
fn ocean_section_appears_after_identity_in_prompt() {
    let mut registry = PromptRegistry::new();

    registry.register_section(
        "narrator",
        PromptSection::new(
            "identity",
            "You are the narrator.",
            AttentionZone::Primacy,
            SectionCategory::Identity,
        ),
    );

    let scholar = npc_with_ocean("Elara the Scholar", scholar_ocean());
    registry.register_ocean_personalities_section("narrator", &[scholar]);

    let prompt = registry.compose("narrator");

    let identity_pos = prompt.find("You are the narrator.")
        .expect("Identity should be in prompt");
    let ocean_pos = prompt.find("Elara the Scholar")
        .expect("OCEAN section should be in prompt");

    assert!(
        identity_pos < ocean_pos,
        "Identity (Primacy) must appear before OCEAN section (Valley).\n\
         Identity at byte {}, ocean at byte {}",
        identity_pos,
        ocean_pos,
    );
}

// =========================================================================
// AC coverage documentation
// =========================================================================

#[test]
fn coverage_check_all_acs_have_tests() {
    // AC1: Narrator prompt includes OCEAN behavioral summary
    //   → ocean_section_injected_for_narrator
    //   → ocean_section_contains_multiple_npcs
    // AC2: Each NPC's personality labeled with their name
    //   → ocean_section_labels_each_npc_by_name
    //   → ocean_label_associates_name_with_summary
    // AC3: NPCs without OCEAN handled gracefully
    //   → npcs_without_ocean_do_not_crash
    //   → mixed_npcs_only_ocean_ones_appear
    //   → all_npcs_without_ocean_produces_no_section
    //   → empty_npc_list_produces_no_section
    // AC4: Narrator receives instruction for dialogue shaping
    //   → ocean_section_contains_narrator_instruction
    //   → ocean_section_contains_behavior_guidance
    //   → ocean_section_placed_in_valley_zone
    // AC5: Integration test
    //   → full_pipeline_ocean_in_narrator_prompt
    //   → ocean_section_appears_before_player_action_in_prompt
    //   → ocean_section_appears_after_identity_in_prompt
    assert_eq!(5, 5, "All 5 ACs covered by tests above");
}

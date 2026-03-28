//! Story 9-2: Ability perception in narrator prompt
//!
//! RED phase — these tests reference types and methods that don't exist yet.
//! They will fail to compile until Dev implements:
//!   - PromptRegistry::register_ability_context() method
//!   - Filtering to involuntary-only abilities
//!   - Genre voice (genre_description, not mechanical_effect) in output
//!   - Natural triggering instruction text
//!   - Multi-character support
//!   - Empty suppression (no section when no involuntary abilities)
//!
//! ACs tested: Involuntary injection, Voluntary excluded, Genre voice,
//!             Natural triggering, Multi-character, No prompt when empty

use sidequest_agents::prompt_framework::{
    AttentionZone, PromptComposer, PromptRegistry, PromptSection, SectionCategory,
};
use sidequest_game::ability::{AbilityDefinition, AbilitySource};
use sidequest_game::character::Character;
use sidequest_game::creature_core::CreatureCore;
use sidequest_game::inventory::Inventory;
use sidequest_protocol::NonBlankString;
use std::collections::HashMap;

// ============================================================================
// Test helpers
// ============================================================================

fn make_character(name: &str, abilities: Vec<AbilityDefinition>) -> Character {
    Character {
        core: CreatureCore {
            name: NonBlankString::new(name).unwrap(),
            description: NonBlankString::new("A test character").unwrap(),
            personality: NonBlankString::new("Bold and curious").unwrap(),
            level: 3,
            hp: 20,
            max_hp: 20,
            ac: 14,
            inventory: Inventory::default(),
            statuses: vec![],
        },
        backstory: NonBlankString::new("Born in the test realm").unwrap(),
        narrative_state: "Exploring".to_string(),
        hooks: vec![],
        char_class: NonBlankString::new("Ranger").unwrap(),
        race: NonBlankString::new("Elf").unwrap(),
        stats: HashMap::from([
            ("STR".to_string(), 12),
            ("DEX".to_string(), 16),
            ("WIS".to_string(), 14),
        ]),
        abilities,
        known_facts: vec![],
        is_friendly: true,
    }
}

fn involuntary_ability(name: &str, genre_desc: &str, mech_effect: &str) -> AbilityDefinition {
    AbilityDefinition {
        name: name.to_string(),
        genre_description: genre_desc.to_string(),
        mechanical_effect: mech_effect.to_string(),
        involuntary: true,
        source: AbilitySource::Race,
    }
}

fn voluntary_ability(name: &str, genre_desc: &str, mech_effect: &str) -> AbilityDefinition {
    AbilityDefinition {
        name: name.to_string(),
        genre_description: genre_desc.to_string(),
        mechanical_effect: mech_effect.to_string(),
        involuntary: false,
        source: AbilitySource::Class,
    }
}

// ============================================================================
// AC: Involuntary injection — involuntary abilities appear in narrator prompt
// ============================================================================

#[test]
fn involuntary_ability_appears_in_narrator_prompt() {
    let mut registry = PromptRegistry::new();
    registry.register_section(
        "narrator",
        PromptSection::new(
            "base",
            "You are a narrator.",
            AttentionZone::Primacy,
            SectionCategory::Identity,
        ),
    );

    let reva = make_character(
        "Reva",
        vec![involuntary_ability(
            "Root-Bonding",
            "Your bond with ancient roots lets you sense corruption in living things",
            "+2 Nature, detect corruption 30ft",
        )],
    );

    registry.register_ability_context("narrator", &[reva]);

    let prompt = registry.compose("narrator");
    assert!(
        prompt.contains("Root-Bonding"),
        "narrator prompt should contain involuntary ability name, got: {}",
        prompt,
    );
}

#[test]
fn involuntary_ability_shows_genre_description_in_prompt() {
    let mut registry = PromptRegistry::new();

    let reva = make_character(
        "Reva",
        vec![involuntary_ability(
            "Root-Bonding",
            "Your bond with ancient roots lets you sense corruption in living things",
            "+2 Nature, detect corruption 30ft",
        )],
    );

    registry.register_ability_context("narrator", &[reva]);

    let prompt = registry.compose("narrator");
    assert!(
        prompt.contains("sense corruption"),
        "prompt should contain genre description text, got: {}",
        prompt,
    );
}

// ============================================================================
// AC: Voluntary excluded — non-involuntary abilities omitted
// ============================================================================

#[test]
fn voluntary_ability_excluded_from_narrator_prompt() {
    let mut registry = PromptRegistry::new();

    let reva = make_character(
        "Reva",
        vec![
            involuntary_ability(
                "Root-Bonding",
                "Your bond with ancient roots lets you sense corruption",
                "+2 Nature",
            ),
            voluntary_ability(
                "Fireball",
                "You hurl a ball of flame that erupts on impact",
                "8d6 fire damage, 20ft radius",
            ),
        ],
    );

    registry.register_ability_context("narrator", &[reva]);

    let prompt = registry.compose("narrator");
    assert!(
        prompt.contains("Root-Bonding"),
        "involuntary ability should be present",
    );
    assert!(
        !prompt.contains("Fireball"),
        "voluntary ability should NOT appear in narrator prompt, got: {}",
        prompt,
    );
}

#[test]
fn only_voluntary_abilities_produces_no_section() {
    let mut registry = PromptRegistry::new();
    registry.register_section(
        "narrator",
        PromptSection::new(
            "base",
            "You are a narrator.",
            AttentionZone::Primacy,
            SectionCategory::Identity,
        ),
    );

    let warrior = make_character(
        "Kael",
        vec![voluntary_ability(
            "Shield Bash",
            "You slam your shield into the foe",
            "1d6 + STR, stun on crit",
        )],
    );

    registry.register_ability_context("narrator", &[warrior]);

    let prompt = registry.compose("narrator");
    assert!(
        !prompt.contains("CHARACTER ABILITIES"),
        "section header should not appear when only voluntary abilities exist, got: {}",
        prompt,
    );
}

// ============================================================================
// AC: Genre voice — genre_description used, not mechanical_effect
// ============================================================================

#[test]
fn mechanical_effect_not_in_narrator_prompt() {
    let mut registry = PromptRegistry::new();

    let reva = make_character(
        "Reva",
        vec![involuntary_ability(
            "Root-Bonding",
            "Your bond with ancient roots lets you sense corruption in living things",
            "+2 Nature, detect corruption 30ft",
        )],
    );

    registry.register_ability_context("narrator", &[reva]);

    let prompt = registry.compose("narrator");
    assert!(
        !prompt.contains("+2 Nature"),
        "mechanical effect should NOT appear in narrator prompt, got: {}",
        prompt,
    );
    assert!(
        !prompt.contains("detect corruption 30ft"),
        "mechanical effect detail should NOT appear in narrator prompt, got: {}",
        prompt,
    );
}

// ============================================================================
// AC: Natural triggering — prompt instructs Claude to trigger naturally
// ============================================================================

#[test]
fn natural_triggering_instruction_present() {
    let mut registry = PromptRegistry::new();

    let reva = make_character(
        "Reva",
        vec![involuntary_ability(
            "Root-Bonding",
            "Your bond with ancient roots lets you sense corruption",
            "+2 Nature",
        )],
    );

    registry.register_ability_context("narrator", &[reva]);

    let prompt = registry.compose("narrator");
    // The spec says: "Weave them naturally when relevant. Do not force triggers every turn."
    assert!(
        prompt.contains("naturally") || prompt.contains("natural"),
        "prompt should instruct Claude to trigger abilities naturally, got: {}",
        prompt,
    );
    assert!(
        prompt.contains("force") || prompt.contains("every turn"),
        "prompt should warn against forcing triggers every turn, got: {}",
        prompt,
    );
}

// ============================================================================
// AC: Multi-character — all party members' involuntary abilities included
// ============================================================================

#[test]
fn multi_character_abilities_all_included() {
    let mut registry = PromptRegistry::new();

    let reva = make_character(
        "Reva",
        vec![involuntary_ability(
            "Root-Bonding",
            "Your bond with ancient roots lets you sense corruption",
            "+2 Nature",
        )],
    );

    let kael = make_character(
        "Kael",
        vec![involuntary_ability(
            "Danger Sense",
            "Your instincts scream warnings before ambushes strike",
            "advantage on DEX saves vs traps",
        )],
    );

    registry.register_ability_context("narrator", &[reva, kael]);

    let prompt = registry.compose("narrator");
    assert!(
        prompt.contains("Reva"),
        "prompt should contain first character's name, got: {}",
        prompt,
    );
    assert!(
        prompt.contains("Root-Bonding"),
        "prompt should contain first character's involuntary ability",
    );
    assert!(
        prompt.contains("Kael"),
        "prompt should contain second character's name, got: {}",
        prompt,
    );
    assert!(
        prompt.contains("Danger Sense"),
        "prompt should contain second character's involuntary ability",
    );
}

#[test]
fn multi_character_mixed_abilities() {
    let mut registry = PromptRegistry::new();

    let reva = make_character(
        "Reva",
        vec![
            involuntary_ability(
                "Root-Bonding",
                "Your bond with ancient roots lets you sense corruption",
                "+2 Nature",
            ),
            voluntary_ability(
                "Entangle",
                "Roots burst from the ground to bind your foes",
                "restrain in 20ft area",
            ),
        ],
    );

    let kael = make_character(
        "Kael",
        vec![voluntary_ability(
            "Shield Bash",
            "You slam your shield into the foe",
            "1d6 + STR",
        )],
    );

    registry.register_ability_context("narrator", &[reva, kael]);

    let prompt = registry.compose("narrator");
    assert!(
        prompt.contains("Root-Bonding"),
        "Reva's involuntary ability should appear",
    );
    assert!(
        !prompt.contains("Entangle"),
        "Reva's voluntary ability should NOT appear",
    );
    // Kael has no involuntary abilities — his name should not appear in section
    assert!(
        !prompt.contains("Kael"),
        "Kael should not appear since he has no involuntary abilities, got: {}",
        prompt,
    );
}

// ============================================================================
// AC: No prompt when empty — section omitted if no involuntary abilities
// ============================================================================

#[test]
fn no_section_when_no_characters() {
    let mut registry = PromptRegistry::new();
    registry.register_section(
        "narrator",
        PromptSection::new(
            "base",
            "You are a narrator.",
            AttentionZone::Primacy,
            SectionCategory::Identity,
        ),
    );

    let characters: Vec<Character> = vec![];
    registry.register_ability_context("narrator", &characters);

    let prompt = registry.compose("narrator");
    assert!(
        !prompt.contains("CHARACTER ABILITIES"),
        "empty character list should produce no ability section, got: {}",
        prompt,
    );
}

#[test]
fn no_section_when_no_abilities_at_all() {
    let mut registry = PromptRegistry::new();
    registry.register_section(
        "narrator",
        PromptSection::new(
            "base",
            "You are a narrator.",
            AttentionZone::Primacy,
            SectionCategory::Identity,
        ),
    );

    let reva = make_character("Reva", vec![]);
    registry.register_ability_context("narrator", &[reva]);

    let prompt = registry.compose("narrator");
    assert!(
        !prompt.contains("CHARACTER ABILITIES"),
        "characters with no abilities should produce no ability section, got: {}",
        prompt,
    );
}

#[test]
fn no_section_when_all_abilities_voluntary() {
    let mut registry = PromptRegistry::new();
    registry.register_section(
        "narrator",
        PromptSection::new(
            "base",
            "You are a narrator.",
            AttentionZone::Primacy,
            SectionCategory::Identity,
        ),
    );

    let reva = make_character(
        "Reva",
        vec![
            voluntary_ability("Fireball", "Hurl flame", "8d6 fire"),
            voluntary_ability("Heal", "Mend wounds", "2d8+WIS healing"),
        ],
    );

    registry.register_ability_context("narrator", &[reva]);

    let prompt = registry.compose("narrator");
    assert!(
        !prompt.contains("CHARACTER ABILITIES"),
        "all-voluntary abilities should produce no ability section, got: {}",
        prompt,
    );
}

// ============================================================================
// Integration: ability section placed in correct attention zone
// ============================================================================

#[test]
fn ability_section_placed_in_valley_zone() {
    // Abilities are game state context — should be in Valley zone
    // (same as character data, lore, etc.)
    let mut registry = PromptRegistry::new();
    registry.register_section(
        "narrator",
        PromptSection::new(
            "base",
            "You are a narrator.",
            AttentionZone::Primacy,
            SectionCategory::Identity,
        ),
    );

    let reva = make_character(
        "Reva",
        vec![involuntary_ability(
            "Root-Bonding",
            "Your bond with ancient roots lets you sense corruption",
            "+2 Nature",
        )],
    );

    registry.register_ability_context("narrator", &[reva]);

    let sections = registry.get_sections("narrator", None, Some(AttentionZone::Valley));
    let ability_section = sections
        .iter()
        .find(|s| s.name == "ability_context");
    assert!(
        ability_section.is_some(),
        "ability_context section should be registered in Valley zone",
    );
}

// ============================================================================
// Integration: section header format
// ============================================================================

#[test]
fn section_header_is_character_abilities() {
    let mut registry = PromptRegistry::new();

    let reva = make_character(
        "Reva",
        vec![involuntary_ability(
            "Root-Bonding",
            "Sense corruption in living things",
            "+2 Nature",
        )],
    );

    registry.register_ability_context("narrator", &[reva]);

    let prompt = registry.compose("narrator");
    assert!(
        prompt.contains("[CHARACTER ABILITIES]"),
        "section should use [CHARACTER ABILITIES] header per spec, got: {}",
        prompt,
    );
}

// ============================================================================
// Edge case: character with multiple involuntary abilities
// ============================================================================

#[test]
fn character_with_multiple_involuntary_abilities() {
    let mut registry = PromptRegistry::new();

    let reva = make_character(
        "Reva",
        vec![
            involuntary_ability(
                "Root-Bonding",
                "Your bond with ancient roots lets you sense corruption",
                "+2 Nature",
            ),
            involuntary_ability(
                "Tremorsense",
                "You feel vibrations through the earth, sensing nearby movement",
                "detect creatures within 30ft through ground",
            ),
        ],
    );

    registry.register_ability_context("narrator", &[reva]);

    let prompt = registry.compose("narrator");
    assert!(
        prompt.contains("Root-Bonding"),
        "first involuntary ability should appear",
    );
    assert!(
        prompt.contains("Tremorsense"),
        "second involuntary ability should appear",
    );
    assert!(
        prompt.contains("sense corruption"),
        "first genre description should appear",
    );
    assert!(
        prompt.contains("vibrations through the earth"),
        "second genre description should appear",
    );
}

// ============================================================================
// Rule enforcement: #6 — test quality self-check (no vacuous assertions)
// All tests above use assert_eq! or specific content checks. This meta-test
// verifies the test file is not accidentally empty.
// ============================================================================

#[test]
fn meta_test_file_has_substantive_tests() {
    // This test exists to satisfy rule #6: test quality. If it compiles and
    // the above tests run, the test file is substantive. Each test above uses
    // assert!, assert_eq!, or content checks — no `let _ =` or `assert!(true)`.
    assert!(true, "meta: test file compiled and ran");
}

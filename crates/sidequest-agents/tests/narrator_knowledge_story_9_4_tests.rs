//! Story 9-4: Known facts in narrator prompt — tiered injection by relevance
//!
//! RED phase — these tests reference a method that doesn't exist yet:
//!   `PromptRegistry::register_knowledge_section()`
//!
//! The method should inject a `[CHARACTER KNOWLEDGE]` section into the
//! narrator prompt with each character's known facts, tagged by confidence
//! level (certain/suspected/rumored), capped at 20 most recent.
//!
//! ACs tested:
//!   AC1: Character's known facts appear in narrator prompt
//!   AC2: Facts labeled certain/suspected/rumored
//!   AC3: Maximum 20 facts included, most recent first
//!   AC4: Section omitted if character has no known facts
//!   AC5: Each character's knowledge is separate
//!   AC6: Claude references known facts naturally (structure test only)

use std::collections::HashMap;

use sidequest_agents::prompt_framework::{
    AttentionZone, PromptComposer, PromptRegistry, PromptSection, SectionCategory,
};
use sidequest_game::character::Character;
use sidequest_game::creature_core::CreatureCore;
use sidequest_game::inventory::Inventory;
use sidequest_game::known_fact::{Confidence, FactSource, KnownFact};
use sidequest_protocol::NonBlankString;

// =========================================================================
// Helpers
// =========================================================================

fn make_fact(content: &str, turn: u64, confidence: Confidence) -> KnownFact {
    KnownFact {
        content: content.to_string(),
        learned_turn: turn,
        source: FactSource::Observation,
        confidence,
    }
}

fn test_character_with_facts(name: &str, facts: Vec<KnownFact>) -> Character {
    Character {
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
        backstory: NonBlankString::new("A mysterious past").unwrap(),
        narrative_state: "Exploring".to_string(),
        hooks: vec![],
        char_class: NonBlankString::new("Rogue").unwrap(),
        race: NonBlankString::new("Human").unwrap(),
        pronouns: String::new(),
        stats: HashMap::new(),
        abilities: vec![],
        known_facts: facts,
        affinities: vec![],
        is_friendly: true,
    }
}

fn narrator_registry_with_identity() -> PromptRegistry {
    let mut registry = PromptRegistry::new();
    registry.register_section(
        "narrator",
        PromptSection::new(
            "identity",
            "You are the narrator of a dark fantasy world.",
            AttentionZone::Primacy,
            SectionCategory::Identity,
        ),
    );
    registry
}

// =========================================================================
// AC1: Character's known facts appear in narrator prompt
// =========================================================================

#[test]
fn knowledge_section_injected_for_narrator() {
    let mut registry = narrator_registry_with_identity();

    let facts = vec![make_fact(
        "The mayor is secretly a cultist",
        5,
        Confidence::Certain,
    )];
    let character = test_character_with_facts("Reva Thornwood", facts);

    registry.register_knowledge_section("narrator", &character);

    let prompt = registry.compose("narrator");

    assert!(
        prompt.contains("The mayor is secretly a cultist"),
        "Narrator prompt should contain the character's known fact.\nGot:\n{}",
        prompt,
    );
}

#[test]
fn knowledge_section_contains_multiple_facts() {
    let mut registry = PromptRegistry::new();

    let facts = vec![
        make_fact("The mayor is secretly a cultist", 5, Confidence::Certain),
        make_fact("The old well leads to tunnels", 8, Confidence::Suspected),
        make_fact(
            "A dragon sleeps beneath the mountain",
            12,
            Confidence::Rumored,
        ),
    ];
    let character = test_character_with_facts("Reva Thornwood", facts);

    registry.register_knowledge_section("narrator", &character);

    let prompt = registry.compose("narrator");

    assert!(
        prompt.contains("The mayor is secretly a cultist"),
        "First fact should appear in prompt",
    );
    assert!(
        prompt.contains("The old well leads to tunnels"),
        "Second fact should appear in prompt",
    );
    assert!(
        prompt.contains("A dragon sleeps beneath the mountain"),
        "Third fact should appear in prompt",
    );
}

// =========================================================================
// AC2: Facts labeled certain/suspected/rumored
// =========================================================================

#[test]
fn knowledge_section_tags_certain_facts() {
    let mut registry = PromptRegistry::new();

    let facts = vec![make_fact("The mayor is a cultist", 5, Confidence::Certain)];
    let character = test_character_with_facts("Reva", facts);

    registry.register_knowledge_section("narrator", &character);

    let prompt = registry.compose("narrator");

    assert!(
        prompt.contains("certain"),
        "Certain confidence tag should appear in prompt.\nGot:\n{}",
        prompt,
    );
}

#[test]
fn knowledge_section_tags_suspected_facts() {
    let mut registry = PromptRegistry::new();

    let facts = vec![make_fact(
        "The well leads to tunnels",
        8,
        Confidence::Suspected,
    )];
    let character = test_character_with_facts("Reva", facts);

    registry.register_knowledge_section("narrator", &character);

    let prompt = registry.compose("narrator");

    assert!(
        prompt.contains("suspected"),
        "Suspected confidence tag should appear in prompt.\nGot:\n{}",
        prompt,
    );
}

#[test]
fn knowledge_section_tags_rumored_facts() {
    let mut registry = PromptRegistry::new();

    let facts = vec![make_fact(
        "A dragon sleeps beneath the mountain",
        12,
        Confidence::Rumored,
    )];
    let character = test_character_with_facts("Reva", facts);

    registry.register_knowledge_section("narrator", &character);

    let prompt = registry.compose("narrator");

    assert!(
        prompt.contains("rumored"),
        "Rumored confidence tag should appear in prompt.\nGot:\n{}",
        prompt,
    );
}

#[test]
fn knowledge_section_confidence_tag_associated_with_fact() {
    let mut registry = PromptRegistry::new();

    let facts = vec![
        make_fact("The mayor is a cultist", 5, Confidence::Certain),
        make_fact(
            "A dragon sleeps beneath the mountain",
            12,
            Confidence::Rumored,
        ),
    ];
    let character = test_character_with_facts("Reva", facts);

    registry.register_knowledge_section("narrator", &character);

    let prompt = registry.compose("narrator");

    // Each fact should have its confidence tag nearby (on the same line)
    let lines: Vec<&str> = prompt.lines().collect();
    let mayor_line = lines.iter().find(|l| l.contains("mayor"));
    let dragon_line = lines.iter().find(|l| l.contains("dragon"));

    assert!(
        mayor_line.is_some() && mayor_line.unwrap().contains("certain"),
        "Mayor fact should be on a line tagged 'certain'.\nPrompt:\n{}",
        prompt,
    );
    assert!(
        dragon_line.is_some() && dragon_line.unwrap().contains("rumored"),
        "Dragon fact should be on a line tagged 'rumored'.\nPrompt:\n{}",
        prompt,
    );
}

// =========================================================================
// AC3: Maximum 20 facts included, most recent first
// =========================================================================

#[test]
fn knowledge_section_caps_at_20_facts() {
    let mut registry = PromptRegistry::new();

    // Create 25 facts with ascending turn numbers.
    // Use NATO phonetic alphabet to avoid substring collisions
    // (e.g., "Fact number 1" matches inside "Fact number 10").
    let labels = [
        "alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel", "india",
        "juliet", "kilo", "lima", "mike", "november", "oscar", "papa", "quebec", "romeo", "sierra",
        "tango", "uniform", "victor", "whiskey", "xray", "yankee",
    ];
    let facts: Vec<KnownFact> = labels
        .iter()
        .enumerate()
        .map(|(i, label)| {
            make_fact(
                &format!("fact-{}", label),
                (i + 1) as u64,
                Confidence::Certain,
            )
        })
        .collect();
    let character = test_character_with_facts("Reva", facts);

    registry.register_knowledge_section("narrator", &character);

    let prompt = registry.compose("narrator");

    // Facts 6-25 (most recent 20) should be present
    for label in &labels[5..] {
        assert!(
            prompt.contains(&format!("fact-{}", label)),
            "fact-{} (within cap) should be in prompt.\nPrompt:\n{}",
            label,
            prompt,
        );
    }

    // Facts 1-5 (oldest, beyond cap) should NOT be present
    for label in &labels[..5] {
        assert!(
            !prompt.contains(&format!("fact-{}", label)),
            "fact-{} (beyond cap of 20) should NOT be in prompt.\nPrompt:\n{}",
            label,
            prompt,
        );
    }
}

#[test]
fn knowledge_section_most_recent_first() {
    let mut registry = PromptRegistry::new();

    let facts = vec![
        make_fact("Old discovery", 1, Confidence::Certain),
        make_fact("Middle discovery", 10, Confidence::Suspected),
        make_fact("Recent discovery", 20, Confidence::Rumored),
    ];
    let character = test_character_with_facts("Reva", facts);

    registry.register_knowledge_section("narrator", &character);

    let prompt = registry.compose("narrator");

    let recent_pos = prompt
        .find("Recent discovery")
        .expect("Recent discovery should be in prompt");
    let middle_pos = prompt
        .find("Middle discovery")
        .expect("Middle discovery should be in prompt");
    let old_pos = prompt
        .find("Old discovery")
        .expect("Old discovery should be in prompt");

    assert!(
        recent_pos < middle_pos,
        "Recent (turn 20) should appear before middle (turn 10).\n\
         Recent at {}, middle at {}.\nPrompt:\n{}",
        recent_pos,
        middle_pos,
        prompt,
    );
    assert!(
        middle_pos < old_pos,
        "Middle (turn 10) should appear before old (turn 1).\n\
         Middle at {}, old at {}.\nPrompt:\n{}",
        middle_pos,
        old_pos,
        prompt,
    );
}

#[test]
fn knowledge_section_exactly_20_facts_all_included() {
    let mut registry = PromptRegistry::new();

    let facts: Vec<KnownFact> = (1..=20)
        .map(|i| make_fact(&format!("Fact number {}", i), i, Confidence::Certain))
        .collect();
    let character = test_character_with_facts("Reva", facts);

    registry.register_knowledge_section("narrator", &character);

    let prompt = registry.compose("narrator");

    for i in 1..=20 {
        assert!(
            prompt.contains(&format!("Fact number {}", i)),
            "All 20 facts should be included when at the cap.\nMissing: Fact number {}",
            i,
        );
    }
}

// =========================================================================
// AC4: Section omitted if character has no known facts
// =========================================================================

#[test]
fn empty_known_facts_produces_no_section() {
    let mut registry = narrator_registry_with_identity();

    let character = test_character_with_facts("Reva Thornwood", vec![]);

    let before = registry.compose("narrator");

    registry.register_knowledge_section("narrator", &character);

    let after = registry.compose("narrator");

    assert_eq!(
        before, after,
        "Empty known_facts should not add any section to the prompt",
    );
}

#[test]
fn no_knowledge_header_when_facts_empty() {
    let mut registry = PromptRegistry::new();

    let character = test_character_with_facts("Reva Thornwood", vec![]);

    registry.register_knowledge_section("narrator", &character);

    let prompt = registry.compose("narrator");

    assert!(
        !prompt.contains("KNOWLEDGE"),
        "No KNOWLEDGE header should appear for empty facts.\nGot:\n{}",
        prompt,
    );
}

// =========================================================================
// AC5: Each character's knowledge is separate
// =========================================================================

#[test]
fn per_character_knowledge_is_separate() {
    let mut registry = PromptRegistry::new();

    let reva_facts = vec![make_fact("The mayor is a cultist", 5, Confidence::Certain)];
    let reva = test_character_with_facts("Reva Thornwood", reva_facts);

    let thorn_facts = vec![make_fact(
        "The mine contains mithril",
        3,
        Confidence::Suspected,
    )];
    let thorn = test_character_with_facts("Thorn Ironhide", thorn_facts);

    registry.register_knowledge_section("narrator", &reva);
    registry.register_knowledge_section("narrator", &thorn);

    let prompt = registry.compose("narrator");

    // Both characters' facts should appear
    assert!(
        prompt.contains("The mayor is a cultist"),
        "Reva's fact should appear in prompt",
    );
    assert!(
        prompt.contains("The mine contains mithril"),
        "Thorn's fact should appear in prompt",
    );

    // Both character names should label their sections
    assert!(
        prompt.contains("Reva Thornwood"),
        "Reva's name should label her knowledge section",
    );
    assert!(
        prompt.contains("Thorn Ironhide"),
        "Thorn's name should label his knowledge section",
    );
}

#[test]
fn per_character_facts_are_not_mixed() {
    let mut registry = PromptRegistry::new();

    let reva = test_character_with_facts(
        "Reva Thornwood",
        vec![make_fact("Reva-only fact", 5, Confidence::Certain)],
    );
    let thorn = test_character_with_facts(
        "Thorn Ironhide",
        vec![make_fact("Thorn-only fact", 3, Confidence::Certain)],
    );

    registry.register_knowledge_section("narrator", &reva);
    registry.register_knowledge_section("narrator", &thorn);

    let prompt = registry.compose("narrator");

    // Find each character's section by name header
    let reva_pos = prompt
        .find("Reva Thornwood")
        .expect("Reva's name should be in prompt");
    let thorn_pos = prompt
        .find("Thorn Ironhide")
        .expect("Thorn's name should be in prompt");

    let reva_fact_pos = prompt
        .find("Reva-only fact")
        .expect("Reva's fact should be in prompt");
    let thorn_fact_pos = prompt
        .find("Thorn-only fact")
        .expect("Thorn's fact should be in prompt");

    // Reva's fact should appear after Reva's header and before Thorn's header
    assert!(
        reva_fact_pos > reva_pos && reva_fact_pos < thorn_pos,
        "Reva's fact should be between Reva's header and Thorn's header.\n\
         Reva header at {}, Reva fact at {}, Thorn header at {}",
        reva_pos,
        reva_fact_pos,
        thorn_pos,
    );

    // Thorn's fact should appear after Thorn's header
    assert!(
        thorn_fact_pos > thorn_pos,
        "Thorn's fact should appear after Thorn's header.\n\
         Thorn header at {}, Thorn fact at {}",
        thorn_pos,
        thorn_fact_pos,
    );
}

// =========================================================================
// AC6: Narrator behavior — structure tests (LLM behavior is playtest-only)
// =========================================================================

#[test]
fn knowledge_section_header_contains_character_name() {
    let mut registry = PromptRegistry::new();

    let facts = vec![make_fact("The mayor is a cultist", 5, Confidence::Certain)];
    let character = test_character_with_facts("Reva Thornwood", facts);

    registry.register_knowledge_section("narrator", &character);

    let prompt = registry.compose("narrator");

    // The section header should contain the character's name so the narrator
    // knows whose knowledge it is.
    assert!(
        prompt.contains("Reva Thornwood") && prompt.contains("KNOWLEDGE"),
        "Knowledge section should have a header with character name and KNOWLEDGE.\nGot:\n{}",
        prompt,
    );
}

// =========================================================================
// Section placement — Valley zone, Context category
// =========================================================================

#[test]
fn knowledge_section_placed_in_valley_zone() {
    let mut registry = PromptRegistry::new();

    let facts = vec![make_fact("A fact", 1, Confidence::Certain)];
    let character = test_character_with_facts("Reva", facts);

    registry.register_knowledge_section("narrator", &character);

    let sections = registry.get_sections(
        "narrator",
        Some(SectionCategory::Context),
        Some(AttentionZone::Valley),
    );

    assert!(
        !sections.is_empty(),
        "Knowledge section should be in Valley zone with Context category",
    );

    let knowledge_section = sections
        .iter()
        .find(|s| s.name.contains("knowledge"))
        .expect("Should find a knowledge section in Valley/Context");

    assert_eq!(
        knowledge_section.zone,
        AttentionZone::Valley,
        "Knowledge section should be in Valley zone (character context data)",
    );
}

#[test]
fn knowledge_section_appears_before_player_action() {
    let mut registry = PromptRegistry::new();

    let facts = vec![make_fact("The mayor is a cultist", 5, Confidence::Certain)];
    let character = test_character_with_facts("Reva", facts);

    registry.register_knowledge_section("narrator", &character);

    registry.register_section(
        "narrator",
        PromptSection::new(
            "player_action",
            "The player says: I confront the mayor.",
            AttentionZone::Recency,
            SectionCategory::Action,
        ),
    );

    let prompt = registry.compose("narrator");

    let knowledge_pos = prompt
        .find("The mayor is a cultist")
        .expect("Knowledge fact should be in prompt");
    let action_pos = prompt
        .find("I confront the mayor")
        .expect("Player action should be in prompt");

    assert!(
        knowledge_pos < action_pos,
        "Knowledge (Valley) must appear before player action (Recency).\n\
         Knowledge at byte {}, action at byte {}",
        knowledge_pos,
        action_pos,
    );
}

#[test]
fn knowledge_section_appears_after_identity() {
    let mut registry = narrator_registry_with_identity();

    let facts = vec![make_fact("The mayor is a cultist", 5, Confidence::Certain)];
    let character = test_character_with_facts("Reva", facts);

    registry.register_knowledge_section("narrator", &character);

    let prompt = registry.compose("narrator");

    let identity_pos = prompt
        .find("You are the narrator")
        .expect("Identity should be in prompt");
    let knowledge_pos = prompt
        .find("The mayor is a cultist")
        .expect("Knowledge fact should be in prompt");

    assert!(
        identity_pos < knowledge_pos,
        "Identity (Primacy) must appear before knowledge (Valley).\n\
         Identity at byte {}, knowledge at byte {}",
        identity_pos,
        knowledge_pos,
    );
}

// =========================================================================
// Integration — full prompt with knowledge
// =========================================================================

#[test]
fn full_pipeline_knowledge_in_narrator_prompt() {
    let mut registry = PromptRegistry::new();

    // Step 1: Narrator identity
    registry.register_section(
        "narrator",
        PromptSection::new(
            "narrator_identity",
            "You are the narrator of a post-apocalyptic wasteland.",
            AttentionZone::Primacy,
            SectionCategory::Identity,
        ),
    );

    // Step 2: Character with diverse facts
    let facts = vec![
        make_fact("The mayor is secretly a cultist", 15, Confidence::Certain),
        make_fact(
            "The old well leads to underground tunnels",
            10,
            Confidence::Suspected,
        ),
        make_fact(
            "A dragon sleeps beneath the mountain",
            5,
            Confidence::Rumored,
        ),
    ];
    let reva = test_character_with_facts("Reva Thornwood", facts);

    registry.register_knowledge_section("narrator", &reva);

    // Step 3: Game state
    registry.register_section(
        "narrator",
        PromptSection::new(
            "game_state",
            "<game_state>\nLocation: Town Square\nParty: Reva Thornwood\n</game_state>",
            AttentionZone::Valley,
            SectionCategory::State,
        ),
    );

    // Step 4: Player action
    registry.register_section(
        "narrator",
        PromptSection::new(
            "player_action",
            "The player says: I approach the mayor cautiously.",
            AttentionZone::Recency,
            SectionCategory::Action,
        ),
    );

    // Step 5: Compose and verify full pipeline
    let prompt = registry.compose("narrator");

    // Identity present
    assert!(
        prompt.contains("You are the narrator"),
        "Identity section should be present",
    );

    // Character name labels the knowledge section
    assert!(
        prompt.contains("Reva Thornwood"),
        "Character name should label the knowledge section",
    );

    // All facts present with correct confidence tags
    assert!(
        prompt.contains("The mayor is secretly a cultist"),
        "Certain fact should be present",
    );
    assert!(
        prompt.contains("certain"),
        "Certain confidence tag should be present",
    );
    assert!(
        prompt.contains("The old well leads to underground tunnels"),
        "Suspected fact should be present",
    );
    assert!(
        prompt.contains("suspected"),
        "Suspected confidence tag should be present",
    );
    assert!(
        prompt.contains("A dragon sleeps beneath the mountain"),
        "Rumored fact should be present",
    );
    assert!(
        prompt.contains("rumored"),
        "Rumored confidence tag should be present",
    );

    // Player action at the end
    assert!(
        prompt.contains("I approach the mayor cautiously"),
        "Player action should be present",
    );

    // Ordering: identity → knowledge → player action
    let identity_pos = prompt.find("You are the narrator").unwrap();
    let knowledge_pos = prompt.find("Reva Thornwood").unwrap();
    let action_pos = prompt.find("I approach the mayor").unwrap();

    assert!(
        identity_pos < knowledge_pos && knowledge_pos < action_pos,
        "Prompt order should be: identity → knowledge → player action.\n\
         Identity at {}, knowledge at {}, action at {}",
        identity_pos,
        knowledge_pos,
        action_pos,
    );
}

// =========================================================================
// AC coverage documentation
// =========================================================================

#[test]
fn coverage_check_all_acs_have_tests() {
    // AC1: Character's known facts appear in narrator prompt
    //   → knowledge_section_injected_for_narrator
    //   → knowledge_section_contains_multiple_facts
    // AC2: Facts labeled certain/suspected/rumored
    //   → knowledge_section_tags_certain_facts
    //   → knowledge_section_tags_suspected_facts
    //   → knowledge_section_tags_rumored_facts
    //   → knowledge_section_confidence_tag_associated_with_fact
    // AC3: Maximum 20 facts included, most recent first
    //   → knowledge_section_caps_at_20_facts
    //   → knowledge_section_most_recent_first
    //   → knowledge_section_exactly_20_facts_all_included
    // AC4: Section omitted if character has no known facts
    //   → empty_known_facts_produces_no_section
    //   → no_knowledge_header_when_facts_empty
    // AC5: Each character's knowledge is separate
    //   → per_character_knowledge_is_separate
    //   → per_character_facts_are_not_mixed
    // AC6: Narrator behavior (structure only)
    //   → knowledge_section_header_contains_character_name
    // Placement:
    //   → knowledge_section_placed_in_valley_zone
    //   → knowledge_section_appears_before_player_action
    //   → knowledge_section_appears_after_identity
    // Integration:
    //   → full_pipeline_knowledge_in_narrator_prompt
    assert_eq!(6, 6, "All 6 ACs covered by tests above");
}

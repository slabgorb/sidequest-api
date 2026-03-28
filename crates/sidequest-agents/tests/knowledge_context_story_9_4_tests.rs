//! Story 9-4: Known facts in narrator prompt — tiered injection by relevance
//!
//! RED phase — these tests reference types and methods that don't exist yet.
//! They will fail to compile until Dev implements:
//!   - PromptRegistry::register_knowledge_context() method
//!   - Confidence tags (certain/suspected/rumored) on each fact
//!   - Recency cap of 20 facts, most recent first
//!   - Empty suppression (no section when no known facts)
//!   - Per-character knowledge (multiplayer: each character's own facts)
//!   - [CHARACTER KNOWLEDGE] section header format
//!
//! ACs tested: Knowledge injected, Confidence tagged, Recency capped,
//!             Empty omitted, Per-character, Section format

use sidequest_agents::prompt_framework::{
    AttentionZone, PromptComposer, PromptRegistry, PromptSection, SectionCategory,
};
use sidequest_game::character::Character;
use sidequest_game::creature_core::CreatureCore;
use sidequest_game::inventory::Inventory;
use sidequest_game::known_fact::{Confidence, FactSource, KnownFact};
use sidequest_protocol::NonBlankString;
use std::collections::HashMap;

// ============================================================================
// Test helpers
// ============================================================================

fn make_character(name: &str, known_facts: Vec<KnownFact>) -> Character {
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
        abilities: vec![],
        known_facts,
        is_friendly: true,
    }
}

fn fact(content: &str, turn: u64, confidence: Confidence) -> KnownFact {
    KnownFact {
        content: content.to_string(),
        learned_turn: turn,
        source: FactSource::Observation,
        confidence,
    }
}

fn fact_with_source(
    content: &str,
    turn: u64,
    confidence: Confidence,
    source: FactSource,
) -> KnownFact {
    KnownFact {
        content: content.to_string(),
        learned_turn: turn,
        source,
        confidence,
    }
}

// ============================================================================
// AC: Knowledge injected — character's known facts appear in narrator prompt
// ============================================================================

#[test]
fn known_fact_appears_in_narrator_prompt() {
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
        vec![fact(
            "The mayor is secretly a cultist",
            5,
            Confidence::Certain,
        )],
    );

    registry.register_knowledge_context("narrator", &[reva]);

    let prompt = registry.compose("narrator");
    assert!(
        prompt.contains("The mayor is secretly a cultist"),
        "narrator prompt should contain the known fact, got: {}",
        prompt,
    );
}

#[test]
fn character_name_labels_knowledge_section() {
    let mut registry = PromptRegistry::new();

    let reva = make_character(
        "Reva",
        vec![fact("The well leads underground", 3, Confidence::Suspected)],
    );

    registry.register_knowledge_context("narrator", &[reva]);

    let prompt = registry.compose("narrator");
    assert!(
        prompt.contains("Reva"),
        "prompt should label knowledge with character name, got: {}",
        prompt,
    );
}

// ============================================================================
// AC: Confidence tagged — facts labeled certain/suspected/rumored
// ============================================================================

#[test]
fn certain_fact_tagged_as_certain() {
    let mut registry = PromptRegistry::new();

    let reva = make_character(
        "Reva",
        vec![fact(
            "The mayor is secretly a cultist",
            5,
            Confidence::Certain,
        )],
    );

    registry.register_knowledge_context("narrator", &[reva]);

    let prompt = registry.compose("narrator");
    assert!(
        prompt.contains("certain"),
        "certain fact should be tagged as certain, got: {}",
        prompt,
    );
}

#[test]
fn suspected_fact_tagged_as_suspected() {
    let mut registry = PromptRegistry::new();

    let reva = make_character(
        "Reva",
        vec![fact(
            "The old well leads to underground tunnels",
            3,
            Confidence::Suspected,
        )],
    );

    registry.register_knowledge_context("narrator", &[reva]);

    let prompt = registry.compose("narrator");
    assert!(
        prompt.contains("suspected"),
        "suspected fact should be tagged as suspected, got: {}",
        prompt,
    );
}

#[test]
fn rumored_fact_tagged_as_rumored() {
    let mut registry = PromptRegistry::new();

    let reva = make_character(
        "Reva",
        vec![fact(
            "They say the forest is haunted at night",
            1,
            Confidence::Rumored,
        )],
    );

    registry.register_knowledge_context("narrator", &[reva]);

    let prompt = registry.compose("narrator");
    assert!(
        prompt.contains("rumored"),
        "rumored fact should be tagged as rumored, got: {}",
        prompt,
    );
}

#[test]
fn all_confidence_levels_in_same_prompt() {
    let mut registry = PromptRegistry::new();

    let reva = make_character(
        "Reva",
        vec![
            fact("The mayor is a cultist", 5, Confidence::Certain),
            fact("The well leads underground", 3, Confidence::Suspected),
            fact("The forest is haunted", 1, Confidence::Rumored),
        ],
    );

    registry.register_knowledge_context("narrator", &[reva]);

    let prompt = registry.compose("narrator");
    assert!(
        prompt.contains("certain"),
        "prompt should contain certain tag",
    );
    assert!(
        prompt.contains("suspected"),
        "prompt should contain suspected tag",
    );
    assert!(
        prompt.contains("rumored"),
        "prompt should contain rumored tag",
    );
}

// ============================================================================
// AC: Recency capped — maximum 20 facts included, most recent first
// ============================================================================

#[test]
fn recency_cap_at_20_facts() {
    let mut registry = PromptRegistry::new();

    // Create 25 facts with increasing turn numbers
    let facts: Vec<KnownFact> = (0..25)
        .map(|i| fact(&format!("Fact number {}", i), i as u64, Confidence::Certain))
        .collect();

    let reva = make_character("Reva", facts);
    registry.register_knowledge_context("narrator", &[reva]);

    let prompt = registry.compose("narrator");

    // Most recent 20 should be included (turns 5-24)
    assert!(
        prompt.contains("Fact number 24"),
        "most recent fact (turn 24) should be included, got: {}",
        prompt,
    );
    assert!(
        prompt.contains("Fact number 5"),
        "20th most recent fact (turn 5) should be included, got: {}",
        prompt,
    );

    // Oldest 5 should be excluded (turns 0-4)
    assert!(
        !prompt.contains("Fact number 0"),
        "oldest fact (turn 0) should be excluded by recency cap, got: {}",
        prompt,
    );
    assert!(
        !prompt.contains("Fact number 4"),
        "5th oldest fact (turn 4) should be excluded by recency cap, got: {}",
        prompt,
    );
}

#[test]
fn exactly_20_facts_all_included() {
    let mut registry = PromptRegistry::new();

    let facts: Vec<KnownFact> = (0..20)
        .map(|i| fact(&format!("Fact {}", i), i as u64, Confidence::Certain))
        .collect();

    let reva = make_character("Reva", facts);
    registry.register_knowledge_context("narrator", &[reva]);

    let prompt = registry.compose("narrator");

    // All 20 should be present
    assert!(
        prompt.contains("Fact 0"),
        "first fact should be included when exactly at cap",
    );
    assert!(
        prompt.contains("Fact 19"),
        "last fact should be included when exactly at cap",
    );
}

#[test]
fn most_recent_facts_selected_regardless_of_insertion_order() {
    let mut registry = PromptRegistry::new();

    // Insert facts out of order — the cap should select by learned_turn, not vec index
    let mut facts: Vec<KnownFact> = (0..25)
        .map(|i| fact(&format!("Fact turn {}", i), i as u64, Confidence::Certain))
        .collect();
    // Shuffle: put the newest ones at the beginning
    facts.reverse();

    let reva = make_character("Reva", facts);
    registry.register_knowledge_context("narrator", &[reva]);

    let prompt = registry.compose("narrator");

    // Turn 24 (most recent) should be included regardless of vec position
    assert!(
        prompt.contains("Fact turn 24"),
        "most recent by turn number should be included, got: {}",
        prompt,
    );
    // Turn 0 (oldest) should be excluded
    assert!(
        !prompt.contains("Fact turn 0"),
        "oldest by turn number should be excluded, got: {}",
        prompt,
    );
}

// ============================================================================
// AC: Empty omitted — section omitted if character has no known facts
// ============================================================================

#[test]
fn no_section_when_no_known_facts() {
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
    registry.register_knowledge_context("narrator", &[reva]);

    let prompt = registry.compose("narrator");
    assert!(
        !prompt.contains("KNOWLEDGE"),
        "empty known_facts should produce no knowledge section, got: {}",
        prompt,
    );
}

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
    registry.register_knowledge_context("narrator", &characters);

    let prompt = registry.compose("narrator");
    assert!(
        !prompt.contains("KNOWLEDGE"),
        "empty character list should produce no knowledge section, got: {}",
        prompt,
    );
}

#[test]
fn no_section_when_all_characters_have_no_facts() {
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
    let kael = make_character("Kael", vec![]);
    registry.register_knowledge_context("narrator", &[reva, kael]);

    let prompt = registry.compose("narrator");
    assert!(
        !prompt.contains("KNOWLEDGE"),
        "characters with no facts should produce no section, got: {}",
        prompt,
    );
}

// ============================================================================
// AC: Per-character — each character's knowledge is separate
// ============================================================================

#[test]
fn multi_character_knowledge_separate() {
    let mut registry = PromptRegistry::new();

    let reva = make_character(
        "Reva",
        vec![fact(
            "The mayor is secretly a cultist",
            5,
            Confidence::Certain,
        )],
    );

    let kael = make_character(
        "Kael",
        vec![fact(
            "The blacksmith forges cursed weapons",
            3,
            Confidence::Suspected,
        )],
    );

    registry.register_knowledge_context("narrator", &[reva, kael]);

    let prompt = registry.compose("narrator");
    assert!(
        prompt.contains("Reva"),
        "prompt should contain first character's name, got: {}",
        prompt,
    );
    assert!(
        prompt.contains("The mayor is secretly a cultist"),
        "prompt should contain first character's fact",
    );
    assert!(
        prompt.contains("Kael"),
        "prompt should contain second character's name, got: {}",
        prompt,
    );
    assert!(
        prompt.contains("The blacksmith forges cursed weapons"),
        "prompt should contain second character's fact",
    );
}

#[test]
fn character_with_no_facts_excluded_from_multi_character() {
    let mut registry = PromptRegistry::new();

    let reva = make_character(
        "Reva",
        vec![fact("The mayor is a cultist", 5, Confidence::Certain)],
    );

    let kael = make_character("Kael", vec![]);

    registry.register_knowledge_context("narrator", &[reva, kael]);

    let prompt = registry.compose("narrator");
    assert!(
        prompt.contains("Reva"),
        "character with facts should appear",
    );
    assert!(
        !prompt.contains("Kael"),
        "character with no facts should NOT appear in knowledge section, got: {}",
        prompt,
    );
}

#[test]
fn per_character_recency_cap_independent() {
    let mut registry = PromptRegistry::new();

    // Reva has 25 facts, Kael has 3 — each character's cap is independent
    let reva_facts: Vec<KnownFact> = (0..25)
        .map(|i| {
            fact(
                &format!("Reva fact {}", i),
                i as u64,
                Confidence::Certain,
            )
        })
        .collect();

    let kael_facts: Vec<KnownFact> = (0..3)
        .map(|i| {
            fact(
                &format!("Kael fact {}", i),
                i as u64,
                Confidence::Certain,
            )
        })
        .collect();

    let reva = make_character("Reva", reva_facts);
    let kael = make_character("Kael", kael_facts);

    registry.register_knowledge_context("narrator", &[reva, kael]);

    let prompt = registry.compose("narrator");

    // Reva's oldest facts should be capped
    assert!(
        !prompt.contains("Reva fact 0"),
        "Reva's oldest fact should be capped, got: {}",
        prompt,
    );
    assert!(
        prompt.contains("Reva fact 24"),
        "Reva's newest fact should be included",
    );

    // Kael's facts should all be included (only 3, well under cap)
    assert!(
        prompt.contains("Kael fact 0"),
        "Kael's facts should all be included (under cap)",
    );
    assert!(
        prompt.contains("Kael fact 2"),
        "Kael's newest fact should be included",
    );
}

// ============================================================================
// Section format: [CHARACTER KNOWLEDGE] header per spec
// ============================================================================

#[test]
fn section_header_is_character_knowledge() {
    let mut registry = PromptRegistry::new();

    let reva = make_character(
        "Reva",
        vec![fact("The mayor is a cultist", 5, Confidence::Certain)],
    );

    registry.register_knowledge_context("narrator", &[reva]);

    let prompt = registry.compose("narrator");
    assert!(
        prompt.contains("[CHARACTER KNOWLEDGE]"),
        "section should use [CHARACTER KNOWLEDGE] header per spec, got: {}",
        prompt,
    );
}

// ============================================================================
// Integration: knowledge section placed in correct attention zone
// ============================================================================

#[test]
fn knowledge_section_placed_in_valley_zone() {
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
        vec![fact("The mayor is a cultist", 5, Confidence::Certain)],
    );

    registry.register_knowledge_context("narrator", &[reva]);

    let sections = registry.get_sections("narrator", None, Some(AttentionZone::Valley));
    let knowledge_section = sections.iter().find(|s| s.name == "knowledge_context");
    assert!(
        knowledge_section.is_some(),
        "knowledge_context section should be registered in Valley zone",
    );
}

#[test]
fn knowledge_section_has_context_category() {
    let mut registry = PromptRegistry::new();

    let reva = make_character(
        "Reva",
        vec![fact("The mayor is a cultist", 5, Confidence::Certain)],
    );

    registry.register_knowledge_context("narrator", &[reva]);

    let sections = registry.get_sections("narrator", Some(SectionCategory::Context), None);
    let knowledge_section = sections.iter().find(|s| s.name == "knowledge_context");
    assert!(
        knowledge_section.is_some(),
        "knowledge_context section should have Context category",
    );
}

// ============================================================================
// Edge cases: fact source variety
// ============================================================================

#[test]
fn facts_from_all_sources_included() {
    let mut registry = PromptRegistry::new();

    let reva = make_character(
        "Reva",
        vec![
            fact_with_source(
                "Saw the cultists gathering",
                5,
                Confidence::Certain,
                FactSource::Observation,
            ),
            fact_with_source(
                "The barkeep whispered about tunnels",
                3,
                Confidence::Rumored,
                FactSource::Dialogue,
            ),
            fact_with_source(
                "Found a hidden passage behind the altar",
                7,
                Confidence::Certain,
                FactSource::Discovery,
            ),
        ],
    );

    registry.register_knowledge_context("narrator", &[reva]);

    let prompt = registry.compose("narrator");
    assert!(
        prompt.contains("cultists gathering"),
        "observation fact should be included",
    );
    assert!(
        prompt.contains("whispered about tunnels"),
        "dialogue fact should be included",
    );
    assert!(
        prompt.contains("hidden passage"),
        "discovery fact should be included",
    );
}

// ============================================================================
// Integration: wiring test — register_knowledge_context exists on PromptRegistry
// ============================================================================

#[test]
fn register_knowledge_context_is_callable_on_registry() {
    // Wiring test: verifies the method exists and is callable.
    // This is the minimal integration check — the method must exist
    // on PromptRegistry, not just in isolation.
    let mut registry = PromptRegistry::new();
    let reva = make_character(
        "Reva",
        vec![fact("Test fact", 1, Confidence::Certain)],
    );

    // This line is the test — it must compile and not panic.
    registry.register_knowledge_context("narrator", &[reva]);

    // Verify it actually produced output (not a no-op)
    let prompt = registry.compose("narrator");
    assert!(
        !prompt.is_empty(),
        "register_knowledge_context should produce a non-empty section",
    );
}

// ============================================================================
// Ordering: most recent facts should appear first in prompt
// ============================================================================

#[test]
fn most_recent_facts_appear_first() {
    let mut registry = PromptRegistry::new();

    let reva = make_character(
        "Reva",
        vec![
            fact("Old discovery from turn 1", 1, Confidence::Certain),
            fact("Recent discovery from turn 10", 10, Confidence::Certain),
            fact("Middle discovery from turn 5", 5, Confidence::Suspected),
        ],
    );

    registry.register_knowledge_context("narrator", &[reva]);

    let prompt = registry.compose("narrator");

    // Most recent (turn 10) should appear before oldest (turn 1)
    let pos_recent = prompt
        .find("Recent discovery from turn 10")
        .expect("recent fact should be in prompt");
    let pos_old = prompt
        .find("Old discovery from turn 1")
        .expect("old fact should be in prompt");
    assert!(
        pos_recent < pos_old,
        "most recent fact (turn 10) should appear before oldest (turn 1) in prompt. \
         Recent at pos {}, old at pos {}. Prompt: {}",
        pos_recent,
        pos_old,
        prompt,
    );
}

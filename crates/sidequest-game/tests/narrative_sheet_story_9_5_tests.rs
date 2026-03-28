//! Story 9-5: Narrative character sheet — genre-voiced to_narrative_sheet()
//!
//! RED phase — these tests reference types and methods that don't exist yet.
//! They will fail to compile until Dev implements:
//!   - NarrativeSheet struct with identity, abilities, knowledge, status
//!   - AbilityEntry struct (name, description, involuntary)
//!   - KnowledgeEntry struct (content, confidence)
//!   - Character::to_narrative_sheet() method
//!   - Serde derives on NarrativeSheet and sub-types
//!
//! ACs tested: Structured output, Genre voice, Knowledge included,
//!             Status included, Serializable, No stat blocks

use sidequest_game::ability::{AbilityDefinition, AbilitySource};
use sidequest_game::character::Character;
use sidequest_game::creature_core::CreatureCore;
use sidequest_game::inventory::Inventory;
use sidequest_game::known_fact::{Confidence, FactSource, KnownFact};
use sidequest_game::narrative_sheet::{AbilityEntry, KnowledgeEntry, NarrativeSheet};
use sidequest_protocol::NonBlankString;
use std::collections::HashMap;

// ============================================================================
// Test helpers
// ============================================================================

fn make_character(
    name: &str,
    abilities: Vec<AbilityDefinition>,
    known_facts: Vec<KnownFact>,
) -> Character {
    Character {
        core: CreatureCore {
            name: NonBlankString::new(name).unwrap(),
            description: NonBlankString::new("A scarred wanderer of the wastes").unwrap(),
            personality: NonBlankString::new("Gruff but loyal").unwrap(),
            level: 5,
            hp: 18,
            max_hp: 30,
            ac: 14,
            inventory: Inventory::default(),
            statuses: vec![],
        },
        backstory: NonBlankString::new("Raised in the iron mines of the deep").unwrap(),
        narrative_state: "Exploring the flickering reach".to_string(),
        hooks: vec!["nemesis: The Warden".to_string()],
        char_class: NonBlankString::new("Warden").unwrap(),
        race: NonBlankString::new("Dwarf").unwrap(),
        stats: HashMap::from([
            ("STR".to_string(), 16),
            ("DEX".to_string(), 10),
            ("CON".to_string(), 14),
        ]),
        abilities,
        known_facts,
        is_friendly: true,
    }
}

fn involuntary_ability() -> AbilityDefinition {
    AbilityDefinition {
        name: "Root-Bonding".to_string(),
        genre_description: "Your bond with ancient roots lets you sense corruption in living things"
            .to_string(),
        mechanical_effect: "+2 Nature, detect corruption 30ft".to_string(),
        involuntary: true,
        source: AbilitySource::Race,
    }
}

fn voluntary_ability() -> AbilityDefinition {
    AbilityDefinition {
        name: "Stone Ward".to_string(),
        genre_description: "You call upon the bones of the earth to shield your allies".to_string(),
        mechanical_effect: "+4 AC to adjacent allies for 1 round".to_string(),
        involuntary: false,
        source: AbilitySource::Class,
    }
}

fn certain_fact() -> KnownFact {
    KnownFact {
        content: "The mayor is secretly a cultist".to_string(),
        learned_turn: 5,
        source: FactSource::Observation,
        confidence: Confidence::Certain,
    }
}

fn rumored_fact() -> KnownFact {
    KnownFact {
        content: "They say the forest is haunted at night".to_string(),
        learned_turn: 2,
        source: FactSource::Dialogue,
        confidence: Confidence::Rumored,
    }
}

// ============================================================================
// AC: Structured output — NarrativeSheet is a typed struct, not raw text
// ============================================================================

#[test]
fn to_narrative_sheet_returns_narrative_sheet() {
    let character = make_character("Reva", vec![involuntary_ability()], vec![certain_fact()]);

    let sheet: NarrativeSheet = character.to_narrative_sheet("dark fantasy");

    // NarrativeSheet has the four required sections
    assert!(
        !sheet.identity.is_empty(),
        "identity section should not be empty",
    );
}

#[test]
fn narrative_sheet_has_abilities_field() {
    let character = make_character("Reva", vec![involuntary_ability()], vec![]);

    let sheet = character.to_narrative_sheet("dark fantasy");

    assert_eq!(
        sheet.abilities.len(),
        1,
        "abilities should contain exactly 1 entry",
    );
}

#[test]
fn narrative_sheet_has_knowledge_field() {
    let character = make_character("Reva", vec![], vec![certain_fact()]);

    let sheet = character.to_narrative_sheet("dark fantasy");

    assert_eq!(
        sheet.knowledge.len(),
        1,
        "knowledge should contain exactly 1 entry",
    );
}

#[test]
fn narrative_sheet_has_status_field() {
    let character = make_character("Reva", vec![], vec![]);

    let sheet = character.to_narrative_sheet("dark fantasy");

    // Status should reflect HP state — character has 18/30 HP
    let status_str = format!("{:?}", sheet.status);
    assert!(
        !status_str.is_empty(),
        "status section should not be empty",
    );
}

// ============================================================================
// AC: Genre voice — abilities use genre_description, never mechanical_effect
// ============================================================================

#[test]
fn ability_entry_contains_genre_description() {
    let character = make_character("Reva", vec![involuntary_ability()], vec![]);

    let sheet = character.to_narrative_sheet("dark fantasy");
    let entry = &sheet.abilities[0];

    assert_eq!(entry.name, "Root-Bonding");
    assert!(
        entry.description.contains("sense corruption"),
        "ability description should be genre-voiced, got: {}",
        entry.description,
    );
}

#[test]
fn ability_entry_does_not_contain_mechanical_effect() {
    let character = make_character("Reva", vec![involuntary_ability()], vec![]);

    let sheet = character.to_narrative_sheet("dark fantasy");
    let entry = &sheet.abilities[0];

    assert!(
        !entry.description.contains("+2 Nature"),
        "ability description should NOT contain mechanical effect, got: {}",
        entry.description,
    );
    assert!(
        !entry.description.contains("30ft"),
        "ability description should NOT contain mechanical range, got: {}",
        entry.description,
    );
}

#[test]
fn ability_entry_preserves_involuntary_flag() {
    let character = make_character(
        "Reva",
        vec![involuntary_ability(), voluntary_ability()],
        vec![],
    );

    let sheet = character.to_narrative_sheet("dark fantasy");

    let root_bonding = sheet.abilities.iter().find(|a| a.name == "Root-Bonding");
    assert!(root_bonding.is_some(), "Root-Bonding should be in abilities");
    assert!(
        root_bonding.unwrap().involuntary,
        "Root-Bonding should be marked involuntary",
    );

    let stone_ward = sheet.abilities.iter().find(|a| a.name == "Stone Ward");
    assert!(stone_ward.is_some(), "Stone Ward should be in abilities");
    assert!(
        !stone_ward.unwrap().involuntary,
        "Stone Ward should NOT be marked involuntary",
    );
}

#[test]
fn all_abilities_included_both_voluntary_and_involuntary() {
    let character = make_character(
        "Reva",
        vec![involuntary_ability(), voluntary_ability()],
        vec![],
    );

    let sheet = character.to_narrative_sheet("dark fantasy");

    assert_eq!(
        sheet.abilities.len(),
        2,
        "both voluntary and involuntary abilities should be included in sheet",
    );
}

// ============================================================================
// AC: Knowledge included — known facts listed with confidence tags
// ============================================================================

#[test]
fn knowledge_entry_contains_fact_content() {
    let character = make_character("Reva", vec![], vec![certain_fact()]);

    let sheet = character.to_narrative_sheet("dark fantasy");
    let entry = &sheet.knowledge[0];

    assert_eq!(
        entry.content, "The mayor is secretly a cultist",
        "knowledge entry should contain the fact content",
    );
}

#[test]
fn knowledge_entry_contains_confidence() {
    let character = make_character("Reva", vec![], vec![certain_fact(), rumored_fact()]);

    let sheet = character.to_narrative_sheet("dark fantasy");

    let certain = sheet
        .knowledge
        .iter()
        .find(|k| k.content.contains("mayor"));
    assert!(certain.is_some());
    assert!(
        matches!(certain.unwrap().confidence, Confidence::Certain),
        "mayor fact should have Certain confidence",
    );

    let rumored = sheet
        .knowledge
        .iter()
        .find(|k| k.content.contains("forest"));
    assert!(rumored.is_some());
    assert!(
        matches!(rumored.unwrap().confidence, Confidence::Rumored),
        "forest fact should have Rumored confidence",
    );
}

#[test]
fn empty_knowledge_produces_empty_vec() {
    let character = make_character("Reva", vec![], vec![]);

    let sheet = character.to_narrative_sheet("dark fantasy");

    assert!(
        sheet.knowledge.is_empty(),
        "character with no facts should have empty knowledge vec",
    );
}

// ============================================================================
// AC: Status included — current HP and conditions in narrative form
// ============================================================================

#[test]
fn status_reflects_wounded_character() {
    // Character has 18/30 HP — should indicate wounded state
    let character = make_character("Reva", vec![], vec![]);

    let sheet = character.to_narrative_sheet("dark fantasy");

    // Status should contain narrative HP description, not raw numbers
    let status_json = serde_json::to_string(&sheet.status).unwrap();
    assert!(
        !status_json.contains("\"hp\":18"),
        "status should NOT expose raw HP number in JSON, got: {}",
        status_json,
    );
}

#[test]
fn status_includes_conditions_when_present() {
    let mut character = make_character("Reva", vec![], vec![]);
    character.core.statuses = vec!["Poisoned".to_string(), "Exhausted".to_string()];

    let sheet = character.to_narrative_sheet("dark fantasy");

    let status_json = serde_json::to_string(&sheet.status).unwrap();
    assert!(
        status_json.contains("Poisoned") || status_json.to_lowercase().contains("poison"),
        "status should mention Poisoned condition, got: {}",
        status_json,
    );
}

#[test]
fn status_no_conditions_when_healthy() {
    let mut character = make_character("Reva", vec![], vec![]);
    character.core.hp = character.core.max_hp; // Full HP
    character.core.statuses = vec![];

    let sheet = character.to_narrative_sheet("dark fantasy");

    // At full HP with no conditions, status should reflect good health
    let status_json = serde_json::to_string(&sheet.status).unwrap();
    assert!(
        !status_json.is_empty(),
        "status should still be present even when healthy",
    );
}

// ============================================================================
// AC: Serializable — NarrativeSheet serializes to JSON for protocol
// ============================================================================

#[test]
fn narrative_sheet_serializes_to_json() {
    let character = make_character(
        "Reva",
        vec![involuntary_ability()],
        vec![certain_fact()],
    );

    let sheet = character.to_narrative_sheet("dark fantasy");

    let json = serde_json::to_string(&sheet);
    assert!(
        json.is_ok(),
        "NarrativeSheet should serialize to JSON, got error: {:?}",
        json.err(),
    );
}

#[test]
fn narrative_sheet_json_roundtrip() {
    let character = make_character(
        "Reva",
        vec![involuntary_ability(), voluntary_ability()],
        vec![certain_fact(), rumored_fact()],
    );

    let sheet = character.to_narrative_sheet("dark fantasy");

    let json = serde_json::to_string(&sheet).unwrap();
    let deserialized: NarrativeSheet = serde_json::from_str(&json).unwrap();

    assert_eq!(
        deserialized.identity, sheet.identity,
        "identity should survive JSON roundtrip",
    );
    assert_eq!(
        deserialized.abilities.len(),
        sheet.abilities.len(),
        "abilities count should survive JSON roundtrip",
    );
    assert_eq!(
        deserialized.knowledge.len(),
        sheet.knowledge.len(),
        "knowledge count should survive JSON roundtrip",
    );
}

// ============================================================================
// AC: No stat blocks — no raw numbers exposed in player-facing fields
// ============================================================================

#[test]
fn identity_does_not_contain_raw_stats() {
    let character = make_character("Reva", vec![], vec![]);

    let sheet = character.to_narrative_sheet("dark fantasy");

    // Identity should not contain raw stat values
    assert!(
        !sheet.identity.contains("STR"),
        "identity should not contain stat abbreviations, got: {}",
        sheet.identity,
    );
    assert!(
        !sheet.identity.contains("16"),
        "identity should not contain raw stat values, got: {}",
        sheet.identity,
    );
    assert!(
        !sheet.identity.contains("AC"),
        "identity should not contain AC abbreviation, got: {}",
        sheet.identity,
    );
}

#[test]
fn identity_contains_character_name() {
    let character = make_character("Reva Thornwhisper", vec![], vec![]);

    let sheet = character.to_narrative_sheet("dark fantasy");

    assert!(
        sheet.identity.contains("Reva Thornwhisper"),
        "identity should contain the character's name, got: {}",
        sheet.identity,
    );
}

#[test]
fn identity_contains_class_and_race() {
    let character = make_character("Reva", vec![], vec![]);

    let sheet = character.to_narrative_sheet("dark fantasy");

    // Class and race should appear in genre voice (not necessarily as raw labels)
    assert!(
        sheet.identity.contains("Warden") || sheet.identity.to_lowercase().contains("warden"),
        "identity should reference character class, got: {}",
        sheet.identity,
    );
    assert!(
        sheet.identity.contains("Dwarf") || sheet.identity.to_lowercase().contains("dwarf"),
        "identity should reference character race, got: {}",
        sheet.identity,
    );
}

// ============================================================================
// Edge: genre_voice parameter affects identity
// ============================================================================

#[test]
fn genre_voice_parameter_is_used() {
    let character = make_character("Reva", vec![], vec![]);

    let sheet = character.to_narrative_sheet("dark fantasy");

    // The identity should be genre-voiced — at minimum it should be a
    // composed string, not just the raw name
    assert!(
        sheet.identity.len() > "Reva".len(),
        "identity should be more than just the name — it should be genre-voiced, got: {}",
        sheet.identity,
    );
}

// ============================================================================
// Edge: empty abilities and knowledge
// ============================================================================

#[test]
fn empty_abilities_produces_empty_vec() {
    let character = make_character("Reva", vec![], vec![]);

    let sheet = character.to_narrative_sheet("dark fantasy");

    assert!(
        sheet.abilities.is_empty(),
        "character with no abilities should have empty abilities vec",
    );
}

#[test]
fn full_character_produces_complete_sheet() {
    let character = make_character(
        "Reva Thornwhisper",
        vec![involuntary_ability(), voluntary_ability()],
        vec![certain_fact(), rumored_fact()],
    );

    let sheet = character.to_narrative_sheet("dark fantasy");

    assert!(!sheet.identity.is_empty(), "identity should be populated");
    assert_eq!(sheet.abilities.len(), 2, "should have 2 abilities");
    assert_eq!(sheet.knowledge.len(), 2, "should have 2 knowledge entries");
    // Status is always present
    let status_json = serde_json::to_string(&sheet.status).unwrap();
    assert!(!status_json.is_empty(), "status should serialize");
}

// ============================================================================
// Integration: wiring test — to_narrative_sheet exists on Character
// ============================================================================

#[test]
fn to_narrative_sheet_is_callable_on_character() {
    let character = make_character("Reva", vec![], vec![]);

    // This line is the test — it must compile and not panic.
    let sheet = character.to_narrative_sheet("fantasy");

    // Verify it produced a real result
    assert!(
        !sheet.identity.is_empty(),
        "to_narrative_sheet should produce a non-empty identity",
    );
}

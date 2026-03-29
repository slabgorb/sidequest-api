//! Story 9-3: KnownFact model — play-derived knowledge accumulation
//!
//! RED phase — these tests reference types and methods that don't exist yet.
//! They will fail to compile until Dev implements:
//!   - KnownFact struct with content, learned_turn, source, confidence
//!   - FactSource enum (Observation, Dialogue, Discovery)
//!   - Confidence enum (Certain, Suspected, Rumored)
//!   - Character.known_facts: Vec<KnownFact>
//!   - WorldStatePatch.discovered_facts: Vec<DiscoveredFact>
//!   - DiscoveredFact struct with character name + fact
//!   - Serde round-trip for all types
//!
//! ACs tested: Model defined, Source types, Confidence levels,
//!             Patch extension, Character storage, Persistence, Accumulation

use sidequest_game::ability::AbilityDefinition;
use sidequest_game::character::Character;
use sidequest_game::creature_core::CreatureCore;
use sidequest_game::inventory::Inventory;
use sidequest_game::known_fact::{Confidence, DiscoveredFact, FactSource, KnownFact};
use sidequest_game::state::WorldStatePatch;
use sidequest_protocol::NonBlankString;
use std::collections::HashMap;

// ============================================================================
// Test helpers
// ============================================================================

fn make_fact(content: &str, turn: u64, source: FactSource, confidence: Confidence) -> KnownFact {
    KnownFact {
        content: content.to_string(),
        learned_turn: turn,
        source,
        confidence,
    }
}

fn make_character(name: &str, facts: Vec<KnownFact>) -> Character {
    Character {
        core: CreatureCore {
            name: NonBlankString::new(name).unwrap(),
            description: NonBlankString::new("A test character").unwrap(),
            personality: NonBlankString::new("Bold").unwrap(),
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
        pronouns: String::new(),
        stats: HashMap::from([
            ("STR".to_string(), 12),
            ("DEX".to_string(), 16),
            ("WIS".to_string(), 14),
        ]),
        abilities: vec![],
        known_facts: facts,
        affinities: vec![],
        is_friendly: true,
    }
}

// ============================================================================
// AC: Model defined — KnownFact with content, learned_turn, source, confidence
// ============================================================================

#[test]
fn known_fact_has_all_fields() {
    let fact = make_fact(
        "The mayor is secretly a cultist",
        14,
        FactSource::Dialogue,
        Confidence::Certain,
    );
    assert_eq!(fact.content, "The mayor is secretly a cultist");
    assert_eq!(fact.learned_turn, 14);
}

#[test]
fn known_fact_content_preserves_text() {
    let fact = make_fact(
        "The old well connects to underground tunnels",
        7,
        FactSource::Discovery,
        Confidence::Suspected,
    );
    assert_eq!(fact.content, "The old well connects to underground tunnels");
    assert_eq!(fact.learned_turn, 7);
}

// ============================================================================
// AC: Source types — Observation, Dialogue, Discovery supported
// ============================================================================

#[test]
fn fact_source_observation() {
    let fact = make_fact(
        "Smoke rising from the north",
        3,
        FactSource::Observation,
        Confidence::Certain,
    );
    assert!(matches!(fact.source, FactSource::Observation));
}

#[test]
fn fact_source_dialogue() {
    let fact = make_fact(
        "The innkeeper warned of bandits",
        5,
        FactSource::Dialogue,
        Confidence::Certain,
    );
    assert!(matches!(fact.source, FactSource::Dialogue));
}

#[test]
fn fact_source_discovery() {
    let fact = make_fact(
        "Hidden passage behind the bookcase",
        12,
        FactSource::Discovery,
        Confidence::Certain,
    );
    assert!(matches!(fact.source, FactSource::Discovery));
}

// ============================================================================
// AC: Confidence levels — Certain, Suspected, Rumored supported
// ============================================================================

#[test]
fn confidence_certain() {
    let fact = make_fact(
        "The bridge is collapsed",
        2,
        FactSource::Observation,
        Confidence::Certain,
    );
    assert!(matches!(fact.confidence, Confidence::Certain));
}

#[test]
fn confidence_suspected() {
    let fact = make_fact(
        "The merchant may be lying",
        8,
        FactSource::Dialogue,
        Confidence::Suspected,
    );
    assert!(matches!(fact.confidence, Confidence::Suspected));
}

#[test]
fn confidence_rumored() {
    let fact = make_fact(
        "They say a dragon sleeps beneath",
        1,
        FactSource::Dialogue,
        Confidence::Rumored,
    );
    assert!(matches!(fact.confidence, Confidence::Rumored));
}

// ============================================================================
// AC: Patch extension — WorldStatePatch carries discovered facts
// ============================================================================

#[test]
fn world_state_patch_has_discovered_facts_field() {
    let patch = WorldStatePatch {
        discovered_facts: Some(vec![DiscoveredFact {
            character_name: "Reva".to_string(),
            fact: make_fact(
                "Corruption in the grove",
                14,
                FactSource::Observation,
                Confidence::Certain,
            ),
        }]),
        ..Default::default()
    };
    assert_eq!(patch.discovered_facts.as_ref().unwrap().len(), 1);
}

#[test]
fn world_state_patch_discovered_facts_defaults_to_none() {
    let patch = WorldStatePatch::default();
    assert!(
        patch.discovered_facts.is_none(),
        "discovered_facts should default to None (no change)",
    );
}

#[test]
fn discovered_fact_carries_character_and_fact() {
    let discovered = DiscoveredFact {
        character_name: "Kael".to_string(),
        fact: make_fact(
            "Ambush planned at dawn",
            10,
            FactSource::Discovery,
            Confidence::Suspected,
        ),
    };
    assert_eq!(discovered.character_name, "Kael");
    assert_eq!(discovered.fact.content, "Ambush planned at dawn");
    assert_eq!(discovered.fact.learned_turn, 10);
}

// ============================================================================
// AC: Character storage — facts stored in character's known_facts vec
// ============================================================================

#[test]
fn character_has_known_facts_field() {
    let reva = make_character(
        "Reva",
        vec![make_fact(
            "The mayor is a cultist",
            14,
            FactSource::Dialogue,
            Confidence::Certain,
        )],
    );
    assert_eq!(reva.known_facts.len(), 1);
    assert_eq!(reva.known_facts[0].content, "The mayor is a cultist");
}

#[test]
fn character_with_no_facts() {
    let reva = make_character("Reva", vec![]);
    assert!(
        reva.known_facts.is_empty(),
        "new character should start with no known facts",
    );
}

#[test]
fn character_with_multiple_facts() {
    let reva = make_character(
        "Reva",
        vec![
            make_fact(
                "The bridge is broken",
                2,
                FactSource::Observation,
                Confidence::Certain,
            ),
            make_fact(
                "The innkeeper is suspicious",
                5,
                FactSource::Dialogue,
                Confidence::Suspected,
            ),
            make_fact(
                "Secret tunnel exists",
                12,
                FactSource::Discovery,
                Confidence::Certain,
            ),
        ],
    );
    assert_eq!(reva.known_facts.len(), 3);
    assert_eq!(reva.known_facts[0].learned_turn, 2);
    assert_eq!(reva.known_facts[1].learned_turn, 5);
    assert_eq!(reva.known_facts[2].learned_turn, 12);
}

// ============================================================================
// AC: Persistence — facts survive save/load cycle via serde
// ============================================================================

#[test]
fn known_fact_serde_round_trip() {
    let fact = make_fact(
        "The grove is corrupted",
        14,
        FactSource::Observation,
        Confidence::Certain,
    );
    let json = serde_json::to_string(&fact).expect("serialize fact");
    let restored: KnownFact = serde_json::from_str(&json).expect("deserialize fact");
    assert_eq!(restored.content, "The grove is corrupted");
    assert_eq!(restored.learned_turn, 14);
    assert!(matches!(restored.source, FactSource::Observation));
    assert!(matches!(restored.confidence, Confidence::Certain));
}

#[test]
fn fact_source_serde_round_trip() {
    for source in [
        FactSource::Observation,
        FactSource::Dialogue,
        FactSource::Discovery,
    ] {
        let json = serde_json::to_string(&source).expect("serialize source");
        let restored: FactSource = serde_json::from_str(&json).expect("deserialize source");
        assert_eq!(
            std::mem::discriminant(&source),
            std::mem::discriminant(&restored),
            "source variant should survive round-trip: {}",
            json,
        );
    }
}

#[test]
fn confidence_serde_round_trip() {
    for confidence in [
        Confidence::Certain,
        Confidence::Suspected,
        Confidence::Rumored,
    ] {
        let json = serde_json::to_string(&confidence).expect("serialize confidence");
        let restored: Confidence = serde_json::from_str(&json).expect("deserialize confidence");
        assert_eq!(
            std::mem::discriminant(&confidence),
            std::mem::discriminant(&restored),
            "confidence variant should survive round-trip: {}",
            json,
        );
    }
}

#[test]
fn character_with_facts_serde_round_trip() {
    let reva = make_character(
        "Reva",
        vec![
            make_fact(
                "The mayor is a cultist",
                14,
                FactSource::Dialogue,
                Confidence::Certain,
            ),
            make_fact(
                "Dragon beneath the mountain",
                1,
                FactSource::Dialogue,
                Confidence::Rumored,
            ),
        ],
    );
    let json = serde_json::to_string(&reva).expect("serialize character with facts");
    let restored: Character =
        serde_json::from_str(&json).expect("deserialize character with facts");
    assert_eq!(restored.known_facts.len(), 2);
    assert_eq!(restored.known_facts[0].content, "The mayor is a cultist");
    assert_eq!(
        restored.known_facts[1].content,
        "Dragon beneath the mountain"
    );
    assert!(matches!(
        restored.known_facts[1].confidence,
        Confidence::Rumored
    ));
}

#[test]
fn character_without_facts_deserializes_with_empty_vec() {
    // Characters serialized before 9-3 won't have known_facts field.
    // serde(default) should handle this gracefully.
    let json = r#"{
        "name": "Reva",
        "description": "A ranger",
        "personality": "Bold",
        "level": 3,
        "hp": 20,
        "max_hp": 20,
        "ac": 14,
        "inventory": {"items": [], "gold": 0},
        "statuses": [],
        "backstory": "Born in the wilds",
        "narrative_state": "Exploring",
        "hooks": [],
        "char_class": "Ranger",
        "race": "Elf",
        "stats": {"STR": 12}
    }"#;
    let character: Character = serde_json::from_str(json).expect("deserialize legacy character");
    assert!(
        character.known_facts.is_empty(),
        "legacy character without known_facts should deserialize with empty vec",
    );
}

// ============================================================================
// AC: Accumulation — new facts append, existing facts not modified
// ============================================================================

#[test]
fn facts_accumulate_by_push() {
    let mut reva = make_character("Reva", vec![]);
    assert!(reva.known_facts.is_empty());

    reva.known_facts.push(make_fact(
        "The bridge is broken",
        2,
        FactSource::Observation,
        Confidence::Certain,
    ));
    assert_eq!(reva.known_facts.len(), 1);

    reva.known_facts.push(make_fact(
        "Secret tunnel found",
        12,
        FactSource::Discovery,
        Confidence::Certain,
    ));
    assert_eq!(reva.known_facts.len(), 2);

    // First fact unchanged after second push
    assert_eq!(reva.known_facts[0].content, "The bridge is broken");
    assert_eq!(reva.known_facts[0].learned_turn, 2);
}

#[test]
fn duplicate_content_facts_both_kept() {
    // Facts are monotonic — even duplicates are kept (no dedup in this story)
    let mut reva = make_character("Reva", vec![]);
    let fact1 = make_fact(
        "The mayor is a cultist",
        14,
        FactSource::Dialogue,
        Confidence::Suspected,
    );
    let fact2 = make_fact(
        "The mayor is a cultist",
        20,
        FactSource::Observation,
        Confidence::Certain,
    );

    reva.known_facts.push(fact1);
    reva.known_facts.push(fact2);

    assert_eq!(reva.known_facts.len(), 2);
    assert_eq!(reva.known_facts[0].learned_turn, 14);
    assert_eq!(reva.known_facts[1].learned_turn, 20);
    assert!(matches!(
        reva.known_facts[0].confidence,
        Confidence::Suspected
    ));
    assert!(matches!(
        reva.known_facts[1].confidence,
        Confidence::Certain
    ));
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn discovered_fact_serde_round_trip() {
    let discovered = DiscoveredFact {
        character_name: "Reva".to_string(),
        fact: make_fact(
            "Corruption detected",
            14,
            FactSource::Observation,
            Confidence::Certain,
        ),
    };
    let json = serde_json::to_string(&discovered).expect("serialize discovered fact");
    let restored: DiscoveredFact =
        serde_json::from_str(&json).expect("deserialize discovered fact");
    assert_eq!(restored.character_name, "Reva");
    assert_eq!(restored.fact.content, "Corruption detected");
}

#[test]
fn world_state_patch_with_facts_serde_round_trip() {
    let patch = WorldStatePatch {
        discovered_facts: Some(vec![
            DiscoveredFact {
                character_name: "Reva".to_string(),
                fact: make_fact(
                    "Grove is corrupted",
                    14,
                    FactSource::Observation,
                    Confidence::Certain,
                ),
            },
            DiscoveredFact {
                character_name: "Kael".to_string(),
                fact: make_fact(
                    "Ambush at dawn",
                    15,
                    FactSource::Discovery,
                    Confidence::Suspected,
                ),
            },
        ]),
        ..Default::default()
    };
    let json = serde_json::to_string(&patch).expect("serialize patch with facts");
    let restored: WorldStatePatch =
        serde_json::from_str(&json).expect("deserialize patch with facts");
    let facts = restored.discovered_facts.unwrap();
    assert_eq!(facts.len(), 2);
    assert_eq!(facts[0].character_name, "Reva");
    assert_eq!(facts[1].character_name, "Kael");
}

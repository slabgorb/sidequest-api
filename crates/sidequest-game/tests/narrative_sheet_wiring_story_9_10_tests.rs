//! Story 9-10: Wire narrative sheet to React client.
//!
//! Tests that CharacterSheetPayload embeds NarrativeSheet fields and that
//! the server constructs CHARACTER_SHEET messages via to_narrative_sheet().

use sidequest_game::ability::AbilityDefinition;
use sidequest_game::character::Character;
use sidequest_game::creature_core::CreatureCore;
use sidequest_game::known_fact::{Confidence, KnownFact};
use sidequest_game::narrative_sheet::NarrativeSheet;

/// Build a test character with abilities and known facts for wiring tests.
fn test_character() -> Character {
    let mut core = CreatureCore::default();
    core.name = "Reva".into();
    core.hp = 20;
    core.max_hp = 30;
    core.level = 4;
    core.statuses = vec!["poisoned".to_string()];

    let mut ch = Character {
        core,
        race: "Elf".into(),
        char_class: "Ranger".into(),
        backstory: "Born under starlight.".into(),
        narrative_state: String::new(),
        hooks: vec![],
        stats: std::collections::HashMap::new(),
        abilities: vec![
            AbilityDefinition {
                name: "Root-Bonding".into(),
                genre_description: "You sense the health of living things through the earth.".into(),
                mechanical_effect: "+2 perception in forests".into(),
                involuntary: true,
            },
            AbilityDefinition {
                name: "Arrow Storm".into(),
                genre_description: "A hail of arrows descends on your foes.".into(),
                mechanical_effect: "3d6 ranged AoE".into(),
                involuntary: false,
            },
        ],
        known_facts: vec![
            KnownFact {
                content: "The grove is corrupted by blight.".into(),
                confidence: Confidence::Certain,
                source: "observation".into(),
                learned_turn: 5,
                category: "Lore".into(),
                fact_id: None,
            },
            KnownFact {
                content: "Elder Mossbeard may know a cure.".into(),
                confidence: Confidence::Suspected,
                source: "rumor".into(),
                learned_turn: 7,
                category: "Person".into(),
                fact_id: None,
            },
        ],
    };
    ch
}

// ── AC-1: NarrativeSheet serializes to JSON matching protocol shape ──────────

#[test]
fn narrative_sheet_serializes_identity_field() {
    let ch = test_character();
    let sheet = ch.to_narrative_sheet("");
    let json = serde_json::to_value(&sheet).unwrap();

    // Identity must be a genre-voiced string, not raw stats
    let identity = json["identity"].as_str().unwrap();
    assert!(identity.contains("Reva"), "identity should contain character name");
    assert!(identity.contains("Elf"), "identity should contain race");
    assert!(identity.contains("Ranger"), "identity should contain class");
    // Must NOT contain raw numbers
    assert!(!identity.contains("20"), "identity must not contain raw HP");
    assert!(!identity.contains("30"), "identity must not contain raw max_hp");
}

#[test]
fn narrative_sheet_serializes_abilities_with_descriptions() {
    let ch = test_character();
    let sheet = ch.to_narrative_sheet("");
    let json = serde_json::to_value(&sheet).unwrap();

    let abilities = json["abilities"].as_array().unwrap();
    assert_eq!(abilities.len(), 2);

    // Each ability has name, description (genre voice), involuntary flag
    let first = &abilities[0];
    assert_eq!(first["name"].as_str().unwrap(), "Root-Bonding");
    assert!(first["description"].as_str().unwrap().contains("sense the health"));
    assert_eq!(first["involuntary"].as_bool().unwrap(), true);

    // Mechanical effects must NOT appear in serialized output
    let json_str = serde_json::to_string(&sheet).unwrap();
    assert!(!json_str.contains("+2 perception"), "mechanical_effect must not appear in serialized sheet");
    assert!(!json_str.contains("3d6"), "mechanical_effect must not appear in serialized sheet");
}

#[test]
fn narrative_sheet_serializes_knowledge_with_confidence() {
    let ch = test_character();
    let sheet = ch.to_narrative_sheet("");
    let json = serde_json::to_value(&sheet).unwrap();

    let knowledge = json["knowledge"].as_array().unwrap();
    assert_eq!(knowledge.len(), 2);

    // Most recent fact first is a nice property but not required by this test;
    // we just verify all facts appear with confidence tags.
    let fact_contents: Vec<&str> = knowledge
        .iter()
        .map(|k| k["content"].as_str().unwrap())
        .collect();
    assert!(fact_contents.iter().any(|c| c.contains("grove is corrupted")));
    assert!(fact_contents.iter().any(|c| c.contains("Elder Mossbeard")));

    // Confidence tags must be present
    let confidences: Vec<&str> = knowledge
        .iter()
        .map(|k| k["confidence"].as_str().unwrap())
        .collect();
    assert!(confidences.contains(&"Certain"));
    assert!(confidences.contains(&"Suspected"));
}

#[test]
fn narrative_sheet_serializes_status_as_narrative_voice() {
    let ch = test_character();
    let sheet = ch.to_narrative_sheet("");
    let json = serde_json::to_value(&sheet).unwrap();

    let status = &json["status"];
    let health = status["health"].as_str().unwrap();
    // HP 20/30 = 66% → "wounded"
    assert_eq!(health, "wounded");

    let conditions = status["conditions"].as_array().unwrap();
    assert_eq!(conditions.len(), 1);
    assert_eq!(conditions[0].as_str().unwrap(), "poisoned");

    // Must NOT contain raw HP numbers
    let status_str = serde_json::to_string(&json["status"]).unwrap();
    assert!(!status_str.contains("20"), "status must not contain raw HP numbers");
    assert!(!status_str.contains("30"), "status must not contain raw max_hp numbers");
}

#[test]
fn narrative_sheet_empty_abilities_serializes_as_empty_array() {
    let mut ch = test_character();
    ch.abilities.clear();
    let sheet = ch.to_narrative_sheet("");
    let json = serde_json::to_value(&sheet).unwrap();
    assert_eq!(json["abilities"].as_array().unwrap().len(), 0);
}

#[test]
fn narrative_sheet_empty_knowledge_serializes_as_empty_array() {
    let mut ch = test_character();
    ch.known_facts.clear();
    let sheet = ch.to_narrative_sheet("");
    let json = serde_json::to_value(&sheet).unwrap();
    assert_eq!(json["knowledge"].as_array().unwrap().len(), 0);
}

#[test]
fn narrative_sheet_no_conditions_serializes_empty() {
    let mut ch = test_character();
    ch.core.statuses.clear();
    let sheet = ch.to_narrative_sheet("");
    let json = serde_json::to_value(&sheet).unwrap();
    assert_eq!(json["status"]["conditions"].as_array().unwrap().len(), 0);
}

// ── AC-2: CharacterSheetPayload matches NarrativeSheet shape ─────────────────

/// The protocol's CharacterSheetPayload must have the same top-level fields
/// as NarrativeSheet: identity, abilities (with descriptions), knowledge, status.
/// This test will fail until the protocol type is updated.
#[test]
fn character_sheet_payload_has_narrative_fields() {
    use sidequest_protocol::CharacterSheetPayload;

    // After story 9-10, CharacterSheetPayload should accept NarrativeSheet-shaped data.
    // This test constructs a payload using the new field names.
    let payload = CharacterSheetPayload {
        identity: "Reva, Elf Ranger".into(),
        abilities: vec![sidequest_protocol::SheetAbility {
            name: "Root-Bonding".into(),
            description: "You sense the health of living things.".into(),
            involuntary: true,
        }],
        knowledge: vec![sidequest_protocol::SheetKnowledge {
            content: "The grove is corrupted.".into(),
            confidence: "Certain".into(),
        }],
        status: sidequest_protocol::SheetStatus {
            health: "wounded".into(),
            conditions: vec!["poisoned".into()],
        },
        portrait_url: None,
    };

    // Round-trip through JSON
    let json = serde_json::to_string(&payload).unwrap();
    let decoded: CharacterSheetPayload = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.identity, "Reva, Elf Ranger");
    assert_eq!(decoded.abilities.len(), 1);
    assert_eq!(decoded.knowledge.len(), 1);
    assert_eq!(decoded.status.health, "wounded");
}

/// The old stat-based fields (level, stats HashMap, backstory) must NOT exist
/// on the updated CharacterSheetPayload.
#[test]
fn character_sheet_payload_rejects_old_stat_fields() {
    // Old-format JSON with raw stats — must fail deserialization
    let old_json = r#"{
        "name": "Grok",
        "class": "Warrior",
        "level": 3,
        "stats": {"strength": 16},
        "abilities": ["Power Strike"],
        "backstory": "A wandering fighter."
    }"#;

    let result = serde_json::from_str::<sidequest_protocol::CharacterSheetPayload>(old_json);
    assert!(result.is_err(), "old stat-based format must be rejected by updated payload");
}

// ── AC-3: CHARACTER_SHEET round-trip with new shape ──────────────────────────

#[test]
fn character_sheet_message_round_trip_new_shape() {
    use sidequest_protocol::{
        CharacterSheetPayload, GameMessage, SheetAbility, SheetKnowledge, SheetStatus,
    };

    let msg = GameMessage::CharacterSheet {
        payload: CharacterSheetPayload {
            identity: "Thrain, Dwarf Warrior".into(),
            abilities: vec![SheetAbility {
                name: "Axe Mastery".into(),
                description: "Your axe is an extension of your will.".into(),
                involuntary: false,
            }],
            knowledge: vec![SheetKnowledge {
                content: "The dragon sleeps beneath the mountain.".into(),
                confidence: "Rumored".into(),
            }],
            status: SheetStatus {
                health: "in good health".into(),
                conditions: vec![],
            },
            portrait_url: Some("/renders/thrain.png".into()),
        },
        player_id: "p1".into(),
    };

    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains(r#""type":"CHARACTER_SHEET""#));
    assert!(json.contains("identity"));
    assert!(!json.contains(r#""level""#), "must not contain old level field");

    let decoded: GameMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(msg, decoded);
}

//! RED tests for Story 9-1: AbilityDefinition model.
//!
//! Genre-voiced ability descriptions with mechanical effects. Each ability
//! carries a narrative description for the player and a mechanical effect
//! for game logic. The `involuntary` flag marks abilities the narrator
//! can trigger without player action.
//!
//! Types under test:
//!   - `AbilityDefinition` — ability with dual voice (genre + mechanical)
//!   - `AbilitySource` — Race, Class, Item, Play
//!   - `AbilityDefinition::display()` — returns genre-voiced text
//!   - `AbilityDefinition::is_involuntary()` — checks involuntary flag

use sidequest_game::ability::{AbilityDefinition, AbilitySource};

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn root_bonding() -> AbilityDefinition {
    AbilityDefinition {
        name: "Root-Bonding".to_string(),
        genre_description: "Your bond with ancient roots lets you sense corruption in living things within thirty paces.".to_string(),
        mechanical_effect: "+2 Nature, detect corruption 30ft".to_string(),
        involuntary: true,
        source: AbilitySource::Race,
    }
}

fn fireball() -> AbilityDefinition {
    AbilityDefinition {
        name: "Fireball".to_string(),
        genre_description: "You gather the heat of the world into your palm and hurl it screaming."
            .to_string(),
        mechanical_effect: "8d6 fire damage, 20ft radius, DEX save for half".to_string(),
        involuntary: false,
        source: AbilitySource::Class,
    }
}

// ===========================================================================
// AC: Dual voice — each ability has genre_description and mechanical_effect
// ===========================================================================

#[test]
fn ability_has_genre_description() {
    let ability = root_bonding();
    assert!(
        ability.genre_description.contains("ancient roots"),
        "genre_description should contain narrative text"
    );
}

#[test]
fn ability_has_mechanical_effect() {
    let ability = root_bonding();
    assert!(
        ability.mechanical_effect.contains("+2 Nature"),
        "mechanical_effect should contain game mechanics"
    );
}

#[test]
fn genre_and_mechanical_are_different() {
    let ability = root_bonding();
    assert_ne!(
        ability.genre_description, ability.mechanical_effect,
        "genre and mechanical descriptions should be different"
    );
}

// ===========================================================================
// AC: Genre display — display() returns genre-voiced text, not mechanics
// ===========================================================================

#[test]
fn display_returns_genre_description() {
    let ability = root_bonding();
    assert_eq!(
        ability.display(),
        "Your bond with ancient roots lets you sense corruption in living things within thirty paces."
    );
}

#[test]
fn display_does_not_return_mechanical_effect() {
    let ability = root_bonding();
    assert!(
        !ability.display().contains("+2"),
        "display() should NOT contain mechanical text"
    );
}

// ===========================================================================
// AC: Involuntary flag — abilities marked for narrator context injection
// ===========================================================================

#[test]
fn involuntary_ability_detected() {
    let ability = root_bonding();
    assert!(
        ability.is_involuntary(),
        "Root-Bonding should be involuntary"
    );
}

#[test]
fn voluntary_ability_detected() {
    let ability = fireball();
    assert!(!ability.is_involuntary(), "Fireball should be voluntary");
}

// ===========================================================================
// AC: Source tracking — ability source (Race/Class/Item/Play) recorded
// ===========================================================================

#[test]
fn source_race() {
    let ability = root_bonding();
    assert_eq!(ability.source, AbilitySource::Race);
}

#[test]
fn source_class() {
    let ability = fireball();
    assert_eq!(ability.source, AbilitySource::Class);
}

#[test]
fn source_item() {
    let ability = AbilityDefinition {
        name: "Flame Tongue".to_string(),
        genre_description: "The blade whispers fire.".to_string(),
        mechanical_effect: "+1d6 fire on hit".to_string(),
        involuntary: false,
        source: AbilitySource::Item,
    };
    assert_eq!(ability.source, AbilitySource::Item);
}

#[test]
fn source_play() {
    let ability = AbilityDefinition {
        name: "Street Wisdom".to_string(),
        genre_description: "Years in the Undercity taught you to read a room before entering."
            .to_string(),
        mechanical_effect: "+1 Perception in urban environments".to_string(),
        involuntary: false,
        source: AbilitySource::Play,
    };
    assert_eq!(ability.source, AbilitySource::Play);
}

// ===========================================================================
// AC: YAML loading — abilities loaded from genre pack YAML
// ===========================================================================

#[test]
fn yaml_deserialization() {
    let yaml = r#"
name: Root-Bonding
genre_description: "Your bond with ancient roots lets you sense corruption."
mechanical_effect: "+2 Nature, detect corruption 30ft"
involuntary: true
source: Race
"#;
    let ability: AbilityDefinition =
        serde_yaml::from_str(yaml).expect("should deserialize from YAML");
    assert_eq!(ability.name, "Root-Bonding");
    assert!(ability.involuntary);
    assert_eq!(ability.source, AbilitySource::Race);
}

#[test]
fn yaml_list_deserialization() {
    let yaml = r#"
- name: Root-Bonding
  genre_description: "Sense corruption in living things."
  mechanical_effect: "+2 Nature"
  involuntary: true
  source: Race
- name: Fireball
  genre_description: "Hurl fire from your palm."
  mechanical_effect: "8d6 fire damage"
  involuntary: false
  source: Class
"#;
    let abilities: Vec<AbilityDefinition> =
        serde_yaml::from_str(yaml).expect("should deserialize list from YAML");
    assert_eq!(abilities.len(), 2);
    assert_eq!(abilities[0].name, "Root-Bonding");
    assert_eq!(abilities[1].name, "Fireball");
}

// ===========================================================================
// AC: Serialization — AbilityDefinition round-trips through serde JSON
// ===========================================================================

#[test]
fn json_round_trip() {
    let original = root_bonding();
    let json = serde_json::to_string(&original).expect("should serialize to JSON");
    let restored: AbilityDefinition =
        serde_json::from_str(&json).expect("should deserialize from JSON");
    assert_eq!(restored.name, original.name);
    assert_eq!(restored.genre_description, original.genre_description);
    assert_eq!(restored.mechanical_effect, original.mechanical_effect);
    assert_eq!(restored.involuntary, original.involuntary);
    assert_eq!(restored.source, original.source);
}

#[test]
fn json_contains_all_fields() {
    let ability = root_bonding();
    let json = serde_json::to_string(&ability).expect("should serialize");
    assert!(json.contains("name"), "JSON should contain name");
    assert!(
        json.contains("genre_description"),
        "JSON should contain genre_description"
    );
    assert!(
        json.contains("mechanical_effect"),
        "JSON should contain mechanical_effect"
    );
    assert!(
        json.contains("involuntary"),
        "JSON should contain involuntary"
    );
    assert!(json.contains("source"), "JSON should contain source");
}

// ===========================================================================
// AC: Character integration — Character holds Vec<AbilityDefinition>
// ===========================================================================

#[test]
fn character_has_abilities_field() {
    use sidequest_game::character::Character;
    use sidequest_game::creature_core::CreatureCore;
    use sidequest_game::inventory::Inventory;
    use sidequest_protocol::NonBlankString;
    use std::collections::HashMap;

    let character = Character {
        core: CreatureCore {
            name: NonBlankString::new("Thorn").unwrap(),
            description: NonBlankString::new("A scarred warrior").unwrap(),
            personality: NonBlankString::new("Stoic").unwrap(),
            level: 3,
            hp: 25,
            max_hp: 30,
            ac: 16,
            inventory: Inventory::default(),
            statuses: vec![],
        },
        backstory: NonBlankString::new("Raised in the wastes").unwrap(),
        narrative_state: String::new(),
        hooks: vec![],
        char_class: NonBlankString::new("Fighter").unwrap(),
        race: NonBlankString::new("Human").unwrap(),
        stats: HashMap::new(),
        abilities: vec![root_bonding(), fireball()],
        is_friendly: true,
    };

    assert_eq!(character.abilities.len(), 2);
    assert_eq!(character.abilities[0].name, "Root-Bonding");
    assert!(character.abilities[0].is_involuntary());
    assert!(!character.abilities[1].is_involuntary());
}

// ===========================================================================
// Edge cases
// ===========================================================================

#[test]
fn empty_mechanical_effect() {
    let ability = AbilityDefinition {
        name: "Passive Aura".to_string(),
        genre_description: "A faint shimmer surrounds you.".to_string(),
        mechanical_effect: String::new(),
        involuntary: true,
        source: AbilitySource::Play,
    };
    assert!(ability.mechanical_effect.is_empty());
    assert!(
        !ability.display().is_empty(),
        "genre display should still work"
    );
}

#[test]
fn ability_implements_debug() {
    let ability = root_bonding();
    let debug = format!("{ability:?}");
    assert!(debug.contains("Root-Bonding"));
}

#[test]
fn ability_implements_clone() {
    let original = root_bonding();
    let cloned = original.clone();
    assert_eq!(cloned.name, "Root-Bonding");
    assert_eq!(cloned.source, AbilitySource::Race);
}

#[test]
fn ability_source_implements_debug() {
    let source = AbilitySource::Play;
    let debug = format!("{source:?}");
    assert!(!debug.is_empty());
}

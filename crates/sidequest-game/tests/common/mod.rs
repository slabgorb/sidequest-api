//! Shared test helpers for sidequest-game integration tests.

use std::collections::HashMap;

use sidequest_game::character::Character;
use sidequest_game::creature_core::CreatureCore;
use sidequest_game::inventory::Inventory;
use sidequest_protocol::NonBlankString;

/// Create a test character with the given name. All other fields use sensible defaults.
pub fn make_character(name: &str) -> Character {
    Character {
        core: CreatureCore {
            name: NonBlankString::new(name).unwrap(),
            description: NonBlankString::new("A brave adventurer").unwrap(),
            personality: NonBlankString::new("Bold and curious").unwrap(),
            level: 1,
            hp: 20,
            max_hp: 20,
            ac: 12,
            xp: 0,
            inventory: Inventory::default(),
            statuses: vec![],
        },
        backstory: NonBlankString::new("Grew up on the frontier").unwrap(),
        narrative_state: String::new(),
        hooks: vec![],
        char_class: NonBlankString::new("Fighter").unwrap(),
        race: NonBlankString::new("Human").unwrap(),
        pronouns: String::new(),
        stats: HashMap::new(),
        abilities: vec![],
        known_facts: vec![],
        affinities: vec![],
        is_friendly: true,
        resolved_archetype: None,
    }
}

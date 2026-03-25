//! Character — unified model combining narrative identity and mechanical stats.
//!
//! ADR-007: Single struct, narrative-first field ordering.
//! Implements Combatant trait (port-lessons.md #10).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sidequest_protocol::NonBlankString;

use crate::combatant::Combatant;
use crate::disposition::Disposition;
use crate::hp::clamp_hp;
use crate::inventory::Inventory;

/// A player character with unified narrative + mechanical identity.
///
/// Narrative fields come first (ADR-007). All string fields that represent
/// identity use `NonBlankString` for validation at construction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Character {
    // Narrative identity (primary)
    /// Character's display name.
    pub name: NonBlankString,
    /// Physical and personality description.
    pub description: NonBlankString,
    /// Character backstory.
    pub backstory: NonBlankString,
    /// Personality traits and mannerisms.
    pub personality: NonBlankString,
    /// Current narrative state summary.
    pub narrative_state: String,
    /// Active narrative hooks (nemesis, mystery, etc.).
    pub hooks: Vec<String>,

    // Mechanical stats
    /// Character class (e.g., "Fighter", "Wizard").
    pub char_class: NonBlankString,
    /// Character race (e.g., "Human", "Dwarf").
    pub race: NonBlankString,
    /// Character level (1+).
    pub level: u32,
    /// Current hit points (0..=max_hp).
    pub hp: i32,
    /// Maximum hit points (>= 1).
    pub max_hp: i32,
    /// Armor class.
    pub ac: i32,
    /// Ability scores (STR, DEX, CON, INT, WIS, CHA).
    pub stats: HashMap<String, i32>,
    /// Inventory of carried items.
    pub inventory: Inventory,
    /// Active status conditions.
    pub statuses: Vec<String>,
}

impl Character {
    /// Apply HP damage or healing, clamped to [0, max_hp].
    pub fn apply_hp_delta(&mut self, delta: i32) {
        self.hp = clamp_hp(self.hp, delta, self.max_hp);
    }
}

impl Combatant for Character {
    fn name(&self) -> &str {
        self.name.as_str()
    }
    fn hp(&self) -> i32 {
        self.hp
    }
    fn max_hp(&self) -> i32 {
        self.max_hp
    }
    fn level(&self) -> u32 {
        self.level
    }
    fn ac(&self) -> i32 {
        self.ac
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to build a valid Character for testing.
    fn test_character() -> Character {
        Character {
            name: NonBlankString::new("Thorn Ironhide").unwrap(),
            description: NonBlankString::new("A scarred dwarf warrior").unwrap(),
            backstory: NonBlankString::new("Raised in the iron mines").unwrap(),
            personality: NonBlankString::new("Gruff but loyal").unwrap(),
            narrative_state: "Exploring the wastes".to_string(),
            hooks: vec!["nemesis: The Warden".to_string()],
            char_class: NonBlankString::new("Fighter").unwrap(),
            race: NonBlankString::new("Dwarf").unwrap(),
            level: 3,
            hp: 25,
            max_hp: 30,
            ac: 16,
            stats: HashMap::from([
                ("STR".to_string(), 16),
                ("DEX".to_string(), 10),
                ("CON".to_string(), 14),
                ("INT".to_string(), 8),
                ("WIS".to_string(), 12),
                ("CHA".to_string(), 6),
            ]),
            inventory: Inventory::default(),
            statuses: vec![],
        }
    }

    // === Combatant trait implementation ===

    #[test]
    fn combatant_name() {
        let c = test_character();
        assert_eq!(c.name(), "Thorn Ironhide");
    }

    #[test]
    fn combatant_hp() {
        let c = test_character();
        assert_eq!(Combatant::hp(&c), 25);
    }

    #[test]
    fn combatant_max_hp() {
        let c = test_character();
        assert_eq!(Combatant::max_hp(&c), 30);
    }

    #[test]
    fn combatant_level() {
        let c = test_character();
        assert_eq!(Combatant::level(&c), 3);
    }

    #[test]
    fn combatant_ac() {
        let c = test_character();
        assert_eq!(Combatant::ac(&c), 16);
    }

    #[test]
    fn combatant_is_alive() {
        let c = test_character();
        assert!(c.is_alive());
    }

    #[test]
    fn combatant_is_dead_at_zero() {
        let mut c = test_character();
        c.hp = 0;
        assert!(!c.is_alive());
    }

    // === HP delta (uses clamp_hp) ===

    #[test]
    fn apply_damage() {
        let mut c = test_character();
        c.apply_hp_delta(-10);
        assert_eq!(c.hp, 15);
    }

    #[test]
    fn apply_healing() {
        let mut c = test_character();
        c.hp = 10;
        c.apply_hp_delta(5);
        assert_eq!(c.hp, 15);
    }

    #[test]
    fn heal_capped_at_max() {
        let mut c = test_character();
        c.apply_hp_delta(100);
        assert_eq!(c.hp, 30); // max_hp
    }

    #[test]
    fn damage_floored_at_zero() {
        let mut c = test_character();
        c.apply_hp_delta(-100);
        assert_eq!(c.hp, 0);
    }

    // === Serde round-trip ===

    #[test]
    fn json_roundtrip() {
        let c = test_character();
        let json = serde_json::to_string(&c).unwrap();
        let back: Character = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name.as_str(), "Thorn Ironhide");
        assert_eq!(back.hp, 25);
        assert_eq!(back.level, 3);
    }

    #[test]
    fn blank_name_rejected_in_json() {
        let json = r#"{"name":"","description":"x","backstory":"x","personality":"x","narrative_state":"","hooks":[],"char_class":"Fighter","race":"Dwarf","level":1,"hp":10,"max_hp":10,"ac":10,"stats":{},"inventory":{"items":[],"gold":0},"statuses":[]}"#;
        let result = serde_json::from_str::<Character>(json);
        assert!(result.is_err(), "blank name should fail deserialization");
    }

    // === Field validation ===

    #[test]
    fn nonblank_fields_validated() {
        // Each NonBlankString field rejects blank input
        assert!(NonBlankString::new("").is_err());
        assert!(NonBlankString::new("   ").is_err());
        assert!(NonBlankString::new("valid").is_ok());
    }
}

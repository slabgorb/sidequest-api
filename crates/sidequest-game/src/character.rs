//! Character — unified model combining narrative identity and mechanical stats.
//!
//! ADR-007: Single struct, narrative-first field ordering.
//! Implements Combatant trait (port-lessons.md #10).
//! Story 1-13: Shared fields extracted to CreatureCore via composition.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sidequest_protocol::NonBlankString;

use crate::ability::AbilityDefinition;
use crate::affinity::AffinityState;
use crate::combatant::Combatant;
use crate::creature_core::CreatureCore;
use crate::known_fact::KnownFact;

/// A player character with unified narrative + mechanical identity.
///
/// Narrative fields come first (ADR-007). Shared creature fields are
/// embedded via `CreatureCore` with `#[serde(flatten)]` for unchanged JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Character {
    /// Shared creature fields (name, description, personality, level, hp, max_hp, ac, inventory, statuses).
    #[serde(flatten)]
    pub core: CreatureCore,

    // Narrative identity (unique to Character)
    /// Character backstory.
    pub backstory: NonBlankString,
    /// Current narrative state summary.
    pub narrative_state: String,
    /// Active narrative hooks (nemesis, mystery, etc.).
    pub hooks: Vec<String>,

    // Mechanical stats (unique to Character)
    /// Character class (e.g., "Fighter", "Wizard").
    pub char_class: NonBlankString,
    /// Character race (e.g., "Human", "Dwarf").
    pub race: NonBlankString,
    /// Player-selected pronouns (e.g., "she/her", "he/him", "they/them").
    #[serde(default)]
    pub pronouns: String,
    /// Ability scores (STR, DEX, CON, INT, WIS, CHA).
    pub stats: HashMap<String, i32>,

    // Character abilities (Story 9-1)
    /// Genre-voiced ability definitions with mechanical effects.
    #[serde(default)]
    pub abilities: Vec<AbilityDefinition>,

    // Character knowledge (Story 9-3)
    /// Facts learned during play — accumulates monotonically.
    #[serde(default)]
    pub known_facts: Vec<KnownFact>,

    // Affinity progression (Story F8)
    /// Per-affinity tier tracking for ability progression.
    #[serde(default)]
    pub affinities: Vec<AffinityState>,

    /// Whether this is a player-controlled (friendly) character.
    #[serde(default = "default_friendly")]
    pub is_friendly: bool,
}

fn default_friendly() -> bool {
    true
}

impl Character {
    /// Apply HP damage or healing, clamped to [0, max_hp].
    pub fn apply_hp_delta(&mut self, delta: i32) {
        self.core.apply_hp_delta(delta);
    }
}

impl Combatant for Character {
    fn name(&self) -> &str {
        self.core.name()
    }
    fn hp(&self) -> i32 {
        Combatant::hp(&self.core)
    }
    fn max_hp(&self) -> i32 {
        Combatant::max_hp(&self.core)
    }
    fn level(&self) -> u32 {
        Combatant::level(&self.core)
    }
    fn ac(&self) -> i32 {
        Combatant::ac(&self.core)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::Inventory;

    /// Helper to build a valid Character for testing.
    fn test_character() -> Character {
        Character {
            core: CreatureCore {
                name: NonBlankString::new("Thorn Ironhide").unwrap(),
                description: NonBlankString::new("A scarred dwarf warrior").unwrap(),
                personality: NonBlankString::new("Gruff but loyal").unwrap(),
                level: 3,
                hp: 25,
                max_hp: 30,
                ac: 16,
                inventory: Inventory::default(),
                statuses: vec![],
            },
            backstory: NonBlankString::new("Raised in the iron mines").unwrap(),
            narrative_state: "Exploring the wastes".to_string(),
            hooks: vec!["nemesis: The Warden".to_string()],
            char_class: NonBlankString::new("Fighter").unwrap(),
            race: NonBlankString::new("Dwarf").unwrap(),
            pronouns: "he/him".to_string(),
            stats: HashMap::from([
                ("STR".to_string(), 16),
                ("DEX".to_string(), 10),
                ("CON".to_string(), 14),
                ("INT".to_string(), 8),
                ("WIS".to_string(), 12),
                ("CHA".to_string(), 6),
            ]),
            abilities: vec![],
            known_facts: vec![],
            affinities: vec![],
            is_friendly: true,
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
        c.core.hp = 0;
        assert!(!c.is_alive());
    }

    // === HP delta (uses clamp_hp) ===

    #[test]
    fn apply_damage() {
        let mut c = test_character();
        c.apply_hp_delta(-10);
        assert_eq!(c.core.hp, 15);
    }

    #[test]
    fn apply_healing() {
        let mut c = test_character();
        c.core.hp = 10;
        c.apply_hp_delta(5);
        assert_eq!(c.core.hp, 15);
    }

    #[test]
    fn heal_capped_at_max() {
        let mut c = test_character();
        c.apply_hp_delta(100);
        assert_eq!(c.core.hp, 30); // max_hp
    }

    #[test]
    fn damage_floored_at_zero() {
        let mut c = test_character();
        c.apply_hp_delta(-100);
        assert_eq!(c.core.hp, 0);
    }

    // === Serde round-trip ===

    #[test]
    fn json_roundtrip() {
        let c = test_character();
        let json = serde_json::to_string(&c).unwrap();
        let back: Character = serde_json::from_str(&json).unwrap();
        assert_eq!(back.core.name.as_str(), "Thorn Ironhide");
        assert_eq!(back.core.hp, 25);
        assert_eq!(back.core.level, 3);
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

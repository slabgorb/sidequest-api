//! CreatureCore — shared fields and behavior for Character and NPC.
//!
//! Story 1-13: Extracted from Character and NPC to eliminate duplication.
//! Both types embed `CreatureCore` via composition.

use serde::{Deserialize, Serialize};
use sidequest_protocol::NonBlankString;

use crate::combatant::Combatant;
use crate::hp::clamp_hp;
use crate::inventory::Inventory;

/// Shared fields for any creature (Character or NPC).
///
/// Embedded via composition with `#[serde(flatten)]` so JSON
/// serialization remains unchanged.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatureCore {
    /// Creature's display name.
    pub name: NonBlankString,
    /// Physical description.
    pub description: NonBlankString,
    /// Personality traits and mannerisms.
    pub personality: NonBlankString,
    /// Creature level (1+).
    pub level: u32,
    /// Current hit points (0..=max_hp).
    pub hp: i32,
    /// Maximum hit points (>= 1).
    pub max_hp: i32,
    /// Armor class.
    pub ac: i32,
    /// Experience points accumulated.
    #[serde(default)]
    pub xp: u32,
    /// Inventory of carried items.
    pub inventory: Inventory,
    /// Active status conditions.
    pub statuses: Vec<String>,
}

impl CreatureCore {
    /// Apply HP damage or healing, clamped to [0, max_hp].
    pub fn apply_hp_delta(&mut self, delta: i32) {
        let old_hp = self.hp;
        self.hp = clamp_hp(self.hp, delta, self.max_hp);
        let clamped = self.hp != old_hp + delta;
        let span = tracing::info_span!(
            "creature.hp_delta",
            name = %self.name,
            old_hp = old_hp,
            new_hp = self.hp,
            delta = delta,
            clamped = clamped,
        );
        let _guard = span.enter();
    }
}

impl Combatant for CreatureCore {
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

    fn test_core() -> CreatureCore {
        CreatureCore {
            name: NonBlankString::new("Test Creature").unwrap(),
            description: NonBlankString::new("A test creature").unwrap(),
            personality: NonBlankString::new("Testy").unwrap(),
            level: 3,
            hp: 20,
            max_hp: 30,
            ac: 15,
            xp: 0,
            inventory: Inventory::default(),
            statuses: vec![],
        }
    }

    #[test]
    fn combatant_accessors() {
        let c = test_core();
        assert_eq!(c.name(), "Test Creature");
        assert_eq!(Combatant::hp(&c), 20);
        assert_eq!(Combatant::max_hp(&c), 30);
        assert_eq!(Combatant::level(&c), 3);
        assert_eq!(Combatant::ac(&c), 15);
    }

    #[test]
    fn combatant_is_alive() {
        let c = test_core();
        assert!(c.is_alive());
    }

    #[test]
    fn apply_damage() {
        let mut c = test_core();
        c.apply_hp_delta(-10);
        assert_eq!(c.hp, 10);
    }

    #[test]
    fn heal_capped_at_max() {
        let mut c = test_core();
        c.apply_hp_delta(100);
        assert_eq!(c.hp, 30);
    }

    #[test]
    fn damage_floored_at_zero() {
        let mut c = test_core();
        c.apply_hp_delta(-100);
        assert_eq!(c.hp, 0);
    }

    #[test]
    fn json_roundtrip() {
        let c = test_core();
        let json = serde_json::to_string(&c).unwrap();
        let back: CreatureCore = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name.as_str(), "Test Creature");
        assert_eq!(back.hp, 20);
        assert_eq!(back.level, 3);
    }
}

//! NPC — Non-Player Characters with disposition-based attitude.
//!
//! ADR-020: Numeric disposition derives qualitative attitude.
//! Implements Combatant trait (port-lessons.md #10).

use serde::{Deserialize, Serialize};
use sidequest_protocol::NonBlankString;

use crate::combatant::Combatant;
use crate::disposition::{Attitude, Disposition};
use crate::hp::clamp_hp;
use crate::inventory::Inventory;

/// A non-player character in the game world.
///
/// NPCs have a numeric disposition that maps to an attitude (ADR-020).
/// Agents see the attitude string; the world_state agent patches disposition numerically.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Npc {
    // Identity
    /// NPC's display name.
    pub name: NonBlankString,
    /// Physical description.
    pub description: NonBlankString,
    /// Personality and behavior patterns.
    pub personality: NonBlankString,
    /// Optional TTS voice mapping.
    pub voice_id: Option<i32>,

    // Disposition (ADR-020)
    /// Numeric disposition value — derives attitude via thresholds.
    pub disposition: Disposition,

    // Mechanical (Combatant)
    /// NPC level.
    pub level: u32,
    /// Current hit points.
    pub hp: i32,
    /// Maximum hit points.
    pub max_hp: i32,
    /// Armor class.
    pub ac: i32,

    // State
    /// Current location (None = off-stage).
    pub location: Option<NonBlankString>,
    /// Active status conditions.
    pub statuses: Vec<String>,
    /// NPC inventory.
    pub inventory: Inventory,
}

impl Npc {
    /// Get the NPC's current attitude based on disposition.
    pub fn attitude(&self) -> Attitude {
        self.disposition.attitude()
    }

    /// Apply HP damage or healing, clamped to [0, max_hp].
    pub fn apply_hp_delta(&mut self, delta: i32) {
        self.hp = clamp_hp(self.hp, delta, self.max_hp);
    }

    /// Apply a disposition delta and return the new attitude.
    pub fn apply_disposition_delta(&mut self, delta: i32) -> Attitude {
        self.disposition.apply_delta(delta);
        self.attitude()
    }
}

impl Combatant for Npc {
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

    fn friendly_innkeeper() -> Npc {
        Npc {
            name: NonBlankString::new("Marta the Innkeeper").unwrap(),
            description: NonBlankString::new("A stout woman with flour-dusted hands").unwrap(),
            personality: NonBlankString::new("Warm and gossipy").unwrap(),
            voice_id: Some(3),
            disposition: Disposition::new(15),
            level: 2,
            hp: 12,
            max_hp: 12,
            ac: 10,
            location: Some(NonBlankString::new("The Rusty Nail Inn").unwrap()),
            statuses: vec![],
            inventory: Inventory::default(),
        }
    }

    fn hostile_bandit() -> Npc {
        Npc {
            name: NonBlankString::new("Razortooth").unwrap(),
            description: NonBlankString::new("A scarred raider with missing teeth").unwrap(),
            personality: NonBlankString::new("Cruel and cunning").unwrap(),
            voice_id: None,
            disposition: Disposition::new(-20),
            level: 4,
            hp: 18,
            max_hp: 22,
            ac: 14,
            location: None, // off-stage
            statuses: vec!["enraged".to_string()],
            inventory: Inventory::default(),
        }
    }

    // === Attitude derivation (ADR-020) ===

    #[test]
    fn friendly_npc_attitude() {
        let npc = friendly_innkeeper();
        assert_eq!(npc.attitude(), Attitude::Friendly);
    }

    #[test]
    fn hostile_npc_attitude() {
        let npc = hostile_bandit();
        assert_eq!(npc.attitude(), Attitude::Hostile);
    }

    #[test]
    fn neutral_npc_attitude() {
        let mut npc = friendly_innkeeper();
        npc.disposition = Disposition::new(0);
        assert_eq!(npc.attitude(), Attitude::Neutral);
    }

    // === Disposition delta ===

    #[test]
    fn disposition_delta_changes_attitude() {
        let mut npc = friendly_innkeeper(); // disposition 15 = friendly
        let attitude = npc.apply_disposition_delta(-20); // now -5 = neutral
        assert_eq!(attitude, Attitude::Neutral);
    }

    #[test]
    fn disposition_delta_crosses_to_hostile() {
        let mut npc = friendly_innkeeper(); // disposition 15
        let attitude = npc.apply_disposition_delta(-30); // now -15 = hostile
        assert_eq!(attitude, Attitude::Hostile);
    }

    // === Combatant trait ===

    #[test]
    fn combatant_name() {
        let npc = friendly_innkeeper();
        assert_eq!(Combatant::name(&npc), "Marta the Innkeeper");
    }

    #[test]
    fn combatant_is_alive() {
        let npc = friendly_innkeeper();
        assert!(npc.is_alive());
    }

    #[test]
    fn combatant_dead_at_zero() {
        let mut npc = friendly_innkeeper();
        npc.hp = 0;
        assert!(!npc.is_alive());
    }

    // === HP delta ===

    #[test]
    fn apply_damage() {
        let mut npc = friendly_innkeeper();
        npc.apply_hp_delta(-5);
        assert_eq!(npc.hp, 7);
    }

    #[test]
    fn damage_floored_at_zero() {
        let mut npc = friendly_innkeeper();
        npc.apply_hp_delta(-100);
        assert_eq!(npc.hp, 0);
    }

    #[test]
    fn heal_capped_at_max() {
        let mut npc = friendly_innkeeper();
        npc.hp = 5;
        npc.apply_hp_delta(100);
        assert_eq!(npc.hp, 12);
    }

    // === Location ===

    #[test]
    fn on_stage_npc_has_location() {
        let npc = friendly_innkeeper();
        assert!(npc.location.is_some());
        assert_eq!(npc.location.unwrap().as_str(), "The Rusty Nail Inn");
    }

    #[test]
    fn off_stage_npc_has_no_location() {
        let npc = hostile_bandit();
        assert!(npc.location.is_none());
    }

    // === Voice ID ===

    #[test]
    fn voice_id_present() {
        let npc = friendly_innkeeper();
        assert_eq!(npc.voice_id, Some(3));
    }

    #[test]
    fn voice_id_absent() {
        let npc = hostile_bandit();
        assert_eq!(npc.voice_id, None);
    }

    // === Serde round-trip ===

    #[test]
    fn json_roundtrip() {
        let npc = friendly_innkeeper();
        let json = serde_json::to_string(&npc).unwrap();
        let back: Npc = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name.as_str(), "Marta the Innkeeper");
        assert_eq!(back.disposition.value(), 15);
        assert_eq!(back.attitude(), Attitude::Friendly);
    }

    #[test]
    fn blank_name_rejected_in_json() {
        let json = r#"{"name":"","description":"x","personality":"x","voice_id":null,"disposition":0,"level":1,"hp":10,"max_hp":10,"ac":10,"location":null,"statuses":[],"inventory":{"items":[],"gold":0}}"#;
        let result = serde_json::from_str::<Npc>(json);
        assert!(result.is_err(), "blank name should fail deserialization");
    }
}

//! NPC — Non-Player Characters with disposition-based attitude.
//!
//! ADR-020: Numeric disposition derives qualitative attitude.
//! Implements Combatant trait (port-lessons.md #10).
//! Story 1-13: Shared fields extracted to CreatureCore via composition.

use serde::{Deserialize, Serialize};
use sidequest_protocol::NonBlankString;

use crate::belief_state::BeliefState;
use crate::combatant::Combatant;
use crate::creature_core::CreatureCore;
use crate::disposition::{Attitude, Disposition};
use crate::ocean::OceanProfile;
use crate::state::NpcPatch;

/// A non-player character in the game world.
///
/// NPCs have a numeric disposition that maps to an attitude (ADR-020).
/// Agents see the attitude string; the world_state agent patches disposition numerically.
/// Shared creature fields are embedded via `CreatureCore` with `#[serde(flatten)]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Npc {
    /// Shared creature fields (name, description, personality, level, hp, max_hp, ac, inventory, statuses).
    #[serde(flatten)]
    pub core: CreatureCore,

    // NPC-specific fields
    /// Optional TTS voice mapping.
    pub voice_id: Option<i32>,
    /// Numeric disposition value — derives attitude via thresholds.
    pub disposition: Disposition,
    /// Current location (None = off-stage).
    pub location: Option<NonBlankString>,
    /// Pronouns (identity-locked: set once, never overwritten).
    #[serde(default)]
    pub pronouns: Option<String>,
    /// Physical appearance (identity-locked: set once, never overwritten).
    #[serde(default)]
    pub appearance: Option<String>,
    /// Age description (identity-locked: set once, never overwritten).
    #[serde(default)]
    pub age: Option<String>,
    /// Body build descriptor (identity-locked: "stocky", "slender", "muscular", etc.).
    #[serde(default)]
    pub build: Option<String>,
    /// Relative height descriptor (identity-locked: "tall", "short", "towering", etc.).
    #[serde(default)]
    pub height: Option<String>,
    /// Specific visual details for consistent portrayal (identity-locked).
    /// E.g., "scar across left eye", "silver-streaked hair", "missing right hand".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub distinguishing_features: Vec<String>,
    /// OCEAN personality profile (Story 10-1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ocean: Option<OceanProfile>,
    /// Per-NPC knowledge bubbles for the Scenario System (Story 7-1).
    #[serde(default)]
    pub belief_state: BeliefState,
}

impl Npc {
    /// Create a minimal NPC for combat — just enough for resolve_attack to work.
    /// Full NPC data (appearance, personality, etc.) can be enriched later.
    pub fn combat_minimal(name: &str, hp: i32, max_hp: i32, level: u32) -> Self {
        Self {
            core: CreatureCore {
                name: NonBlankString::new(name).unwrap_or_else(|_| NonBlankString::new("Unknown").unwrap()),
                description: NonBlankString::new("combatant").unwrap(),
                personality: NonBlankString::new("hostile").unwrap(),
                level,
                hp,
                max_hp,
                ac: 10,
                inventory: crate::Inventory::default(),
                statuses: vec![],
                xp: 0,
            },
            voice_id: None,
            disposition: Disposition::new(-20), // hostile
            location: None,
            pronouns: None,
            appearance: None,
            age: None,
            build: None,
            height: None,
            distinguishing_features: vec![],
            ocean: None,
            belief_state: BeliefState::default(),
        }
    }

    /// Get the NPC's current attitude based on disposition + OCEAN agreeableness offset.
    pub fn attitude(&self) -> Attitude {
        Disposition::new(self.effective_disposition()).attitude()
    }

    /// Apply HP damage or healing, clamped to [0, max_hp].
    pub fn apply_hp_delta(&mut self, delta: i32) {
        self.core.apply_hp_delta(delta);
    }

    /// Apply a disposition delta and return the new attitude.
    pub fn apply_disposition_delta(&mut self, delta: i32) -> Attitude {
        self.disposition.apply_delta(delta);
        self.attitude()
    }

    /// Disposition offset from OCEAN Agreeableness dimension.
    /// Returns 0 if no OCEAN profile is set.
    pub fn agreeableness_disposition_offset(&self) -> i32 {
        match &self.ocean {
            Some(ocean) => ((ocean.agreeableness - 5.0) * 1.0).round() as i32,
            None => 0,
        }
    }

    /// Effective disposition: base disposition value plus agreeableness offset.
    pub fn effective_disposition(&self) -> i32 {
        self.disposition.value() + self.agreeableness_disposition_offset()
    }

    /// Merge mutable fields from a patch. Identity fields (pronouns, appearance)
    /// are locked after first set — subsequent patches cannot overwrite them.
    pub fn merge_patch(&mut self, patch: &NpcPatch) {
        let span = tracing::info_span!(
            "npc_merge_patch",
            npc_name = self.core.name.as_str(),
            fields_changed = tracing::field::Empty,
            identity_fields_locked = tracing::field::Empty,
        );
        let _guard = span.enter();

        let mut changed = Vec::new();
        let mut locked = Vec::new();

        if let Some(ref desc) = patch.description {
            self.core.description =
                NonBlankString::new(desc).unwrap_or_else(|_| self.core.description.clone());
            changed.push("description");
        }
        if let Some(ref loc) = patch.location {
            self.location = NonBlankString::new(loc).ok();
            changed.push("location");
        }

        // Identity-locked: only write if currently empty
        if self.pronouns.is_none() {
            if let Some(ref p) = patch.pronouns {
                self.pronouns = Some(p.clone());
                changed.push("pronouns");
            }
        } else if patch.pronouns.is_some() {
            locked.push("pronouns");
        }
        if self.appearance.is_none() {
            if let Some(ref a) = patch.appearance {
                self.appearance = Some(a.clone());
                changed.push("appearance");
            }
        } else if patch.appearance.is_some() {
            locked.push("appearance");
        }
        if self.age.is_none() {
            if let Some(ref a) = patch.age {
                self.age = Some(a.clone());
                changed.push("age");
            }
        } else if patch.age.is_some() {
            locked.push("age");
        }
        if self.build.is_none() {
            if let Some(ref b) = patch.build {
                self.build = Some(b.clone());
                changed.push("build");
            }
        } else if patch.build.is_some() {
            locked.push("build");
        }
        if self.height.is_none() {
            if let Some(ref h) = patch.height {
                self.height = Some(h.clone());
                changed.push("height");
            }
        } else if patch.height.is_some() {
            locked.push("height");
        }
        if self.distinguishing_features.is_empty() {
            if let Some(ref df) = patch.distinguishing_features {
                self.distinguishing_features = df.clone();
                changed.push("distinguishing_features");
            }
        } else if patch.distinguishing_features.is_some() {
            locked.push("distinguishing_features");
        }

        span.record("fields_changed", tracing::field::display(changed.join(",")));
        if !locked.is_empty() {
            span.record("identity_fields_locked", tracing::field::display(locked.join(",")));
        }
    }
}

impl Combatant for Npc {
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

    fn friendly_innkeeper() -> Npc {
        Npc {
            core: CreatureCore {
                name: NonBlankString::new("Marta the Innkeeper").unwrap(),
                description: NonBlankString::new("A stout woman with flour-dusted hands").unwrap(),
                personality: NonBlankString::new("Warm and gossipy").unwrap(),
                level: 2,
                hp: 12,
                max_hp: 12,
                ac: 10,
                xp: 0,
                statuses: vec![],
                inventory: Inventory::default(),
            },
            voice_id: Some(3),
            disposition: Disposition::new(15),
            location: Some(NonBlankString::new("The Rusty Nail Inn").unwrap()),
            pronouns: Some("she/her".to_string()),
            appearance: Some("Flour-dusted apron".to_string()),
            age: Some("middle-aged".to_string()),
            build: Some("stocky".to_string()),
            height: Some("short".to_string()),
            distinguishing_features: vec!["flour-dusted hands".to_string()],
            ocean: None,
            belief_state: BeliefState::default(),
        }
    }

    fn hostile_bandit() -> Npc {
        Npc {
            core: CreatureCore {
                name: NonBlankString::new("Razortooth").unwrap(),
                description: NonBlankString::new("A scarred raider with missing teeth").unwrap(),
                personality: NonBlankString::new("Cruel and cunning").unwrap(),
                level: 4,
                hp: 18,
                max_hp: 22,
                ac: 14,
                xp: 0,
                statuses: vec!["enraged".to_string()],
                inventory: Inventory::default(),
            },
            voice_id: None,
            disposition: Disposition::new(-20),
            location: None, // off-stage
            pronouns: None,
            appearance: None,
            age: None,
            build: None,
            height: None,
            distinguishing_features: vec![],
            ocean: None,
            belief_state: BeliefState::default(),
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
        npc.core.hp = 0;
        assert!(!npc.is_alive());
    }

    // === HP delta ===

    #[test]
    fn apply_damage() {
        let mut npc = friendly_innkeeper();
        npc.apply_hp_delta(-5);
        assert_eq!(npc.core.hp, 7);
    }

    #[test]
    fn damage_floored_at_zero() {
        let mut npc = friendly_innkeeper();
        npc.apply_hp_delta(-100);
        assert_eq!(npc.core.hp, 0);
    }

    #[test]
    fn heal_capped_at_max() {
        let mut npc = friendly_innkeeper();
        npc.core.hp = 5;
        npc.apply_hp_delta(100);
        assert_eq!(npc.core.hp, 12);
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
        assert_eq!(back.core.name.as_str(), "Marta the Innkeeper");
        assert_eq!(back.disposition.value(), 15);
        assert_eq!(back.attitude(), Attitude::Friendly);
    }

    #[test]
    fn blank_name_rejected_in_json() {
        let json = r#"{"name":"","description":"x","personality":"x","voice_id":null,"disposition":0,"level":1,"hp":10,"max_hp":10,"ac":10,"location":null,"statuses":[],"inventory":{"items":[],"gold":0}}"#;
        let result = serde_json::from_str::<Npc>(json);
        assert!(result.is_err(), "blank name should fail deserialization");
    }

    // === Physical description fields ===

    #[test]
    fn physical_fields_present() {
        let npc = friendly_innkeeper();
        assert_eq!(npc.build, Some("stocky".to_string()));
        assert_eq!(npc.height, Some("short".to_string()));
        assert_eq!(npc.distinguishing_features, vec!["flour-dusted hands"]);
    }

    #[test]
    fn physical_fields_identity_locked_in_merge() {
        let mut npc = friendly_innkeeper();
        let patch = NpcPatch {
            name: "Marta the Innkeeper".to_string(),
            description: None,
            personality: None,
            role: None,
            pronouns: Some("he/him".to_string()), // should NOT overwrite
            appearance: Some("New appearance".to_string()), // should NOT overwrite
            age: Some("young".to_string()), // should NOT overwrite
            build: Some("slender".to_string()), // should NOT overwrite
            height: Some("tall".to_string()), // should NOT overwrite
            distinguishing_features: Some(vec!["tattoo".to_string()]), // should NOT overwrite
            location: None,
        };
        npc.merge_patch(&patch);
        // All identity-locked fields should retain original values
        assert_eq!(npc.pronouns, Some("she/her".to_string()));
        assert_eq!(npc.appearance, Some("Flour-dusted apron".to_string()));
        assert_eq!(npc.age, Some("middle-aged".to_string()));
        assert_eq!(npc.build, Some("stocky".to_string()));
        assert_eq!(npc.height, Some("short".to_string()));
        assert_eq!(npc.distinguishing_features, vec!["flour-dusted hands"]);
    }

    #[test]
    fn physical_fields_set_when_empty() {
        let mut npc = hostile_bandit(); // all physical fields empty
        let patch = NpcPatch {
            name: "Razortooth".to_string(),
            description: None,
            personality: None,
            role: None,
            pronouns: Some("he/him".to_string()),
            appearance: Some("Scarred face".to_string()),
            age: Some("old".to_string()),
            build: Some("muscular".to_string()),
            height: Some("tall".to_string()),
            distinguishing_features: Some(vec!["missing teeth".to_string(), "neck scar".to_string()]),
            location: None,
        };
        npc.merge_patch(&patch);
        assert_eq!(npc.pronouns, Some("he/him".to_string()));
        assert_eq!(npc.appearance, Some("Scarred face".to_string()));
        assert_eq!(npc.age, Some("old".to_string()));
        assert_eq!(npc.build, Some("muscular".to_string()));
        assert_eq!(npc.height, Some("tall".to_string()));
        assert_eq!(npc.distinguishing_features, vec!["missing teeth", "neck scar"]);
    }

    // === Registry enrichment ===

    #[test]
    fn enrich_registry_backfills_physical_data() {
        let npcs = vec![friendly_innkeeper()];
        let mut registry = vec![NpcRegistryEntry {
            name: "Marta the Innkeeper".to_string(),
            pronouns: String::new(),
            role: "innkeeper".to_string(),
            location: "The Rusty Nail Inn".to_string(),
            last_seen_turn: 1,
            age: String::new(),
            appearance: String::new(),
            ocean_summary: String::new(),
            ocean: None,
            hp: 0,
            max_hp: 0,
        }];
        enrich_registry_from_npcs(&mut registry, &npcs);
        assert_eq!(registry[0].pronouns, "she/her");
        assert_eq!(registry[0].age, "middle-aged");
        assert_eq!(registry[0].appearance, "Flour-dusted apron");
    }

    #[test]
    fn enrich_registry_does_not_overwrite_existing() {
        let npcs = vec![friendly_innkeeper()];
        let mut registry = vec![NpcRegistryEntry {
            name: "Marta the Innkeeper".to_string(),
            pronouns: "they/them".to_string(), // pre-existing
            role: "innkeeper".to_string(),
            location: "The Rusty Nail Inn".to_string(),
            last_seen_turn: 1,
            age: "elderly".to_string(), // pre-existing
            appearance: String::new(), // empty — should be backfilled
            ocean_summary: String::new(),
            ocean: None,
            hp: 0,
            max_hp: 0,
        }];
        enrich_registry_from_npcs(&mut registry, &npcs);
        assert_eq!(registry[0].pronouns, "they/them"); // unchanged
        assert_eq!(registry[0].age, "elderly"); // unchanged
        assert_eq!(registry[0].appearance, "Flour-dusted apron"); // backfilled
    }
}

/// Lightweight NPC identity entry for the narrator registry.
///
/// Tracks name, pronouns, role, last-seen location, and physical description so
/// the narrator prompt can maintain NPC identity consistency across turns. Much
/// lighter than `Npc` — no combat stats, no inventory, no disposition.
///
/// Serializable for persistence in GameSnapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NpcRegistryEntry {
    /// NPC name (as extracted from narration).
    pub name: String,
    /// Pronouns (he/him, she/her, they/them, it).
    pub pronouns: String,
    /// Brief role description (e.g., "merchant", "guard captain").
    pub role: String,
    /// Last known location.
    pub location: String,
    /// Interaction number when this NPC was last seen.
    pub last_seen_turn: u32,
    /// Age description (backfilled from Npc data when available).
    #[serde(default)]
    pub age: String,
    /// Physical appearance (backfilled from Npc data when available).
    #[serde(default)]
    pub appearance: String,
    /// OCEAN behavioral summary (generated from archetype baseline with jitter).
    /// E.g., "reserved and quiet, meticulous and disciplined".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ocean_summary: String,
    /// Full OCEAN personality profile (Story 15-2).
    /// Source of truth — `ocean_summary` is derived from this via `behavioral_summary()`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ocean: Option<crate::ocean::OceanProfile>,
    /// Current HP (tracked during combat via combat patches).
    #[serde(default)]
    pub hp: i32,
    /// Maximum HP (set when NPC enters combat).
    #[serde(default)]
    pub max_hp: i32,
}

/// Enrich registry entries with physical description data from full Npc structs.
///
/// Called after `update_npc_registry` to backfill age, appearance, and other
/// identity-locked fields that regex extraction can't capture. Matches by name.
pub fn enrich_registry_from_npcs(registry: &mut [NpcRegistryEntry], npcs: &[Npc]) {
    for entry in registry.iter_mut() {
        if let Some(npc) = npcs.iter().find(|n| n.name() == entry.name) {
            if entry.age.is_empty() {
                if let Some(ref age) = npc.age {
                    entry.age = age.clone();
                }
            }
            if entry.appearance.is_empty() {
                if let Some(ref appearance) = npc.appearance {
                    entry.appearance = appearance.clone();
                }
            }
            if entry.pronouns.is_empty() {
                if let Some(ref pronouns) = npc.pronouns {
                    entry.pronouns = pronouns.clone();
                }
            }
        }
    }
}

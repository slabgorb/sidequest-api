//! Character — unified model combining narrative identity and mechanical stats.
//!
//! ADR-007: Single struct, narrative-first field ordering.
//! Implements Combatant trait (port-lessons.md #10).
//! Story 1-13: Shared fields extracted to CreatureCore via composition.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sidequest_genre::archetype::ArchetypeResolved;
use sidequest_genre::resolver::Resolved;
use sidequest_protocol::{NonBlankString, Provenance, Tier};

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

    /// Resolved archetype name from the three-axis system (jungian/rpg_role).
    /// Set during chargen when axis hints are available. Full resolution through
    /// constraints and funnels happens in the dispatch layer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_archetype: Option<String>,

    /// Provenance for `resolved_archetype` — which tier (Global / Genre /
    /// World / Culture) and which YAML file produced the final archetype
    /// value, plus the full merge trail. Populated by the dispatch layer
    /// at the same call site that sets `resolved_archetype`. Flows out to
    /// the UI on `CharacterState.archetype_provenance` for GM-panel display.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archetype_provenance: Option<Provenance>,
}

fn default_friendly() -> bool {
    true
}

impl Character {
    /// Apply a four-tier archetype resolution to the character — sets
    /// both the display name (`resolved_archetype`) and the full
    /// tier-annotated provenance (`archetype_provenance`) in one place.
    ///
    /// Called by the dispatch layer after `sidequest_genre::archetype::
    /// resolve_archetype` produces a result, so the two fields stay in
    /// lockstep. The provenance is what the GM panel consumes via the
    /// `CharacterState.archetype_provenance` wire field (Phase G2).
    pub fn apply_archetype_resolved(&mut self, resolved: &Resolved<ArchetypeResolved>) {
        self.resolved_archetype = Some(resolved.value.name.clone());
        self.archetype_provenance = Some(resolved.provenance.clone());
    }

    /// Produce a genre-voiced narrative character sheet for player display.
    ///
    /// The sheet uses narrative descriptions throughout — no raw numbers,
    /// no mechanical effects, no stat blocks. Genre voice is used to compose
    /// the identity line from name, race, and class.
    ///
    /// Story 9-5.
    pub fn to_narrative_sheet(&self, _genre_voice: &str) -> crate::narrative_sheet::NarrativeSheet {
        use crate::narrative_sheet::{
            AbilityEntry, CharacterStatus, KnowledgeEntry, NarrativeSheet,
        };

        let identity = format!("{}, {} {}", self.core.name, self.race, self.char_class);

        let abilities = self
            .abilities
            .iter()
            .map(|a| AbilityEntry {
                name: a.name.clone(),
                description: a.genre_description.clone(),
                involuntary: a.involuntary,
            })
            .collect();

        let knowledge = self
            .known_facts
            .iter()
            .map(|f| KnowledgeEntry {
                content: f.content.clone(),
                confidence: f.confidence.clone(),
            })
            .collect();

        let status = CharacterStatus::from_creature(
            self.core.edge.current,
            self.core.edge.max,
            &self.core.statuses,
        );

        NarrativeSheet {
            identity,
            abilities,
            knowledge,
            status,
        }
    }
}

impl Combatant for Character {
    fn name(&self) -> &str {
        self.core.name()
    }
    fn edge(&self) -> i32 {
        Combatant::edge(&self.core)
    }
    fn max_edge(&self) -> i32 {
        Combatant::max_edge(&self.core)
    }
    fn level(&self) -> u32 {
        Combatant::level(&self.core)
    }
}

/// Extension trait that gives the GM-panel / OTEL watcher a stable,
/// lowercase tier label for a resolved value.
///
/// `format!("{:?}", tier)` would print `"Culture"`; the panel wants
/// `"culture"` to match the wire format (see `Tier`'s
/// `#[serde(rename_all = "lowercase")]`). Any `Resolved<T>` — archetype
/// or future content types — can call `.source_tier_for_panel()`.
pub trait ProvenancePanelExt {
    /// Lowercase tier label — `"global" | "genre" | "world" | "culture"`.
    fn source_tier_for_panel(&self) -> &'static str;
}

impl<T> ProvenancePanelExt for Resolved<T> {
    fn source_tier_for_panel(&self) -> &'static str {
        match self.provenance.source_tier {
            Tier::Global => "global",
            Tier::Genre => "genre",
            Tier::World => "world",
            Tier::Culture => "culture",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::Inventory;

    /// Helper to build a valid Character for testing.
    fn test_character() -> Character {
        use crate::creature_core::placeholder_edge_pool;
        Character {
            core: CreatureCore {
                name: NonBlankString::new("Thorn Ironhide").unwrap(),
                description: NonBlankString::new("A scarred dwarf warrior").unwrap(),
                personality: NonBlankString::new("Gruff but loyal").unwrap(),
                level: 3,
                xp: 0,
                inventory: Inventory::default(),
                statuses: vec![],
                edge: placeholder_edge_pool(),
                acquired_advancements: vec![],
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
            resolved_archetype: None,
            archetype_provenance: None,
        }
    }

    // === Combatant trait implementation ===

    #[test]
    fn combatant_name() {
        let c = test_character();
        assert_eq!(c.name(), "Thorn Ironhide");
    }

    #[test]
    fn combatant_edge() {
        let c = test_character();
        assert_eq!(Combatant::edge(&c), c.core.edge.current);
    }

    #[test]
    fn combatant_max_edge() {
        let c = test_character();
        assert_eq!(Combatant::max_edge(&c), c.core.edge.max);
    }

    #[test]
    fn combatant_level() {
        let c = test_character();
        assert_eq!(Combatant::level(&c), 3);
    }

    #[test]
    fn combatant_not_broken_at_full_edge() {
        let c = test_character();
        assert!(!c.is_broken());
    }

    #[test]
    fn combatant_broken_at_zero_edge() {
        let mut c = test_character();
        c.core.edge.current = 0;
        assert!(c.is_broken());
    }

    // === Edge delta (uses EdgePool::apply_delta) ===

    #[test]
    fn apply_damage_via_edge() {
        let mut c = test_character();
        let before = c.core.edge.current;
        c.core.edge.apply_delta(-3);
        assert_eq!(c.core.edge.current, before - 3);
    }

    #[test]
    fn damage_floored_at_zero() {
        let mut c = test_character();
        c.core.edge.apply_delta(-1000);
        assert_eq!(c.core.edge.current, 0);
    }

    // === Serde round-trip ===

    #[test]
    fn json_roundtrip() {
        let c = test_character();
        let json = serde_json::to_string(&c).unwrap();
        let back: Character = serde_json::from_str(&json).unwrap();
        assert_eq!(back.core.name.as_str(), "Thorn Ironhide");
        assert_eq!(back.core.edge.base_max, c.core.edge.base_max);
        assert_eq!(back.core.level, 3);
    }

    #[test]
    fn blank_name_rejected_in_json() {
        let json = r#"{"name":"","description":"x","backstory":"x","personality":"x","narrative_state":"","hooks":[],"char_class":"Fighter","race":"Dwarf","level":1,"edge":{"current":5,"max":5,"base_max":5,"recovery_triggers":[],"thresholds":[]},"stats":{},"inventory":{"items":[],"gold":0},"statuses":[]}"#;
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

//! World-level archetype funnel structs.
//!
//! Funnels resolve [jungian, rpg_role] axis pairs to named world archetypes,
//! collapsing the combinatorial space defined by genre-level constraints into
//! concrete, lore-grounded identities specific to a world.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single funnel entry — maps multiple axis combinations to one named archetype.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Funnel {
    /// Display name for this world archetype.
    pub name: String,
    /// Axis pairs that resolve to this archetype. Each entry is [jungian_id, rpg_role_id].
    pub absorbs: Vec<[String; 2]>,
    /// Faction this archetype belongs to, if any.
    #[serde(default)]
    pub faction: Option<String>,
    /// World-grounded lore description.
    pub lore: String,
    /// How society views this archetype.
    #[serde(default)]
    pub cultural_status: Option<String>,
    /// Attitudes toward other factions.
    #[serde(default)]
    pub disposition_toward: HashMap<String, String>,
}

/// World-level additional constraints.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct WorldConstraints {
    /// Pairings forbidden at the world level.
    #[serde(default)]
    pub forbidden: Vec<[String; 2]>,
}

/// World-level archetype funnels — resolves axis pairs to named archetypes.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ArchetypeFunnels {
    /// The funnel definitions.
    pub funnels: Vec<Funnel>,
    /// Additional world-level constraints.
    #[serde(default)]
    pub additional_constraints: WorldConstraints,
}

impl ArchetypeFunnels {
    /// Resolve a [jungian, rpg_role] pair to a funnel entry.
    /// Returns None if no funnel claims this combination.
    pub fn resolve(&self, jungian: &str, rpg_role: &str) -> Option<&Funnel> {
        self.funnels.iter().find(|f| {
            f.absorbs
                .iter()
                .any(|pair| pair[0] == jungian && pair[1] == rpg_role)
        })
    }

    /// Check if a pairing is forbidden at the world level.
    pub fn is_forbidden(&self, jungian: &str, rpg_role: &str) -> bool {
        self.additional_constraints
            .forbidden
            .iter()
            .any(|pair| pair[0] == jungian && pair[1] == rpg_role)
    }
}

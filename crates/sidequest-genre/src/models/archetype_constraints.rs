use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Weight classification for a Jungian x RPG Role pairing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PairingWeight {
    /// This pairing occurs frequently in the genre.
    Common,
    /// This pairing occurs occasionally in the genre.
    Uncommon,
    /// This pairing is possible but atypical in the genre.
    Rare,
    /// This pairing is not permitted in the genre.
    Forbidden,
}

/// Valid pairings grouped by weight. Each entry is [jungian_id, rpg_role_id].
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ValidPairings {
    /// Common [jungian, rpg_role] pairs for this genre.
    #[serde(default)]
    pub common: Vec<[String; 2]>,
    /// Uncommon [jungian, rpg_role] pairs for this genre.
    #[serde(default)]
    pub uncommon: Vec<[String; 2]>,
    /// Rare [jungian, rpg_role] pairs for this genre.
    #[serde(default)]
    pub rare: Vec<[String; 2]>,
    /// Forbidden [jungian, rpg_role] pairs for this genre.
    #[serde(default)]
    pub forbidden: Vec<[String; 2]>,
}

/// Genre-specific flavor for a Jungian archetype.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct JungianFlavor {
    /// Characteristic speech pattern for this archetype in the genre.
    #[serde(default)]
    pub speech_pattern: String,
    /// Tendency toward certain equipment types in the genre.
    #[serde(default)]
    pub equipment_tendency: String,
    /// Visual identifiers for this archetype in the genre.
    #[serde(default)]
    pub visual_cues: String,
}

/// Genre-specific flavor for an RPG role.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RpgRoleFlavor {
    /// Genre-localized name for this role (e.g. `"Shield-Bearer"` for `tank`).
    pub fallback_name: String,
}

/// Genre-level flavor collections.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct GenreFlavor {
    /// Jungian archetype flavor overrides keyed by archetype id.
    #[serde(default)]
    pub jungian: HashMap<String, JungianFlavor>,
    /// RPG role flavor overrides keyed by role id.
    #[serde(default)]
    pub rpg_roles: HashMap<String, RpgRoleFlavor>,
}

/// Genre-level archetype constraints — valid pairings and flavor.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ArchetypeConstraints {
    /// Permitted Jungian × RPG role pairings, grouped by frequency weight.
    pub valid_pairings: ValidPairings,
    /// Genre-specific flavor overrides for archetypes and roles.
    pub genre_flavor: GenreFlavor,
    /// NPC narrative role ids available in this genre.
    #[serde(default)]
    pub npc_roles_available: Vec<String>,
}

impl ArchetypeConstraints {
    /// Look up the weight of a [jungian, rpg_role] pairing.
    ///
    /// Returns `None` if the pairing is not listed in any weight category.
    pub fn pairing_weight(&self, jungian: &str, rpg_role: &str) -> Option<PairingWeight> {
        let matches = |pair: &[String; 2]| pair[0] == jungian && pair[1] == rpg_role;

        if self.valid_pairings.common.iter().any(matches) {
            Some(PairingWeight::Common)
        } else if self.valid_pairings.uncommon.iter().any(matches) {
            Some(PairingWeight::Uncommon)
        } else if self.valid_pairings.rare.iter().any(matches) {
            Some(PairingWeight::Rare)
        } else if self.valid_pairings.forbidden.iter().any(matches) {
            Some(PairingWeight::Forbidden)
        } else {
            None
        }
    }

    /// Get the fallback name for an RPG role in this genre.
    ///
    /// Returns `None` if no genre flavor is registered for the given role id.
    pub fn fallback_name(&self, rpg_role: &str) -> Option<&str> {
        self.genre_flavor
            .rpg_roles
            .get(rpg_role)
            .map(|f| f.fallback_name.as_str())
    }
}

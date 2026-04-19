//! Character progression types from `progression.yaml`.

use serde::{Deserialize, Serialize};

use super::advancement::AdvancementEffect;

/// Character progression configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProgressionConfig {
    /// Skill/affinity trees.
    #[serde(default)]
    pub affinities: Vec<Affinity>,
    /// Categories for milestone tracking.
    #[serde(default)]
    pub milestone_categories: Vec<String>,
    /// Milestones required per level.
    #[serde(default)]
    pub milestones_per_level: u32,
    /// Maximum character level.
    #[serde(default)]
    pub max_level: u32,
    /// Item naming/power-up thresholds.
    #[serde(default)]
    pub item_evolution: Option<ItemEvolution>,
    /// Per-level stat bonuses.
    #[serde(default)]
    pub level_bonuses: Option<LevelBonuses>,
    /// Wealth tier labels.
    #[serde(default)]
    pub wealth_tiers: Vec<WealthTier>,
}

/// A skill/affinity tree.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Affinity {
    /// Affinity name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Player actions that earn XP in this affinity.
    pub triggers: Vec<String>,
    /// XP thresholds for each tier.
    pub tier_thresholds: Vec<u32>,
    /// Unlockable abilities per tier.
    #[serde(default)]
    pub unlocks: Option<AffinityUnlocks>,
}

/// Tier unlocks for an affinity (fixed set: tier_1, tier_2, tier_3).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AffinityUnlocks {
    /// Tier 0 (starting) abilities.
    #[serde(default)]
    pub tier_0: Option<AffinityTier>,
    /// Tier 1 abilities.
    #[serde(default)]
    pub tier_1: Option<AffinityTier>,
    /// Tier 2 abilities.
    #[serde(default)]
    pub tier_2: Option<AffinityTier>,
    /// Tier 3 abilities.
    #[serde(default)]
    pub tier_3: Option<AffinityTier>,
}

/// A single tier within an affinity.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AffinityTier {
    /// Tier display name.
    pub name: String,
    /// Description of reaching this tier.
    pub description: String,
    /// Abilities unlocked at this tier.
    pub abilities: Vec<Ability>,
    /// Authored mechanical effects this tier applies (Story 39-5 /
    /// ADR-078). Heavy_metal hosts its advancement tree here; other
    /// genres use a standalone `advancements.yaml` sibling file (see
    /// [`crate::load_advancement_tree`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mechanical_effects: Option<Vec<AdvancementEffect>>,
}

/// An ability within an affinity tier.
///
/// Can be either a simple string description or a full struct with
/// name, experience narrative, and limits.
#[derive(Debug, Clone, Serialize)]
pub struct Ability {
    /// Ability name.
    pub name: String,
    /// Narrative description of using the ability.
    pub experience: String,
    /// Limitations and costs.
    pub limits: String,
}

impl<'de> Deserialize<'de> for Ability {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum AbilityRepr {
            Simple(String),
            Full {
                name: String,
                experience: String,
                limits: String,
            },
        }

        match AbilityRepr::deserialize(deserializer)? {
            AbilityRepr::Simple(s) => Ok(Ability {
                name: s,
                experience: String::new(),
                limits: String::new(),
            }),
            AbilityRepr::Full {
                name,
                experience,
                limits,
            } => Ok(Ability {
                name,
                experience,
                limits,
            }),
        }
    }
}

/// Item evolution thresholds.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ItemEvolution {
    /// Bond threshold for naming an item.
    #[serde(default)]
    pub naming_threshold: f64,
    /// Bond threshold for powering up.
    #[serde(default)]
    pub power_up_threshold: f64,
}

/// Per-level bonuses.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LevelBonuses {
    /// Stat points gained per level.
    #[serde(default)]
    pub stat_points: u32,
    /// HP bonus strategy (e.g., "class_based").
    #[serde(default)]
    pub hp_bonus: String,
}

/// A wealth tier with optional gold cap.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WealthTier {
    /// Maximum gold for this tier (None = no cap).
    pub max_gold: Option<u32>,
    /// Display label.
    pub label: String,
}

use serde::{Deserialize, Serialize};

/// OCEAN score ranges — [min, max] for each Big Five dimension.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OceanTendencies {
    /// Openness to experience score range [min, max].
    pub openness: [f64; 2],
    /// Conscientiousness score range [min, max].
    pub conscientiousness: [f64; 2],
    /// Extraversion score range [min, max].
    pub extraversion: [f64; 2],
    /// Agreeableness score range [min, max].
    pub agreeableness: [f64; 2],
    /// Neuroticism score range [min, max].
    pub neuroticism: [f64; 2],
}

/// Base Jungian archetype — personality core, genre-agnostic.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JungianArchetype {
    /// Unique identifier (e.g. `"sage"`, `"hero"`).
    pub id: String,
    /// Core motivational drive for this archetype.
    pub drive: String,
    /// OCEAN personality score ranges for this archetype.
    pub ocean_tendencies: OceanTendencies,
    /// Genre-agnostic stat names this archetype naturally favours.
    pub stat_affinity: Vec<String>,
}

/// Base RPG role — mechanical combat function, genre-agnostic.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RpgRole {
    /// Unique identifier (e.g. `"healer"`, `"tank"`).
    pub id: String,
    /// Prose description of the role's combat purpose.
    pub combat_function: String,
    /// Genre-agnostic stat names this role naturally favours.
    pub stat_affinity: Vec<String>,
}

/// NPC narrative role — assigned by the system, never player-facing.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NpcRole {
    /// Unique identifier (e.g. `"mook"`, `"mentor"`).
    pub id: String,
    /// Prose description of the role's narrative purpose.
    pub narrative_function: String,
    /// When `true`, skip the NPC enrichment pipeline for this role.
    #[serde(default)]
    pub skip_enrichment: bool,
}

/// Top-level container for the base archetype definitions file.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BaseArchetypes {
    /// Jungian personality archetypes.
    pub jungian: Vec<JungianArchetype>,
    /// Mechanical RPG combat roles.
    pub rpg_roles: Vec<RpgRole>,
    /// Narrative NPC roles assigned by the system.
    pub npc_roles: Vec<NpcRole>,
}

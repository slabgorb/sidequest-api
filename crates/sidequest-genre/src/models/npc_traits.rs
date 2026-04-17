//! NPC trait database — personality, physical, and behavioral quirks for spawn-tier NPCs.

use serde::{Deserialize, Serialize};

/// A single NPC trait entry with optional Jungian affinity weighting.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NpcTrait {
    /// The trait description (lowercase).
    /// YAML key is "trait" — a Rust reserved word, so we rename via serde.
    #[serde(rename = "trait")]
    pub trait_name: String,
    /// Jungian archetypes this trait is more likely for.
    #[serde(default)]
    pub jungian_affinity: Vec<String>,
}

/// Master NPC traits database loaded from `npc_traits.yaml`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NpcTraitsDatabase {
    /// Personality traits — inner character, emotional disposition.
    pub personality: Vec<NpcTrait>,
    /// Physical traits — appearance, body, distinguishing marks.
    pub physical: Vec<NpcTrait>,
    /// Behavioral traits — observable habits, tics, mannerisms.
    pub behavioral: Vec<NpcTrait>,
}

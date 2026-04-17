//! `ArchetypeResolved` — the archetype value type on the Layered framework.
//!
//! Each field is per-tier mergeable. Phase 1 populates the struct from the
//! shim's axis-lookup logic; Phase 2 content migration will add per-tier
//! YAML fragments that feed through [`crate::resolver::Resolver`] directly.

use crate::Layered;
use serde::Deserialize;

/// Archetype value after four-tier resolution.
///
/// Every field uses the `replace` merge strategy: a deeper tier's value
/// overrides a shallower tier's. Fields absent from a deeper tier leave
/// the shallower tier's value in place (serde default + replace semantics
/// means "missing in deeper YAML" === "default value in deeper struct",
/// which under strict replace clobbers — this will be refined when a
/// smarter `replace` strategy lands; see the Phase D commit body).
#[derive(Debug, Clone, Default, Deserialize, Layered)]
#[serde(default)]
pub struct ArchetypeResolved {
    /// Display name shown to the player (e.g. "Thornwall Mender").
    #[layer(merge = "replace")]
    pub name: String,
    /// Jungian axis identifier (e.g. "sage").
    #[layer(merge = "replace")]
    pub jungian: String,
    /// RPG role axis identifier (e.g. "healer").
    #[layer(merge = "replace")]
    pub rpg_role: String,
    /// NPC role identifier (e.g. "mentor"). Only populated for NPCs.
    #[layer(merge = "replace")]
    pub npc_role: Option<String>,
    /// Speech pattern hint for the narrator. Typically genre-level flavor.
    #[layer(merge = "replace")]
    pub speech_pattern: String,
    /// Lore prose, typically authored at the world tier per funnel.
    #[layer(merge = "replace")]
    pub lore: String,
    /// Faction name from a world-level funnel, if any.
    #[layer(merge = "replace")]
    pub faction: Option<String>,
    /// Cultural status marker from a world-level funnel, if any.
    #[layer(merge = "replace")]
    pub cultural_status: Option<String>,
}

//! Typed patch structs for agent output — WorldStatePatch.
//!
//! Port lesson #4: Typed patches replace the Python god-object's 255-line apply_patch().
//! ADR-011: Structured patches, not freeform text.
//! CombatPatch and ChasePatch removed in story 28-9 — StructuredEncounter replaces them.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Patch produced by the WorldBuilder agent for world state mutations.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldStatePatch {
    /// New location name.
    pub location: Option<String>,
    /// Time of day update.
    pub time_of_day: Option<String>,
    /// Per-character HP deltas.
    pub hp_changes: Option<HashMap<String, i32>>,
    /// NPC disposition deltas (signed integers on the -100 to +100 scale).
    /// Positive = friendlier, negative = more hostile.
    pub npc_attitudes: Option<HashMap<String, i32>>,
    /// Quest log updates.
    pub quest_updates: Option<HashMap<String, String>>,
    /// Narrative note to append.
    pub notes: Option<String>,
    /// Atmosphere description.
    pub atmosphere: Option<String>,
    /// Active stakes description.
    pub active_stakes: Option<String>,
    /// Current region update.
    pub current_region: Option<String>,
    /// Regions to discover.
    pub discover_regions: Option<Vec<String>>,
    /// Routes to discover.
    pub discover_routes: Option<Vec<String>>,
    /// Lore fragments established.
    pub lore_established: Option<Vec<String>>,
}

// CombatPatch and ChasePatch deleted in story 28-9.
// StructuredEncounter (via beat selections) is the sole encounter mutation model.

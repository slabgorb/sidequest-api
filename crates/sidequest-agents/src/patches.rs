//! Typed patch structs for agent output — WorldStatePatch, CombatPatch, ChasePatch.
//!
//! Port lesson #4: Typed patches replace the Python god-object's 255-line apply_patch().
//! ADR-011: Structured patches, not freeform text.

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
    /// NPC attitude changes.
    pub npc_attitudes: Option<HashMap<String, String>>,
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

/// Patch produced by the CreatureSmith agent for combat state mutations.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatPatch {
    /// Whether combat is active.
    pub in_combat: Option<bool>,
    /// Per-combatant HP deltas.
    pub hp_changes: Option<HashMap<String, i32>>,
    /// Turn order.
    pub turn_order: Option<Vec<String>>,
    /// Current turn holder.
    pub current_turn: Option<String>,
    /// Available player actions.
    pub available_actions: Option<Vec<String>>,
    /// Drama weight for pacing.
    pub drama_weight: Option<f64>,
    /// Whether to advance the combat round.
    #[serde(default)]
    pub advance_round: bool,
}

/// Patch produced by the Dialectician agent for chase state mutations.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChasePatch {
    /// Distance between pursuer and quarry.
    pub separation: Option<i32>,
    /// Current chase phase.
    pub phase: Option<String>,
    /// Chase event description.
    pub event: Option<String>,
    /// Chase roll result (0.0 to 1.0).
    pub roll: Option<f64>,
}

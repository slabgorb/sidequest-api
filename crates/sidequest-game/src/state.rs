//! Game state composition — GameSnapshot and typed patches.
//!
//! Port lesson #4: GameSnapshot composes domain structs, no god object.
//! Each domain struct owns its mutations via typed patch application.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::character::Character;
use crate::chase::{ChaseState, ChaseType};
use crate::combat::CombatState;
use crate::narrative::NarrativeEntry;
use crate::npc::Npc;
use crate::turn::TurnManager;

/// The complete game state at a point in time.
///
/// Composes all domain types (port lesson #4). Serializable for persistence
/// and WebSocket broadcast. Port lesson #11: captures ALL client-visible fields,
/// not just characters/location/quest_log like the Python version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameSnapshot {
    /// Genre pack identifier (e.g., "mutant_wasteland").
    pub genre_slug: String,
    /// World identifier within the genre pack.
    pub world_slug: String,
    /// Player characters.
    pub characters: Vec<Character>,
    /// Non-player characters.
    pub npcs: Vec<Npc>,
    /// Current location name.
    pub location: String,
    /// Current time of day.
    pub time_of_day: String,
    /// Active quests (quest_name → description).
    pub quest_log: HashMap<String, String>,
    /// Player notes.
    pub notes: Vec<String>,
    /// Narrative history.
    pub narrative_log: Vec<NarrativeEntry>,
    /// Active combat state.
    pub combat: CombatState,
    /// Active chase sequence (None if no chase in progress).
    pub chase: Option<ChaseState>,
    /// Currently active narrative tropes.
    pub active_tropes: Vec<String>,
    /// Current atmosphere description.
    pub atmosphere: String,
    /// Current region name.
    pub current_region: String,
    /// Regions the player has visited.
    pub discovered_regions: Vec<String>,
    /// Routes the player has discovered.
    pub discovered_routes: Vec<String>,
    /// Turn sequencing and barrier tracking.
    pub turn_manager: TurnManager,
    /// When this snapshot was last persisted (set by GameStore on save).
    pub last_saved_at: Option<DateTime<Utc>>,
}

impl GameSnapshot {
    /// Apply a world state patch (location, atmosphere, quest_log, etc.).
    /// Only fields that are `Some` in the patch are updated.
    /// Emits a tracing span with patch_type and fields_changed (story 3-1).
    pub fn apply_world_patch(&mut self, patch: &WorldStatePatch) {
        let span = tracing::info_span!(
            "apply_world_patch",
            patch_type = "world",
            fields_changed = tracing::field::Empty,
        );
        let _guard = span.enter();

        let mut changed = Vec::new();
        if let Some(ref loc) = patch.location {
            self.location = loc.clone();
            changed.push("location");
        }
        if let Some(ref atm) = patch.atmosphere {
            self.atmosphere = atm.clone();
            changed.push("atmosphere");
        }
        if let Some(ref ql) = patch.quest_log {
            self.quest_log = ql.clone();
            changed.push("quest_log");
        }
        if let Some(ref n) = patch.notes {
            self.notes = n.clone();
            changed.push("notes");
        }
        if let Some(ref cr) = patch.current_region {
            self.current_region = cr.clone();
            changed.push("current_region");
        }
        if let Some(ref regions) = patch.discovered_regions {
            self.discovered_regions = regions.clone();
            changed.push("discovered_regions");
        }
        if let Some(ref routes) = patch.discovered_routes {
            self.discovered_routes = routes.clone();
            changed.push("discovered_routes");
        }

        span.record(
            "fields_changed",
            &tracing::field::display(&changed.join(",")),
        );
    }

    /// Apply a combat patch.
    /// Emits a tracing span with patch_type and fields_changed (story 3-1).
    pub fn apply_combat_patch(&mut self, patch: &CombatPatch) {
        let span = tracing::info_span!(
            "apply_combat_patch",
            patch_type = "combat",
            fields_changed = tracing::field::Empty,
        );
        let _guard = span.enter();

        let mut changed = Vec::new();
        if patch.advance_round {
            self.combat.advance_round();
            changed.push("round");
        }

        span.record(
            "fields_changed",
            &tracing::field::display(&changed.join(",")),
        );
    }

    /// Apply a chase patch (start a chase or record a roll).
    /// Emits a tracing span with patch_type and fields_changed (story 3-1).
    pub fn apply_chase_patch(&mut self, patch: &ChasePatch) {
        let span = tracing::info_span!(
            "apply_chase_patch",
            patch_type = "chase",
            fields_changed = tracing::field::Empty,
        );
        let _guard = span.enter();

        let mut changed = Vec::new();
        if let Some((chase_type, threshold)) = patch.start {
            self.chase = Some(ChaseState::new(chase_type, threshold));
            changed.push("chase_started");
        }
        if let Some(roll) = patch.roll {
            if let Some(ref mut chase) = self.chase {
                chase.record_roll(roll);
                changed.push("escape_roll");
            }
        }

        span.record(
            "fields_changed",
            &tracing::field::display(&changed.join(",")),
        );
    }
}

/// Patch for world-level state (location, atmosphere, quests, regions).
///
/// Only `Some` fields are applied; `None` means "no change."
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldStatePatch {
    /// New location.
    pub location: Option<String>,
    /// New atmosphere.
    pub atmosphere: Option<String>,
    /// Replacement quest log (full replace, not merge).
    pub quest_log: Option<HashMap<String, String>>,
    /// Replacement notes list.
    pub notes: Option<Vec<String>>,
    /// New current region.
    pub current_region: Option<String>,
    /// Replacement discovered regions list.
    pub discovered_regions: Option<Vec<String>>,
    /// Replacement discovered routes list.
    pub discovered_routes: Option<Vec<String>>,
}

/// Patch for combat state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombatPatch {
    /// Whether to advance the combat round.
    pub advance_round: bool,
}

/// Patch for chase state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChasePatch {
    /// Start a new chase with (type, escape_threshold).
    pub start: Option<(ChaseType, f64)>,
    /// Record an escape roll.
    pub roll: Option<f64>,
}

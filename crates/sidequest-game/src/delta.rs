//! State delta — captures ALL client-visible changes between two snapshots.
//!
//! Port lesson #11: Python's snapshot_state() only captures characters,
//! location, quest_log. This implementation captures everything.

use serde::{Deserialize, Serialize};

use crate::state::GameSnapshot;

/// Serialize a value to JSON for snapshot comparison, defaulting to empty string on error.
fn to_json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

/// A frozen JSON snapshot of game state for comparison.
///
/// Uses serialized JSON strings for each field group so that
/// comparison is a simple string equality check.
#[derive(Debug, Clone)]
pub struct StateSnapshot {
    characters_json: String,
    npcs_json: String,
    location: String,
    time_of_day: String,
    quest_log_json: String,
    notes_json: String,
    combat_json: String,
    chase_json: String,
    active_tropes_json: String,
    atmosphere: String,
    #[allow(dead_code)]
    current_region: String,
    discovered_regions_json: String,
    discovered_routes_json: String,
}

/// Take a snapshot of the game state for later delta comparison.
pub fn snapshot(state: &GameSnapshot) -> StateSnapshot {
    StateSnapshot {
        characters_json: to_json(&state.characters),
        npcs_json: to_json(&state.npcs),
        location: state.location.clone(),
        time_of_day: state.time_of_day.clone(),
        quest_log_json: to_json(&state.quest_log),
        notes_json: to_json(&state.notes),
        combat_json: to_json(&state.combat),
        chase_json: to_json(&state.chase),
        active_tropes_json: to_json(&state.active_tropes),
        atmosphere: state.atmosphere.clone(),
        current_region: state.current_region.clone(),
        discovered_regions_json: to_json(&state.discovered_regions),
        discovered_routes_json: to_json(&state.discovered_routes),
    }
}

/// The set of changes between two game state snapshots.
///
/// Each field indicates whether that aspect of the state changed.
/// ADR-026/027: Piggybacked on narration messages for client state mirror.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateDelta {
    characters: bool,
    npcs: bool,
    location: bool,
    time_of_day: bool,
    quest_log: bool,
    notes: bool,
    combat: bool,
    chase: bool,
    tropes: bool,
    atmosphere: bool,
    regions: bool,
    routes: bool,
    new_location: Option<String>,
}

/// Compute the delta between two state snapshots.
pub fn compute_delta(before: &StateSnapshot, after: &StateSnapshot) -> StateDelta {
    let location_changed = before.location != after.location;
    StateDelta {
        characters: before.characters_json != after.characters_json,
        npcs: before.npcs_json != after.npcs_json,
        location: location_changed,
        time_of_day: before.time_of_day != after.time_of_day,
        quest_log: before.quest_log_json != after.quest_log_json,
        notes: before.notes_json != after.notes_json,
        combat: before.combat_json != after.combat_json,
        chase: before.chase_json != after.chase_json,
        tropes: before.active_tropes_json != after.active_tropes_json,
        atmosphere: before.atmosphere != after.atmosphere,
        regions: before.discovered_regions_json != after.discovered_regions_json,
        routes: before.discovered_routes_json != after.discovered_routes_json,
        new_location: if location_changed {
            Some(after.location.clone())
        } else {
            None
        },
    }
}

impl StateDelta {
    /// Whether any field changed.
    pub fn is_empty(&self) -> bool {
        !self.characters
            && !self.npcs
            && !self.location
            && !self.time_of_day
            && !self.quest_log
            && !self.notes
            && !self.combat
            && !self.chase
            && !self.tropes
            && !self.atmosphere
            && !self.regions
            && !self.routes
    }

    /// Whether characters changed.
    pub fn characters_changed(&self) -> bool {
        self.characters
    }

    /// Whether NPCs changed.
    pub fn npcs_changed(&self) -> bool {
        self.npcs
    }

    /// Whether location changed.
    pub fn location_changed(&self) -> bool {
        self.location
    }

    /// The new location, if it changed.
    pub fn new_location(&self) -> Option<&str> {
        self.new_location.as_deref()
    }

    /// Whether quest log changed.
    pub fn quest_log_changed(&self) -> bool {
        self.quest_log
    }

    /// Whether combat state changed.
    pub fn combat_changed(&self) -> bool {
        self.combat
    }

    /// Whether chase state changed.
    pub fn chase_changed(&self) -> bool {
        self.chase
    }

    /// Whether atmosphere changed.
    pub fn atmosphere_changed(&self) -> bool {
        self.atmosphere
    }

    /// Whether discovered regions changed.
    pub fn regions_changed(&self) -> bool {
        self.regions
    }

    /// Whether active tropes changed.
    pub fn tropes_changed(&self) -> bool {
        self.tropes
    }
}

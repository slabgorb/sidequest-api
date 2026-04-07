//! Treasure-as-XP — gold extraction on the surface grants affinity progress (story 19-9).
//!
//! Classic OSR mechanic: XP comes from extracting treasure, not killing monsters.
//! When gold increases while the player is on the surface (not inside a dungeon),
//! the gold amount is applied as progress to the configured affinity.
//!
//! Surface detection:
//! - Region mode (no room graph): all locations are surface.
//! - Room graph mode: `room_type == "entrance"` or location not in graph = surface.
//!   All other rooms are dungeon interior.

use sidequest_genre::RoomDef;

use crate::affinity::increment_affinity_progress;
use crate::state::GameSnapshot;

/// Configuration for treasure-as-XP, sourced from `RulesConfig.xp_affinity`.
#[derive(Debug, Clone)]
pub struct TreasureXpConfig {
    /// Which affinity receives progress from gold extraction.
    /// None means treasure-as-XP is disabled for this genre.
    pub xp_affinity: Option<String>,
}

/// Result of applying treasure XP — carries OTEL metadata for the server to emit.
#[derive(Debug, Clone)]
pub struct TreasureXpResult {
    /// Whether affinity progress was actually applied.
    pub applied: bool,
    /// Gold amount that triggered the XP (0 if not applied).
    pub gold_amount: u32,
    /// Affinity name that received progress (None if not applied).
    pub affinity_name: Option<String>,
    /// New total progress on the affinity after application (None if not applied).
    pub new_progress: Option<u32>,
}

/// Check whether the player's current location is on the surface.
///
/// - If `rooms` is `None` (region mode), all locations are surface.
/// - If `rooms` is `Some`, the player is on the surface when:
///   - Their location matches a room with `room_type == "entrance"`, OR
///   - Their location doesn't match any room ID (outside the graph).
fn is_surface(location: &str, rooms: Option<&[RoomDef]>) -> bool {
    let Some(rooms) = rooms else {
        // Region mode — no room graph, everything is surface
        return true;
    };

    // Find the room matching the player's location
    match rooms.iter().find(|r| r.id == location) {
        Some(room) => room.room_type == "entrance",
        None => true, // Location not in graph = outside = surface
    }
}

/// Apply treasure-as-XP: if the player is on the surface and gold was gained,
/// advance the configured affinity by the gold amount.
///
/// Returns a `TreasureXpResult` with OTEL-ready metadata for the server layer
/// to emit a `treasure.extracted` event.
pub fn apply_treasure_xp(
    snap: &mut GameSnapshot,
    gold_amount: u32,
    config: &TreasureXpConfig,
    rooms: Option<&[RoomDef]>,
) -> TreasureXpResult {
    let not_applied = TreasureXpResult {
        applied: false,
        gold_amount: 0,
        affinity_name: None,
        new_progress: None,
    };

    // Guard: must have a configured affinity
    let Some(affinity_name) = &config.xp_affinity else {
        return not_applied;
    };

    // Guard: must have gold to extract
    if gold_amount == 0 {
        return not_applied;
    }

    // Guard: must be on the surface
    if !is_surface(&snap.location, rooms) {
        return not_applied;
    }

    // Guard: must have at least one character
    let Some(character) = snap.characters.first_mut() else {
        return not_applied;
    };

    // Apply affinity progress (creates the affinity if absent)
    increment_affinity_progress(&mut character.affinities, affinity_name, gold_amount);

    // Read back the new progress for OTEL metadata
    let new_progress = character
        .affinities
        .iter()
        .find(|a| a.name == *affinity_name)
        .map(|a| a.progress);

    TreasureXpResult {
        applied: true,
        gold_amount,
        affinity_name: Some(affinity_name.clone()),
        new_progress,
    }
}

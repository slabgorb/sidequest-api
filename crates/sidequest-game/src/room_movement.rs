//! Room-graph movement validation (story 19-2).
//!
//! When `NavigationMode::RoomGraph` is active, location changes must target
//! a valid room reachable via an exit from the current room. Invalid moves
//! are rejected with `DispatchError::InvalidRoomTransition`.

use std::collections::HashSet;

use sidequest_genre::{RoomDef, RoomExit};
use sidequest_protocol::{ExploredLocation, RoomExitInfo, TacticalFeaturePayload, TacticalGridPayload};

use crate::tactical::{TacticalCell, TacticalGrid};

use crate::state::GameSnapshot;

/// Successful room transition metadata — returned by `apply_validated_move`.
///
/// Carries the data the caller needs for OTEL/WatcherEvent emission.
/// The game crate doesn't emit dispatch-level telemetry — that's the server's job.
#[derive(Debug, Clone)]
pub struct RoomTransition {
    /// Room the player moved from.
    pub from_room: String,
    /// Room the player moved to.
    pub to_room: String,
    /// Exit type used (e.g., "door", "corridor", "chute down").
    pub exit_type: String,
}

/// Errors from the dispatch/movement pipeline.
#[derive(Debug)]
pub enum DispatchError {
    /// Attempted move to a room that is unreachable or doesn't exist.
    InvalidRoomTransition {
        /// Room the player is currently in.
        from_room: String,
        /// Room the player tried to move to.
        to_room: String,
        /// Human-readable reason (e.g., "no_exit", "room_not_found").
        reason: String,
    },
}

/// Resolve a display name or ID to a canonical room ID.
///
/// Matching priority:
/// 1. Exact ID match (`"threshold"` → `"threshold"`)
/// 2. Exact name match (`"The Threshold"` → `"threshold"`)
/// 3. Case-insensitive name match (`"the threshold"` → `"threshold"`)
///
/// Returns `None` if no room matches — the narrator invented a room name.
pub fn resolve_room_id<'a>(name_or_id: &str, rooms: &'a [RoomDef]) -> Option<&'a str> {
    // 1. Exact ID match
    if let Some(room) = rooms.iter().find(|r| r.id == name_or_id) {
        return Some(&room.id);
    }
    // 2. Exact name match
    if let Some(room) = rooms.iter().find(|r| r.name == name_or_id) {
        return Some(&room.id);
    }
    // 3. Case-insensitive name match
    let lower = name_or_id.to_lowercase();
    if let Some(room) = rooms.iter().find(|r| r.name.to_lowercase() == lower) {
        return Some(&room.id);
    }
    None
}

/// Validate that a room transition is legal in room-graph mode.
///
/// Checks:
/// 1. `to_room` exists in the room graph
/// 2. The current room (`snap.location`) exists in the graph
/// 3. An exit from the current room leads to `to_room`
pub fn validate_room_transition(
    snap: &GameSnapshot,
    to_room: &str,
    rooms: &[RoomDef],
) -> Result<(), DispatchError> {
    let from_room = &snap.location;

    // Check target room exists in graph
    if !rooms.iter().any(|r| r.id == to_room) {
        return Err(DispatchError::InvalidRoomTransition {
            from_room: from_room.clone(),
            to_room: to_room.to_string(),
            reason: "room_not_found".to_string(),
        });
    }

    // Check current room exists in graph
    let current = rooms.iter().find(|r| r.id == *from_room);
    let Some(current) = current else {
        return Err(DispatchError::InvalidRoomTransition {
            from_room: from_room.clone(),
            to_room: to_room.to_string(),
            reason: format!("current room '{}' not found in graph", from_room),
        });
    };

    // Check an exit leads to the target room
    let has_exit = current.exits.iter().any(|exit| exit.target() == to_room);
    if !has_exit {
        return Err(DispatchError::InvalidRoomTransition {
            from_room: from_room.clone(),
            to_room: to_room.to_string(),
            reason: "no_exit".to_string(),
        });
    }

    Ok(())
}

/// Validate and apply a room move. On success, updates location and discovered_rooms.
///
/// Returns `RoomTransition` with metadata for the caller to emit OTEL/WatcherEvents.
/// The game crate does not emit dispatch-level telemetry — that's the server's responsibility.
pub fn apply_validated_move(
    snap: &mut GameSnapshot,
    to_room: &str,
    rooms: &[RoomDef],
) -> Result<RoomTransition, DispatchError> {
    validate_room_transition(snap, to_room, rooms)?;

    // Find the exit type from the current room's exit list
    let exit_type = rooms
        .iter()
        .find(|r| r.id == snap.location)
        .and_then(|r| {
            r.exits
                .iter()
                .find(|e| e.target() == to_room)
                .map(|e| e.display_name().to_string())
        })
        .unwrap_or_default();

    let from = snap.location.clone();

    // Apply the move
    snap.location = to_room.to_string();
    snap.discovered_rooms.insert(to_room.to_string());

    Ok(RoomTransition {
        from_room: from,
        to_room: to_room.to_string(),
        exit_type,
    })
}

/// Set the starting location to the entrance room in the graph.
///
/// # Panics
/// Panics if no room with `room_type == "entrance"` exists — no silent fallback.
pub fn init_room_graph_location(snap: &mut GameSnapshot, rooms: &[RoomDef]) {
    let entrance = rooms
        .iter()
        .find(|r| r.room_type == "entrance")
        .unwrap_or_else(|| {
            panic!(
                "room graph has no entrance room — {} rooms checked",
                rooms.len()
            )
        });

    snap.location = entrance.id.clone();
    snap.discovered_rooms.insert(entrance.id.clone());
}

/// Build the explored location list for MAP_UPDATE in room graph mode.
///
/// Filters to only discovered rooms, populates room metadata from `RoomDef`,
/// flags the current room, and hides undiscovered secret exits.
pub fn build_room_graph_explored(
    rooms: &[RoomDef],
    discovered: &HashSet<String>,
    current_room_id: &str,
) -> Vec<ExploredLocation> {
    rooms
        .iter()
        .filter(|room| discovered.contains(&room.id))
        .map(|room| {
            let visible_exits: Vec<RoomExitInfo> = room
                .exits
                .iter()
                .filter(|exit| is_exit_visible(exit))
                .map(|exit| RoomExitInfo {
                    target: exit.target().to_string(),
                    exit_type: exit_type_slug(exit),
                })
                .collect();

            let connections: Vec<String> = visible_exits.iter().map(|e| e.target.clone()).collect();

            let tactical_grid = parse_room_grid(room);

            ExploredLocation {
                name: room.name.clone(),
                x: 0,
                y: 0,
                location_type: room.room_type.clone(),
                connections,
                room_exits: visible_exits,
                room_type: room.room_type.clone(),
                size: Some(room.size),
                is_current_room: room.id == current_room_id,
                tactical_grid,
            }
        })
        .collect()
}

/// Parse a room's ASCII grid into a TacticalGridPayload, if the room has one.
/// Returns None for rooms without grid data (AC-3: not an error).
fn parse_room_grid(room: &RoomDef) -> Option<TacticalGridPayload> {
    let grid_str = room.grid.as_ref()?;
    let legend = room.legend.as_ref().cloned().unwrap_or_default();

    let grid = match TacticalGrid::parse(grid_str, &legend) {
        Ok(g) => g,
        Err(e) => {
            tracing::warn!(
                room_id = %room.id,
                error = %e,
                "failed to parse tactical grid for room — skipping"
            );
            return None;
        }
    };

    Some(tactical_grid_to_payload(&grid))
}

/// Convert a parsed TacticalGrid into a wire-format TacticalGridPayload.
fn tactical_grid_to_payload(grid: &TacticalGrid) -> TacticalGridPayload {
    let cells: Vec<Vec<String>> = grid
        .cells()
        .iter()
        .map(|row| row.iter().map(cell_to_string).collect())
        .collect();

    // Collect features from the legend — find all positions of each glyph.
    let mut features: Vec<TacticalFeaturePayload> = Vec::new();
    for (&glyph, def) in grid.legend() {
        let mut positions = Vec::new();
        for (y, row) in grid.cells().iter().enumerate() {
            for (x, cell) in row.iter().enumerate() {
                if matches!(cell, TacticalCell::Feature(g) if *g == glyph) {
                    positions.push([x as u32, y as u32]);
                }
            }
        }
        if !positions.is_empty() {
            features.push(TacticalFeaturePayload {
                glyph,
                feature_type: def.feature_type.to_string(),
                label: def.label.clone(),
                positions,
            });
        }
    }

    TacticalGridPayload {
        width: grid.width(),
        height: grid.height(),
        cells,
        features,
    }
}

/// Map a TacticalCell to its wire-format string name.
fn cell_to_string(cell: &TacticalCell) -> String {
    match cell {
        TacticalCell::Floor => "floor".into(),
        TacticalCell::Wall => "wall".into(),
        TacticalCell::Void => "void".into(),
        TacticalCell::DoorClosed => "door_closed".into(),
        TacticalCell::DoorOpen => "door_open".into(),
        TacticalCell::Water => "water".into(),
        TacticalCell::DifficultTerrain => "difficult_terrain".into(),
        TacticalCell::Feature(_) => "feature".into(),
    }
}

/// Whether an exit is visible to the player (for MAP_UPDATE).
/// Secret passages are hidden until their `discovered` flag is true.
fn is_exit_visible(exit: &RoomExit) -> bool {
    match exit {
        RoomExit::Secret { discovered, .. } => *discovered,
        _ => true,
    }
}

/// Slug string for exit type — matches the protocol's exit_type field.
fn exit_type_slug(exit: &RoomExit) -> String {
    match exit {
        RoomExit::Door { .. } => "door".to_string(),
        RoomExit::Corridor { .. } => "corridor".to_string(),
        RoomExit::ChuteDown { .. } => "chute_down".to_string(),
        RoomExit::ChuteUp { .. } => "chute_up".to_string(),
        RoomExit::Secret { .. } => "secret".to_string(),
    }
}

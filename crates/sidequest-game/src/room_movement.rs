//! Room-graph movement validation (story 19-2).
//!
//! When `NavigationMode::RoomGraph` is active, location changes must target
//! a valid room reachable via an exit from the current room. Invalid moves
//! are rejected with `DispatchError::InvalidRoomTransition`.

use sidequest_genre::RoomDef;

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

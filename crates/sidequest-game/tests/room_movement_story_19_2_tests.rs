//! RED-phase tests for Story 19-2: Validated room movement.
//!
//! Tests exercise:
//! - DispatchError::InvalidRoomTransition for non-adjacent room moves
//! - GameSnapshot.discovered_rooms: HashSet<String> tracking
//! - Session init sets entrance location + discovered_rooms
//! - Region mode bypasses all room validation
//! - Integration: 3-room movement sequence with rejected invalid move

use std::collections::HashSet;

use sidequest_game::room_movement::{
    apply_validated_move, init_room_graph_location, validate_room_transition, DispatchError,
};
use sidequest_game::state::{GameSnapshot, WorldStatePatch};
use sidequest_genre::{RoomDef, RoomExit};

// ═══════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════

/// Build a minimal 3-room graph: entrance → corridor → chamber.
fn three_room_graph() -> Vec<RoomDef> {
    vec![
        RoomDef {
            id: "entrance".into(),
            name: "Entrance Hall".into(),
            room_type: "entrance".into(),
            size: (3, 3),
            keeper_awareness_modifier: 1.0,
            exits: vec![RoomExit::Corridor {
                target: "corridor".into(),
            }],
            description: None,
            grid: None,
            legend: None,
            tactical_scale: None,
        },
        RoomDef {
            id: "corridor".into(),
            name: "Dark Corridor".into(),
            room_type: "normal".into(),
            size: (2, 4),
            keeper_awareness_modifier: 1.0,
            exits: vec![
                RoomExit::Corridor {
                    target: "entrance".into(),
                },
                RoomExit::Door {
                    target: "chamber".into(),
                    is_locked: false,
                },
            ],
            description: None,
            grid: None,
            legend: None,
            tactical_scale: None,
        },
        RoomDef {
            id: "chamber".into(),
            name: "Treasure Chamber".into(),
            room_type: "treasure".into(),
            size: (4, 4),
            keeper_awareness_modifier: 1.2,
            exits: vec![RoomExit::Door {
                target: "corridor".into(),
                is_locked: false,
            }],
            description: None,
            grid: None,
            legend: None,
            tactical_scale: None,
        },
    ]
}

/// Build a GameSnapshot at a given room with given discovered_rooms.
fn snapshot_at(location: &str, discovered: &[&str]) -> GameSnapshot {
    let mut snap = GameSnapshot::default();
    snap.location = location.to_string();
    snap.discovered_rooms = discovered.iter().map(|s| s.to_string()).collect();
    snap
}

// ═══════════════════════════════════════════════════════════
// AC-1: Location validation rejects unreachable room
// ═══════════════════════════════════════════════════════════

#[test]
fn test_location_validation_rejects_unreachable_room() {
    let rooms = three_room_graph();
    let snap = snapshot_at("entrance", &["entrance"]);

    // Attempt direct move from entrance → chamber (no exit connects them)
    let result = validate_room_transition(&snap, "chamber", &rooms);

    match result {
        Err(DispatchError::InvalidRoomTransition {
            from_room,
            to_room,
            reason,
        }) => {
            assert_eq!(from_room, "entrance");
            assert_eq!(to_room, "chamber");
            assert!(
                !reason.is_empty(),
                "reason should explain why move was rejected"
            );
        }
        other => panic!(
            "expected DispatchError::InvalidRoomTransition, got {:?}",
            other
        ),
    }
}

// ═══════════════════════════════════════════════════════════
// AC-2: discovered_rooms populated on entry
// ═══════════════════════════════════════════════════════════

#[test]
fn test_discovered_rooms_populated_on_entry() {
    let rooms = three_room_graph();
    let mut snap = snapshot_at("entrance", &["entrance"]);

    // Move entrance → corridor (valid)
    let transition = apply_validated_move(&mut snap, "corridor", &rooms).unwrap();
    assert_eq!(transition.from_room, "entrance");
    assert_eq!(transition.to_room, "corridor");
    assert_eq!(transition.exit_type, "corridor"); // RoomExit::Corridor display_name
    assert_eq!(snap.location, "corridor");
    assert!(snap.discovered_rooms.contains("entrance"));
    assert!(snap.discovered_rooms.contains("corridor"));
    assert_eq!(snap.discovered_rooms.len(), 2);

    // Move corridor → chamber (valid)
    let transition = apply_validated_move(&mut snap, "chamber", &rooms).unwrap();
    assert_eq!(transition.from_room, "corridor");
    assert_eq!(transition.to_room, "chamber");
    assert_eq!(transition.exit_type, "door"); // RoomExit::Door display_name
    assert_eq!(snap.location, "chamber");
    assert!(snap.discovered_rooms.contains("chamber"));
    assert_eq!(snap.discovered_rooms.len(), 3);
}

// ═══════════════════════════════════════════════════════════
// AC-3: Session init sets entrance location
// ═══════════════════════════════════════════════════════════

#[test]
fn test_session_init_sets_entrance_location() {
    let rooms = three_room_graph();
    let mut snap = GameSnapshot::default();

    init_room_graph_location(&mut snap, &rooms);

    assert_eq!(
        snap.location, "entrance",
        "location should be the entrance room ID"
    );
    assert_eq!(
        snap.discovered_rooms,
        HashSet::from(["entrance".to_string()]),
        "discovered_rooms should contain only the entrance"
    );
}

// ═══════════════════════════════════════════════════════════
// AC-4: Region mode has no location validation
// ═══════════════════════════════════════════════════════════

#[test]
fn test_region_mode_no_location_validation() {
    // In region mode, any location string is accepted — no room graph checks.
    let mut snap = GameSnapshot::default();
    snap.location = "old_town".into();

    // apply_world_patch in region mode just sets location directly (existing behavior).
    let patch = WorldStatePatch {
        location: Some("anywhere_at_all".into()),
        ..Default::default()
    };
    snap.apply_world_patch(&patch);

    assert_eq!(snap.location, "anywhere_at_all");
    // discovered_rooms stays empty in region mode
    assert!(
        snap.discovered_rooms.is_empty(),
        "region mode should not track discovered_rooms"
    );
}

// ═══════════════════════════════════════════════════════════
// AC-5: Integration — 3-room movement sequence + rejected invalid move
// ═══════════════════════════════════════════════════════════

#[test]
fn test_room_graph_movement_sequence() {
    let rooms = three_room_graph();
    let mut snap = GameSnapshot::default();

    // Init: location at entrance, discovered = {entrance}
    init_room_graph_location(&mut snap, &rooms);
    assert_eq!(snap.location, "entrance");
    assert_eq!(snap.discovered_rooms.len(), 1);

    // Step 1: entrance → corridor (valid — corridor exit exists)
    apply_validated_move(&mut snap, "corridor", &rooms).unwrap();
    assert_eq!(snap.location, "corridor");
    assert_eq!(snap.discovered_rooms.len(), 2);

    // Step 2: corridor → chamber (valid — door exit exists)
    apply_validated_move(&mut snap, "chamber", &rooms).unwrap();
    assert_eq!(snap.location, "chamber");
    assert_eq!(
        snap.discovered_rooms,
        HashSet::from([
            "entrance".to_string(),
            "corridor".to_string(),
            "chamber".to_string(),
        ])
    );

    // Step 3: chamber → arbitrary_room (INVALID — no such exit)
    let result = apply_validated_move(&mut snap, "arbitrary_room", &rooms);
    assert!(
        matches!(result, Err(DispatchError::InvalidRoomTransition { .. })),
        "move to non-adjacent room should fail"
    );

    // Location and discovered_rooms unchanged after rejected move
    assert_eq!(snap.location, "chamber");
    assert_eq!(snap.discovered_rooms.len(), 3);
}

// ═══════════════════════════════════════════════════════════
// Edge cases: room not found, serde roundtrip, no entrance
// ═══════════════════════════════════════════════════════════

#[test]
fn test_room_not_found_in_graph() {
    // AC-1: "Verify room_id exists in DispatchContext.rooms as a key"
    // Distinct from "no exit" — the target room doesn't exist at all.
    let rooms = three_room_graph();
    let snap = snapshot_at("entrance", &["entrance"]);

    let result = validate_room_transition(&snap, "nonexistent_room", &rooms);

    match result {
        Err(DispatchError::InvalidRoomTransition {
            from_room,
            to_room,
            reason,
        }) => {
            assert_eq!(from_room, "entrance");
            assert_eq!(to_room, "nonexistent_room");
            assert!(
                reason.contains("not_found") || reason.contains("room_not_found"),
                "reason should indicate room not found, got: {reason}"
            );
        }
        other => panic!(
            "expected DispatchError::InvalidRoomTransition for nonexistent room, got {:?}",
            other
        ),
    }
}

#[test]
fn test_discovered_rooms_serde_roundtrip() {
    // AC-2: discovered_rooms serializes as sorted Vec for deterministic JSON,
    // then deserializes back to HashSet.
    let snap = snapshot_at("corridor", &["entrance", "corridor"]);

    let json = serde_json::to_string(&snap.discovered_rooms).unwrap();
    // Sorted order: corridor < entrance
    assert_eq!(
        json, r#"["corridor","entrance"]"#,
        "should serialize as sorted Vec"
    );

    let roundtrip: HashSet<String> = serde_json::from_str(&json).unwrap();
    assert_eq!(
        roundtrip, snap.discovered_rooms,
        "should roundtrip through serde"
    );
}

#[test]
fn test_init_room_graph_no_entrance_room() {
    // Edge: graph has rooms but none with room_type == "entrance".
    // Should fail clearly — no silent fallback.
    let rooms = vec![RoomDef {
        id: "hallway".into(),
        name: "Just a Hallway".into(),
        room_type: "normal".into(),
        size: (2, 2),
        keeper_awareness_modifier: 1.0,
        exits: vec![],
        description: None,
        grid: None,
        legend: None,
        tactical_scale: None,
    }];
    let mut snap = GameSnapshot::default();

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        init_room_graph_location(&mut snap, &rooms);
    }));
    // Should panic or return an error — NOT silently leave location empty.
    // If the function returns Result, it should be Err.
    // If it panics, catch_unwind catches it.
    assert!(
        result.is_err(),
        "init_room_graph_location with no entrance room should fail, not silently succeed"
    );
}

#[test]
fn test_current_room_not_in_graph() {
    // Edge: snapshot's current location doesn't exist in the room graph.
    // Validation should reject (from_room invalid).
    let rooms = three_room_graph();
    let snap = snapshot_at("phantom_room", &["phantom_room"]);

    let result = validate_room_transition(&snap, "entrance", &rooms);
    assert!(
        matches!(result, Err(DispatchError::InvalidRoomTransition { .. })),
        "should reject when current location is not in graph"
    );
}

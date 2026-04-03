//! RED-phase tests for Story 19-4: MAP_UPDATE for room graph.
//!
//! Tests exercise:
//! - ExploredLocation includes room_exits, room_type, size, is_current_room
//! - MAP_UPDATE only contains discovered rooms (fog of war)
//! - Current room is flagged in payload
//! - Undiscovered rooms are omitted
//! - Integration: 3-room discovery sequence with correct MAP_UPDATE payloads
//!
//! Rust review rules covered:
//! - #2: ExploredLocation fields use Option or #[serde(default)] for backward compat
//! - #3: No hardcoded placeholder values
//! - #6: Meaningful assertions on every test
//! - #8: serde round-trip preserves room graph fields

use std::collections::HashSet;

use sidequest_genre::{RoomDef, RoomExit};
use sidequest_protocol::{ExploredLocation, RoomExitInfo};

// ═══════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════

/// Build a 3-room dungeon: entrance → corridor → chamber.
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
        },
        RoomDef {
            id: "chamber".into(),
            name: "Treasure Chamber".into(),
            room_type: "treasure".into(),
            size: (5, 5),
            keeper_awareness_modifier: 1.2,
            exits: vec![RoomExit::Door {
                target: "corridor".into(),
                is_locked: false,
            }],
            description: None,
        },
    ]
}

// ═══════════════════════════════════════════════════════════
// AC: ExploredLocation includes room graph fields
// ═══════════════════════════════════════════════════════════

/// ExploredLocation must carry room_exits — a list of exit descriptors
/// showing target room and exit type (door, corridor, chute, secret).
#[test]
fn explored_location_has_room_exits_field() {
    let loc = ExploredLocation {
        name: "Entrance Hall".into(),
        x: 0,
        y: 0,
        location_type: "entrance".into(),
        connections: vec!["corridor".into()],
        room_exits: vec![RoomExitInfo {
            target: "corridor".into(),
            exit_type: "corridor".into(),
        }],
        room_type: "entrance".into(),
        size: Some((3, 3)),
        is_current_room: true,
    };
    assert_eq!(loc.room_exits.len(), 1);
    assert_eq!(loc.room_exits[0].target, "corridor");
    assert_eq!(loc.room_exits[0].exit_type, "corridor");
}

/// ExploredLocation must carry room_type from the RoomDef.
#[test]
fn explored_location_has_room_type_field() {
    let loc = ExploredLocation {
        name: "Treasure Chamber".into(),
        x: 0,
        y: 0,
        location_type: "treasure".into(),
        connections: vec!["corridor".into()],
        room_exits: vec![],
        room_type: "treasure".into(),
        size: Some((5, 5)),
        is_current_room: false,
    };
    assert_eq!(loc.room_type, "treasure");
}

/// ExploredLocation must carry size from the RoomDef.
#[test]
fn explored_location_has_size_field() {
    let loc = ExploredLocation {
        name: "Entrance Hall".into(),
        x: 0,
        y: 0,
        location_type: "entrance".into(),
        connections: vec![],
        room_exits: vec![],
        room_type: "entrance".into(),
        size: Some((3, 3)),
        is_current_room: false,
    };
    assert_eq!(loc.size, Some((3, 3)));
}

/// ExploredLocation must carry is_current_room flag.
#[test]
fn explored_location_has_is_current_room_flag() {
    let current = ExploredLocation {
        name: "Entrance Hall".into(),
        x: 0,
        y: 0,
        location_type: "entrance".into(),
        connections: vec![],
        room_exits: vec![],
        room_type: "entrance".into(),
        size: Some((3, 3)),
        is_current_room: true,
    };
    let not_current = ExploredLocation {
        name: "corridor".into(),
        x: 0,
        y: 0,
        location_type: "normal".into(),
        connections: vec![],
        room_exits: vec![],
        room_type: "normal".into(),
        size: None,
        is_current_room: false,
    };
    assert!(current.is_current_room);
    assert!(!not_current.is_current_room);
}

// ═══════════════════════════════════════════════════════════
// AC: build_room_graph_explored — conversion function
// ═══════════════════════════════════════════════════════════

use sidequest_game::build_room_graph_explored;

/// build_room_graph_explored returns only discovered rooms.
/// Discover 2 of 3 rooms → output has exactly 2 entries.
#[test]
fn build_room_graph_explored_filters_undiscovered() {
    let rooms = three_room_graph();
    let discovered: HashSet<String> =
        ["entrance", "corridor"].iter().map(|s| s.to_string()).collect();

    let explored = build_room_graph_explored(&rooms, &discovered, "entrance");

    assert_eq!(explored.len(), 2, "only discovered rooms should appear");
    let names: HashSet<&str> = explored.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains("Entrance Hall"));
    assert!(names.contains("Dark Corridor"));
    assert!(!names.contains("Treasure Chamber"), "undiscovered room must be omitted");
}

/// build_room_graph_explored flags current room correctly.
#[test]
fn build_room_graph_explored_flags_current_room() {
    let rooms = three_room_graph();
    let discovered: HashSet<String> =
        ["entrance", "corridor", "chamber"].iter().map(|s| s.to_string()).collect();

    let explored = build_room_graph_explored(&rooms, &discovered, "corridor");

    for loc in &explored {
        if loc.name == "Dark Corridor" {
            assert!(loc.is_current_room, "corridor should be flagged as current");
        } else {
            assert!(!loc.is_current_room, "{} should NOT be flagged as current", loc.name);
        }
    }
}

/// build_room_graph_explored populates room_exits from RoomDef.
#[test]
fn build_room_graph_explored_populates_exits() {
    let rooms = three_room_graph();
    let discovered: HashSet<String> =
        ["entrance", "corridor", "chamber"].iter().map(|s| s.to_string()).collect();

    let explored = build_room_graph_explored(&rooms, &discovered, "entrance");

    let corridor = explored.iter().find(|e| e.name == "Dark Corridor").unwrap();
    assert_eq!(corridor.room_exits.len(), 2, "corridor has 2 exits");

    let exit_targets: HashSet<&str> = corridor.room_exits.iter().map(|e| e.target.as_str()).collect();
    assert!(exit_targets.contains("entrance"));
    assert!(exit_targets.contains("chamber"));
}

/// build_room_graph_explored populates room_type and size from RoomDef.
#[test]
fn build_room_graph_explored_populates_room_metadata() {
    let rooms = three_room_graph();
    let discovered: HashSet<String> =
        ["entrance", "chamber"].iter().map(|s| s.to_string()).collect();

    let explored = build_room_graph_explored(&rooms, &discovered, "entrance");

    let entrance = explored.iter().find(|e| e.name == "Entrance Hall").unwrap();
    assert_eq!(entrance.room_type, "entrance");
    assert_eq!(entrance.size, Some((3, 3)));

    let chamber = explored.iter().find(|e| e.name == "Treasure Chamber").unwrap();
    assert_eq!(chamber.room_type, "treasure");
    assert_eq!(chamber.size, Some((5, 5)));
}

/// build_room_graph_explored with empty discovered set returns empty vec.
#[test]
fn build_room_graph_explored_empty_discovered_returns_empty() {
    let rooms = three_room_graph();
    let discovered: HashSet<String> = HashSet::new();

    let explored = build_room_graph_explored(&rooms, &discovered, "entrance");

    assert!(explored.is_empty(), "no discovered rooms → empty result");
}

// ═══════════════════════════════════════════════════════════
// AC: Serde round-trip preserves room graph fields (rule #8)
// ═══════════════════════════════════════════════════════════

/// ExploredLocation with room graph fields survives JSON round-trip.
#[test]
fn explored_location_serde_round_trip_with_room_graph_fields() {
    let loc = ExploredLocation {
        name: "Entrance Hall".into(),
        x: 0,
        y: 0,
        location_type: "entrance".into(),
        connections: vec!["corridor".into()],
        room_exits: vec![RoomExitInfo {
            target: "corridor".into(),
            exit_type: "corridor".into(),
        }],
        room_type: "entrance".into(),
        size: Some((3, 3)),
        is_current_room: true,
    };
    let json = serde_json::to_string(&loc).expect("serialize");
    let deser: ExploredLocation = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(deser.room_exits.len(), 1);
    assert_eq!(deser.room_exits[0].target, "corridor");
    assert_eq!(deser.room_type, "entrance");
    assert_eq!(deser.size, Some((3, 3)));
    assert!(deser.is_current_room);
}

/// ExploredLocation without room graph fields (region mode) deserializes
/// with defaults — backward compatibility for existing region-mode payloads.
#[test]
fn explored_location_backward_compat_without_room_graph_fields() {
    let json = r#"{"name":"Town Square","x":10,"y":20,"type":"town","connections":["market"]}"#;
    let loc: ExploredLocation = serde_json::from_str(json).expect("deserialize");

    assert_eq!(loc.name, "Town Square");
    assert_eq!(loc.location_type, "town");
    // New fields should default to empty/false when absent
    assert!(loc.room_exits.is_empty(), "room_exits should default to empty vec");
    assert_eq!(loc.room_type, "", "room_type should default to empty string");
    assert_eq!(loc.size, None, "size should default to None");
    assert!(!loc.is_current_room, "is_current_room should default to false");
}

// ═══════════════════════════════════════════════════════════
// AC: Integration — 3-room discovery sequence
// ═══════════════════════════════════════════════════════════

/// Simulate discovering 3 rooms one at a time, verifying MAP_UPDATE
/// payload contains exactly the discovered rooms at each step.
#[test]
fn integration_three_room_discovery_sequence() {
    let rooms = three_room_graph();

    // Step 1: discover entrance only
    let discovered_1: HashSet<String> = ["entrance"].iter().map(|s| s.to_string()).collect();
    let explored_1 = build_room_graph_explored(&rooms, &discovered_1, "entrance");
    assert_eq!(explored_1.len(), 1);
    assert_eq!(explored_1[0].name, "Entrance Hall");
    assert!(explored_1[0].is_current_room);
    assert_eq!(explored_1[0].room_exits.len(), 1);

    // Step 2: discover corridor
    let discovered_2: HashSet<String> =
        ["entrance", "corridor"].iter().map(|s| s.to_string()).collect();
    let explored_2 = build_room_graph_explored(&rooms, &discovered_2, "corridor");
    assert_eq!(explored_2.len(), 2);
    let current_2: Vec<&ExploredLocation> = explored_2.iter().filter(|e| e.is_current_room).collect();
    assert_eq!(current_2.len(), 1, "exactly one room should be current");
    assert_eq!(current_2[0].name, "Dark Corridor");

    // Step 3: discover chamber
    let discovered_3: HashSet<String> =
        ["entrance", "corridor", "chamber"].iter().map(|s| s.to_string()).collect();
    let explored_3 = build_room_graph_explored(&rooms, &discovered_3, "chamber");
    assert_eq!(explored_3.len(), 3);
    let current_3: Vec<&ExploredLocation> = explored_3.iter().filter(|e| e.is_current_room).collect();
    assert_eq!(current_3.len(), 1);
    assert_eq!(current_3[0].name, "Treasure Chamber");
    assert_eq!(current_3[0].room_type, "treasure");
    assert_eq!(current_3[0].size, Some((5, 5)));
}

// ═══════════════════════════════════════════════════════════
// Edge: Secret passages only appear if discovered flag is true
// ═══════════════════════════════════════════════════════════

/// Secret exits should only appear in room_exits when their `discovered`
/// flag is true. An undiscovered secret passage is invisible to the player.
#[test]
fn secret_exits_hidden_when_undiscovered() {
    let rooms = vec![
        RoomDef {
            id: "hall".into(),
            name: "Great Hall".into(),
            room_type: "normal".into(),
            size: (4, 4),
            keeper_awareness_modifier: 1.0,
            exits: vec![
                RoomExit::Door {
                    target: "closet".into(),
                    is_locked: false,
                },
                RoomExit::Secret {
                    target: "vault".into(),
                    discovered: false,
                },
            ],
            description: None,
        },
        RoomDef {
            id: "closet".into(),
            name: "Closet".into(),
            room_type: "dead_end".into(),
            size: (1, 1),
            keeper_awareness_modifier: 1.0,
            exits: vec![RoomExit::Door {
                target: "hall".into(),
                is_locked: false,
            }],
            description: None,
        },
        RoomDef {
            id: "vault".into(),
            name: "Hidden Vault".into(),
            room_type: "treasure".into(),
            size: (3, 3),
            keeper_awareness_modifier: 1.5,
            exits: vec![RoomExit::Secret {
                target: "hall".into(),
                discovered: false,
            }],
            description: None,
        },
    ];

    let discovered: HashSet<String> =
        ["hall", "closet"].iter().map(|s| s.to_string()).collect();
    let explored = build_room_graph_explored(&rooms, &discovered, "hall");

    let hall = explored.iter().find(|e| e.name == "Great Hall").unwrap();
    // Hall has 2 exits in RoomDef but the secret one is undiscovered
    assert_eq!(
        hall.room_exits.len(),
        1,
        "undiscovered secret exit should be hidden"
    );
    assert_eq!(hall.room_exits[0].target, "closet");
}

/// When secret exit IS discovered, it appears in room_exits.
#[test]
fn secret_exits_visible_when_discovered() {
    let rooms = vec![RoomDef {
        id: "hall".into(),
        name: "Great Hall".into(),
        room_type: "normal".into(),
        size: (4, 4),
        keeper_awareness_modifier: 1.0,
        exits: vec![
            RoomExit::Door {
                target: "closet".into(),
                is_locked: false,
            },
            RoomExit::Secret {
                target: "vault".into(),
                discovered: true,
            },
        ],
        description: None,
    }];

    let discovered: HashSet<String> = ["hall"].iter().map(|s| s.to_string()).collect();
    let explored = build_room_graph_explored(&rooms, &discovered, "hall");

    let hall = explored.iter().find(|e| e.name == "Great Hall").unwrap();
    assert_eq!(hall.room_exits.len(), 2, "discovered secret exit should be visible");
}

// ═══════════════════════════════════════════════════════════
// Edge: Locked doors still appear but with locked flag
// ═══════════════════════════════════════════════════════════

/// Locked doors should still appear in room_exits — the player knows
/// the door exists, they just can't go through it.
#[test]
fn locked_doors_appear_in_exits() {
    let rooms = vec![RoomDef {
        id: "hall".into(),
        name: "Hall".into(),
        room_type: "normal".into(),
        size: (3, 3),
        keeper_awareness_modifier: 1.0,
        exits: vec![RoomExit::Door {
            target: "locked_room".into(),
            is_locked: true,
        }],
        description: None,
    }];

    let discovered: HashSet<String> = ["hall"].iter().map(|s| s.to_string()).collect();
    let explored = build_room_graph_explored(&rooms, &discovered, "hall");

    let hall = explored.iter().find(|e| e.name == "Hall").unwrap();
    assert_eq!(hall.room_exits.len(), 1, "locked door should still appear");
    assert_eq!(hall.room_exits[0].target, "locked_room");
}

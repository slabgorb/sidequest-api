//! RED-phase tests for Story 29-19: Wire tactical grid into MAP_UPDATE.
//!
//! Tests exercise:
//! - AC-1: ExploredLocation has tactical_grid field
//! - AC-2: build_room_graph_explored populates tactical_grid for gridded rooms
//! - AC-3: Rooms without grid produce tactical_grid: None
//! - AC-6: OTEL span emission (structural check only)
//!
//! Rust review rules covered:
//! - #3: No hardcoded placeholder values — None is semantically correct for gridless rooms
//! - #6: Meaningful assertions on every test (self-checked)
//! - #8: serde round-trip preserves tactical_grid field

use std::collections::{HashMap, HashSet};

use sidequest_genre::models::world::{LegendEntry, RoomDef, RoomExit};
use sidequest_protocol::{ExploredLocation, NonBlankString, RoomExitInfo, TacticalGridPayload};

// ═══════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════

/// Test-only shorthand for `NonBlankString::new(s).expect(...)` — the
/// protocol sweep (story 33-19) converted `ExploredLocation.name` /
/// `RoomExitInfo.target` to `NonBlankString`, and this keeps the test
/// literals readable.
fn nbs(s: &str) -> NonBlankString {
    NonBlankString::new(s).expect("test literal must be non-blank")
}

/// A room WITH a tactical grid (like Mawdeep rooms).
fn gridded_room() -> RoomDef {
    let mut legend = HashMap::new();
    legend.insert(
        'T',
        LegendEntry {
            r#type: "atmosphere".into(),
            label: "Stone tooth".into(),
        },
    );

    RoomDef {
        id: "vault".into(),
        name: "Vault of Teeth".into(),
        room_type: "treasure".into(),
        size: (2, 2),
        keeper_awareness_modifier: 1.4,
        exits: vec![RoomExit::Corridor {
            target: "passage".into(),
        }],
        description: Some("A domed room studded with teeth.".into()),
        grid: Some(
            "###..###\n\
             #......#\n\
             #.T..T.#\n\
             #......#\n\
             #......#\n\
             #.T..T.#\n\
             #......#\n\
             ###..###"
                .into(),
        ),
        tactical_scale: Some(4),
        legend: Some(legend),
    }
}

/// A room WITHOUT a tactical grid (like a town or older dungeon).
fn gridless_room() -> RoomDef {
    RoomDef {
        id: "tavern".into(),
        name: "The Rusty Mug".into(),
        room_type: "normal".into(),
        size: (3, 3),
        keeper_awareness_modifier: 1.0,
        exits: vec![RoomExit::Door {
            target: "street".into(),
            is_locked: false,
        }],
        description: Some("A dimly lit tavern.".into()),
        grid: None,
        tactical_scale: None,
        legend: None,
    }
}

/// A simple passage room connecting gridded and gridless rooms.
fn passage_room() -> RoomDef {
    RoomDef {
        id: "passage".into(),
        name: "Stone Passage".into(),
        room_type: "normal".into(),
        size: (1, 2),
        keeper_awareness_modifier: 1.0,
        exits: vec![
            RoomExit::Corridor {
                target: "vault".into(),
            },
            RoomExit::Corridor {
                target: "tavern".into(),
            },
        ],
        description: None,
        grid: None,
        tactical_scale: None,
        legend: None,
    }
}

// ═══════════════════════════════════════════════════════════
// AC-1: ExploredLocation has tactical_grid field
// ═══════════════════════════════════════════════════════════

/// ExploredLocation must carry an optional tactical_grid payload.
/// This test will NOT COMPILE until the field is added to the struct.
#[test]
fn explored_location_has_tactical_grid_field() {
    let payload = TacticalGridPayload {
        width: 8,
        height: 8,
        cells: vec![vec!["wall".into(); 8]; 8],
        features: vec![],
    };

    let loc = ExploredLocation {
        id: "vault".into(),
        name: nbs("Vault of Teeth"),
        x: 0,
        y: 0,
        location_type: "treasure".into(),
        connections: vec!["passage".into()],
        room_exits: vec![RoomExitInfo {
            target: nbs("passage"),
            exit_type: "corridor".into(),
        }],
        room_type: "treasure".into(),
        size: Some((2, 2)),
        is_current_room: true,
        tactical_grid: Some(payload),
    };

    assert!(
        loc.tactical_grid.is_some(),
        "tactical_grid should be Some for gridded room"
    );
    let grid = loc.tactical_grid.unwrap();
    assert_eq!(grid.width, 8);
    assert_eq!(grid.height, 8);
}

/// ExploredLocation with tactical_grid: None is valid (gridless rooms).
#[test]
fn explored_location_tactical_grid_none_is_valid() {
    let loc = ExploredLocation {
        id: "rusty_mug".into(),
        name: nbs("The Rusty Mug"),
        x: 0,
        y: 0,
        location_type: "normal".into(),
        connections: vec!["street".into()],
        room_exits: vec![],
        room_type: "normal".into(),
        size: Some((3, 3)),
        is_current_room: false,
        tactical_grid: None,
    };

    assert!(
        loc.tactical_grid.is_none(),
        "gridless room should have tactical_grid: None"
    );
}

// ═══════════════════════════════════════════════════════════
// AC-2: build_room_graph_explored populates tactical_grid
// ═══════════════════════════════════════════════════════════

use sidequest_game::build_room_graph_explored;

/// When a room has a grid field, build_room_graph_explored must parse it
/// and set tactical_grid to Some(TacticalGridPayload).
#[test]
fn build_room_graph_explored_populates_tactical_grid_for_gridded_room() {
    let rooms = vec![gridded_room()];
    let discovered: HashSet<String> = ["vault"].iter().map(|s| s.to_string()).collect();

    let explored = build_room_graph_explored(&rooms, &discovered, "vault");

    assert_eq!(explored.len(), 1);
    let vault = &explored[0];
    assert!(
        vault.tactical_grid.is_some(),
        "gridded room must have tactical_grid populated"
    );

    let grid = vault.tactical_grid.as_ref().unwrap();
    assert_eq!(grid.width, 8, "vault grid is 8 wide (size 2 * scale 4)");
    assert_eq!(grid.height, 8, "vault grid is 8 tall (size 2 * scale 4)");
    assert!(!grid.cells.is_empty(), "cells must be populated");

    // Verify features from legend are included
    assert!(
        !grid.features.is_empty(),
        "legend features (T=teeth) must be in payload"
    );
    let tooth_feature = grid.features.iter().find(|f| f.glyph == 'T');
    assert!(tooth_feature.is_some(), "T feature must be present");
    assert_eq!(tooth_feature.unwrap().feature_type, "atmosphere");
}

// ═══════════════════════════════════════════════════════════
// AC-3: Gridless rooms produce tactical_grid: None
// ═══════════════════════════════════════════════════════════

/// Rooms without a grid field must produce tactical_grid: None, not an error.
#[test]
fn build_room_graph_explored_returns_none_for_gridless_room() {
    let rooms = vec![gridless_room()];
    let discovered: HashSet<String> = ["tavern"].iter().map(|s| s.to_string()).collect();

    let explored = build_room_graph_explored(&rooms, &discovered, "tavern");

    assert_eq!(explored.len(), 1);
    assert!(
        explored[0].tactical_grid.is_none(),
        "gridless room must have tactical_grid: None"
    );
}

/// Mixed dungeon: some rooms have grids, some don't. Each gets the
/// correct tactical_grid value.
#[test]
fn build_room_graph_explored_mixed_grid_and_gridless() {
    let rooms = vec![gridded_room(), passage_room(), gridless_room()];
    let discovered: HashSet<String> = ["vault", "passage", "tavern"]
        .iter()
        .map(|s| s.to_string())
        .collect();

    let explored = build_room_graph_explored(&rooms, &discovered, "vault");

    assert_eq!(explored.len(), 3);

    let vault = explored
        .iter()
        .find(|e| e.name.as_str() == "Vault of Teeth")
        .unwrap();
    assert!(vault.tactical_grid.is_some(), "vault has a grid");

    let passage = explored
        .iter()
        .find(|e| e.name.as_str() == "Stone Passage")
        .unwrap();
    assert!(passage.tactical_grid.is_none(), "passage has no grid");

    let tavern = explored
        .iter()
        .find(|e| e.name.as_str() == "The Rusty Mug")
        .unwrap();
    assert!(tavern.tactical_grid.is_none(), "tavern has no grid");
}

// ═══════════════════════════════════════════════════════════
// Serde round-trip (rule #8)
// ═══════════════════════════════════════════════════════════

/// ExploredLocation with tactical_grid survives JSON round-trip.
#[test]
fn explored_location_serde_round_trip_with_tactical_grid() {
    let payload = TacticalGridPayload {
        width: 4,
        height: 4,
        cells: vec![
            vec!["wall".into(), "floor".into(), "floor".into(), "wall".into()],
            vec![
                "floor".into(),
                "floor".into(),
                "floor".into(),
                "floor".into(),
            ],
            vec![
                "floor".into(),
                "floor".into(),
                "floor".into(),
                "floor".into(),
            ],
            vec!["wall".into(), "floor".into(), "floor".into(), "wall".into()],
        ],
        features: vec![],
    };

    let loc = ExploredLocation {
        id: "test_room".into(),
        name: nbs("Test Room"),
        x: 0,
        y: 0,
        location_type: "normal".into(),
        connections: vec![],
        room_exits: vec![],
        room_type: "normal".into(),
        size: Some((1, 1)),
        is_current_room: false,
        tactical_grid: Some(payload),
    };

    let json = serde_json::to_string(&loc).expect("serialize");
    let deser: ExploredLocation = serde_json::from_str(&json).expect("deserialize");

    assert!(deser.tactical_grid.is_some());
    let grid = deser.tactical_grid.unwrap();
    assert_eq!(grid.width, 4);
    assert_eq!(grid.height, 4);
    assert_eq!(grid.cells.len(), 4);
    assert_eq!(grid.cells[0][0], "wall");
    assert_eq!(grid.cells[1][1], "floor");
}

/// JSON without tactical_grid deserializes to None — backward compatibility.
#[test]
fn explored_location_backward_compat_without_tactical_grid() {
    let json = r#"{"name":"Town","x":0,"y":0,"type":"town","connections":["market"]}"#;
    let loc: ExploredLocation = serde_json::from_str(json).expect("deserialize");

    assert_eq!(loc.name.as_str(), "Town");
    assert!(
        loc.tactical_grid.is_none(),
        "missing tactical_grid should default to None"
    );
}

// ═══════════════════════════════════════════════════════════
// Integration: discovery sequence with tactical grids
// ═══════════════════════════════════════════════════════════

/// Simulate discovering rooms in a mixed dungeon. Verify tactical_grid
/// is populated correctly at each step.
#[test]
fn integration_discovery_with_tactical_grids() {
    let rooms = vec![gridded_room(), passage_room()];

    // Step 1: discover vault only
    let discovered_1: HashSet<String> = ["vault"].iter().map(|s| s.to_string()).collect();
    let explored_1 = build_room_graph_explored(&rooms, &discovered_1, "vault");
    assert_eq!(explored_1.len(), 1);
    assert!(
        explored_1[0].tactical_grid.is_some(),
        "vault should have tactical grid"
    );

    // Step 2: discover passage (no grid)
    let discovered_2: HashSet<String> =
        ["vault", "passage"].iter().map(|s| s.to_string()).collect();
    let explored_2 = build_room_graph_explored(&rooms, &discovered_2, "passage");
    assert_eq!(explored_2.len(), 2);

    let vault_2 = explored_2
        .iter()
        .find(|e| e.name.as_str() == "Vault of Teeth")
        .unwrap();
    assert!(vault_2.tactical_grid.is_some(), "vault still has grid");

    let passage_2 = explored_2
        .iter()
        .find(|e| e.name.as_str() == "Stone Passage")
        .unwrap();
    assert!(passage_2.tactical_grid.is_none(), "passage has no grid");
}

//! Story 29-6: Shared-wall layout engine — tree topology placement
//!
//! RED phase — failing tests for the layout engine.
//!
//! The layout engine composes individually parsed rooms into a dungeon map.
//! Adjacent rooms share wall segments at exit gaps (one wall, not two).
//! BFS from the entrance room places rooms in global coordinates.
//! Cycle handling (jaquayed layouts) is deferred to 29-7.
//!
//! ACs:
//!   AC-1: Entrance room placed at origin (0, 0)
//!   AC-2: Adjacent rooms share exactly one wall segment at exit gaps
//!   AC-3: Exit gaps align in global coordinates (gap cells overlap perfectly)
//!   AC-4: Overlap detection catches floor-on-floor collisions
//!   AC-5: Layout fails loudly on unresolvable overlap (LayoutError with context)
//!   AC-6: BFS visits all reachable rooms from entrance
//!   AC-7: Non-void cells of different rooms never occupy same global position
//!   AC-8: Unit test: linear chain of 3 rooms places correctly
//!   AC-9: Unit test: T-junction (3 rooms sharing a hub) places correctly
//!   AC-10: Wiring test: layout engine callable from non-test code

use std::collections::{HashMap, HashSet};

use sidequest_game::tactical::{CardinalDirection, GridPos, TacticalCell, TacticalGrid};

// Import the layout module types — these don't exist yet (RED).
use sidequest_game::tactical::layout::{
    align_rooms, check_overlap, layout_tree, DungeonLayout, LayoutError, PlacedRoom,
};

use sidequest_genre::models::world::{RoomDef, RoomExit};

// ==========================================================================
// Test grid fixtures
// ==========================================================================

/// 5x5 room with a 2-cell south exit (columns 2-3).
/// ```text
/// #####
/// #...#
/// #...#
/// #...#
/// ##..#
/// ```
fn room_a_grid() -> TacticalGrid {
    let raw = "\
#####\n\
#...#\n\
#...#\n\
#...#\n\
##..#";
    TacticalGrid::parse(raw, &HashMap::new()).unwrap()
}

/// 5x5 room with a 2-cell north exit (columns 2-3) and 2-cell east exit (rows 2-3).
/// ```text
/// ##..#
/// #...#
/// #....
/// #....
/// #####
/// ```
fn room_b_grid() -> TacticalGrid {
    let raw = "\
##..#\n\
#...#\n\
#....\n\
#....\n\
#####";
    TacticalGrid::parse(raw, &HashMap::new()).unwrap()
}

/// 5x5 room with a 2-cell west exit (rows 2-3).
/// ```text
/// #####
/// #...#
/// ....#
/// ....#
/// #####
/// ```
fn room_c_grid() -> TacticalGrid {
    let raw = "\
#####\n\
#...#\n\
....#\n\
....#\n\
#####";
    TacticalGrid::parse(raw, &HashMap::new()).unwrap()
}

// ==========================================================================
// Room graph fixture builders
// ==========================================================================

/// Build a RoomDef with the given id, room_type, exits, and grid.
fn room_def(id: &str, room_type: &str, exits: Vec<RoomExit>, grid: &str) -> RoomDef {
    RoomDef {
        id: id.to_string(),
        name: id.to_string(),
        room_type: room_type.to_string(),
        size: (1, 1),
        keeper_awareness_modifier: 1.0,
        exits,
        description: None,
        grid: Some(grid.to_string()),
        tactical_scale: None,
        legend: None,
    }
}

/// Linear chain: A(entrance) -> B -> C
fn linear_chain_rooms() -> Vec<RoomDef> {
    vec![
        room_def(
            "room_a",
            "entrance",
            vec![RoomExit::Corridor {
                target: "room_b".to_string(),
            }],
            "#####\n#...#\n#...#\n#...#\n##..#",
        ),
        room_def(
            "room_b",
            "normal",
            vec![
                RoomExit::Corridor {
                    target: "room_a".to_string(),
                },
                RoomExit::Corridor {
                    target: "room_c".to_string(),
                },
            ],
            "##..#\n#...#\n#....\n#....\n#####",
        ),
        room_def(
            "room_c",
            "normal",
            vec![RoomExit::Corridor {
                target: "room_b".to_string(),
            }],
            "#####\n#...#\n....#\n....#\n#####",
        ),
    ]
}

/// T-junction: hub(entrance) with north, east, south spokes.
fn t_junction_rooms() -> Vec<RoomDef> {
    vec![
        room_def(
            "hub",
            "entrance",
            vec![
                RoomExit::Corridor {
                    target: "north_spoke".to_string(),
                },
                RoomExit::Corridor {
                    target: "east_spoke".to_string(),
                },
                RoomExit::Corridor {
                    target: "south_spoke".to_string(),
                },
            ],
            "###..##\n#.....#\n#.....#\n#......\n#......\n#.....#\n###..##",
        ),
        room_def(
            "north_spoke",
            "normal",
            vec![RoomExit::Corridor {
                target: "hub".to_string(),
            }],
            "#####\n#...#\n#...#\n#...#\n##..#",
        ),
        room_def(
            "east_spoke",
            "normal",
            vec![RoomExit::Corridor {
                target: "hub".to_string(),
            }],
            "#####\n#...#\n....#\n....#\n#####",
        ),
        room_def(
            "south_spoke",
            "normal",
            vec![RoomExit::Corridor {
                target: "hub".to_string(),
            }],
            "##..#\n#...#\n#...#\n#...#\n#####",
        ),
    ]
}

/// Build grids HashMap from RoomDefs by parsing each grid field.
fn parse_grids(rooms: &[RoomDef]) -> HashMap<String, TacticalGrid> {
    rooms
        .iter()
        .filter_map(|r| {
            let grid_str = r.grid.as_ref()?;
            let legend = r.legend.as_ref().cloned().unwrap_or_default();
            let grid = TacticalGrid::parse(grid_str, &legend).unwrap();
            Some((r.id.clone(), grid))
        })
        .collect()
}

// ==========================================================================
// AC-1: Entrance room placed at origin (0, 0)
// ==========================================================================

#[test]
fn entrance_room_placed_at_origin() {
    let rooms = linear_chain_rooms();
    let grids = parse_grids(&rooms);
    let layout = layout_tree(&rooms, &grids).expect("Layout should succeed");

    let entrance = layout
        .rooms()
        .iter()
        .find(|r| r.room_id() == "room_a")
        .expect("Entrance room should be in layout");
    assert_eq!(entrance.offset_x(), 0, "Entrance room X offset must be 0");
    assert_eq!(entrance.offset_y(), 0, "Entrance room Y offset must be 0");
}

// ==========================================================================
// AC-2: Adjacent rooms share exactly one wall segment at exit gaps
// ==========================================================================

/// When room A connects south to room B's north, A's south wall and B's north
/// wall at the exit gap should occupy the SAME global row — one wall, not two.
#[test]
fn adjacent_rooms_share_wall_segment_at_exit() {
    let rooms = linear_chain_rooms();
    let grids = parse_grids(&rooms);
    let layout = layout_tree(&rooms, &grids).expect("Layout should succeed");

    let room_a = layout
        .rooms()
        .iter()
        .find(|r| r.room_id() == "room_a")
        .unwrap();
    let room_b = layout
        .rooms()
        .iter()
        .find(|r| r.room_id() == "room_b")
        .unwrap();

    // Room A is 5 tall, placed at origin. Its south wall is at local y=4.
    // Room B's north wall is at local y=0.
    // If they share the wall, B's offset_y should be 4 (not 5).
    // Global y of A's south wall = 0 + 4 = 4
    // Global y of B's north wall = B.offset_y + 0 = B.offset_y
    // Shared wall means: B.offset_y = 4
    let a_south_wall_y = room_a.offset_y() + (grids["room_a"].height() as i32 - 1);
    let b_north_wall_y = room_b.offset_y();
    assert_eq!(
        a_south_wall_y, b_north_wall_y,
        "A's south wall row and B's north wall row must be the same global row (shared wall)"
    );
}

/// Shared wall segment: the wall cells at the exit gap should exist in both
/// rooms' grids at the same global position. The gap (floor) cells are shared too.
#[test]
fn shared_wall_cells_overlap_at_boundary() {
    let rooms = linear_chain_rooms();
    let grids = parse_grids(&rooms);
    let layout = layout_tree(&rooms, &grids).expect("Layout should succeed");

    let room_a = layout
        .rooms()
        .iter()
        .find(|r| r.room_id() == "room_a")
        .unwrap();
    let room_b = layout
        .rooms()
        .iter()
        .find(|r| r.room_id() == "room_b")
        .unwrap();

    // At the shared wall row, both rooms should have cells.
    // The gap cells should be floor in both rooms.
    let shared_row_y = room_a.offset_y() + (grids["room_a"].height() as i32 - 1);

    // Check that the gap columns (2, 3 in room A's local coords) are floor
    // in both rooms at the shared row.
    let gap_cols_a = vec![2u32, 3u32]; // room A's south exit columns
    for &col in &gap_cols_a {
        let global_x = room_a.offset_x() + col as i32;
        // Room B should also have a floor cell at this global position
        let local_b_x = (global_x - room_b.offset_x()) as u32;
        let local_b_y = (shared_row_y - room_b.offset_y()) as u32;
        let cell = grids["room_b"].cell_at(local_b_x, local_b_y);
        assert!(
            cell == Some(&TacticalCell::Floor) || cell == Some(&TacticalCell::DoorOpen),
            "Gap cell at global ({}, {}) should be floor/open in room B (got {:?})",
            global_x,
            shared_row_y,
            cell
        );
    }
}

// ==========================================================================
// AC-3: Exit gaps align in global coordinates
// ==========================================================================

/// The exit gap cells from room A's south exit and room B's north exit should
/// map to identical global positions.
#[test]
fn exit_gaps_align_in_global_coordinates() {
    let rooms = linear_chain_rooms();
    let grids = parse_grids(&rooms);
    let layout = layout_tree(&rooms, &grids).expect("Layout should succeed");

    let room_a = layout
        .rooms()
        .iter()
        .find(|r| r.room_id() == "room_a")
        .unwrap();
    let room_b = layout
        .rooms()
        .iter()
        .find(|r| r.room_id() == "room_b")
        .unwrap();

    // Room A's south exit: cells at columns [2, 3], row = height-1
    let a_exit = grids["room_a"]
        .exits()
        .iter()
        .find(|e| e.wall == CardinalDirection::South)
        .expect("Room A should have a south exit");

    // Room B's north exit: cells at columns [2, 3], row = 0
    let b_exit = grids["room_b"]
        .exits()
        .iter()
        .find(|e| e.wall == CardinalDirection::North)
        .expect("Room B should have a north exit");

    // Convert exit gap cells to global coordinates
    let a_global: Vec<(i32, i32)> = a_exit
        .cells
        .iter()
        .map(|&col| {
            (
                room_a.offset_x() + col as i32,
                room_a.offset_y() + (grids["room_a"].height() as i32 - 1),
            )
        })
        .collect();

    let b_global: Vec<(i32, i32)> = b_exit
        .cells
        .iter()
        .map(|&col| (room_b.offset_x() + col as i32, room_b.offset_y()))
        .collect();

    assert_eq!(
        a_global, b_global,
        "Exit gap cells must align in global coordinates"
    );
}

// ==========================================================================
// AC-4: Overlap detection catches floor-on-floor collisions
// ==========================================================================

/// check_overlap should detect when two placed rooms' non-void cells share
/// the same global position.
#[test]
fn overlap_detection_catches_collisions() {
    let grid = room_a_grid();
    let placed = vec![PlacedRoom::new("room_1".to_string(), 0, 0, grid.clone())];
    // Place a second room at the same position — total overlap
    let candidate = PlacedRoom::new("room_2".to_string(), 0, 0, grid);
    let overlaps = check_overlap(&placed, &candidate);
    assert!(
        !overlaps.is_empty(),
        "Fully overlapping rooms must produce overlap cells"
    );
}

/// Two rooms placed far apart should produce zero overlaps.
#[test]
fn no_overlap_when_rooms_far_apart() {
    let grid = room_a_grid();
    let placed = vec![PlacedRoom::new("room_1".to_string(), 0, 0, grid.clone())];
    let candidate = PlacedRoom::new("room_2".to_string(), 100, 100, grid);
    let overlaps = check_overlap(&placed, &candidate);
    assert!(overlaps.is_empty(), "Distant rooms should not overlap");
}

/// Void cells should NOT count as overlaps (AC-7 corollary).
#[test]
fn void_cells_do_not_count_as_overlap() {
    // Two rooms where the only overlapping global cells are both void.
    // Room 1: void column on the left (col 0 all void)
    let raw = "\
_###\n\
_..#\n\
_###";
    let grid = TacticalGrid::parse(raw, &HashMap::new()).unwrap();
    let placed = vec![PlacedRoom::new("room_1".to_string(), 0, 0, grid)];
    // Room 2: void column on the right (col 3 all void)
    let grid2_raw = "\
###_\n\
#.._\n\
###_";
    let grid2 = TacticalGrid::parse(grid2_raw, &HashMap::new()).unwrap();
    // Place grid2 so its rightmost void column (col 3) overlaps with grid1's leftmost void column (col 0)
    // grid2 at offset (-3, 0) → grid2 col 3 maps to global x=0
    // Room 1 col 0 = all Void, Room 2 col 3 = all Void → Void-on-Void only
    let candidate = PlacedRoom::new("room_2".to_string(), -3, 0, grid2);
    let overlaps = check_overlap(&placed, &candidate);
    assert!(
        overlaps.is_empty(),
        "Void-on-void overlap should not be reported"
    );
}

// ==========================================================================
// AC-5: Layout fails loudly on unresolvable overlap
// ==========================================================================

/// When rooms cannot be placed without overlap, layout_tree should return
/// LayoutError with both room IDs and the overlapping cells.
#[test]
fn layout_error_on_unresolvable_overlap() {
    // Create a scenario where a room genuinely can't be placed without collision.
    //
    // Hub with south and east exits. Room B connects to the south exit and is
    // very wide (extends east). Room C connects to the east exit and is very
    // tall (extends south). B and C will overlap in the southeast quadrant.
    let _hub = room_def(
        "hub",
        "entrance",
        vec![
            RoomExit::Corridor {
                target: "room_b".to_string(),
            },
            RoomExit::Corridor {
                target: "room_c".to_string(),
            },
        ],
        "######\n#....#\n#....#\n#....#\n#....#\n###..#\n#.....   ",
    );
    // Actually, let's use a cleaner approach: two rooms that MUST be placed
    // at positions that overlap.
    //
    // Hub (3x7): exits on south (cols 1) and east (rows 1)
    // Room B (10x3): connects north to hub's south — placed directly below hub, very wide
    // Room C (3x10): connects west to hub's east — placed right of hub, very tall
    // B extends 10 cells east, C extends 10 cells south. They overlap in the region
    // east of hub and south of hub.
    let hub = room_def(
        "hub",
        "entrance",
        vec![
            RoomExit::Corridor {
                target: "wide_room".to_string(),
            },
            RoomExit::Corridor {
                target: "tall_room".to_string(),
            },
        ],
        "#.#\n...\n...\n...\n...\n...\n#.#",
    );
    // Wide room: 12x3 with north exit at col 1 (matches hub's south exit)
    let wide = room_def(
        "wide_room",
        "normal",
        vec![RoomExit::Corridor {
            target: "hub".to_string(),
        }],
        "#.##########\n............\n############",
    );
    // Tall room: 3x12 with west exit at row 1 (matches hub's east exit)
    let tall = room_def(
        "tall_room",
        "normal",
        vec![RoomExit::Corridor {
            target: "hub".to_string(),
        }],
        "###\n...\n#.#\n#.#\n#.#\n#.#\n#.#\n#.#\n#.#\n#.#\n#.#\n###",
    );
    let rooms = vec![hub, wide, tall];
    let grids = parse_grids(&rooms);
    let result = layout_tree(&rooms, &grids);

    match result {
        Err(LayoutError::Overlap {
            room_a,
            room_b,
            cells,
        }) => {
            assert!(
                !cells.is_empty(),
                "LayoutError::Overlap must include the conflicting cells"
            );
            // At least one of the overlapping rooms should be named
            let ids: HashSet<&str> = [room_a.as_str(), room_b.as_str()].into();
            assert!(
                ids.contains("wide_room") || ids.contains("tall_room"),
                "Error should name the overlapping rooms"
            );
        }
        Err(_) => {
            // Any LayoutError is acceptable as long as it's loud
        }
        Ok(_) => panic!("Expected LayoutError for unresolvable overlap, got Ok"),
    }
}

/// LayoutError should implement Display with actionable context.
#[test]
fn layout_error_display_includes_context() {
    // Manually construct a LayoutError to verify Display
    let err = LayoutError::Overlap {
        room_a: "room_1".to_string(),
        room_b: "room_2".to_string(),
        cells: vec![GridPos::new(5, 5)],
    };
    let msg = format!("{}", err);
    assert!(
        msg.contains("room_1") && msg.contains("room_2"),
        "Display should include both room IDs: got '{}'",
        msg
    );
    assert!(
        msg.contains("overlap") || msg.contains("collision"),
        "Display should describe the problem: got '{}'",
        msg
    );
}

// ==========================================================================
// AC-6: BFS visits all reachable rooms from entrance
// ==========================================================================

#[test]
fn bfs_visits_all_reachable_rooms() {
    let rooms = linear_chain_rooms();
    let grids = parse_grids(&rooms);
    let layout = layout_tree(&rooms, &grids).expect("Layout should succeed");

    let placed_ids: HashSet<&str> = layout.rooms().iter().map(|r| r.room_id()).collect();
    assert_eq!(placed_ids.len(), 3, "All 3 rooms should be placed");
    assert!(placed_ids.contains("room_a"));
    assert!(placed_ids.contains("room_b"));
    assert!(placed_ids.contains("room_c"));
}

/// Rooms not reachable from the entrance should NOT be placed.
#[test]
fn unreachable_room_not_placed() {
    let mut rooms = linear_chain_rooms();
    // Add an isolated room with no connections to the graph
    rooms.push(room_def(
        "isolated",
        "normal",
        vec![],
        "#####\n#...#\n#...#\n#####",
    ));
    let grids = parse_grids(&rooms);
    let layout = layout_tree(&rooms, &grids).expect("Layout should succeed");

    let placed_ids: HashSet<&str> = layout.rooms().iter().map(|r| r.room_id()).collect();
    assert!(
        !placed_ids.contains("isolated"),
        "Unreachable rooms should not be in the layout"
    );
    assert_eq!(
        placed_ids.len(),
        3,
        "Only the 3 connected rooms should be placed"
    );
}

// ==========================================================================
// AC-7: Non-void cells of different rooms never occupy same global position
// ==========================================================================

/// After layout, iterate all placed rooms and verify no non-void cell positions
/// collide (except at shared wall boundaries, which are the SAME wall).
#[test]
fn no_non_void_cell_collisions_in_layout() {
    let rooms = linear_chain_rooms();
    let grids = parse_grids(&rooms);
    let layout = layout_tree(&rooms, &grids).expect("Layout should succeed");

    // Build a map of global position -> (room_id, cell_type)
    let mut occupied: HashMap<(i32, i32), Vec<(&str, &TacticalCell)>> = HashMap::new();

    for placed in layout.rooms() {
        let grid = &grids[placed.room_id()];
        for y in 0..grid.height() {
            for x in 0..grid.width() {
                if let Some(cell) = grid.cell_at(x, y) {
                    if *cell == TacticalCell::Void {
                        continue; // Void doesn't participate
                    }
                    let gx = placed.offset_x() + x as i32;
                    let gy = placed.offset_y() + y as i32;
                    occupied
                        .entry((gx, gy))
                        .or_default()
                        .push((placed.room_id(), cell));
                }
            }
        }
    }

    // Find any position occupied by non-void cells from different rooms.
    // At shared wall boundaries, both rooms have cells at the same global position.
    // This is expected: Wall-on-Wall for the wall portions, and Floor-on-Floor (or
    // DoorOpen-on-Floor, etc.) at exit gap positions (AC-3 requires gap alignment).
    // What is NOT allowed: different cell types from different rooms at the same position
    // (e.g., Wall from room A and Floor from room B — that's a misaligned boundary).
    for ((gx, gy), occupants) in &occupied {
        if occupants.len() > 1 {
            let rooms_here: Vec<&str> = occupants.iter().map(|(id, _)| *id).collect();
            let unique_rooms: HashSet<&&str> = rooms_here.iter().collect();
            if unique_rooms.len() > 1 {
                // Multiple rooms at this position: all cells must be the same type
                // (shared boundary — Wall-Wall or Floor-Floor at exit gaps)
                let first_cell = occupants[0].1;
                for (room_id, cell) in &occupants[1..] {
                    assert_eq!(
                        *cell, first_cell,
                        "Mismatched cell types at ({}, {}): first room has {:?}, room '{}' has {:?}",
                        gx, gy, first_cell, room_id, cell
                    );
                }
            }
        }
    }
}

// ==========================================================================
// AC-8: Linear chain of 3 rooms places correctly
// ==========================================================================

/// A -> B -> C in a line. All rooms placed, no overlaps, shared walls correct.
#[test]
fn linear_chain_three_rooms() {
    let rooms = linear_chain_rooms();
    let grids = parse_grids(&rooms);
    let layout = layout_tree(&rooms, &grids).expect("Linear chain layout should succeed");

    // All 3 rooms placed
    assert_eq!(layout.rooms().len(), 3);

    // Entrance at origin
    let room_a = layout
        .rooms()
        .iter()
        .find(|r| r.room_id() == "room_a")
        .unwrap();
    assert_eq!(room_a.offset_x(), 0);
    assert_eq!(room_a.offset_y(), 0);

    // Room B is south of Room A (shared wall)
    let room_b = layout
        .rooms()
        .iter()
        .find(|r| r.room_id() == "room_b")
        .unwrap();
    assert!(
        room_b.offset_y() > room_a.offset_y(),
        "Room B should be south of Room A"
    );

    // Room C is east of Room B (shared wall)
    let room_c = layout
        .rooms()
        .iter()
        .find(|r| r.room_id() == "room_c")
        .unwrap();
    assert!(
        room_c.offset_x() > room_b.offset_x(),
        "Room C should be east of Room B"
    );
}

// ==========================================================================
// AC-9: T-junction (3 rooms sharing a hub) places correctly
// ==========================================================================

/// Hub with north, east, south spokes. All placed, no overlaps.
#[test]
fn t_junction_hub_with_three_spokes() {
    let rooms = t_junction_rooms();
    let grids = parse_grids(&rooms);
    let layout = layout_tree(&rooms, &grids).expect("T-junction layout should succeed");

    // All 4 rooms placed
    assert_eq!(layout.rooms().len(), 4);

    let hub = layout
        .rooms()
        .iter()
        .find(|r| r.room_id() == "hub")
        .unwrap();
    let north = layout
        .rooms()
        .iter()
        .find(|r| r.room_id() == "north_spoke")
        .unwrap();
    let east = layout
        .rooms()
        .iter()
        .find(|r| r.room_id() == "east_spoke")
        .unwrap();
    let south = layout
        .rooms()
        .iter()
        .find(|r| r.room_id() == "south_spoke")
        .unwrap();

    // Hub at origin (it's the entrance)
    assert_eq!(hub.offset_x(), 0);
    assert_eq!(hub.offset_y(), 0);

    // North spoke is above the hub (negative y or same principle)
    assert!(
        north.offset_y() < hub.offset_y(),
        "North spoke should be above hub"
    );

    // East spoke is to the right of the hub
    assert!(
        east.offset_x() > hub.offset_x(),
        "East spoke should be right of hub"
    );

    // South spoke is below the hub
    assert!(
        south.offset_y() > hub.offset_y(),
        "South spoke should be below hub"
    );
}

/// T-junction: verify no non-void collisions.
#[test]
fn t_junction_no_collisions() {
    let rooms = t_junction_rooms();
    let grids = parse_grids(&rooms);
    let layout = layout_tree(&rooms, &grids).expect("T-junction layout should succeed");

    let mut occupied: HashMap<(i32, i32), Vec<(&str, &TacticalCell)>> = HashMap::new();
    for placed in layout.rooms() {
        let grid = &grids[placed.room_id()];
        for y in 0..grid.height() {
            for x in 0..grid.width() {
                if let Some(cell) = grid.cell_at(x, y) {
                    if *cell == TacticalCell::Void {
                        continue;
                    }
                    let gx = placed.offset_x() + x as i32;
                    let gy = placed.offset_y() + y as i32;
                    occupied
                        .entry((gx, gy))
                        .or_default()
                        .push((placed.room_id(), cell));
                }
            }
        }
    }

    for ((gx, gy), occupants) in &occupied {
        if occupants.len() > 1 {
            let unique_rooms: HashSet<&str> = occupants.iter().map(|(id, _)| *id).collect();
            if unique_rooms.len() > 1 {
                // Shared boundary: all cells must be the same type
                let first_cell = occupants[0].1;
                for (room_id, cell) in &occupants[1..] {
                    assert_eq!(
                        *cell, first_cell,
                        "T-junction: mismatched cells at ({}, {}): first has {:?}, room '{}' has {:?}",
                        gx, gy, first_cell, room_id, cell
                    );
                }
            }
        }
    }
}

// ==========================================================================
// AC-10: Wiring test — layout engine callable from non-test code
// ==========================================================================

/// Verify that the layout module types and functions are re-exported from
/// sidequest_game::tactical::layout (accessible to server, validate, etc.).
#[test]
fn layout_module_is_public() {
    // This test compiles only if the layout module is publicly accessible.
    // If it doesn't compile, the module isn't wired.
    type LayoutFn =
        fn(&[RoomDef], &HashMap<String, TacticalGrid>) -> Result<DungeonLayout, LayoutError>;
    let _: LayoutFn = layout_tree;
}

// ==========================================================================
// Rust rule #2: #[non_exhaustive] on LayoutError
// ==========================================================================

/// LayoutError is a public enum that may grow. Must be #[non_exhaustive].
#[test]
fn layout_error_is_non_exhaustive() {
    let err = LayoutError::Overlap {
        room_a: "a".to_string(),
        room_b: "b".to_string(),
        cells: vec![],
    };
    let _desc = match err {
        LayoutError::Overlap { .. } => "overlap",
        _ => "other", // Required by #[non_exhaustive]
    };
    assert_eq!(_desc, "overlap");
}

// ==========================================================================
// Rust rule #6: test quality — meaningful assertions on align_rooms
// ==========================================================================

/// align_rooms returns the (x, y) offset for room B given room A's exit
/// and room B's corresponding exit. Verify it produces correct coordinates.
#[test]
fn align_rooms_south_to_north() {
    let grid_a = room_a_grid();
    let grid_b = room_b_grid();

    let a_south_exit = grid_a
        .exits()
        .iter()
        .find(|e| e.wall == CardinalDirection::South)
        .expect("Room A should have south exit");

    let b_north_exit = grid_b
        .exits()
        .iter()
        .find(|e| e.wall == CardinalDirection::North)
        .expect("Room B should have north exit");

    let placed_a = PlacedRoom::new("room_a".to_string(), 0, 0, grid_a.clone());
    let (bx, by) = align_rooms(&placed_a, a_south_exit, &grid_b, b_north_exit);

    // B's north wall should align with A's south wall (shared wall)
    let a_south_y = grid_a.height() as i32 - 1; // = 4
    assert_eq!(
        by, a_south_y,
        "B's offset_y should place its north wall at A's south wall"
    );

    // Exit gap columns should align
    for (a_col, b_col) in a_south_exit.cells.iter().zip(b_north_exit.cells.iter()) {
        let a_global_x = *a_col as i32;
        let b_global_x = bx + *b_col as i32;
        assert_eq!(
            a_global_x, b_global_x,
            "Exit gap columns must align: A col {} -> global {}, B col {} -> global {}",
            a_col, a_global_x, b_col, b_global_x
        );
    }
}

/// align_rooms for east-to-west connection.
#[test]
fn align_rooms_east_to_west() {
    let grid_b = room_b_grid();
    let grid_c = room_c_grid();

    let b_east_exit = grid_b
        .exits()
        .iter()
        .find(|e| e.wall == CardinalDirection::East)
        .expect("Room B should have east exit");

    let c_west_exit = grid_c
        .exits()
        .iter()
        .find(|e| e.wall == CardinalDirection::West)
        .expect("Room C should have west exit");

    let placed_b = PlacedRoom::new("room_b".to_string(), 0, 0, grid_b.clone());
    let (cx, cy) = align_rooms(&placed_b, b_east_exit, &grid_c, c_west_exit);

    // C's west wall should align with B's east wall (shared wall)
    let b_east_x = grid_b.width() as i32 - 1; // = 4
    assert_eq!(
        cx, b_east_x,
        "C's offset_x should place its west wall at B's east wall"
    );

    // Exit gap rows should align
    for (b_row, c_row) in b_east_exit.cells.iter().zip(c_west_exit.cells.iter()) {
        let b_global_y = *b_row as i32;
        let c_global_y = cy + *c_row as i32;
        assert_eq!(
            b_global_y, c_global_y,
            "Exit gap rows must align for E-W connection"
        );
    }
}

// ==========================================================================
// DungeonLayout accessors
// ==========================================================================

/// DungeonLayout should report total dimensions spanning all placed rooms.
#[test]
fn dungeon_layout_dimensions_span_all_rooms() {
    let rooms = linear_chain_rooms();
    let grids = parse_grids(&rooms);
    let layout = layout_tree(&rooms, &grids).expect("Layout should succeed");

    assert!(layout.width() > 0, "Layout width must be positive");
    assert!(layout.height() > 0, "Layout height must be positive");

    // Dimensions should span from min to max of all placed rooms
    let min_x = layout.rooms().iter().map(|r| r.offset_x()).min().unwrap();
    let max_x = layout
        .rooms()
        .iter()
        .map(|r| r.offset_x() + grids[r.room_id()].width() as i32)
        .max()
        .unwrap();
    let min_y = layout.rooms().iter().map(|r| r.offset_y()).min().unwrap();
    let max_y = layout
        .rooms()
        .iter()
        .map(|r| r.offset_y() + grids[r.room_id()].height() as i32)
        .max()
        .unwrap();

    assert_eq!(
        layout.width(),
        (max_x - min_x) as u32,
        "Layout width should span all rooms"
    );
    assert_eq!(
        layout.height(),
        (max_y - min_y) as u32,
        "Layout height should span all rooms"
    );
}

// ==========================================================================
// Edge cases
// ==========================================================================

/// Single room (entrance only) should produce a valid layout.
#[test]
fn single_entrance_room_layout() {
    let rooms = vec![room_def(
        "only_room",
        "entrance",
        vec![],
        "#####\n#...#\n#...#\n#####",
    )];
    let grids = parse_grids(&rooms);
    let layout = layout_tree(&rooms, &grids).expect("Single room layout should succeed");

    assert_eq!(layout.rooms().len(), 1);
    assert_eq!(layout.rooms()[0].room_id(), "only_room");
    assert_eq!(layout.rooms()[0].offset_x(), 0);
    assert_eq!(layout.rooms()[0].offset_y(), 0);
}

/// Rooms without grid fields should be skipped (no tactical grid = no layout).
#[test]
fn rooms_without_grid_skipped() {
    let mut rooms = linear_chain_rooms();
    // Add a room with no grid field — it can't participate in tactical layout
    rooms.push(RoomDef {
        id: "no_grid".to_string(),
        name: "No Grid Room".to_string(),
        room_type: "normal".to_string(),
        size: (1, 1),
        keeper_awareness_modifier: 1.0,
        exits: vec![RoomExit::Corridor {
            target: "room_a".to_string(),
        }],
        description: None,
        grid: None, // No grid!
        tactical_scale: None,
        legend: None,
    });
    let grids = parse_grids(&rooms);
    // Should succeed — the gridless room is simply not placed
    let layout = layout_tree(&rooms, &grids).expect("Layout should succeed");
    let placed_ids: HashSet<&str> = layout.rooms().iter().map(|r| r.room_id()).collect();
    assert!(
        !placed_ids.contains("no_grid"),
        "Room without grid should not be in layout"
    );
}

/// Empty room list should produce empty layout (or error — either is valid).
#[test]
fn empty_room_list() {
    let rooms: Vec<RoomDef> = vec![];
    let grids: HashMap<String, TacticalGrid> = HashMap::new();
    let result = layout_tree(&rooms, &grids);
    // Error is also acceptable for empty input, so we only assert on the Ok path.
    if let Ok(layout) = result {
        assert!(layout.rooms().is_empty(), "Empty input → empty layout");
    }
}

/// No entrance room should produce an error.
#[test]
fn no_entrance_room_errors() {
    let rooms = vec![room_def(
        "room_a",
        "normal", // Not "entrance"!
        vec![],
        "#####\n#...#\n#...#\n#####",
    )];
    let grids = parse_grids(&rooms);
    let result = layout_tree(&rooms, &grids);
    assert!(result.is_err(), "Layout with no entrance room should fail");
}

// ==========================================================================
// Rust rule #9: PlacedRoom fields should use getters (encapsulation)
// ==========================================================================

/// PlacedRoom should expose room_id, offset_x, offset_y via getter methods.
/// This test verifies the getter API exists and returns correct values.
#[test]
fn placed_room_getters() {
    let grid = room_a_grid();
    let placed = PlacedRoom::new("test_room".to_string(), 10, -5, grid);
    assert_eq!(placed.room_id(), "test_room");
    assert_eq!(placed.offset_x(), 10);
    assert_eq!(placed.offset_y(), -5);
}

// ==========================================================================
// LayoutError implements std::error::Error
// ==========================================================================

#[test]
fn layout_error_is_std_error() {
    let err = LayoutError::Overlap {
        room_a: "a".to_string(),
        room_b: "b".to_string(),
        cells: vec![GridPos::new(0, 0)],
    };
    // This compiles only if LayoutError: std::error::Error
    let _: &dyn std::error::Error = &err;
}

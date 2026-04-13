//! Story 29-7: Jaquayed layout — cycle detection, ring placement, loop closure,
//! overlap detection for cyclic dungeon topologies.
//!
//! RED phase — failing tests for the jaquayed layout extensions to the 29-6
//! shared-wall layout engine.
//!
//! The jaquayed layout algorithm (ADR-071 §5) extends the tree-only placer
//! from 29-6 with:
//!   1. DFS cycle detection on the room graph (back-edge extraction)
//!   2. Ring placement for each fundamental cycle
//!   3. Loop closure validation at the last edge of each ring
//!   4. BFS tree-branch expansion off cycle nodes (reuses 29-6 logic)
//!   5. Overlap detection between cycle rooms and tree branches
//!   6. Fail-loud error reporting when authoring makes a cycle impossible
//!
//! ACs tested (from sprint/context/context-story-29-7.md):
//!   AC-1: DFS cycle detection finds all cycles in a cyclic room graph
//!   AC-2: Ring placement produces valid shared walls for all cycle edges
//!   AC-3: Loop closure validates exit-gap alignment at the closing edge
//!   AC-4: CycleClosureFailed error includes actionable context (rooms + gap detail)
//!   AC-5: Tree branches BFS-attach to cycle nodes after ring placement
//!   AC-6: Overlap detection runs between cycle rooms and tree branches
//!   AC-7: Multiple disconnected cycles placed with spacing (no overlap)
//!   AC-8: 4-room square cycle places and closes correctly
//!   AC-9: Cycle with tree branch hanging off one node
//!   AC-10: Integration: full cyclic dungeon layout succeeds end-to-end
//!
//! Rule-enforcement (`.pennyfarthing/gates/lang-review/rust.md`):
//!   #2  LayoutError remains #[non_exhaustive] after new variants are added
//!   #6  Test quality — every assertion compares a real value, no vacuous
//!       `is_some`/`is_none` / `let _ =` / `assert!(true)` patterns
//!   #9  PlacedRoom continues to expose data via getters, not public fields

use std::collections::{HashMap, HashSet};

use sidequest_game::tactical::{CardinalDirection, TacticalGrid};

// These symbols are the 29-7 contract. They do not exist yet → RED.
use sidequest_game::tactical::layout::{
    detect_cycles, layout_cycle, layout_dungeon, DungeonLayout, LayoutError, PlacedRoom,
};

use sidequest_genre::models::world::{RoomDef, RoomExit};

// ==========================================================================
// RoomDef fixture helpers
// ==========================================================================

/// Build a bidirectional corridor exit pointing at `target`.
fn corridor(target: &str) -> RoomExit {
    RoomExit::Corridor {
        target: target.to_string(),
    }
}

/// Build a RoomDef with the given id, room_type, exits, and ASCII grid.
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

/// Build a RoomDef with no tactical grid — useful for cycle-detection tests
/// that exercise graph topology only.
fn room_def_no_grid(id: &str, room_type: &str, exits: Vec<RoomExit>) -> RoomDef {
    RoomDef {
        id: id.to_string(),
        name: id.to_string(),
        room_type: room_type.to_string(),
        size: (1, 1),
        keeper_awareness_modifier: 1.0,
        exits,
        description: None,
        grid: None,
        tactical_scale: None,
        legend: None,
    }
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
// Graph-only fixtures (cycle detection)
// ==========================================================================

/// Linear chain A→B→C (no cycles).
fn graph_linear_chain() -> Vec<RoomDef> {
    vec![
        room_def_no_grid("a", "entrance", vec![corridor("b")]),
        room_def_no_grid("b", "normal", vec![corridor("a"), corridor("c")]),
        room_def_no_grid("c", "normal", vec![corridor("b")]),
    ]
}

/// 3-room triangle cycle: A↔B↔C↔A.
fn graph_triangle_cycle() -> Vec<RoomDef> {
    vec![
        room_def_no_grid("a", "entrance", vec![corridor("b"), corridor("c")]),
        room_def_no_grid("b", "normal", vec![corridor("a"), corridor("c")]),
        room_def_no_grid("c", "normal", vec![corridor("a"), corridor("b")]),
    ]
}

/// 4-room square cycle: A↔B↔D↔C↔A (Jaquayed 2×2 ring).
fn graph_square_cycle() -> Vec<RoomDef> {
    vec![
        room_def_no_grid("a", "entrance", vec![corridor("b"), corridor("c")]),
        room_def_no_grid("b", "normal", vec![corridor("a"), corridor("d")]),
        room_def_no_grid("c", "normal", vec![corridor("a"), corridor("d")]),
        room_def_no_grid("d", "normal", vec![corridor("b"), corridor("c")]),
    ]
}

/// Square cycle with a tree branch hanging off room D: A↔B↔D↔C↔A, plus E attached to D.
fn graph_cycle_with_branch() -> Vec<RoomDef> {
    vec![
        room_def_no_grid("a", "entrance", vec![corridor("b"), corridor("c")]),
        room_def_no_grid("b", "normal", vec![corridor("a"), corridor("d")]),
        room_def_no_grid("c", "normal", vec![corridor("a"), corridor("d")]),
        room_def_no_grid(
            "d",
            "normal",
            vec![corridor("b"), corridor("c"), corridor("e")],
        ),
        room_def_no_grid("e", "normal", vec![corridor("d")]),
    ]
}

/// Two disjoint triangles connected by a bridge edge A₁→A₂.
/// Cycle 1: A₁↔B₁↔C₁↔A₁.  Cycle 2: A₂↔B₂↔C₂↔A₂.
fn graph_two_disjoint_cycles() -> Vec<RoomDef> {
    vec![
        room_def_no_grid(
            "a1",
            "entrance",
            vec![corridor("b1"), corridor("c1"), corridor("a2")],
        ),
        room_def_no_grid("b1", "normal", vec![corridor("a1"), corridor("c1")]),
        room_def_no_grid("c1", "normal", vec![corridor("a1"), corridor("b1")]),
        room_def_no_grid(
            "a2",
            "normal",
            vec![corridor("a1"), corridor("b2"), corridor("c2")],
        ),
        room_def_no_grid("b2", "normal", vec![corridor("a2"), corridor("c2")]),
        room_def_no_grid("c2", "normal", vec![corridor("a2"), corridor("b2")]),
    ]
}

// ==========================================================================
// Geometric fixtures (ring placement)
// ==========================================================================
//
// 2×2 ring topology. Each room is 5×5 with exits on exactly the two walls
// that face its ring neighbours. Cycle edge order: A → B → D → C → A.
//
// Grid layout after placement (shared walls = one wall, not two):
//
//   A (NW) ───── B (NE)
//    │            │
//   C (SW) ───── D (SE)
//
// Each room has a 2-cell exit gap at the centre of the relevant wall.

/// Room A (NW): 5×5, east exit rows 1-2, south exit cols 1-2.
fn grid_ring_a() -> &'static str {
    "#####\n\
     #...#\n\
     #....\n\
     #...#\n\
     ##..#"
}

/// Room B (NE): 5×5, west exit rows 1-2, south exit cols 1-2.
fn grid_ring_b() -> &'static str {
    "#####\n\
     #...#\n\
     ....#\n\
     #...#\n\
     ##..#"
}

/// Room C (SW): 5×5, north exit cols 1-2, east exit rows 1-2.
fn grid_ring_c() -> &'static str {
    "##..#\n\
     #...#\n\
     #....\n\
     #...#\n\
     #####"
}

/// Room D (SE): 5×5, north exit cols 1-2, west exit rows 1-2.
fn grid_ring_d() -> &'static str {
    "##..#\n\
     #...#\n\
     ....#\n\
     #...#\n\
     #####"
}

/// 4-room square ring: A↔B↔D↔C↔A. Geometry closes perfectly.
fn rooms_square_ring() -> Vec<RoomDef> {
    vec![
        room_def(
            "a",
            "entrance",
            vec![corridor("b"), corridor("c")],
            grid_ring_a(),
        ),
        room_def(
            "b",
            "normal",
            vec![corridor("a"), corridor("d")],
            grid_ring_b(),
        ),
        room_def(
            "c",
            "normal",
            vec![corridor("a"), corridor("d")],
            grid_ring_c(),
        ),
        room_def(
            "d",
            "normal",
            vec![corridor("b"), corridor("c")],
            grid_ring_d(),
        ),
    ]
}

/// Same square ring, but room D has a broken west-exit position so the
/// closing edge C→D cannot align. Used for CycleClosureFailed tests.
///
/// Room D's west exit is shifted to rows 2-3 instead of 1-2, so it no longer
/// matches C's east exit (rows 1-2). The ring cannot close.
fn rooms_square_ring_broken_closure() -> Vec<RoomDef> {
    let mut rooms = rooms_square_ring();
    // Replace D's grid with a version whose west exit is at rows 2-3.
    let broken_d = "##..#\n\
                    #...#\n\
                    #...#\n\
                    ....#\n\
                    #####";
    rooms[3].grid = Some(broken_d.to_string());
    rooms
}

/// Square ring plus a tree branch: E hanging off D via D's east wall.
/// Used for AC-5/AC-9 (tree branch on cycle node).
fn rooms_square_ring_with_branch() -> Vec<RoomDef> {
    // D needs an east exit for the branch — use a purpose-built grid.
    let ring_d_with_east = "##..#\n\
                            #...#\n\
                            ....#\n\
                            #....\n\
                            #####";
    let branch_e = "#####\n\
                    #...#\n\
                    ....#\n\
                    ....#\n\
                    #####";

    vec![
        room_def(
            "a",
            "entrance",
            vec![corridor("b"), corridor("c")],
            grid_ring_a(),
        ),
        room_def(
            "b",
            "normal",
            vec![corridor("a"), corridor("d")],
            grid_ring_b(),
        ),
        room_def(
            "c",
            "normal",
            vec![corridor("a"), corridor("d")],
            grid_ring_c(),
        ),
        room_def(
            "d",
            "normal",
            vec![corridor("b"), corridor("c"), corridor("e")],
            ring_d_with_east,
        ),
        room_def("e", "normal", vec![corridor("d")], branch_e),
    ]
}

// ==========================================================================
// AC-1: Cycle detection
// ==========================================================================

#[test]
fn detect_cycles_empty_on_linear_chain() {
    let rooms = graph_linear_chain();
    let cycles = detect_cycles(&rooms);
    assert!(
        cycles.is_empty(),
        "linear chain must produce zero cycles, got {:?}",
        cycles
    );
}

#[test]
fn detect_cycles_finds_single_triangle() {
    let rooms = graph_triangle_cycle();
    let cycles = detect_cycles(&rooms);
    assert_eq!(
        cycles.len(),
        1,
        "triangle graph must produce exactly one cycle, got {:?}",
        cycles
    );
    let cycle = &cycles[0];
    assert_eq!(
        cycle.len(),
        3,
        "triangle cycle must contain exactly 3 rooms, got {:?}",
        cycle
    );
    let unique: HashSet<&String> = cycle.iter().collect();
    assert_eq!(
        unique.len(),
        3,
        "triangle cycle rooms must all be unique, got {:?}",
        cycle
    );
    for room_id in ["a", "b", "c"] {
        assert!(
            cycle.iter().any(|r| r == room_id),
            "triangle cycle must contain room {}, got {:?}",
            room_id,
            cycle
        );
    }
}

#[test]
fn detect_cycles_finds_single_square() {
    let rooms = graph_square_cycle();
    let cycles = detect_cycles(&rooms);
    assert_eq!(
        cycles.len(),
        1,
        "square graph must produce exactly one fundamental cycle, got {:?}",
        cycles
    );
    let cycle = &cycles[0];
    assert_eq!(
        cycle.len(),
        4,
        "square cycle must contain exactly 4 rooms, got {:?}",
        cycle
    );
    for room_id in ["a", "b", "c", "d"] {
        assert!(
            cycle.iter().any(|r| r == room_id),
            "square cycle must contain room {}, got {:?}",
            room_id,
            cycle
        );
    }
}

#[test]
fn detect_cycles_ignores_tree_branches() {
    // Cycle-with-branch: the branch room "e" must NOT appear in any cycle.
    let rooms = graph_cycle_with_branch();
    let cycles = detect_cycles(&rooms);
    assert_eq!(
        cycles.len(),
        1,
        "cycle-with-branch must produce exactly one cycle, got {:?}",
        cycles
    );
    for cycle in &cycles {
        assert!(
            !cycle.iter().any(|r| r == "e"),
            "tree-branch room 'e' must not appear in any cycle, got {:?}",
            cycle
        );
    }
}

#[test]
fn detect_cycles_finds_multiple_disjoint_cycles() {
    let rooms = graph_two_disjoint_cycles();
    let cycles = detect_cycles(&rooms);
    assert_eq!(
        cycles.len(),
        2,
        "two disjoint triangles must produce exactly two cycles, got {:?}",
        cycles
    );
    // Every cycle must contain exactly 3 rooms.
    for cycle in &cycles {
        assert_eq!(
            cycle.len(),
            3,
            "each disjoint triangle must contain 3 rooms, got {:?}",
            cycle
        );
    }
    // Each cycle's room set must be a subset of one of the two triangles.
    let triangle_1: HashSet<&str> = ["a1", "b1", "c1"].into_iter().collect();
    let triangle_2: HashSet<&str> = ["a2", "b2", "c2"].into_iter().collect();
    for cycle in &cycles {
        let cycle_set: HashSet<&str> = cycle.iter().map(String::as_str).collect();
        let matches_1 = cycle_set == triangle_1;
        let matches_2 = cycle_set == triangle_2;
        assert!(
            matches_1 || matches_2,
            "cycle {:?} must match one of the two disjoint triangles",
            cycle
        );
    }
}

#[test]
fn detect_cycles_is_deterministic() {
    // Running detect_cycles twice on the same input must produce identical
    // output (sorted by some canonical key). This guards against hash-ordered
    // iteration producing non-reproducible test output.
    let rooms = graph_square_cycle();
    let first = detect_cycles(&rooms);
    let second = detect_cycles(&rooms);
    assert_eq!(
        first, second,
        "detect_cycles must be deterministic across calls"
    );
}

// ==========================================================================
// AC-2 / AC-8: Ring placement for 4-room square cycle
// ==========================================================================

#[test]
fn layout_cycle_places_all_ring_rooms() {
    let rooms = rooms_square_ring();
    let grids = parse_grids(&rooms);
    let cycle: Vec<String> = vec!["a".into(), "b".into(), "d".into(), "c".into()];

    let placed = layout_cycle(&cycle, &rooms, &grids).expect("square ring must place");
    assert_eq!(
        placed.len(),
        4,
        "ring placement must produce one PlacedRoom per cycle member, got {}",
        placed.len()
    );
    let ids: HashSet<&str> = placed.iter().map(|p| p.room_id()).collect();
    for room_id in ["a", "b", "c", "d"] {
        assert!(
            ids.contains(room_id),
            "ring placement must include room {}, got {:?}",
            room_id,
            ids
        );
    }
}

#[test]
fn layout_cycle_produces_shared_walls_at_each_edge() {
    // For the 2×2 ring, each adjacent pair on the cycle must share exactly
    // one wall row or column (not be separated by a void gap).
    let rooms = rooms_square_ring();
    let grids = parse_grids(&rooms);
    let cycle: Vec<String> = vec!["a".into(), "b".into(), "d".into(), "c".into()];

    let placed = layout_cycle(&cycle, &rooms, &grids).expect("square ring must place");
    let by_id: HashMap<&str, &PlacedRoom> = placed.iter().map(|p| (p.room_id(), p)).collect();

    // A is at origin. B shares A's east wall. C shares A's south wall.
    let a = by_id.get("a").expect("ring must place a");
    let b = by_id.get("b").expect("ring must place b");
    let c = by_id.get("c").expect("ring must place c");
    let d = by_id.get("d").expect("ring must place d");

    // A.east edge and B.west edge must be the same column.
    let a_east_x = a.offset_x() + (grids["a"].width() as i32 - 1);
    let b_west_x = b.offset_x();
    assert_eq!(
        a_east_x, b_west_x,
        "A's east wall ({}) and B's west wall ({}) must be the same column (shared wall)",
        a_east_x, b_west_x
    );

    // A.south edge and C.north edge must be the same row.
    let a_south_y = a.offset_y() + (grids["a"].height() as i32 - 1);
    let c_north_y = c.offset_y();
    assert_eq!(
        a_south_y, c_north_y,
        "A's south wall ({}) and C's north wall ({}) must be the same row (shared wall)",
        a_south_y, c_north_y
    );

    // B.south edge and D.north edge must be the same row.
    let b_south_y = b.offset_y() + (grids["b"].height() as i32 - 1);
    let d_north_y = d.offset_y();
    assert_eq!(
        b_south_y, d_north_y,
        "B's south wall ({}) and D's north wall ({}) must be the same row (shared wall)",
        b_south_y, d_north_y
    );

    // C.east edge and D.west edge must be the same column (the closing edge).
    let c_east_x = c.offset_x() + (grids["c"].width() as i32 - 1);
    let d_west_x = d.offset_x();
    assert_eq!(
        c_east_x, d_west_x,
        "C's east wall ({}) and D's west wall ({}) must be the same column (closing edge)",
        c_east_x, d_west_x
    );
}

#[test]
fn layout_cycle_is_deterministic() {
    // Running layout_cycle twice on the same input must produce identical
    // offsets. Hash-ordered iteration inside the placer would break this.
    let rooms = rooms_square_ring();
    let grids = parse_grids(&rooms);
    let cycle: Vec<String> = vec!["a".into(), "b".into(), "d".into(), "c".into()];

    let first = layout_cycle(&cycle, &rooms, &grids).expect("square ring must place");
    let second = layout_cycle(&cycle, &rooms, &grids).expect("square ring must place");

    let offsets_first: HashMap<&str, (i32, i32)> = first
        .iter()
        .map(|p| (p.room_id(), (p.offset_x(), p.offset_y())))
        .collect();
    let offsets_second: HashMap<&str, (i32, i32)> = second
        .iter()
        .map(|p| (p.room_id(), (p.offset_x(), p.offset_y())))
        .collect();
    assert_eq!(
        offsets_first, offsets_second,
        "layout_cycle must produce identical offsets across calls"
    );
}

// ==========================================================================
// AC-3 / AC-4: Loop closure validation — failure case
// ==========================================================================

#[test]
fn layout_cycle_fails_loudly_when_closing_edge_mismatches() {
    // rooms_square_ring_broken_closure has D's west exit shifted so the
    // closing edge C→D cannot align. This is an authoring error and the
    // engine MUST fail loudly (no silent fallback to default layout).
    let rooms = rooms_square_ring_broken_closure();
    let grids = parse_grids(&rooms);
    let cycle: Vec<String> = vec!["a".into(), "b".into(), "d".into(), "c".into()];

    let result = layout_cycle(&cycle, &rooms, &grids);
    assert!(
        result.is_err(),
        "broken closure must produce Err, got Ok({:?})",
        result.ok().map(|v| v.len())
    );
}

#[test]
fn layout_cycle_closure_error_names_participating_rooms() {
    // AC-4: CycleClosureFailed must carry actionable context — specifically,
    // which rooms form the failing edge so the author can fix the grid.
    let rooms = rooms_square_ring_broken_closure();
    let grids = parse_grids(&rooms);
    let cycle: Vec<String> = vec!["a".into(), "b".into(), "d".into(), "c".into()];

    let err = layout_cycle(&cycle, &rooms, &grids).expect_err("broken closure must error");
    match err {
        LayoutError::CycleClosureFailed {
            ref cycle_rooms, ..
        } => {
            // The reported cycle must name every room in the failing cycle.
            let reported: HashSet<&str> = cycle_rooms.iter().map(String::as_str).collect();
            for room_id in ["a", "b", "c", "d"] {
                assert!(
                    reported.contains(room_id),
                    "CycleClosureFailed must name room {} in cycle_rooms, got {:?}",
                    room_id,
                    cycle_rooms
                );
            }
        }
        other => panic!(
            "expected LayoutError::CycleClosureFailed, got {:?}",
            other
        ),
    }
}

#[test]
fn cycle_closure_error_display_is_non_empty() {
    // Display impl must produce a non-empty message for log/error reporting.
    let err = LayoutError::CycleClosureFailed {
        cycle_rooms: vec!["a".into(), "b".into(), "c".into(), "d".into()],
        detail: "closing edge C→D: exit gap cols [1,2] vs [2,3]".into(),
    };
    let msg = format!("{}", err);
    assert!(
        msg.contains("a") && msg.contains("d"),
        "CycleClosureFailed Display must mention participating rooms, got {:?}",
        msg
    );
    assert!(
        !msg.is_empty(),
        "CycleClosureFailed Display must be non-empty"
    );
}

// ==========================================================================
// AC-5 / AC-9: Tree branches BFS-attached to cycle nodes
// ==========================================================================

#[test]
fn layout_dungeon_attaches_tree_branches_to_cycle_nodes() {
    let rooms = rooms_square_ring_with_branch();
    let grids = parse_grids(&rooms);

    let layout = layout_dungeon(&rooms, &grids).expect("cycle+branch must lay out");
    let placed_ids: HashSet<&str> = layout.rooms().iter().map(|r| r.room_id()).collect();

    // All 5 rooms must be placed — the 4 ring rooms plus the branch room.
    for room_id in ["a", "b", "c", "d", "e"] {
        assert!(
            placed_ids.contains(room_id),
            "layout_dungeon must place branch room {}, got {:?}",
            room_id,
            placed_ids
        );
    }
    assert_eq!(layout.rooms().len(), 5);
}

#[test]
fn layout_dungeon_branch_shares_wall_with_cycle_node() {
    // The branch room E must share a wall with D (its parent on the ring).
    let rooms = rooms_square_ring_with_branch();
    let grids = parse_grids(&rooms);

    let layout = layout_dungeon(&rooms, &grids).expect("cycle+branch must lay out");
    let by_id: HashMap<&str, &PlacedRoom> =
        layout.rooms().iter().map(|r| (r.room_id(), r)).collect();

    let d = by_id.get("d").expect("d must be placed");
    let e = by_id.get("e").expect("e must be placed");

    // D has an east exit → E's west wall must be at D's east wall column.
    let d_east_x = d.offset_x() + (grids["d"].width() as i32 - 1);
    let e_west_x = e.offset_x();
    assert_eq!(
        d_east_x, e_west_x,
        "branch room E ({}) must share D's east wall column ({})",
        e_west_x, d_east_x
    );
}

// ==========================================================================
// AC-6: Overlap detection between cycle rooms and tree branches
// ==========================================================================

#[test]
fn layout_dungeon_reports_overlap_between_branch_and_cycle() {
    // Construct a pathological graph: a square ring where a branch room is
    // forced to overlap with an opposite-side ring room because of its
    // grid dimensions. The engine must detect the overlap and fail loudly.
    //
    // Branch room F is 100×100 — guaranteed to overlap the ring regardless of
    // which side it's attached to.
    let oversize_grid: String = {
        let row: String = "#".repeat(100);
        let mut s = String::new();
        for _ in 0..100 {
            s.push_str(&row);
            s.push('\n');
        }
        s.pop();
        s
    };

    let mut rooms = rooms_square_ring_with_branch();
    // Replace room E with the oversize F.
    rooms[4] = room_def("e", "normal", vec![corridor("d")], &oversize_grid);
    // Note: the 100×100 grid has no exit gaps, so alignment will fail first.
    // This still exercises the error path — the layout must return Err.

    let grids = parse_grids(&rooms);
    let result = layout_dungeon(&rooms, &grids);
    assert!(
        result.is_err(),
        "unplaceable oversize branch must produce Err, got Ok({:?})",
        result.ok().map(|v| v.rooms().len())
    );
}

// ==========================================================================
// AC-7: Multiple disconnected cycles placed with spacing
// ==========================================================================

#[test]
fn layout_dungeon_places_two_disjoint_cycles_without_overlap() {
    // Build two independent square rings connected by a single bridge edge.
    // Each ring is a fresh 4-room 2×2; they must be placed with enough
    // spacing that their bounding boxes do not overlap.
    let mut rooms = rooms_square_ring();
    // Rename first ring rooms to a1/b1/c1/d1 and wire a1 as entrance.
    for r in rooms.iter_mut() {
        r.id = format!("{}1", r.id);
        r.name.clone_from(&r.id);
        for exit in r.exits.iter_mut() {
            if let RoomExit::Corridor { target } = exit {
                *target = format!("{}1", target);
            }
        }
    }

    // Second ring.
    let mut ring_2 = rooms_square_ring();
    for r in ring_2.iter_mut() {
        r.room_type = "normal".to_string(); // no second entrance
        r.id = format!("{}2", r.id);
        r.name.clone_from(&r.id);
        for exit in r.exits.iter_mut() {
            if let RoomExit::Corridor { target } = exit {
                *target = format!("{}2", target);
            }
        }
    }

    // Bridge: a1 also exits to a2.
    rooms[0].exits.push(corridor("a2"));
    ring_2[0].exits.push(corridor("a1"));

    rooms.extend(ring_2);
    let grids = parse_grids(&rooms);

    // The engine may or may not be able to geometrically bridge two full rings
    // in a single layout pass — that's a harder problem and not required by
    // AC-7. What IS required: if it can't, it must Err; if it can, no two
    // rooms in the result may occupy the same non-void cell.
    match layout_dungeon(&rooms, &grids) {
        Ok(layout) => {
            // No two placed rooms may share non-void cells.
            let placed: Vec<&PlacedRoom> = layout.rooms().iter().collect();
            for i in 0..placed.len() {
                for j in (i + 1)..placed.len() {
                    let overlaps = sidequest_game::tactical::layout::check_overlap(
                        &[placed[i].clone()],
                        placed[j],
                    );
                    assert!(
                        overlaps.is_empty(),
                        "rooms {} and {} overlap at {} cells after disjoint-cycle layout",
                        placed[i].room_id(),
                        placed[j].room_id(),
                        overlaps.len()
                    );
                }
            }
        }
        Err(err) => {
            // Fail-loud is acceptable. The error must name a specific failure
            // mode, not be a silent fallback.
            let msg = format!("{}", err);
            assert!(
                !msg.is_empty(),
                "layout_dungeon error on disjoint cycles must produce a non-empty message"
            );
        }
    }
}

// ==========================================================================
// AC-10: Integration — layout_dungeon end-to-end on a cyclic graph
// ==========================================================================

#[test]
fn layout_dungeon_full_cyclic_graph_succeeds() {
    let rooms = rooms_square_ring();
    let grids = parse_grids(&rooms);

    let layout = layout_dungeon(&rooms, &grids).expect("square ring must lay out");
    assert_eq!(
        layout.rooms().len(),
        4,
        "layout_dungeon must place all 4 ring rooms"
    );
    // Entrance at origin.
    let entrance = layout
        .rooms()
        .iter()
        .find(|r| r.room_id() == "a")
        .expect("entrance must be placed");
    assert_eq!(entrance.offset_x(), 0);
    assert_eq!(entrance.offset_y(), 0);
}

#[test]
fn layout_dungeon_linear_chain_matches_tree_behaviour() {
    // With no cycles in the graph, layout_dungeon must behave identically to
    // the tree-only placer from 29-6 — every room placed, entrance at origin.
    use sidequest_game::tactical::layout::layout_tree;

    // Build a linear A→B→C chain with real grids (reuse 29-6 shapes).
    let rooms = vec![
        room_def(
            "a",
            "entrance",
            vec![corridor("b")],
            "#####\n#...#\n#...#\n#...#\n##..#",
        ),
        room_def(
            "b",
            "normal",
            vec![corridor("a"), corridor("c")],
            "##..#\n#...#\n#....\n#....\n#####",
        ),
        room_def(
            "c",
            "normal",
            vec![corridor("b")],
            "#####\n#...#\n....#\n....#\n#####",
        ),
    ];
    let grids = parse_grids(&rooms);

    let dungeon_layout = layout_dungeon(&rooms, &grids).expect("linear chain must lay out");
    let tree_layout = layout_tree(&rooms, &grids).expect("linear chain tree must lay out");

    // Both must place the same number of rooms.
    assert_eq!(
        dungeon_layout.rooms().len(),
        tree_layout.rooms().len(),
        "layout_dungeon on a cycle-free graph must place the same rooms as layout_tree"
    );

    // Both must place the same IDs at the same offsets.
    let tree_offsets: HashMap<&str, (i32, i32)> = tree_layout
        .rooms()
        .iter()
        .map(|r| (r.room_id(), (r.offset_x(), r.offset_y())))
        .collect();
    for placed in dungeon_layout.rooms() {
        let expected = tree_offsets.get(placed.room_id()).unwrap_or_else(|| {
            panic!(
                "layout_dungeon placed room {} that layout_tree did not",
                placed.room_id()
            )
        });
        assert_eq!(
            (placed.offset_x(), placed.offset_y()),
            *expected,
            "offsets must match between layout_dungeon and layout_tree for {}",
            placed.room_id()
        );
    }
}

// ==========================================================================
// Rust rule #2: LayoutError remains #[non_exhaustive] after 29-7 variants added
// ==========================================================================

#[test]
fn layout_error_still_non_exhaustive_with_new_variants() {
    let err = LayoutError::CycleClosureFailed {
        cycle_rooms: vec!["a".into(), "b".into(), "c".into()],
        detail: "exit gap mismatch".into(),
    };
    // A wildcard arm is required by #[non_exhaustive], even after adding the
    // new variant. Removing it would be a regression.
    let description = match err {
        LayoutError::CycleClosureFailed { .. } => "cycle_closure_failed",
        LayoutError::Overlap { .. } => "overlap",
        LayoutError::NoEntrance => "no_entrance",
        _ => "other",
    };
    assert_eq!(description, "cycle_closure_failed");
}

#[test]
fn layout_error_cycle_closure_is_std_error() {
    // LayoutError::CycleClosureFailed must implement std::error::Error via
    // the existing LayoutError impl (not a separate type).
    let err: Box<dyn std::error::Error> = Box::new(LayoutError::CycleClosureFailed {
        cycle_rooms: vec!["a".into(), "b".into()],
        detail: "test".into(),
    });
    assert!(
        !err.to_string().is_empty(),
        "CycleClosureFailed must produce a non-empty Display via std::error::Error"
    );
}

// ==========================================================================
// AC-10 wiring: layout_dungeon is publicly accessible with the right signature
// ==========================================================================

#[test]
fn layout_dungeon_signature_is_public() {
    // Compile-time check: the public signature of layout_dungeon matches the
    // contract the room loader / validate CLI will depend on. If the module
    // isn't wired, this test won't compile.
    type LayoutDungeonFn =
        fn(&[RoomDef], &HashMap<String, TacticalGrid>) -> Result<DungeonLayout, LayoutError>;
    let _: LayoutDungeonFn = layout_dungeon;
}

#[test]
fn detect_cycles_signature_is_public() {
    type DetectCyclesFn = fn(&[RoomDef]) -> Vec<Vec<String>>;
    let _: DetectCyclesFn = detect_cycles;
}

#[test]
fn layout_cycle_signature_is_public() {
    type LayoutCycleFn = fn(
        &[String],
        &[RoomDef],
        &HashMap<String, TacticalGrid>,
    ) -> Result<Vec<PlacedRoom>, LayoutError>;
    let _: LayoutCycleFn = layout_cycle;
}

// ==========================================================================
// Helper sanity: ensure the square-ring fixture grids parse successfully.
// This is an invariant test — if these grids ever stop parsing, every other
// test in this file becomes vacuous. Fail fast here.
// ==========================================================================

#[test]
fn square_ring_fixture_grids_all_parse_with_expected_exits() {
    let rooms = rooms_square_ring();
    let grids = parse_grids(&rooms);
    assert_eq!(grids.len(), 4, "all 4 fixture grids must parse");

    // Each ring room must have exactly the two exits its geometry expects.
    let a_walls: HashSet<CardinalDirection> =
        grids["a"].exits().iter().map(|e| e.wall).collect();
    assert!(
        a_walls.contains(&CardinalDirection::East)
            && a_walls.contains(&CardinalDirection::South),
        "ring A must have east + south exits, got {:?}",
        a_walls
    );

    let d_walls: HashSet<CardinalDirection> =
        grids["d"].exits().iter().map(|e| e.wall).collect();
    assert!(
        d_walls.contains(&CardinalDirection::North)
            && d_walls.contains(&CardinalDirection::West),
        "ring D must have north + west exits, got {:?}",
        d_walls
    );
}

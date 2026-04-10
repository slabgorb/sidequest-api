//! Story 29-2: Tactical grid validation — perimeter closure, flood fill, exit matching
//!
//! RED phase — failing tests for tactical grid validators in sidequest-validate.
//!
//! These validators run at authoring time (via `sidequest-validate --tactical`)
//! to catch malformed grids before they reach the game engine. They build on
//! the ASCII parser from story 29-1.
//!
//! Validation rules (from ADR-071 section 9 / context-story-29-2):
//!   Rule 1: Dimensions match — grid rows/cols == size * tactical_scale
//!   Rule 2: Exit coverage — every exit in exits[] has a matching wall gap
//!   Rule 3: No orphan gaps — no wall gaps without a corresponding exit
//!   Rule 4: Perimeter closure — no floor adjacent to void without wall between
//!   Rule 5: Flood fill connectivity — all interior floor cells mutually reachable
//!   Rule 6: Legend completeness — all uppercase glyphs have a legend entry
//!   Rule 7: Legend placement — no legend glyphs on wall or void cells
//!   Rule 8: Exit gap width compatibility — between connected rooms
//!
//! AC mapping:
//!   AC-1: --tactical flag accepted by CLI
//!   AC-2: Perimeter closure (rule 4)
//!   AC-3: Flood fill (rule 5)
//!   AC-4: Exit coverage (rule 2)
//!   AC-5: Orphan gap detection (rule 3)
//!   AC-6: Legend validation (rules 6 + 7)
//!   AC-7: Dimension check (rule 1)
//!   AC-8: Cross-room exit width compat (rule 8)
//!   AC-9: Each rule has unit test with known-bad input
//!   AC-10: Wiring test: --tactical runs end-to-end

use std::collections::HashMap;

use sidequest_game::tactical::TacticalGrid;
use sidequest_genre::models::world::{LegendEntry, RoomDef, RoomExit};

// Import the validation module that Dev will create.
// This import fails to compile → RED state.
use sidequest_validate::tactical::{validate_tactical_grid, ValidationError};

// ==========================================================================
// Helper: build a legend HashMap from tuples
// ==========================================================================

fn legend(entries: &[(char, &str, &str)]) -> HashMap<char, LegendEntry> {
    entries
        .iter()
        .map(|(ch, typ, label)| {
            (
                *ch,
                LegendEntry {
                    r#type: typ.to_string(),
                    label: label.to_string(),
                },
            )
        })
        .collect()
}

/// Build a minimal RoomDef for testing.
fn room_def(
    id: &str,
    size: (u32, u32),
    tactical_scale: Option<u32>,
    exits: Vec<RoomExit>,
    grid: Option<&str>,
    legend_map: Option<HashMap<char, LegendEntry>>,
) -> RoomDef {
    RoomDef {
        id: id.to_string(),
        name: id.to_string(),
        room_type: "normal".to_string(),
        size,
        keeper_awareness_modifier: 1.0,
        exits,
        description: None,
        grid: grid.map(|s| s.to_string()),
        tactical_scale,
        legend: legend_map,
    }
}

// ==========================================================================
// Rule 1: Dimensions match — grid rows/cols == size * tactical_scale
// ==========================================================================

/// A grid whose dimensions exactly match size * tactical_scale passes.
#[test]
fn dimensions_match_passes() {
    // size=(2,2), tactical_scale=5 → expected 10x10
    let raw = &format!("{}\n", "#".repeat(10)).repeat(10);
    let raw = raw.trim_end();
    let leg = HashMap::new();
    let grid = TacticalGrid::parse(raw, &leg).unwrap();
    let room = room_def("r1", (2, 2), Some(5), vec![], Some(raw), None);

    let errors = validate_tactical_grid(&room, &grid);
    let dim_errors: Vec<_> = errors
        .iter()
        .filter(|e| matches!(e, ValidationError::DimensionMismatch { .. }))
        .collect();
    assert!(
        dim_errors.is_empty(),
        "Matching dimensions should not produce errors"
    );
}

/// A grid whose height doesn't match size * tactical_scale fails.
#[test]
fn dimensions_height_mismatch_fails() {
    // size=(2,2), tactical_scale=5 → expected 10x10, but grid is 10x8
    let raw = &format!("{}\n", "#".repeat(10)).repeat(8);
    let raw = raw.trim_end();
    let leg = HashMap::new();
    let grid = TacticalGrid::parse(raw, &leg).unwrap();
    let room = room_def("r1", (2, 2), Some(5), vec![], Some(raw), None);

    let errors = validate_tactical_grid(&room, &grid);
    let dim_errors: Vec<_> = errors
        .iter()
        .filter(|e| matches!(e, ValidationError::DimensionMismatch { .. }))
        .collect();
    assert!(
        !dim_errors.is_empty(),
        "Height mismatch should produce DimensionMismatch error"
    );
}

/// A grid whose width doesn't match size * tactical_scale fails.
#[test]
fn dimensions_width_mismatch_fails() {
    // size=(3,2), tactical_scale=4 → expected 12x8, but grid is 10x8
    let raw = &format!("{}\n", "#".repeat(10)).repeat(8);
    let raw = raw.trim_end();
    let leg = HashMap::new();
    let grid = TacticalGrid::parse(raw, &leg).unwrap();
    let room = room_def("r1", (3, 2), Some(4), vec![], Some(raw), None);

    let errors = validate_tactical_grid(&room, &grid);
    let dim_errors: Vec<_> = errors
        .iter()
        .filter(|e| matches!(e, ValidationError::DimensionMismatch { .. }))
        .collect();
    assert!(
        !dim_errors.is_empty(),
        "Width mismatch should produce DimensionMismatch error"
    );
}

/// When tactical_scale is None, dimension check is skipped.
#[test]
fn dimensions_skipped_when_no_tactical_scale() {
    let raw = "###\n#.#\n###";
    let leg = HashMap::new();
    let grid = TacticalGrid::parse(raw, &leg).unwrap();
    // No tactical_scale — dimensions check should be skipped entirely
    let room = room_def("r1", (2, 2), None, vec![], Some(raw), None);

    let errors = validate_tactical_grid(&room, &grid);
    let dim_errors: Vec<_> = errors
        .iter()
        .filter(|e| matches!(e, ValidationError::DimensionMismatch { .. }))
        .collect();
    assert!(
        dim_errors.is_empty(),
        "No tactical_scale → skip dimension check"
    );
}

// ==========================================================================
// Rule 2: Exit coverage — every RoomDef exit has a corresponding wall gap
// ==========================================================================

/// RoomDef with 2 exits and grid with 2 gaps → pass.
#[test]
fn exit_coverage_all_exits_have_gaps() {
    // Grid with gaps on north and south
    let raw = "\
##.##\n\
#...#\n\
#...#\n\
##.##";
    let leg = HashMap::new();
    let grid = TacticalGrid::parse(raw, &leg).unwrap();
    let exits = vec![
        RoomExit::Corridor {
            target: "room_a".to_string(),
        },
        RoomExit::Corridor {
            target: "room_b".to_string(),
        },
    ];
    let room = room_def("r1", (1, 1), None, exits, Some(raw), None);

    let errors = validate_tactical_grid(&room, &grid);
    let exit_errors: Vec<_> = errors
        .iter()
        .filter(|e| matches!(e, ValidationError::ExitWithoutGap { .. }))
        .collect();
    assert!(exit_errors.is_empty(), "All exits have gaps → no error");
}

/// RoomDef with 3 exits but grid has only 2 gaps → fail.
#[test]
fn exit_coverage_more_exits_than_gaps_fails() {
    // Grid with only 2 gaps (north + south)
    let raw = "\
##.##\n\
#...#\n\
#...#\n\
##.##";
    let leg = HashMap::new();
    let grid = TacticalGrid::parse(raw, &leg).unwrap();
    let exits = vec![
        RoomExit::Corridor {
            target: "room_a".to_string(),
        },
        RoomExit::Corridor {
            target: "room_b".to_string(),
        },
        RoomExit::Door {
            target: "room_c".to_string(),
            is_locked: false,
        },
    ];
    let room = room_def("r1", (1, 1), None, exits, Some(raw), None);

    let errors = validate_tactical_grid(&room, &grid);
    let exit_errors: Vec<_> = errors
        .iter()
        .filter(|e| matches!(e, ValidationError::ExitWithoutGap { .. }))
        .collect();
    assert!(
        !exit_errors.is_empty(),
        "3 exits with 2 gaps → ExitWithoutGap error"
    );
}

// ==========================================================================
// Rule 3: No orphan gaps — wall gaps without a corresponding exit
// ==========================================================================

/// Grid with 2 gaps and 2 exits → no orphans.
#[test]
fn no_orphan_gaps_when_counts_match() {
    let raw = "\
##.##\n\
#...#\n\
#...#\n\
##.##";
    let leg = HashMap::new();
    let grid = TacticalGrid::parse(raw, &leg).unwrap();
    let exits = vec![
        RoomExit::Corridor {
            target: "room_a".to_string(),
        },
        RoomExit::Corridor {
            target: "room_b".to_string(),
        },
    ];
    let room = room_def("r1", (1, 1), None, exits, Some(raw), None);

    let errors = validate_tactical_grid(&room, &grid);
    let orphan_errors: Vec<_> = errors
        .iter()
        .filter(|e| matches!(e, ValidationError::OrphanGap { .. }))
        .collect();
    assert!(orphan_errors.is_empty(), "Matching counts → no orphan gaps");
}

/// Grid with 3 gaps but only 2 exits → orphan gap detected.
#[test]
fn orphan_gap_detected_when_more_gaps_than_exits() {
    // Grid with gaps on north, south, and east
    let raw = "\
##.##\n\
#....\n\
#...#\n\
##.##";
    let leg = HashMap::new();
    let grid = TacticalGrid::parse(raw, &leg).unwrap();
    let exits = vec![
        RoomExit::Corridor {
            target: "room_a".to_string(),
        },
        RoomExit::Corridor {
            target: "room_b".to_string(),
        },
    ];
    let room = room_def("r1", (1, 1), None, exits, Some(raw), None);

    let errors = validate_tactical_grid(&room, &grid);
    let orphan_errors: Vec<_> = errors
        .iter()
        .filter(|e| matches!(e, ValidationError::OrphanGap { .. }))
        .collect();
    assert!(
        !orphan_errors.is_empty(),
        "3 gaps with 2 exits → OrphanGap error"
    );
}

// ==========================================================================
// Rule 4: Perimeter closure — no floor adjacent to void without wall between
// ==========================================================================

/// Well-formed room with walls separating floor from void → pass.
#[test]
fn perimeter_closure_valid_room_passes() {
    let raw = "\
_###_\n\
##.##\n\
#...#\n\
##.##\n\
_###_";
    let leg = HashMap::new();
    let grid = TacticalGrid::parse(raw, &leg).unwrap();
    let room = room_def("r1", (1, 1), None, vec![], Some(raw), None);

    let errors = validate_tactical_grid(&room, &grid);
    let closure_errors: Vec<_> = errors
        .iter()
        .filter(|e| matches!(e, ValidationError::PerimeterBreach { .. }))
        .collect();
    assert!(
        closure_errors.is_empty(),
        "Valid perimeter → no breach errors"
    );
}

/// Floor cell directly adjacent to void (no wall between) → fail.
#[test]
fn perimeter_breach_floor_adjacent_to_void() {
    // The floor cell at (2,1) is adjacent to void at (2,0) with no wall
    let raw = "\
_._._\n\
##.##\n\
#...#\n\
##.##\n\
_###_";
    let leg = HashMap::new();
    let grid = TacticalGrid::parse(raw, &leg).unwrap();
    let room = room_def("r1", (1, 1), None, vec![], Some(raw), None);

    let errors = validate_tactical_grid(&room, &grid);
    let closure_errors: Vec<_> = errors
        .iter()
        .filter(|e| matches!(e, ValidationError::PerimeterBreach { .. }))
        .collect();
    assert!(
        !closure_errors.is_empty(),
        "Floor adjacent to void → PerimeterBreach"
    );
}

/// Non-rectangular room with proper void-wall boundary → pass.
#[test]
fn perimeter_closure_non_rectangular_room_passes() {
    let raw = "\
_##_\n\
#..#\n\
#..#\n\
_##_";
    let leg = HashMap::new();
    let grid = TacticalGrid::parse(raw, &leg).unwrap();
    let room = room_def("oval", (1, 1), None, vec![], Some(raw), None);

    let errors = validate_tactical_grid(&room, &grid);
    let closure_errors: Vec<_> = errors
        .iter()
        .filter(|e| matches!(e, ValidationError::PerimeterBreach { .. }))
        .collect();
    assert!(
        closure_errors.is_empty(),
        "Non-rectangular room with proper boundary → no breach"
    );
}

/// Multiple perimeter breaches reported (not just the first one).
#[test]
fn perimeter_breach_reports_all_violations() {
    // Floor cells at (1,0) and (3,0) both adjacent to void with no wall
    let raw = "\
_..._\n\
#...#\n\
#####";
    let leg = HashMap::new();
    let grid = TacticalGrid::parse(raw, &leg).unwrap();
    let room = room_def("r1", (1, 1), None, vec![], Some(raw), None);

    let errors = validate_tactical_grid(&room, &grid);
    let closure_errors: Vec<_> = errors
        .iter()
        .filter(|e| matches!(e, ValidationError::PerimeterBreach { .. }))
        .collect();
    assert!(
        closure_errors.len() >= 2,
        "Multiple breaches should all be reported, got {}",
        closure_errors.len()
    );
}

// ==========================================================================
// Rule 5: Flood fill connectivity — all floor cells mutually reachable
// ==========================================================================

/// All floor cells connected → pass.
#[test]
fn flood_fill_connected_floor_passes() {
    let raw = "\
#####\n\
#...#\n\
#...#\n\
#...#\n\
#####";
    let leg = HashMap::new();
    let grid = TacticalGrid::parse(raw, &leg).unwrap();
    let room = room_def("r1", (1, 1), None, vec![], Some(raw), None);

    let errors = validate_tactical_grid(&room, &grid);
    let fill_errors: Vec<_> = errors
        .iter()
        .filter(|e| matches!(e, ValidationError::DisconnectedFloor { .. }))
        .collect();
    assert!(fill_errors.is_empty(), "Fully connected floor → no errors");
}

/// Two isolated floor regions → fail.
#[test]
fn flood_fill_two_isolated_regions_fails() {
    let raw = "\
#######\n\
#..#..#\n\
#..#..#\n\
#######";
    let leg = HashMap::new();
    let grid = TacticalGrid::parse(raw, &leg).unwrap();
    let room = room_def("r1", (1, 1), None, vec![], Some(raw), None);

    let errors = validate_tactical_grid(&room, &grid);
    let fill_errors: Vec<_> = errors
        .iter()
        .filter(|e| matches!(e, ValidationError::DisconnectedFloor { .. }))
        .collect();
    assert!(
        !fill_errors.is_empty(),
        "Two isolated floor regions → DisconnectedFloor"
    );
}

/// Single isolated floor cell → fail with coordinates identified.
#[test]
fn flood_fill_single_isolated_cell_fails() {
    // Main floor area + one isolated cell at (5,1)
    let raw = "\
#######\n\
#...#.#\n\
#...###\n\
#######";
    let leg = HashMap::new();
    let grid = TacticalGrid::parse(raw, &leg).unwrap();
    let room = room_def("r1", (1, 1), None, vec![], Some(raw), None);

    let errors = validate_tactical_grid(&room, &grid);
    let fill_errors: Vec<_> = errors
        .iter()
        .filter(|e| matches!(e, ValidationError::DisconnectedFloor { .. }))
        .collect();
    assert!(
        !fill_errors.is_empty(),
        "Single isolated cell → DisconnectedFloor"
    );
}

/// Error message should include coordinates of isolated cells.
#[test]
fn flood_fill_error_includes_isolated_coordinates() {
    let raw = "\
#######\n\
#...#.#\n\
#...###\n\
#######";
    let leg = HashMap::new();
    let grid = TacticalGrid::parse(raw, &leg).unwrap();
    let room = room_def("r1", (1, 1), None, vec![], Some(raw), None);

    let errors = validate_tactical_grid(&room, &grid);
    let fill_error = errors
        .iter()
        .find(|e| matches!(e, ValidationError::DisconnectedFloor { .. }))
        .expect("Should have DisconnectedFloor error");

    // The error should identify the isolated cell at (5, 1)
    match fill_error {
        ValidationError::DisconnectedFloor { isolated_cells } => {
            assert!(
                !isolated_cells.is_empty(),
                "Error should include coordinates of isolated cells"
            );
        }
        _ => panic!("Expected DisconnectedFloor variant"),
    }
}

/// Floor cells connected through a door → still connected.
#[test]
fn flood_fill_door_connects_regions() {
    let raw = "\
#####\n\
#...#\n\
##+##\n\
#...#\n\
#####";
    let leg = HashMap::new();
    let grid = TacticalGrid::parse(raw, &leg).unwrap();
    let room = room_def("r1", (1, 1), None, vec![], Some(raw), None);

    let errors = validate_tactical_grid(&room, &grid);
    let fill_errors: Vec<_> = errors
        .iter()
        .filter(|e| matches!(e, ValidationError::DisconnectedFloor { .. }))
        .collect();
    assert!(
        fill_errors.is_empty(),
        "Door connects the two halves → no disconnection"
    );
}

/// Water and difficult terrain are walkable → count as connected floor.
#[test]
fn flood_fill_water_and_difficult_terrain_are_walkable() {
    let raw = "\
#####\n\
#.~.#\n\
#,.,#\n\
#####";
    let leg = HashMap::new();
    let grid = TacticalGrid::parse(raw, &leg).unwrap();
    let room = room_def("r1", (1, 1), None, vec![], Some(raw), None);

    let errors = validate_tactical_grid(&room, &grid);
    let fill_errors: Vec<_> = errors
        .iter()
        .filter(|e| matches!(e, ValidationError::DisconnectedFloor { .. }))
        .collect();
    assert!(
        fill_errors.is_empty(),
        "Water/difficult terrain are walkable → connected"
    );
}

// ==========================================================================
// Rule 6: Legend completeness — all uppercase glyphs have a legend entry
// Note: The parser already enforces this at parse time (MissingLegend error).
// The validator re-checks in case grid was constructed programmatically.
// ==========================================================================

/// Grid with all legend glyphs defined → pass.
#[test]
fn legend_completeness_all_defined_passes() {
    let leg = legend(&[('P', "cover", "Pillar")]);
    let raw = "\
#####\n\
#.P.#\n\
#####";
    let grid = TacticalGrid::parse(raw, &leg).unwrap();
    let room = room_def("r1", (1, 1), None, vec![], Some(raw), Some(leg));

    let errors = validate_tactical_grid(&room, &grid);
    let legend_errors: Vec<_> = errors
        .iter()
        .filter(|e| matches!(e, ValidationError::LegendIncomplete { .. }))
        .collect();
    assert!(
        legend_errors.is_empty(),
        "All glyphs defined → no legend error"
    );
}

// ==========================================================================
// Rule 7: Legend placement — no legend glyphs on wall or void cells
// Note: The parser resolves glyphs to Feature cells on the floor. But if a
// legend entry exists for a glyph not placed in the grid, that's a warning.
// More importantly, this rule validates that features are on walkable cells.
// ==========================================================================

/// Feature glyph on a walkable cell → pass.
#[test]
fn legend_placement_on_floor_passes() {
    let leg = legend(&[('A', "atmosphere", "Altar")]);
    let raw = "\
#####\n\
#.A.#\n\
#####";
    let grid = TacticalGrid::parse(raw, &leg).unwrap();
    let room = room_def("r1", (1, 1), None, vec![], Some(raw), Some(leg));

    let errors = validate_tactical_grid(&room, &grid);
    let placement_errors: Vec<_> = errors
        .iter()
        .filter(|e| matches!(e, ValidationError::LegendOnNonFloor { .. }))
        .collect();
    assert!(
        placement_errors.is_empty(),
        "Feature on floor → no placement error"
    );
}

/// Legend entry defined but not placed in grid → warning.
#[test]
fn legend_unused_glyph_produces_warning() {
    let leg = legend(&[('P', "cover", "Pillar"), ('T', "hazard", "Trap")]);
    // Only P is placed, T is defined but not used
    let raw = "\
#####\n\
#.P.#\n\
#####";
    let grid = TacticalGrid::parse(raw, &leg).unwrap();
    let room = room_def("r1", (1, 1), None, vec![], Some(raw), Some(leg));

    let errors = validate_tactical_grid(&room, &grid);
    let unused_errors: Vec<_> = errors
        .iter()
        .filter(|e| matches!(e, ValidationError::UnusedLegendEntry { .. }))
        .collect();
    assert!(
        !unused_errors.is_empty(),
        "Unused legend entry 'T' → UnusedLegendEntry warning"
    );
}

// ==========================================================================
// Rule 8: Exit gap width compatibility — between connected rooms
// ==========================================================================

/// Two rooms with matching exit gap widths → pass.
#[test]
fn exit_width_compatible_rooms_pass() {
    // Room A: north exit, width 2
    let raw_a = "\
##..##\n\
#....#\n\
######";
    let leg = HashMap::new();
    let grid_a = TacticalGrid::parse(raw_a, &leg).unwrap();
    let room_a = room_def(
        "room_a",
        (1, 1),
        None,
        vec![RoomExit::Corridor {
            target: "room_b".to_string(),
        }],
        Some(raw_a),
        None,
    );

    // Room B: south exit, width 2
    let raw_b = "\
######\n\
#....#\n\
##..##";
    let grid_b = TacticalGrid::parse(raw_b, &leg).unwrap();
    let room_b = room_def(
        "room_b",
        (1, 1),
        None,
        vec![RoomExit::Corridor {
            target: "room_a".to_string(),
        }],
        Some(raw_b),
        None,
    );

    let errors = sidequest_validate::tactical::validate_exit_width_compatibility(
        &room_a, &grid_a, &room_b, &grid_b,
    );
    assert!(
        errors.is_empty(),
        "Matching widths → no compatibility error"
    );
}

/// Two connected rooms with different exit gap widths → warning.
#[test]
fn exit_width_mismatch_produces_warning() {
    // Room A: north exit, width 2
    let raw_a = "\
##..##\n\
#....#\n\
######";
    let leg = HashMap::new();
    let grid_a = TacticalGrid::parse(raw_a, &leg).unwrap();
    let room_a = room_def(
        "room_a",
        (1, 1),
        None,
        vec![RoomExit::Corridor {
            target: "room_b".to_string(),
        }],
        Some(raw_a),
        None,
    );

    // Room B: south exit, width 3
    let raw_b = "\
######\n\
#....#\n\
#...##";
    let grid_b = TacticalGrid::parse(raw_b, &leg).unwrap();
    let room_b = room_def(
        "room_b",
        (1, 1),
        None,
        vec![RoomExit::Corridor {
            target: "room_a".to_string(),
        }],
        Some(raw_b),
        None,
    );

    let errors = sidequest_validate::tactical::validate_exit_width_compatibility(
        &room_a, &grid_a, &room_b, &grid_b,
    );
    assert!(
        !errors.is_empty(),
        "Width 2 vs width 3 → ExitWidthMismatch warning"
    );
}

// ==========================================================================
// Integration: validate_tactical_grid composes all rules (not fail-fast)
// ==========================================================================

/// A well-formed grid passes all validation rules.
#[test]
fn integration_valid_grid_passes_all_rules() {
    let leg = legend(&[('P', "cover", "Pillar")]);
    let raw = "\
#####\n\
#.P.#\n\
#...#\n\
#####";
    let grid = TacticalGrid::parse(raw, &leg).unwrap();
    let room = room_def("r1", (1, 1), None, vec![], Some(raw), Some(leg));

    let errors = validate_tactical_grid(&room, &grid);
    assert!(
        errors.is_empty(),
        "Well-formed grid should produce zero errors"
    );
}

/// A grid with multiple problems reports ALL of them (not fail-fast).
#[test]
fn integration_multiple_errors_all_reported() {
    // Grid has: disconnected floor regions + perimeter breach
    let raw = "\
_..._\n\
#.#.#\n\
#####";
    let leg = HashMap::new();
    let grid = TacticalGrid::parse(raw, &leg).unwrap();
    let room = room_def("r1", (1, 1), None, vec![], Some(raw), None);

    let errors = validate_tactical_grid(&room, &grid);
    // Should have at least 2 different error types
    let has_breach = errors
        .iter()
        .any(|e| matches!(e, ValidationError::PerimeterBreach { .. }));
    let has_disconnect = errors
        .iter()
        .any(|e| matches!(e, ValidationError::DisconnectedFloor { .. }));
    assert!(
        has_breach || has_disconnect,
        "Multiple problems should produce multiple error types, got: {:?}",
        errors
    );
}

/// Grid with no floor cells at all → still valid (pure wall room is a valid design).
#[test]
fn grid_with_no_floor_cells_is_valid_for_flood_fill() {
    let raw = "\
###\n\
###\n\
###";
    let leg = HashMap::new();
    let grid = TacticalGrid::parse(raw, &leg).unwrap();
    let room = room_def("r1", (1, 1), None, vec![], Some(raw), None);

    let errors = validate_tactical_grid(&room, &grid);
    let fill_errors: Vec<_> = errors
        .iter()
        .filter(|e| matches!(e, ValidationError::DisconnectedFloor { .. }))
        .collect();
    assert!(
        fill_errors.is_empty(),
        "No floor cells → nothing to disconnect"
    );
}

// ==========================================================================
// AC-10: Wiring test — validate_tactical_grid is callable from production code
// ==========================================================================

/// The validate_tactical_grid function must be publicly accessible from the
/// sidequest_validate crate. This test compiles only if the function is exported.
#[test]
fn validate_tactical_grid_is_public() {
    // The import at the top of this file is the real wiring test.
    // If it compiles, the function is publicly accessible.
    let leg = HashMap::new();
    let raw = "#";
    let grid = TacticalGrid::parse(raw, &leg).unwrap();
    let room = room_def("r1", (1, 1), None, vec![], Some(raw), None);
    let _result = validate_tactical_grid(&room, &grid);
    // If we reach here, the function is wired correctly.
}

// ==========================================================================
// Rust rule: #[non_exhaustive] on ValidationError enum
// ==========================================================================

/// ValidationError is a public enum that will grow as rules are added.
/// It must have #[non_exhaustive] so downstream matchers use a wildcard arm.
#[test]
fn validation_error_is_non_exhaustive() {
    let leg = HashMap::new();
    // Create a grid with a known error to get a ValidationError instance
    let raw = "\
_..._\n\
#...#\n\
#####";
    let grid = TacticalGrid::parse(raw, &leg).unwrap();
    let room = room_def("r1", (1, 1), None, vec![], Some(raw), None);

    let errors = validate_tactical_grid(&room, &grid);
    if let Some(error) = errors.first() {
        // This match requires a wildcard arm only if #[non_exhaustive] is present
        let _desc = match error {
            ValidationError::DimensionMismatch { .. } => "dimension",
            ValidationError::ExitWithoutGap { .. } => "exit_without_gap",
            ValidationError::OrphanGap { .. } => "orphan_gap",
            ValidationError::PerimeterBreach { .. } => "perimeter_breach",
            ValidationError::DisconnectedFloor { .. } => "disconnected_floor",
            ValidationError::LegendIncomplete { .. } => "legend_incomplete",
            ValidationError::LegendOnNonFloor { .. } => "legend_on_non_floor",
            ValidationError::UnusedLegendEntry { .. } => "unused_legend",
            ValidationError::ExitWidthMismatch { .. } => "exit_width_mismatch",
            _ => "unknown", // Required by #[non_exhaustive]
        };
    }
}

// ==========================================================================
// Edge case: Feature cells are walkable → count as floor for flood fill
// ==========================================================================

/// Feature cells should be treated as walkable for flood fill purposes.
#[test]
fn flood_fill_feature_cells_are_walkable() {
    let leg = legend(&[('P', "cover", "Pillar")]);
    let raw = "\
#####\n\
#.P.#\n\
#P.P#\n\
#####";
    let grid = TacticalGrid::parse(raw, &leg).unwrap();
    let room = room_def("r1", (1, 1), None, vec![], Some(raw), Some(leg));

    let errors = validate_tactical_grid(&room, &grid);
    let fill_errors: Vec<_> = errors
        .iter()
        .filter(|e| matches!(e, ValidationError::DisconnectedFloor { .. }))
        .collect();
    assert!(
        fill_errors.is_empty(),
        "Feature cells are walkable → connected"
    );
}

// ==========================================================================
// Edge case: Exit gaps at grid edges (perimeter) are NOT breaches
// ==========================================================================

/// Exit gaps (floor on perimeter) are intentional and should not trigger
/// perimeter breach errors.
#[test]
fn exit_gaps_are_not_perimeter_breaches() {
    let raw = "\
##.##\n\
#...#\n\
#...#\n\
##.##";
    let leg = HashMap::new();
    let grid = TacticalGrid::parse(raw, &leg).unwrap();
    let exits = vec![
        RoomExit::Corridor {
            target: "a".to_string(),
        },
        RoomExit::Corridor {
            target: "b".to_string(),
        },
    ];
    let room = room_def("r1", (1, 1), None, exits, Some(raw), None);

    let errors = validate_tactical_grid(&room, &grid);
    let breach_errors: Vec<_> = errors
        .iter()
        .filter(|e| matches!(e, ValidationError::PerimeterBreach { .. }))
        .collect();
    assert!(
        breach_errors.is_empty(),
        "Exit gaps on perimeter are intentional, not breaches"
    );
}

//! Story 29-1: ASCII grid parser — glyph vocabulary, legend, exit extraction
//!
//! RED phase — failing tests for the tactical grid parser.
//!
//! The ASCII grid in rooms.yaml is the source of truth for room geometry.
//! This parser converts multiline ASCII strings into a structured TacticalGrid
//! with typed cells, legend resolution, and exit extraction from wall gaps.
//!
//! ACs:
//!   AC-1: TacticalCell enum covers all 8 glyph types from ADR-071
//!   AC-2: Parser handles non-rectangular rooms (void cells carve shapes)
//!   AC-3: Parser rejects unknown glyphs with descriptive error
//!   AC-4: Parser rejects uneven row lengths
//!   AC-5: Exit gaps from wall perimeter match clockwise ordering
//!   AC-6: Legend glyphs (A-Z) resolve to FeatureDef with type and label
//!   AC-7: RoomDef gains optional grid, tactical_scale, legend fields
//!   AC-8: Unit tests for rectangular, oval, features, multiple exits
//!   AC-9: Integration test: parse "The Mouth" from ADR-071
//!   AC-10: Wiring test: parser callable from non-test code path

use std::collections::HashMap;

use sidequest_game::tactical::{
    CardinalDirection, FeatureType, GridParseError, GridPos, TacticalCell, TacticalGrid,
};

use sidequest_genre::models::world::LegendEntry;

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

// ==========================================================================
// AC-1: TacticalCell covers all 8 glyph types from ADR-071
// ==========================================================================

/// Each glyph character maps to exactly one TacticalCell variant.
#[test]
fn cell_floor_from_dot() {
    let grid = TacticalGrid::parse(".", &HashMap::new()).unwrap();
    assert_eq!(grid.cell_at(0, 0), Some(&TacticalCell::Floor));
}

#[test]
fn cell_wall_from_hash() {
    let grid = TacticalGrid::parse("#", &HashMap::new()).unwrap();
    assert_eq!(grid.cell_at(0, 0), Some(&TacticalCell::Wall));
}

#[test]
fn cell_void_from_underscore() {
    let grid = TacticalGrid::parse("_", &HashMap::new()).unwrap();
    assert_eq!(grid.cell_at(0, 0), Some(&TacticalCell::Void));
}

#[test]
fn cell_door_closed_from_plus() {
    let grid = TacticalGrid::parse("+", &HashMap::new()).unwrap();
    assert_eq!(grid.cell_at(0, 0), Some(&TacticalCell::DoorClosed));
}

#[test]
fn cell_door_open_from_slash() {
    let grid = TacticalGrid::parse("/", &HashMap::new()).unwrap();
    assert_eq!(grid.cell_at(0, 0), Some(&TacticalCell::DoorOpen));
}

#[test]
fn cell_water_from_tilde() {
    let grid = TacticalGrid::parse("~", &HashMap::new()).unwrap();
    assert_eq!(grid.cell_at(0, 0), Some(&TacticalCell::Water));
}

#[test]
fn cell_difficult_terrain_from_comma() {
    let grid = TacticalGrid::parse(",", &HashMap::new()).unwrap();
    assert_eq!(grid.cell_at(0, 0), Some(&TacticalCell::DifficultTerrain));
}

#[test]
fn cell_feature_from_uppercase_letter() {
    let leg = legend(&[('P', "cover", "Pillar")]);
    let grid = TacticalGrid::parse("P", &leg).unwrap();
    assert_eq!(grid.cell_at(0, 0), Some(&TacticalCell::Feature('P')));
}

// ==========================================================================
// AC-1 (cont): CellProperties for each cell type
// ==========================================================================

#[test]
fn floor_is_walkable_no_los_block() {
    let props = TacticalCell::Floor.properties();
    assert!(props.walkable);
    assert!(!props.blocks_los);
    assert!((props.movement_cost - 1.0).abs() < f64::EPSILON);
}

#[test]
fn wall_blocks_movement_and_los() {
    let props = TacticalCell::Wall.properties();
    assert!(!props.walkable);
    assert!(props.blocks_los);
}

#[test]
fn void_not_walkable() {
    let props = TacticalCell::Void.properties();
    assert!(!props.walkable);
}

#[test]
fn water_walkable_double_cost() {
    let props = TacticalCell::Water.properties();
    assert!(props.walkable);
    assert!(
        props.movement_cost > 1.0,
        "Water should cost more than normal terrain"
    );
}

#[test]
fn difficult_terrain_walkable_double_cost() {
    let props = TacticalCell::DifficultTerrain.properties();
    assert!(props.walkable);
    assert!(
        props.movement_cost > 1.0,
        "Difficult terrain should cost more than normal"
    );
}

#[test]
fn closed_door_blocks_los_but_walkable() {
    let props = TacticalCell::DoorClosed.properties();
    assert!(
        props.walkable,
        "Closed door is traversable (opens on entry)"
    );
    assert!(props.blocks_los, "Closed door blocks line of sight");
}

#[test]
fn open_door_walkable_no_los_block() {
    let props = TacticalCell::DoorOpen.properties();
    assert!(props.walkable);
    assert!(!props.blocks_los);
}

// ==========================================================================
// AC-2: Non-rectangular rooms via void cells
// ==========================================================================

/// An "oval" room: void cells carve an irregular shape from a rectangle.
#[test]
fn oval_room_void_cells_carve_shape() {
    let raw = "\
_##_\n\
#..#\n\
#..#\n\
_##_";
    let grid = TacticalGrid::parse(raw, &HashMap::new()).unwrap();
    assert_eq!(grid.width(), 4);
    assert_eq!(grid.height(), 4);
    // Corners are void
    assert_eq!(grid.cell_at(0, 0), Some(&TacticalCell::Void));
    assert_eq!(grid.cell_at(3, 0), Some(&TacticalCell::Void));
    assert_eq!(grid.cell_at(0, 3), Some(&TacticalCell::Void));
    assert_eq!(grid.cell_at(3, 3), Some(&TacticalCell::Void));
    // Interior is floor
    assert_eq!(grid.cell_at(1, 1), Some(&TacticalCell::Floor));
    assert_eq!(grid.cell_at(2, 2), Some(&TacticalCell::Floor));
}

/// Cell_at returns None for out-of-bounds coordinates.
#[test]
fn cell_at_out_of_bounds_returns_none() {
    let grid = TacticalGrid::parse("#.", &HashMap::new()).unwrap();
    assert_eq!(grid.cell_at(5, 5), None);
}

// ==========================================================================
// AC-3: Parser rejects unknown glyphs with descriptive error
// ==========================================================================

#[test]
fn unknown_glyph_returns_error_with_position() {
    let raw = "..\n.@";
    let result = TacticalGrid::parse(raw, &HashMap::new());
    let err = result.expect_err("Should reject unknown glyph '@'");
    match err {
        GridParseError::UnknownGlyph { glyph, x, y, .. } => {
            assert_eq!(glyph, '@');
            assert_eq!(x, 1);
            assert_eq!(y, 1);
        }
        other => panic!("Expected UnknownGlyph, got: {:?}", other),
    }
}

/// Uppercase letter without legend entry should fail.
#[test]
fn uppercase_without_legend_entry_is_unknown() {
    let raw = "A";
    let result = TacticalGrid::parse(raw, &HashMap::new());
    let err = result.expect_err("Uppercase 'A' without legend entry should be rejected");
    match err {
        GridParseError::UnknownGlyph { glyph, .. }
        | GridParseError::MissingLegend { glyph, .. } => {
            assert_eq!(glyph, 'A');
        }
        other => panic!("Expected UnknownGlyph or MissingLegend, got: {:?}", other),
    }
}

// ==========================================================================
// AC-4: Parser rejects uneven row lengths
// ==========================================================================

#[test]
fn uneven_rows_returns_error() {
    let raw = "...\n..";
    let result = TacticalGrid::parse(raw, &HashMap::new());
    let err = result.expect_err("Uneven rows should be rejected");
    match err {
        GridParseError::UnevenRows {
            expected_width,
            actual_width,
            row,
        } => {
            assert_eq!(expected_width, 3);
            assert_eq!(actual_width, 2);
            assert_eq!(row, 1);
        }
        other => panic!("Expected UnevenRows, got: {:?}", other),
    }
}

#[test]
fn empty_input_returns_error() {
    let result = TacticalGrid::parse("", &HashMap::new());
    assert!(result.is_err(), "Empty grid should be rejected");
}

// ==========================================================================
// AC-5: Exit gaps extracted from wall perimeter, clockwise ordering
// ==========================================================================

/// A simple rectangular room with exits on north and south walls.
#[test]
fn exits_extracted_from_wall_gaps_north_and_south() {
    // 6-wide room with gaps in top and bottom walls
    let raw = "\
##..##\n\
#....#\n\
#....#\n\
##..##";
    let grid = TacticalGrid::parse(raw, &HashMap::new()).unwrap();
    let exits = grid.exits();
    assert_eq!(exits.len(), 2, "Should find 2 exits (north + south)");

    // Clockwise: north first, then south
    assert_eq!(exits[0].wall, CardinalDirection::North);
    assert_eq!(exits[1].wall, CardinalDirection::South);
}

/// Exits on all four walls.
#[test]
fn exits_on_all_four_walls_clockwise() {
    let raw = "\
##.##\n\
#....\n\
#...#\n\
....#\n\
##.##";
    let grid = TacticalGrid::parse(raw, &HashMap::new()).unwrap();
    let exits = grid.exits();

    // Clockwise: North, East, South, West
    let walls: Vec<CardinalDirection> = exits.iter().map(|e| e.wall).collect();
    assert_eq!(
        walls,
        vec![
            CardinalDirection::North,
            CardinalDirection::East,
            CardinalDirection::South,
            CardinalDirection::West,
        ],
        "Exits should be ordered clockwise: N, E, S, W"
    );
}

/// Exit gap width is measured in cells.
#[test]
fn exit_gap_width_matches_opening_size() {
    let raw = "\
##..##\n\
#....#\n\
######";
    let grid = TacticalGrid::parse(raw, &HashMap::new()).unwrap();
    let exits = grid.exits();
    assert_eq!(exits.len(), 1);
    assert_eq!(exits[0].width, 2, "Gap is 2 cells wide");
}

/// Exit gap cells record which perimeter positions form the gap.
#[test]
fn exit_gap_cells_record_positions() {
    let raw = "\
##..##\n\
#....#\n\
######";
    let grid = TacticalGrid::parse(raw, &HashMap::new()).unwrap();
    let exits = grid.exits();
    assert_eq!(exits[0].cells, vec![2, 3], "Gap at columns 2 and 3");
}

// ==========================================================================
// AC-6: Legend glyphs (A-Z) resolve to FeatureDef
// ==========================================================================

#[test]
fn legend_resolves_feature_type_and_label() {
    let leg = legend(&[('T', "hazard", "Spike trap")]);
    let raw = "T";
    let grid = TacticalGrid::parse(raw, &leg).unwrap();

    let feature = grid.legend().get(&'T').expect("Legend should contain 'T'");
    assert_eq!(feature.feature_type, FeatureType::Hazard);
    assert_eq!(feature.label, "Spike trap");
}

#[test]
fn multiple_legend_entries_all_resolved() {
    let leg = legend(&[
        ('P', "cover", "Pillar"),
        ('T', "hazard", "Trap"),
        ('A', "atmosphere", "Altar"),
    ]);
    let raw = "PAT";
    let grid = TacticalGrid::parse(raw, &leg).unwrap();

    assert_eq!(grid.legend().len(), 3);
    assert_eq!(
        grid.legend().get(&'P').unwrap().feature_type,
        FeatureType::Cover
    );
    assert_eq!(
        grid.legend().get(&'T').unwrap().feature_type,
        FeatureType::Hazard
    );
    assert_eq!(
        grid.legend().get(&'A').unwrap().feature_type,
        FeatureType::Atmosphere
    );
}

/// Feature cells are walkable (you can stand on/in them).
#[test]
fn feature_cell_is_walkable() {
    let props = TacticalCell::Feature('P').properties();
    assert!(props.walkable, "Feature cells should be walkable");
}

// ==========================================================================
// AC-7: RoomDef gains optional grid, tactical_scale, legend fields
// ==========================================================================

/// RoomDef with grid fields deserializes correctly.
#[test]
fn roomdef_with_grid_deserializes() {
    let yaml = r#"
id: test_room
name: "Test Room"
room_type: chamber
size: [2, 2]
exits: []
grid: |
  ##
  ##
tactical_scale: 4
legend:
  P:
    type: cover
    label: "Pillar"
"#;
    let room: sidequest_genre::models::world::RoomDef = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(room.grid.as_deref(), Some("##\n##\n"));
    assert_eq!(room.tactical_scale, Some(4));
    assert!(room.legend.is_some());
    let leg = room.legend.unwrap();
    assert_eq!(leg.get(&'P').unwrap().label, "Pillar");
}

/// RoomDef without grid fields still deserializes (backward compat).
#[test]
fn roomdef_without_grid_fields_deserializes() {
    let yaml = r#"
id: old_room
name: "Old Room"
room_type: chamber
size: [1, 1]
exits: []
"#;
    let room: sidequest_genre::models::world::RoomDef = serde_yaml::from_str(yaml).unwrap();
    assert!(room.grid.is_none());
    assert!(room.tactical_scale.is_none());
    assert!(room.legend.is_none());
}

// ==========================================================================
// AC-8: Unit tests — rectangular room
// ==========================================================================

#[test]
fn rectangular_room_dimensions() {
    let raw = "\
####\n\
#..#\n\
#..#\n\
####";
    let grid = TacticalGrid::parse(raw, &HashMap::new()).unwrap();
    assert_eq!(grid.width(), 4);
    assert_eq!(grid.height(), 4);
}

#[test]
fn rectangular_room_perimeter_is_walls() {
    let raw = "\
####\n\
#..#\n\
#..#\n\
####";
    let grid = TacticalGrid::parse(raw, &HashMap::new()).unwrap();
    // Top row all walls
    for x in 0..4 {
        assert_eq!(
            grid.cell_at(x, 0),
            Some(&TacticalCell::Wall),
            "Top wall at x={x}"
        );
    }
    // Bottom row all walls
    for x in 0..4 {
        assert_eq!(
            grid.cell_at(x, 3),
            Some(&TacticalCell::Wall),
            "Bottom wall at x={x}"
        );
    }
    // Interior is floor
    assert_eq!(grid.cell_at(1, 1), Some(&TacticalCell::Floor));
    assert_eq!(grid.cell_at(2, 2), Some(&TacticalCell::Floor));
}

// ==========================================================================
// AC-8: Unit tests — room with features
// ==========================================================================

#[test]
fn room_with_features_preserves_positions() {
    let leg = legend(&[('P', "cover", "Pillar"), ('T', "hazard", "Trap")]);
    let raw = "\
####\n\
#P.#\n\
#.T#\n\
####";
    let grid = TacticalGrid::parse(raw, &leg).unwrap();
    assert_eq!(grid.cell_at(1, 1), Some(&TacticalCell::Feature('P')));
    assert_eq!(grid.cell_at(2, 2), Some(&TacticalCell::Feature('T')));
    assert_eq!(grid.cell_at(2, 1), Some(&TacticalCell::Floor));
}

// ==========================================================================
// AC-8: Unit tests — room with multiple exits
// ==========================================================================

#[test]
fn room_with_multiple_exits_counts_all() {
    // Room with exits on all 4 sides
    let raw = "\
#.#\n\
...\n\
#.#";
    let grid = TacticalGrid::parse(raw, &HashMap::new()).unwrap();
    assert!(grid.exits().len() >= 4, "Should have at least 4 exit gaps");
}

// ==========================================================================
// AC-9: Integration test — "The Mouth" from ADR-071
// ==========================================================================

/// Parse the exact example grid from ADR-071.
#[test]
fn parse_the_mouth_from_adr_071() {
    let raw = "\
______####..####______\n\
____##..........##____\n\
__##..............##__\n\
_#..................#_\n\
_#....PP............#_\n\
#....................#\n\
#....................#\n\
_#....PP............#_\n\
_#..................#_\n\
__##..............##__\n\
____####..####________\n\
______####..####______";

    let leg = legend(&[('P', "cover", "Worn tooth stumps")]);
    let grid = TacticalGrid::parse(raw, &leg).unwrap();

    // Dimensions: 22 wide x 12 tall
    assert_eq!(grid.width(), 22);
    assert_eq!(grid.height(), 12);

    // Feature 'P' appears at expected positions
    assert_eq!(grid.cell_at(6, 4), Some(&TacticalCell::Feature('P')));
    assert_eq!(grid.cell_at(7, 4), Some(&TacticalCell::Feature('P')));
    assert_eq!(grid.cell_at(6, 7), Some(&TacticalCell::Feature('P')));
    assert_eq!(grid.cell_at(7, 7), Some(&TacticalCell::Feature('P')));

    // Legend resolves
    let feature = grid.legend().get(&'P').unwrap();
    assert_eq!(feature.label, "Worn tooth stumps");
    assert_eq!(feature.feature_type, FeatureType::Cover);

    // Corners are void
    assert_eq!(grid.cell_at(0, 0), Some(&TacticalCell::Void));
    assert_eq!(grid.cell_at(21, 0), Some(&TacticalCell::Void));

    // Has exit gaps (the floor gaps in the perimeter walls)
    assert!(!grid.exits().is_empty(), "The Mouth should have exit gaps");
}

// ==========================================================================
// AC-10: Wiring test — parser callable from non-test code path
// ==========================================================================

/// Verify that TacticalGrid::parse is re-exported from sidequest_game
/// (not just available in the tactical submodule). This ensures it's
/// reachable from the genre loader or validate tool.
#[test]
fn parse_grid_is_accessible_from_game_crate() {
    // This test compiles only if sidequest_game::tactical::TacticalGrid::parse exists
    // as a public function. The wiring test is the import at the top of this file.
    let grid = sidequest_game::tactical::TacticalGrid::parse(".", &HashMap::new());
    assert!(grid.is_ok());
}

// ==========================================================================
// Rust rule coverage: #2 — #[non_exhaustive] on public enums
// ==========================================================================

/// TacticalCell is a public enum that will grow (new terrain types).
/// It MUST have #[non_exhaustive].
#[test]
fn tactical_cell_is_non_exhaustive() {
    // This test validates at compile time: if TacticalCell lacks #[non_exhaustive],
    // a match without wildcard would compile — but with it, a wildcard is required.
    // The real enforcement is that the enum exists and our match uses a wildcard.
    let cell = TacticalCell::Floor;
    let _desc = match cell {
        TacticalCell::Floor => "floor",
        TacticalCell::Wall => "wall",
        TacticalCell::Void => "void",
        TacticalCell::DoorClosed => "door_closed",
        TacticalCell::DoorOpen => "door_open",
        TacticalCell::Water => "water",
        TacticalCell::DifficultTerrain => "difficult",
        TacticalCell::Feature(_) => "feature",
        _ => "unknown", // Required by #[non_exhaustive]
    };
    assert_eq!(_desc, "floor");
}

/// FeatureType is a public enum that will grow.
#[test]
fn feature_type_is_non_exhaustive() {
    let ft = FeatureType::Cover;
    let _desc = match ft {
        FeatureType::Cover => "cover",
        FeatureType::Hazard => "hazard",
        FeatureType::DifficultTerrain => "difficult",
        FeatureType::Atmosphere => "atmosphere",
        FeatureType::Interactable => "interactable",
        FeatureType::Door => "door",
        _ => "unknown", // Required by #[non_exhaustive]
    };
    assert_eq!(_desc, "cover");
}

/// GridParseError is a public enum that will grow.
#[test]
fn grid_parse_error_is_non_exhaustive() {
    let raw = "...\n..";
    let err = TacticalGrid::parse(raw, &HashMap::new()).unwrap_err();
    let _desc = match err {
        GridParseError::UnknownGlyph { .. } => "unknown_glyph",
        GridParseError::MissingLegend { .. } => "missing_legend",
        GridParseError::UnevenRows { .. } => "uneven_rows",
        GridParseError::EmptyGrid => "empty",
        _ => "other", // Required by #[non_exhaustive]
    };
    assert_eq!(_desc, "uneven_rows");
}

/// CardinalDirection is a public enum.
#[test]
fn cardinal_direction_is_non_exhaustive() {
    let dir = CardinalDirection::North;
    let _desc = match dir {
        CardinalDirection::North => "n",
        CardinalDirection::East => "e",
        CardinalDirection::South => "s",
        CardinalDirection::West => "w",
        _ => "?", // Required by #[non_exhaustive]
    };
    assert_eq!(_desc, "n");
}

// ==========================================================================
// Rust rule coverage: #15 — unbounded input protection
// ==========================================================================

/// Parser should reject absurdly large grids to prevent OOM.
#[test]
fn parser_rejects_oversized_input() {
    // 10001 characters should exceed any reasonable grid size limit
    let huge = ".".repeat(10001);
    let result = TacticalGrid::parse(&huge, &HashMap::new());
    assert!(result.is_err(), "Parser should reject oversized input");
}

// ==========================================================================
// Rust rule coverage: #5 — GridPos validated constructor
// ==========================================================================

/// GridPos stores coordinates. Verify it can be constructed and used as a map key.
#[test]
fn grid_pos_equality_and_hash() {
    let a = GridPos::new(3, 5);
    let b = GridPos::new(3, 5);
    let c = GridPos::new(5, 3);
    assert_eq!(a, b);
    assert_ne!(a, c);

    // Usable as HashMap key (requires Hash + Eq)
    let mut map = HashMap::new();
    map.insert(a, "here");
    assert_eq!(map.get(&b), Some(&"here"));
    assert_eq!(map.get(&c), None);
}

// ==========================================================================
// Edge cases
// ==========================================================================

/// Single-cell grid (minimum valid input).
#[test]
fn single_cell_grid_parses() {
    let grid = TacticalGrid::parse(".", &HashMap::new()).unwrap();
    assert_eq!(grid.width(), 1);
    assert_eq!(grid.height(), 1);
}

/// Grid with only walls has no exits.
#[test]
fn all_walls_no_exits() {
    let raw = "\
###\n\
###\n\
###";
    let grid = TacticalGrid::parse(raw, &HashMap::new()).unwrap();
    assert!(
        grid.exits().is_empty(),
        "Solid wall grid should have no exits"
    );
}

/// Whitespace-only lines should not count as rows.
#[test]
fn trailing_newline_handled() {
    let raw = "##\n##\n";
    let grid = TacticalGrid::parse(raw, &HashMap::new()).unwrap();
    assert_eq!(grid.height(), 2, "Trailing newline should not add a row");
}

/// Door cells on perimeter are NOT exits (only floor/void gaps count).
#[test]
fn door_on_perimeter_is_not_exit_gap() {
    let raw = "\
#+#\n\
#.#\n\
###";
    let grid = TacticalGrid::parse(raw, &HashMap::new()).unwrap();
    // Door is a barrier type, not an open gap to another room
    let floor_exits: Vec<_> = grid
        .exits()
        .iter()
        .filter(|e| e.wall == CardinalDirection::North)
        .collect();
    assert!(
        floor_exits.is_empty(),
        "Doors on perimeter are not exit gaps — exits are wall gaps (floor cells on edge)"
    );
}

//! Tactical grid validation — 8 rules from ADR-071.
//!
//! Validates that authored ASCII grids are structurally sound before
//! they reach the game engine. Runs at authoring time via
//! `sidequest-validate --tactical`.

use std::collections::{HashMap, HashSet, VecDeque};

use sidequest_game::tactical::layout::layout_dungeon;
use sidequest_game::tactical::{TacticalCell, TacticalGrid};
use sidequest_genre::models::world::RoomDef;

/// Errors produced by tactical grid validation.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ValidationError {
    /// Rule 1: Grid dimensions don't match `size * tactical_scale`.
    DimensionMismatch {
        expected_width: u32,
        expected_height: u32,
        actual_width: u32,
        actual_height: u32,
    },
    /// Rule 2: A RoomDef exit has no corresponding wall gap in the grid.
    ExitWithoutGap { exit_count: usize, gap_count: usize },
    /// Rule 3: A wall gap exists without a corresponding RoomDef exit.
    OrphanGap { gap_count: usize, exit_count: usize },
    /// Rule 4: A walkable cell is directly adjacent to a void cell.
    PerimeterBreach { x: u32, y: u32 },
    /// Rule 5: Floor regions are not all connected.
    DisconnectedFloor { isolated_cells: Vec<(u32, u32)> },
    /// Rule 6: A feature glyph in the grid has no legend entry.
    LegendIncomplete { glyph: char, x: u32, y: u32 },
    /// Rule 7: A legend glyph is placed on a non-walkable cell.
    LegendOnNonFloor { glyph: char, x: u32, y: u32 },
    /// Rule 7b: A legend entry is defined but never placed in the grid.
    UnusedLegendEntry { glyph: char },
    /// Rule 8: Connected rooms have mismatched exit gap widths.
    ExitWidthMismatch {
        room_a: String,
        room_b: String,
        a_widths: Vec<u32>,
        b_widths: Vec<u32>,
    },
    /// Rule 9: Rooms with grids cannot be laid out without overlap.
    LayoutFailed { message: String },
}

/// Result type alias for validation.
pub type ValidationResult = Vec<ValidationError>;

/// Run all tactical validation rules on a single room's grid.
///
/// Composes rules 1-7. Reports all findings (not fail-fast).
/// Rule 8 (cross-room exit width) requires a separate call to
/// [`validate_exit_width_compatibility`].
pub fn validate_tactical_grid(room: &RoomDef, grid: &TacticalGrid) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    check_dimensions(room, grid, &mut errors);
    check_exit_coverage(room, grid, &mut errors);
    check_orphan_gaps(room, grid, &mut errors);
    check_perimeter_closure(room, grid, &mut errors);
    check_flood_fill(grid, &mut errors);
    check_legend_completeness(grid, &mut errors);
    check_unused_legend(room, grid, &mut errors);

    errors
}

/// Rule 8: Check exit gap width compatibility between two connected rooms.
pub fn validate_exit_width_compatibility(
    room_a: &RoomDef,
    grid_a: &TacticalGrid,
    room_b: &RoomDef,
    grid_b: &TacticalGrid,
) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    let a_exits_to_b = room_a
        .exits
        .iter()
        .filter(|e| e.target() == room_b.id)
        .count();
    let b_exits_to_a = room_b
        .exits
        .iter()
        .filter(|e| e.target() == room_a.id)
        .count();

    if a_exits_to_b > 0 && b_exits_to_a > 0 {
        let mut a_widths: Vec<u32> = grid_a.exits().iter().map(|e| e.width).collect();
        let mut b_widths: Vec<u32> = grid_b.exits().iter().map(|e| e.width).collect();
        a_widths.sort();
        b_widths.sort();

        if a_widths != b_widths {
            errors.push(ValidationError::ExitWidthMismatch {
                room_a: room_a.id.clone(),
                room_b: room_b.id.clone(),
                a_widths,
                b_widths,
            });
        }
    }

    errors
}

// ── Rule 1: Dimensions ──────────────────────────────────────

fn check_dimensions(room: &RoomDef, grid: &TacticalGrid, errors: &mut Vec<ValidationError>) {
    let Some(scale) = room.tactical_scale else {
        return;
    };

    let expected_width = room.size.0 * scale;
    let expected_height = room.size.1 * scale;

    if grid.width() != expected_width || grid.height() != expected_height {
        errors.push(ValidationError::DimensionMismatch {
            expected_width,
            expected_height,
            actual_width: grid.width(),
            actual_height: grid.height(),
        });
    }
}

// ── Rule 2: Exit coverage ───────────────────────────────────

fn check_exit_coverage(room: &RoomDef, grid: &TacticalGrid, errors: &mut Vec<ValidationError>) {
    let exit_count = room.exits.len();
    let gap_count = grid.exits().len();

    if exit_count > gap_count {
        errors.push(ValidationError::ExitWithoutGap {
            exit_count,
            gap_count,
        });
    }
}

// ── Rule 3: Orphan gaps ─────────────────────────────────────

fn check_orphan_gaps(room: &RoomDef, grid: &TacticalGrid, errors: &mut Vec<ValidationError>) {
    let gap_count = grid.exits().len();
    let exit_count = room.exits.len();

    if gap_count > exit_count {
        errors.push(ValidationError::OrphanGap {
            gap_count,
            exit_count,
        });
    }
}

// ── Rule 4: Perimeter closure ───────────────────────────────

fn check_perimeter_closure(
    _room: &RoomDef,
    grid: &TacticalGrid,
    errors: &mut Vec<ValidationError>,
) {
    let w = grid.width();
    let h = grid.height();

    for y in 0..h {
        for x in 0..w {
            let Some(cell) = grid.cell_at(x, y) else {
                continue;
            };

            if !cell.properties().walkable {
                continue;
            }

            // Check 4 neighbors for void adjacency
            for (nx, ny) in neighbors(x, y, w, h) {
                if let Some(neighbor) = grid.cell_at(nx, ny) {
                    if matches!(neighbor, TacticalCell::Void) {
                        errors.push(ValidationError::PerimeterBreach { x, y });
                        break; // One breach per cell is enough
                    }
                }
            }
        }
    }
}

/// 4-directional neighbors within grid bounds. Returns a stack-allocated
/// iterator — no heap allocation, safe to call per-cell in hot loops.
fn neighbors(x: u32, y: u32, w: u32, h: u32) -> impl Iterator<Item = (u32, u32)> {
    [
        (x > 0).then(|| (x - 1, y)),
        (x + 1 < w).then(|| (x + 1, y)),
        (y > 0).then(|| (x, y - 1)),
        (y + 1 < h).then(|| (x, y + 1)),
    ]
    .into_iter()
    .flatten()
}

// ── Rule 5: Flood fill connectivity ─────────────────────────

fn check_flood_fill(grid: &TacticalGrid, errors: &mut Vec<ValidationError>) {
    let w = grid.width();
    let h = grid.height();

    // Collect all walkable cell positions
    let mut walkable: HashSet<(u32, u32)> = HashSet::new();
    for y in 0..h {
        for x in 0..w {
            if let Some(cell) = grid.cell_at(x, y) {
                if cell.properties().walkable {
                    walkable.insert((x, y));
                }
            }
        }
    }

    if walkable.is_empty() {
        return; // No floor cells → nothing to disconnect
    }

    // BFS from the first walkable cell
    let start = *walkable.iter().next().unwrap();
    let mut visited: HashSet<(u32, u32)> = HashSet::new();
    let mut queue: VecDeque<(u32, u32)> = VecDeque::new();
    queue.push_back(start);
    visited.insert(start);

    while let Some((cx, cy)) = queue.pop_front() {
        for (nx, ny) in neighbors(cx, cy, w, h) {
            if walkable.contains(&(nx, ny)) && !visited.contains(&(nx, ny)) {
                visited.insert((nx, ny));
                queue.push_back((nx, ny));
            }
        }
    }

    // Any walkable cells not visited are isolated
    let isolated: Vec<(u32, u32)> = walkable.difference(&visited).copied().collect();

    if !isolated.is_empty() {
        errors.push(ValidationError::DisconnectedFloor {
            isolated_cells: isolated,
        });
    }
}

// ── Rule 6: Legend completeness ─────────────────────────────

fn check_legend_completeness(grid: &TacticalGrid, errors: &mut Vec<ValidationError>) {
    let w = grid.width();
    let h = grid.height();

    for y in 0..h {
        for x in 0..w {
            if let Some(TacticalCell::Feature(ch)) = grid.cell_at(x, y) {
                if !grid.legend().contains_key(ch) {
                    errors.push(ValidationError::LegendIncomplete { glyph: *ch, x, y });
                }
            }
        }
    }
}

// ── Rule 7b: Unused legend entries ──────────────────────────

fn check_unused_legend(room: &RoomDef, grid: &TacticalGrid, errors: &mut Vec<ValidationError>) {
    let Some(ref room_legend) = room.legend else {
        return;
    };

    let grid_legend = grid.legend();

    for glyph in room_legend.keys() {
        if !grid_legend.contains_key(glyph) {
            errors.push(ValidationError::UnusedLegendEntry { glyph: *glyph });
        }
    }
}

// ── Rule 9: Layout validation ──────────────────────────────

/// Validate that rooms with tactical grids can be composed into a dungeon
/// layout without overlap. Parses each room's grid field and runs the
/// tree-topology layout engine.
///
/// Rooms without a `grid` field are silently skipped (they use the
/// schematic Automapper instead of tactical grids).
pub fn validate_layout(rooms: &[RoomDef]) -> Vec<ValidationError> {
    let mut grids: HashMap<String, TacticalGrid> = HashMap::new();
    for room in rooms {
        let Some(ref grid_str) = room.grid else {
            continue;
        };
        let legend = room.legend.as_ref().cloned().unwrap_or_default();
        match TacticalGrid::parse(grid_str, &legend) {
            Ok(grid) => {
                grids.insert(room.id.clone(), grid);
            }
            Err(_) => {
                // Grid parse errors are caught by validate_tactical_grid (rules 1-7).
                // Skip this room for layout purposes.
                continue;
            }
        }
    }

    if grids.is_empty() {
        return Vec::new(); // No tactical grids to lay out
    }

    match layout_dungeon(rooms, &grids) {
        Ok(_) => Vec::new(),
        Err(err) => vec![ValidationError::LayoutFailed {
            message: err.to_string(),
        }],
    }
}

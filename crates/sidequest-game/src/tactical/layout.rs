//! Dungeon layout engine — shared-wall tree topology placement (ADR-071).
//!
//! Composes individually parsed room grids into a dungeon map. Adjacent rooms
//! share wall segments at exit gaps — one wall, not two. BFS from the entrance
//! room places rooms in global coordinates.
//!
//! Cycle handling (jaquayed layouts) is deferred to story 29-7.

use std::collections::{HashMap, HashSet, VecDeque};

use super::{CardinalDirection, ExitGap, GridPos, TacticalCell, TacticalGrid};
use sidequest_genre::models::world::RoomDef;

// ═══════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════

/// A room placed at a specific offset in the global dungeon grid.
#[derive(Debug, Clone)]
pub struct PlacedRoom {
    room_id: String,
    offset_x: i32,
    offset_y: i32,
    grid: TacticalGrid,
}

impl PlacedRoom {
    /// Create a new placed room at the given global offset.
    pub fn new(room_id: String, offset_x: i32, offset_y: i32, grid: TacticalGrid) -> Self {
        Self {
            room_id,
            offset_x,
            offset_y,
            grid,
        }
    }

    /// Room identifier.
    pub fn room_id(&self) -> &str {
        &self.room_id
    }

    /// X offset in global coordinates.
    pub fn offset_x(&self) -> i32 {
        self.offset_x
    }

    /// Y offset in global coordinates.
    pub fn offset_y(&self) -> i32 {
        self.offset_y
    }
}

/// The composed dungeon layout — all rooms positioned in global coordinates.
#[derive(Debug, Clone)]
pub struct DungeonLayout {
    rooms: Vec<PlacedRoom>,
    width: u32,
    height: u32,
}

impl DungeonLayout {
    /// Placed rooms with their global offsets.
    pub fn rooms(&self) -> &[PlacedRoom] {
        &self.rooms
    }

    /// Total width spanning all placed rooms.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Total height spanning all placed rooms.
    pub fn height(&self) -> u32 {
        self.height
    }
}

/// Errors during dungeon layout.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum LayoutError {
    /// Two rooms overlap at non-void cells that aren't shared wall boundaries.
    Overlap {
        /// First overlapping room.
        room_a: String,
        /// Second overlapping room.
        room_b: String,
        /// Global positions where non-void cells collide.
        cells: Vec<GridPos>,
    },
    /// No room with `room_type == "entrance"` found.
    NoEntrance,
}

impl std::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LayoutError::Overlap {
                room_a,
                room_b,
                cells,
            } => {
                write!(
                    f,
                    "overlap collision between rooms '{}' and '{}' at {} cells",
                    room_a,
                    room_b,
                    cells.len()
                )
            }
            LayoutError::NoEntrance => write!(f, "no entrance room found in room list"),
        }
    }
}

impl std::error::Error for LayoutError {}

// ═══════════════════════════════════════════════════════════════════
// Public API
// ═══════════════════════════════════════════════════════════════════

/// Compute the offset for room B such that the exit gaps of A and B align
/// at a shared wall boundary.
///
/// `a` is the already-placed room. `a_exit` is the exit gap on A's wall
/// that faces B. `b_grid` is B's parsed grid. `b_exit` is the exit gap
/// on B's wall that faces A (must be on the opposite wall from `a_exit`).
///
/// Returns `(offset_x, offset_y)` for room B in global coordinates.
pub fn align_rooms(
    a: &PlacedRoom,
    a_exit: &ExitGap,
    b_grid: &TacticalGrid,
    b_exit: &ExitGap,
) -> (i32, i32) {
    match a_exit.wall {
        CardinalDirection::South => {
            // A's south wall → B's north wall (shared horizontal row)
            let by = a.offset_y + (a.grid.height() as i32 - 1);
            let bx = a.offset_x + a_exit.cells[0] as i32 - b_exit.cells[0] as i32;
            (bx, by)
        }
        CardinalDirection::North => {
            // A's north wall → B's south wall
            let by = a.offset_y - (b_grid.height() as i32 - 1);
            let bx = a.offset_x + a_exit.cells[0] as i32 - b_exit.cells[0] as i32;
            (bx, by)
        }
        CardinalDirection::East => {
            // A's east wall → B's west wall (shared vertical column)
            let bx = a.offset_x + (a.grid.width() as i32 - 1);
            let by = a.offset_y + a_exit.cells[0] as i32 - b_exit.cells[0] as i32;
            (bx, by)
        }
        CardinalDirection::West => {
            // A's west wall → B's east wall
            let bx = a.offset_x - (b_grid.width() as i32 - 1);
            let by = a.offset_y + a_exit.cells[0] as i32 - b_exit.cells[0] as i32;
            (bx, by)
        }
    }
}

/// Check for non-void cell overlaps between already-placed rooms and a candidate.
///
/// Returns global positions where both an existing placed room and the candidate
/// have non-void cells. Void cells are excluded.
pub fn check_overlap(placed: &[PlacedRoom], candidate: &PlacedRoom) -> Vec<GridPos> {
    let mut collisions = Vec::new();

    // Build occupancy set from candidate's non-void cells
    let mut candidate_cells: HashMap<(i32, i32), &TacticalCell> = HashMap::new();
    for y in 0..candidate.grid.height() {
        for x in 0..candidate.grid.width() {
            if let Some(cell) = candidate.grid.cell_at(x, y) {
                if *cell != TacticalCell::Void {
                    let gx = candidate.offset_x + x as i32;
                    let gy = candidate.offset_y + y as i32;
                    candidate_cells.insert((gx, gy), cell);
                }
            }
        }
    }

    // Check each placed room's non-void cells against candidate
    for room in placed {
        for y in 0..room.grid.height() {
            for x in 0..room.grid.width() {
                if let Some(cell) = room.grid.cell_at(x, y) {
                    if *cell != TacticalCell::Void {
                        let gx = room.offset_x + x as i32;
                        let gy = room.offset_y + y as i32;
                        if candidate_cells.contains_key(&(gx, gy)) {
                            // Convert to GridPos (u32). Negative coords get clamped to 0.
                            let px = u32::try_from(gx).unwrap_or(0);
                            let py = u32::try_from(gy).unwrap_or(0);
                            collisions.push(GridPos::new(px, py));
                        }
                    }
                }
            }
        }
    }

    collisions
}

/// Lay out a dungeon from room definitions using BFS tree traversal.
///
/// Starts from the entrance room (room_type == "entrance"), placed at origin (0, 0).
/// For each connected neighbor, finds matching exit gaps on opposite walls, computes
/// shared-wall alignment, and checks for overlap. Rooms without a grid in `grids`
/// are skipped.
///
/// Returns a `DungeonLayout` with all reachable rooms positioned, or a `LayoutError`
/// if placement fails.
pub fn layout_tree(
    rooms: &[RoomDef],
    grids: &HashMap<String, TacticalGrid>,
) -> Result<DungeonLayout, LayoutError> {
    if rooms.is_empty() {
        return Ok(DungeonLayout {
            rooms: Vec::new(),
            width: 0,
            height: 0,
        });
    }

    // Find entrance room
    let entrance = rooms
        .iter()
        .find(|r| r.room_type == "entrance")
        .ok_or(LayoutError::NoEntrance)?;

    let entrance_grid = match grids.get(&entrance.id) {
        Some(g) => g,
        None => {
            return Ok(DungeonLayout {
                rooms: Vec::new(),
                width: 0,
                height: 0,
            })
        }
    };

    // Build adjacency map: room_id -> [(target_id, exit_index)]
    let room_map: HashMap<&str, &RoomDef> = rooms.iter().map(|r| (r.id.as_str(), r)).collect();

    // Track placed rooms and used exit gap indices per room
    let mut placed: Vec<PlacedRoom> = Vec::new();
    let mut placed_ids: HashSet<String> = HashSet::new();
    let mut used_gaps: HashMap<String, HashSet<usize>> = HashMap::new();

    // Place entrance at origin
    placed.push(PlacedRoom::new(
        entrance.id.clone(),
        0,
        0,
        entrance_grid.clone(),
    ));
    placed_ids.insert(entrance.id.clone());

    // BFS queue
    let mut queue: VecDeque<String> = VecDeque::new();
    queue.push_back(entrance.id.clone());

    while let Some(current_id) = queue.pop_front() {
        let current_room = match room_map.get(current_id.as_str()) {
            Some(r) => r,
            None => continue,
        };

        // For each exit from current room
        for exit in &current_room.exits {
            let target_id = exit.target();

            // Skip if already placed or no grid available
            if placed_ids.contains(target_id) {
                continue;
            }
            let target_grid = match grids.get(target_id) {
                Some(g) => g,
                None => continue,
            };

            // Find the current room's PlacedRoom (index for borrow checker)
            let current_placed_idx = placed
                .iter()
                .position(|p| p.room_id == current_id)
                .expect("current room must be placed");

            let current_grid = match grids.get(current_id.as_str()) {
                Some(g) => g,
                None => continue,
            };

            // Try all opposite-wall gap pairings
            let mut placement_found = false;
            let current_used = used_gaps.entry(current_id.clone()).or_default().clone();
            let target_used = used_gaps.entry(target_id.to_string()).or_default().clone();

            for (gi_a, gap_a) in current_grid.exits().iter().enumerate() {
                if current_used.contains(&gi_a) {
                    continue;
                }
                for (gi_b, gap_b) in target_grid.exits().iter().enumerate() {
                    if target_used.contains(&gi_b) {
                        continue;
                    }
                    // Must be opposite walls
                    if !is_opposite(gap_a.wall, gap_b.wall) {
                        continue;
                    }

                    // Compute alignment
                    let (bx, by) =
                        align_rooms(&placed[current_placed_idx], gap_a, target_grid, gap_b);
                    let candidate =
                        PlacedRoom::new(target_id.to_string(), bx, by, target_grid.clone());

                    // Check overlap, excluding shared boundary positions
                    // where both rooms have the same cell type (Wall-Wall or
                    // Floor-Floor at exit gaps). Mismatched types at the boundary
                    // (e.g., Wall-Floor) are real collisions.
                    let all_overlaps = check_overlap(&placed, &candidate);
                    let shared_boundary = shared_boundary_positions(
                        &placed[current_placed_idx],
                        current_grid,
                        gap_a,
                        &candidate,
                        target_grid,
                        gap_b,
                    );
                    let real_overlaps: Vec<_> = all_overlaps
                        .into_iter()
                        .filter(|pos| {
                            let gx = pos.x() as i32;
                            let gy = pos.y() as i32;
                            if !shared_boundary.contains(&(gx, gy)) {
                                return true; // Not on boundary — real collision
                            }
                            // On boundary: only exclude if both cells are same type
                            let a_local_x = (gx - placed[current_placed_idx].offset_x) as u32;
                            let a_local_y = (gy - placed[current_placed_idx].offset_y) as u32;
                            let b_local_x = (gx - candidate.offset_x) as u32;
                            let b_local_y = (gy - candidate.offset_y) as u32;
                            let cell_a = current_grid.cell_at(a_local_x, a_local_y);
                            let cell_b = target_grid.cell_at(b_local_x, b_local_y);
                            cell_a != cell_b // Keep if mismatched (real problem)
                        })
                        .collect();

                    if real_overlaps.is_empty() {
                        // Valid placement
                        placed.push(candidate);
                        placed_ids.insert(target_id.to_string());
                        used_gaps
                            .entry(current_id.clone())
                            .or_default()
                            .insert(gi_a);
                        used_gaps
                            .entry(target_id.to_string())
                            .or_default()
                            .insert(gi_b);
                        queue.push_back(target_id.to_string());
                        placement_found = true;
                        break;
                    }
                }
                if placement_found {
                    break;
                }
            }

            if !placement_found {
                // Compute overlap for error reporting
                // Try first valid opposite-wall pair for error details
                let current_used: HashSet<usize> =
                    used_gaps.get(&current_id).cloned().unwrap_or_default();
                let target_used: HashSet<usize> =
                    used_gaps.get(target_id).cloned().unwrap_or_default();
                for (gi_a, gap_a) in current_grid.exits().iter().enumerate() {
                    if current_used.contains(&gi_a) {
                        continue;
                    }
                    for (gi_b, gap_b) in target_grid.exits().iter().enumerate() {
                        if target_used.contains(&gi_b) {
                            continue;
                        }
                        if !is_opposite(gap_a.wall, gap_b.wall) {
                            continue;
                        }
                        let (bx, by) =
                            align_rooms(&placed[current_placed_idx], gap_a, target_grid, gap_b);
                        let candidate =
                            PlacedRoom::new(target_id.to_string(), bx, by, target_grid.clone());
                        let overlaps = check_overlap(&placed, &candidate);
                        if !overlaps.is_empty() {
                            return Err(LayoutError::Overlap {
                                room_a: current_id.clone(),
                                room_b: target_id.to_string(),
                                cells: overlaps,
                            });
                        }
                    }
                }
                // No opposite-wall pair found at all — can't connect
                return Err(LayoutError::Overlap {
                    room_a: current_id.clone(),
                    room_b: target_id.to_string(),
                    cells: vec![],
                });
            }
        }
    }

    // Compute bounding box
    let (width, height) = if placed.is_empty() {
        (0, 0)
    } else {
        let min_x = placed.iter().map(|r| r.offset_x).min().unwrap();
        let max_x = placed
            .iter()
            .map(|r| r.offset_x + r.grid.width() as i32)
            .max()
            .unwrap();
        let min_y = placed.iter().map(|r| r.offset_y).min().unwrap();
        let max_y = placed
            .iter()
            .map(|r| r.offset_y + r.grid.height() as i32)
            .max()
            .unwrap();
        ((max_x - min_x) as u32, (max_y - min_y) as u32)
    };

    Ok(DungeonLayout {
        rooms: placed,
        width,
        height,
    })
}

// ═══════════════════════════════════════════════════════════════════
// Internal helpers
// ═══════════════════════════════════════════════════════════════════

/// Check if two cardinal directions are opposite.
fn is_opposite(a: CardinalDirection, b: CardinalDirection) -> bool {
    matches!(
        (a, b),
        (CardinalDirection::North, CardinalDirection::South)
            | (CardinalDirection::South, CardinalDirection::North)
            | (CardinalDirection::East, CardinalDirection::West)
            | (CardinalDirection::West, CardinalDirection::East)
    )
}

/// Compute the set of global positions that form the shared boundary between
/// two rooms connected via exit gaps.
fn shared_boundary_positions(
    a: &PlacedRoom,
    a_grid: &TacticalGrid,
    a_exit: &ExitGap,
    b: &PlacedRoom,
    b_grid: &TacticalGrid,
    _b_exit: &ExitGap,
) -> HashSet<(i32, i32)> {
    let mut boundary = HashSet::new();

    match a_exit.wall {
        CardinalDirection::South | CardinalDirection::North => {
            // Shared row: the row where both rooms' walls meet
            let shared_y = if a_exit.wall == CardinalDirection::South {
                a.offset_y + (a_grid.height() as i32 - 1)
            } else {
                a.offset_y
            };

            // The shared boundary spans the overlap of both rooms' x ranges at shared_y
            let a_x_start = a.offset_x;
            let a_x_end = a.offset_x + a_grid.width() as i32;
            let b_x_start = b.offset_x;
            let b_x_end = b.offset_x + b_grid.width() as i32;
            let x_start = a_x_start.max(b_x_start);
            let x_end = a_x_end.min(b_x_end);

            for x in x_start..x_end {
                boundary.insert((x, shared_y));
            }
        }
        CardinalDirection::East | CardinalDirection::West => {
            // Shared column
            let shared_x = if a_exit.wall == CardinalDirection::East {
                a.offset_x + (a_grid.width() as i32 - 1)
            } else {
                a.offset_x
            };

            let a_y_start = a.offset_y;
            let a_y_end = a.offset_y + a_grid.height() as i32;
            let b_y_start = b.offset_y;
            let b_y_end = b.offset_y + b_grid.height() as i32;
            let y_start = a_y_start.max(b_y_start);
            let y_end = a_y_end.min(b_y_end);

            for y in y_start..y_end {
                boundary.insert((shared_x, y));
            }
        }
    }

    boundary
}

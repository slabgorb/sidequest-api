//! Tactical entity types — TacticalEntity, EntitySize, Faction.
//!
//! Entities represent positioned objects on a tactical grid: player characters,
//! NPCs, and creatures. Each has a grid position, size (how many cells it
//! occupies), and faction (for visual distinction and targeting rules).

use serde::{Deserialize, Serialize};
use sidequest_protocol::TacticalEntityPayload;

use super::grid::{CardinalDirection, ExitGap, GridPos, TacticalGrid};

/// Size of an entity on the tactical grid, measured in cells per side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum EntitySize {
    /// 1×1 cell (humanoid, small creature).
    Medium,
    /// 2×2 cells (ogre, horse, large creature).
    Large,
    /// 3×3 cells (dragon, giant, huge creature).
    Huge,
}

impl EntitySize {
    /// Number of cells this entity spans per side.
    pub fn cell_span(&self) -> u32 {
        match self {
            EntitySize::Medium => 1,
            EntitySize::Large => 2,
            EntitySize::Huge => 3,
        }
    }
}

/// Faction of an entity — determines visual coloring and targeting rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Faction {
    /// Player character.
    Player,
    /// Enemy NPC or creature.
    Hostile,
    /// Non-combatant NPC.
    Neutral,
    /// Friendly NPC allied with players.
    Ally,
}

impl Faction {
    /// Wire-format name sent in protocol payloads.
    pub fn wire_name(&self) -> &'static str {
        match self {
            Faction::Player => "player",
            Faction::Hostile => "hostile",
            Faction::Neutral => "neutral",
            Faction::Ally => "ally",
        }
    }
}

/// An entity positioned on a tactical grid.
///
/// Private fields with getters, following the `GridPos` pattern.
/// Position is mutable (entities move); all other fields are immutable
/// after construction.
#[derive(Debug, Clone)]
pub struct TacticalEntity {
    id: String,
    name: String,
    position: GridPos,
    size: EntitySize,
    faction: Faction,
    icon: Option<String>,
}

impl TacticalEntity {
    /// Create a new tactical entity.
    pub fn new(
        id: String,
        name: String,
        position: GridPos,
        size: EntitySize,
        faction: Faction,
        icon: Option<String>,
    ) -> Self {
        Self {
            id,
            name,
            position,
            size,
            faction,
            icon,
        }
    }

    /// Place a PC at the entrance of a room, adjacent to the specified exit direction.
    ///
    /// Scans the grid for a walkable cell near the exit gap on the given wall.
    /// Falls back to the first walkable cell if no exit gap is found.
    pub fn place_pc_at_entrance(
        id: String,
        name: String,
        grid: &TacticalGrid,
        entrance_direction: &str,
    ) -> Self {
        let direction = match entrance_direction {
            "north" => Some(CardinalDirection::North),
            "south" => Some(CardinalDirection::South),
            "east" => Some(CardinalDirection::East),
            "west" => Some(CardinalDirection::West),
            _ => None,
        };

        // Find the exit gap on the specified wall
        let exit_gap = direction.and_then(|dir| {
            grid.exits().iter().find(|gap| gap.wall == dir)
        });

        let position = if let Some(gap) = exit_gap {
            find_walkable_near_gap(grid, gap)
        } else {
            find_first_walkable(grid)
        };

        Self::new(id, name, position, EntitySize::Medium, Faction::Player, None)
    }

    /// Unique entity identifier.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Display name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Current grid position.
    pub fn position(&self) -> GridPos {
        self.position
    }

    /// Entity size.
    pub fn size(&self) -> &EntitySize {
        &self.size
    }

    /// Entity faction.
    pub fn faction(&self) -> &Faction {
        &self.faction
    }

    /// Optional custom icon identifier.
    pub fn icon(&self) -> Option<&String> {
        self.icon.as_ref()
    }

    /// Update entity position (for movement).
    pub fn set_position(&mut self, position: GridPos) {
        self.position = position;
    }

    /// Convert to protocol payload for wire transmission.
    pub fn to_payload(&self) -> TacticalEntityPayload {
        TacticalEntityPayload {
            id: self.id.clone(),
            name: self.name.clone(),
            x: self.position.x(),
            y: self.position.y(),
            size: self.size.cell_span(),
            faction: self.faction.wire_name().to_string(),
        }
    }
}

/// Find a walkable cell adjacent to an exit gap, one row/column inward from the wall.
fn find_walkable_near_gap(grid: &TacticalGrid, gap: &ExitGap) -> GridPos {
    // Step one cell inward from the wall, centered on the gap
    let mid = gap.cells.len() / 2;
    let gap_cell = gap.cells.get(mid).copied().unwrap_or(0);

    let (x, y) = match gap.wall {
        CardinalDirection::North => (gap_cell, 1),
        CardinalDirection::South => (gap_cell, grid.height().saturating_sub(2)),
        CardinalDirection::West => (1, gap_cell),
        CardinalDirection::East => (grid.width().saturating_sub(2), gap_cell),
    };

    // Verify walkable, otherwise search nearby
    if is_walkable(grid, x, y) {
        return GridPos::new(x, y);
    }

    // Expand search around the target position
    for dx in 0..3u32 {
        for dy in 0..3u32 {
            let nx = x.wrapping_add(dx).min(grid.width().saturating_sub(1));
            let ny = y.wrapping_add(dy).min(grid.height().saturating_sub(1));
            if is_walkable(grid, nx, ny) {
                return GridPos::new(nx, ny);
            }
        }
    }

    find_first_walkable(grid)
}

/// Find the first walkable cell in the grid (top-left scan).
fn find_first_walkable(grid: &TacticalGrid) -> GridPos {
    for y in 0..grid.height() {
        for x in 0..grid.width() {
            if is_walkable(grid, x, y) {
                return GridPos::new(x, y);
            }
        }
    }
    // Absolute fallback — center of grid
    GridPos::new(grid.width() / 2, grid.height() / 2)
}

fn is_walkable(grid: &TacticalGrid, x: u32, y: u32) -> bool {
    grid.cell_at(x, y)
        .map(|cell| cell.properties().walkable)
        .unwrap_or(false)
}

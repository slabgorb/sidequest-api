//! Tactical grid types — TacticalCell, GridPos, FeatureDef, ExitGap, and TacticalGrid.
//!
//! These types represent the parsed output of an ASCII grid map (ADR-071).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Maximum input size for grid parsing (bytes). Prevents OOM on malicious input.
pub const MAX_GRID_INPUT_SIZE: usize = 10_000;

/// A single cell in a tactical grid.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum TacticalCell {
    /// Walkable floor (`.`).
    Floor,
    /// Impassable wall (`#`).
    Wall,
    /// Outside room bounds (`_`). Not walkable, not visible.
    Void,
    /// Closed door (`+`). Traversable but blocks line of sight.
    DoorClosed,
    /// Open door (`/`). Traversable and transparent.
    DoorOpen,
    /// Water (`~`). Walkable at increased movement cost.
    Water,
    /// Difficult terrain (`,`). Walkable at increased movement cost.
    DifficultTerrain,
    /// Named feature glyph (`A-Z`), resolved via legend.
    Feature(char),
}

/// Physical properties of a cell type, used for pathfinding and LOS.
#[derive(Debug, Clone, PartialEq)]
pub struct CellProperties {
    /// Whether entities can move through this cell.
    pub walkable: bool,
    /// Whether this cell blocks line of sight.
    pub blocks_los: bool,
    /// Movement cost multiplier (1.0 = normal, 2.0 = double).
    pub movement_cost: f64,
}

impl TacticalCell {
    /// Returns the physical properties of this cell type.
    #[allow(unreachable_patterns)] // _ arm required by #[non_exhaustive] for future variants
    pub fn properties(&self) -> CellProperties {
        match self {
            TacticalCell::Floor | TacticalCell::Feature(_) => CellProperties {
                walkable: true,
                blocks_los: false,
                movement_cost: 1.0,
            },
            TacticalCell::Wall => CellProperties {
                walkable: false,
                blocks_los: true,
                movement_cost: f64::INFINITY,
            },
            TacticalCell::Void => CellProperties {
                walkable: false,
                blocks_los: true,
                movement_cost: f64::INFINITY,
            },
            TacticalCell::DoorClosed => CellProperties {
                walkable: true,
                blocks_los: true,
                movement_cost: 1.0,
            },
            TacticalCell::DoorOpen => CellProperties {
                walkable: true,
                blocks_los: false,
                movement_cost: 1.0,
            },
            TacticalCell::Water => CellProperties {
                walkable: true,
                blocks_los: false,
                movement_cost: 2.0,
            },
            TacticalCell::DifficultTerrain => CellProperties {
                walkable: true,
                blocks_los: false,
                movement_cost: 2.0,
            },
            _ => CellProperties {
                walkable: false,
                blocks_los: true,
                movement_cost: f64::INFINITY,
            },
        }
    }
}

/// A position in the tactical grid (x, y coordinates).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GridPos {
    x: u32,
    y: u32,
}

impl GridPos {
    /// Create a new grid position.
    pub fn new(x: u32, y: u32) -> Self {
        Self { x, y }
    }

    /// X coordinate.
    pub fn x(&self) -> u32 {
        self.x
    }

    /// Y coordinate.
    pub fn y(&self) -> u32 {
        self.y
    }
}

/// The type of a feature placed in the grid via legend.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum FeatureType {
    /// Provides cover in combat.
    Cover,
    /// Hazardous area (traps, fire, etc.).
    Hazard,
    /// Difficult terrain (rubble, mud, etc.).
    DifficultTerrain,
    /// Atmospheric detail (no mechanical effect).
    Atmosphere,
    /// Interactive object (lever, chest, etc.).
    Interactable,
    /// Door feature.
    Door,
}

impl FeatureType {
    /// Parse a feature type from its string representation.
    pub fn from_str_name(s: &str) -> Option<Self> {
        match s {
            "cover" => Some(Self::Cover),
            "hazard" => Some(Self::Hazard),
            "difficult_terrain" => Some(Self::DifficultTerrain),
            "atmosphere" => Some(Self::Atmosphere),
            "interactable" => Some(Self::Interactable),
            "door" => Some(Self::Door),
            _ => None,
        }
    }

    /// Return the string representation of this feature type.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Cover => "cover",
            Self::Hazard => "hazard",
            Self::DifficultTerrain => "difficult_terrain",
            Self::Atmosphere => "atmosphere",
            Self::Interactable => "interactable",
            Self::Door => "door",
        }
    }
}

impl std::fmt::Display for FeatureType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A resolved feature definition from the legend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureDef {
    /// What kind of feature this is.
    pub feature_type: FeatureType,
    /// Human-readable label for UI/narration.
    pub label: String,
}

/// Cardinal direction for exit gap identification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum CardinalDirection {
    /// Top edge of the grid.
    North,
    /// Right edge of the grid.
    East,
    /// Bottom edge of the grid.
    South,
    /// Left edge of the grid.
    West,
}

/// A gap in the wall perimeter where an exit connects to another room.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExitGap {
    /// Which wall the exit is on.
    pub wall: CardinalDirection,
    /// Perimeter positions that form the gap (column indices for N/S, row indices for E/W).
    pub cells: Vec<u32>,
    /// Width of the gap in cells.
    pub width: u32,
}

/// Errors that can occur during grid parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum GridParseError {
    /// A glyph character was not recognized and not in the legend.
    UnknownGlyph {
        /// The unrecognized character.
        glyph: char,
        /// Column position (0-indexed).
        x: usize,
        /// Row position (0-indexed).
        y: usize,
    },
    /// An uppercase letter was found but has no legend entry.
    MissingLegend {
        /// The missing glyph.
        glyph: char,
        /// Column position.
        x: usize,
        /// Row position.
        y: usize,
    },
    /// Rows have inconsistent widths.
    UnevenRows {
        /// Width of the first row.
        expected_width: usize,
        /// Width of the offending row.
        actual_width: usize,
        /// Index of the offending row (0-indexed).
        row: usize,
    },
    /// The input grid is empty.
    EmptyGrid,
    /// The input exceeds the maximum allowed size.
    InputTooLarge {
        /// Size of the input in bytes.
        size: usize,
        /// Maximum allowed size.
        max: usize,
    },
}

impl std::fmt::Display for GridParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GridParseError::UnknownGlyph { glyph, x, y } => {
                write!(f, "unknown glyph '{}' at position ({}, {})", glyph, x, y)
            }
            GridParseError::MissingLegend { glyph, x, y } => {
                write!(f, "glyph '{}' at ({}, {}) not found in legend", glyph, x, y)
            }
            GridParseError::UnevenRows {
                expected_width,
                actual_width,
                row,
            } => {
                write!(
                    f,
                    "row {} has width {} but expected {}",
                    row, actual_width, expected_width
                )
            }
            GridParseError::EmptyGrid => write!(f, "grid input is empty"),
            GridParseError::InputTooLarge { size, max } => {
                write!(f, "input size {} exceeds maximum {}", size, max)
            }
        }
    }
}

impl std::error::Error for GridParseError {}

/// A parsed tactical grid with cells, dimensions, legend, and exits.
#[derive(Debug, Clone)]
pub struct TacticalGrid {
    width: u32,
    height: u32,
    cells: Vec<Vec<TacticalCell>>,
    legend: HashMap<char, FeatureDef>,
    exits: Vec<ExitGap>,
}

impl TacticalGrid {
    /// Parse an ASCII grid string into a TacticalGrid.
    ///
    /// The `legend` maps uppercase letter glyphs (A-Z) to their feature definitions.
    /// All other glyphs use the standard vocabulary from ADR-071.
    pub fn parse(
        raw: &str,
        legend: &HashMap<char, sidequest_genre::models::world::LegendEntry>,
    ) -> Result<Self, GridParseError> {
        crate::tactical::parser::parse_grid(raw, legend)
    }

    /// Get the cell at (x, y), or None if out of bounds.
    pub fn cell_at(&self, x: u32, y: u32) -> Option<&TacticalCell> {
        let row = self.cells.get(y as usize)?;
        row.get(x as usize)
    }

    /// Grid width in cells.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Grid height in cells.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Extracted exit gaps from the wall perimeter, ordered clockwise.
    pub fn exits(&self) -> &[ExitGap] {
        &self.exits
    }

    /// Resolved legend (glyph -> FeatureDef).
    pub fn legend(&self) -> &HashMap<char, FeatureDef> {
        &self.legend
    }

    /// All cells as a 2D grid (rows × columns).
    pub fn cells(&self) -> &[Vec<TacticalCell>] {
        &self.cells
    }

    /// Construct a TacticalGrid from pre-parsed components (used by parser).
    pub(crate) fn from_parts(
        width: u32,
        height: u32,
        cells: Vec<Vec<TacticalCell>>,
        legend: HashMap<char, FeatureDef>,
        exits: Vec<ExitGap>,
    ) -> Self {
        Self {
            width,
            height,
            cells,
            legend,
            exits,
        }
    }
}

//! ASCII grid parser — converts raw grid strings into TacticalGrid.
//!
//! Handles glyph resolution, legend lookup, row validation,
//! and exit extraction from wall perimeter gaps.

use std::collections::HashMap;

use sidequest_genre::models::world::LegendEntry;

use super::grid::{
    CardinalDirection, ExitGap, FeatureDef, FeatureType, GridParseError, TacticalCell,
    TacticalGrid, MAX_GRID_INPUT_SIZE,
};

/// Parse an ASCII grid string into a TacticalGrid.
///
/// Validates input size, row consistency, glyph vocabulary, and legend references.
/// Extracts exit gaps from the wall perimeter in clockwise order (N, E, S, W).
pub fn parse_grid(
    raw: &str,
    legend: &HashMap<char, LegendEntry>,
) -> Result<TacticalGrid, GridParseError> {
    // Size limit check (CWE-400)
    if raw.len() > MAX_GRID_INPUT_SIZE {
        return Err(GridParseError::InputTooLarge {
            size: raw.len(),
            max: MAX_GRID_INPUT_SIZE,
        });
    }

    // Split into rows, filtering trailing empty lines
    let rows: Vec<&str> = raw.lines().collect();
    let rows: Vec<&str> = {
        let mut r = rows;
        while r.last().is_some_and(|l| l.is_empty()) {
            r.pop();
        }
        r
    };

    if rows.is_empty() {
        return Err(GridParseError::EmptyGrid);
    }

    let expected_width = rows[0].len();

    // Validate uniform row widths
    for (i, row) in rows.iter().enumerate() {
        if row.len() != expected_width {
            return Err(GridParseError::UnevenRows {
                expected_width,
                actual_width: row.len(),
                row: i,
            });
        }
    }

    // Parse cells and resolve legend
    let mut cells: Vec<Vec<TacticalCell>> = Vec::with_capacity(rows.len());
    let mut resolved_legend: HashMap<char, FeatureDef> = HashMap::new();

    for (y, row) in rows.iter().enumerate() {
        let mut cell_row = Vec::with_capacity(expected_width);
        for (x, ch) in row.chars().enumerate() {
            let cell = match ch {
                '.' => TacticalCell::Floor,
                '#' => TacticalCell::Wall,
                '_' => TacticalCell::Void,
                '+' => TacticalCell::DoorClosed,
                '/' => TacticalCell::DoorOpen,
                '~' => TacticalCell::Water,
                ',' => TacticalCell::DifficultTerrain,
                'A'..='Z' => {
                    if let Some(entry) = legend.get(&ch) {
                        // Resolve legend entry to FeatureDef if not already cached
                        use std::collections::hash_map::Entry;
                        if let Entry::Vacant(slot) = resolved_legend.entry(ch) {
                            let feature_type =
                                FeatureType::from_str_name(&entry.r#type).ok_or(
                                    GridParseError::UnknownGlyph { glyph: ch, x, y },
                                )?;
                            slot.insert(FeatureDef {
                                feature_type,
                                label: entry.label.clone(),
                            });
                        }
                        TacticalCell::Feature(ch)
                    } else {
                        return Err(GridParseError::MissingLegend { glyph: ch, x, y });
                    }
                }
                _ => return Err(GridParseError::UnknownGlyph { glyph: ch, x, y }),
            };
            cell_row.push(cell);
        }
        cells.push(cell_row);
    }

    let width = expected_width as u32;
    let height = rows.len() as u32;

    // Extract exits from perimeter
    let exits = extract_exits(&cells, width, height);

    Ok(TacticalGrid::from_parts(
        width,
        height,
        cells,
        resolved_legend,
        exits,
    ))
}

/// Check if a cell type counts as an exit gap (floor cell on perimeter).
fn is_exit_cell(cell: &TacticalCell) -> bool {
    matches!(cell, TacticalCell::Floor)
}

/// Extract exit gaps from the grid perimeter in clockwise order.
///
/// Scans: North (left to right), East (top to bottom),
/// South (right to left), West (bottom to top).
fn extract_exits(cells: &[Vec<TacticalCell>], width: u32, height: u32) -> Vec<ExitGap> {
    let mut exits = Vec::new();

    if width == 0 || height == 0 {
        return exits;
    }

    // North edge (row 0, left to right)
    extract_edge_gaps(
        &cells[0],
        CardinalDirection::North,
        false,
        &mut exits,
    );

    // East edge (last column, top to bottom)
    if width > 0 {
        let east_cells: Vec<&TacticalCell> = (0..height as usize)
            .map(|y| &cells[y][(width - 1) as usize])
            .collect();
        extract_directional_gaps(&east_cells, CardinalDirection::East, false, &mut exits);
    }

    // South edge (last row, right to left — but positions stored left-to-right)
    if height > 0 {
        let south_row = &cells[(height - 1) as usize];
        // Scan right to left for clockwise ordering, but record actual positions
        extract_edge_gaps_reverse(south_row, CardinalDirection::South, &mut exits);
    }

    // West edge (first column, bottom to top)
    {
        let west_cells: Vec<&TacticalCell> = (0..height as usize)
            .map(|y| &cells[y][0])
            .collect();
        extract_directional_gaps_reverse(&west_cells, CardinalDirection::West, &mut exits);
    }

    exits
}

/// Extract contiguous floor gaps from a row of cells (left to right).
fn extract_edge_gaps(
    row: &[TacticalCell],
    wall: CardinalDirection,
    _reverse: bool,
    exits: &mut Vec<ExitGap>,
) {
    let mut gap_start: Option<usize> = None;

    for (i, cell) in row.iter().enumerate() {
        if is_exit_cell(cell) {
            if gap_start.is_none() {
                gap_start = Some(i);
            }
        } else if let Some(start) = gap_start {
            let cells: Vec<u32> = (start..i).map(|x| x as u32).collect();
            let width = cells.len() as u32;
            exits.push(ExitGap { wall, cells, width });
            gap_start = None;
        }
    }

    // Close any trailing gap
    if let Some(start) = gap_start {
        let cells: Vec<u32> = (start..row.len()).map(|x| x as u32).collect();
        let width = cells.len() as u32;
        exits.push(ExitGap { wall, cells, width });
    }
}

/// Extract contiguous floor gaps from a row, scanning right to left (south edge).
fn extract_edge_gaps_reverse(
    row: &[TacticalCell],
    wall: CardinalDirection,
    exits: &mut Vec<ExitGap>,
) {
    // Scan right to left to maintain clockwise ordering
    let mut gap_end: Option<usize> = None;

    for i in (0..row.len()).rev() {
        if is_exit_cell(&row[i]) {
            if gap_end.is_none() {
                gap_end = Some(i);
            }
        } else if let Some(end) = gap_end {
            let cells: Vec<u32> = (i + 1..=end).map(|x| x as u32).collect();
            let width = cells.len() as u32;
            exits.push(ExitGap { wall, cells, width });
            gap_end = None;
        }
    }

    if let Some(end) = gap_end {
        let cells: Vec<u32> = (0..=end).map(|x| x as u32).collect();
        let width = cells.len() as u32;
        exits.push(ExitGap { wall, cells, width });
    }
}

/// Extract contiguous floor gaps from a column-wise slice (top to bottom).
fn extract_directional_gaps(
    cells: &[&TacticalCell],
    wall: CardinalDirection,
    _reverse: bool,
    exits: &mut Vec<ExitGap>,
) {
    let mut gap_start: Option<usize> = None;

    for (i, cell) in cells.iter().enumerate() {
        if is_exit_cell(cell) {
            if gap_start.is_none() {
                gap_start = Some(i);
            }
        } else if let Some(start) = gap_start {
            let gap_cells: Vec<u32> = (start..i).map(|y| y as u32).collect();
            let width = gap_cells.len() as u32;
            exits.push(ExitGap {
                wall,
                cells: gap_cells,
                width,
            });
            gap_start = None;
        }
    }

    if let Some(start) = gap_start {
        let gap_cells: Vec<u32> = (start..cells.len()).map(|y| y as u32).collect();
        let width = gap_cells.len() as u32;
        exits.push(ExitGap {
            wall,
            cells: gap_cells,
            width,
        });
    }
}

/// Extract contiguous floor gaps from a column-wise slice (bottom to top for west edge).
fn extract_directional_gaps_reverse(
    cells: &[&TacticalCell],
    wall: CardinalDirection,
    exits: &mut Vec<ExitGap>,
) {
    let mut gap_end: Option<usize> = None;

    for i in (0..cells.len()).rev() {
        if is_exit_cell(cells[i]) {
            if gap_end.is_none() {
                gap_end = Some(i);
            }
        } else if let Some(end) = gap_end {
            let gap_cells: Vec<u32> = (i + 1..=end).map(|y| y as u32).collect();
            let width = gap_cells.len() as u32;
            exits.push(ExitGap {
                wall,
                cells: gap_cells,
                width,
            });
            gap_end = None;
        }
    }

    if let Some(end) = gap_end {
        let gap_cells: Vec<u32> = (0..=end).map(|y| y as u32).collect();
        let width = gap_cells.len() as u32;
        exits.push(ExitGap {
            wall,
            cells: gap_cells,
            width,
        });
    }
}

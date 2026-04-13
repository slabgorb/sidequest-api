//! Tactical entity placement tool (Story 29-11).
//!
//! Validates narrator tool calls for placing entities on the tactical grid.
//! The narrator calls `tactical_place(entity_id, x, y, size, faction)` during
//! narration; the script tool writes to the sidecar; this module validates the
//! result and produces a `TacticalPlaceResult` for `assemble_turn`.

use sidequest_protocol::TacticalEntityPayload;
use tracing::{info, warn};

/// A validated tactical placement result.
#[derive(Debug, Clone)]
pub struct TacticalPlaceResult {
    /// Unique entity identifier.
    pub entity_id: String,
    /// Grid x position (column).
    pub x: u32,
    /// Grid y position (row).
    pub y: u32,
    /// Size in cells per side (1=medium, 2=large, 3=huge).
    pub size: u32,
    /// Faction wire name ("player", "hostile", "neutral", "ally").
    pub faction: String,
}

impl TacticalPlaceResult {
    /// Convert to a protocol entity payload with the given display name.
    pub fn to_entity_payload(&self, name: &str) -> TacticalEntityPayload {
        TacticalEntityPayload {
            id: self.entity_id.clone(),
            name: name.to_string(),
            x: self.x,
            y: self.y,
            size: self.size,
            faction: self.faction.clone(),
        }
    }
}

/// An existing entity on the grid, used for overlap detection.
#[derive(Debug, Clone)]
pub struct PlacedEntity {
    /// Unique entity identifier.
    pub entity_id: String,
    /// Grid x position.
    pub x: u32,
    /// Grid y position.
    pub y: u32,
    /// Size in cells per side.
    pub size: u32,
}

/// Validate a tactical_place tool call.
///
/// Returns `Ok(TacticalPlaceResult)` if the placement is valid, or
/// `Err(String)` with a human-readable error message if invalid.
#[allow(clippy::too_many_arguments)] // Parameter list matches tool call fields; grouping would obscure the 1:1 mapping
#[tracing::instrument(
    name = "tool.tactical_place",
    skip(existing),
    fields(entity_id = %entity_id, x = x, y = y, size = %size_str, faction = %faction_str, valid, error_reason)
)]
pub fn validate_tactical_place(
    entity_id: &str,
    x: u32,
    y: u32,
    size_str: &str,
    faction_str: &str,
    grid_width: u32,
    grid_height: u32,
    existing: &[PlacedEntity],
) -> Result<TacticalPlaceResult, String> {
    // Validate entity_id is non-empty
    if entity_id.is_empty() {
        let reason = "entity_id must not be empty";
        tracing::Span::current().record("valid", false);
        tracing::Span::current().record("error_reason", reason);
        warn!(reason, "tactical_place validation failed");
        return Err(reason.to_string());
    }

    // Parse and validate size (case-insensitive)
    let size = match size_str.to_lowercase().as_str() {
        "medium" => 1u32,
        "large" => 2,
        "huge" => 3,
        _ => {
            let reason = format!(
                "invalid size '{}' — expected one of: medium, large, huge",
                size_str
            );
            tracing::Span::current().record("valid", false);
            tracing::Span::current().record("error_reason", reason.as_str());
            warn!(%reason, "tactical_place validation failed");
            return Err(reason);
        }
    };

    // Parse and validate faction (case-insensitive)
    let faction = match faction_str.to_lowercase().as_str() {
        "player" => "player",
        "hostile" => "hostile",
        "neutral" => "neutral",
        "ally" => "ally",
        _ => {
            let reason = format!(
                "invalid faction '{}' — expected one of: player, hostile, neutral, ally",
                faction_str
            );
            tracing::Span::current().record("valid", false);
            tracing::Span::current().record("error_reason", reason.as_str());
            warn!(%reason, "tactical_place validation failed");
            return Err(reason);
        }
    };

    // Bounds check: entity must fit entirely within grid
    if x + size > grid_width || y + size > grid_height {
        let reason = format!(
            "placement out of bounds: entity at ({},{}) with size {} extends past grid {}×{}",
            x, y, size, grid_width, grid_height
        );
        tracing::Span::current().record("valid", false);
        tracing::Span::current().record("error_reason", reason.as_str());
        warn!(%reason, "tactical_place validation failed");
        return Err(reason);
    }

    // Duplicate entity_id detection
    if existing.iter().any(|e| e.entity_id == entity_id) {
        let reason = format!("entity '{}' is already placed on the grid", entity_id);
        tracing::Span::current().record("valid", false);
        tracing::Span::current().record("error_reason", reason.as_str());
        warn!(%reason, "tactical_place validation failed");
        return Err(reason);
    }

    // Overlap detection: check if any cell in the new entity's footprint
    // is occupied by an existing entity's footprint
    for existing_entity in existing {
        if footprints_overlap(
            x,
            y,
            size,
            existing_entity.x,
            existing_entity.y,
            existing_entity.size,
        ) {
            let reason = format!(
                "placement overlaps with existing entity '{}' — cells occupied",
                existing_entity.entity_id
            );
            tracing::Span::current().record("valid", false);
            tracing::Span::current().record("error_reason", reason.as_str());
            warn!(%reason, entity_id = %existing_entity.entity_id, "tactical_place validation failed");
            return Err(reason);
        }
    }

    tracing::Span::current().record("valid", true);
    info!("tactical_place validated successfully");

    Ok(TacticalPlaceResult {
        entity_id: entity_id.to_string(),
        x,
        y,
        size,
        faction: faction.to_string(),
    })
}

/// Check if two axis-aligned square footprints overlap.
fn footprints_overlap(x1: u32, y1: u32, size1: u32, x2: u32, y2: u32, size2: u32) -> bool {
    // Two squares overlap if they overlap on both axes
    let x_overlap = x1 < x2 + size2 && x2 < x1 + size1;
    let y_overlap = y1 < y2 + size2 && y2 < y1 + size1;
    x_overlap && y_overlap
}

/// Format a compact grid summary for narrator prompt injection.
///
/// Produces a human-readable text showing grid dimensions and all placed entities
/// with their positions, sizes, and factions.
pub fn format_grid_summary(
    grid_width: u32,
    grid_height: u32,
    entities: &[TacticalEntityPayload],
) -> String {
    if entities.is_empty() {
        return format!("Tactical Grid ({grid_width}×{grid_height}): empty — no entities placed.");
    }

    let mut lines = vec![format!("Tactical Grid ({grid_width}×{grid_height}):")];

    for entity in entities {
        let size_label = match entity.size {
            1 => "Medium".to_string(),
            2 => "Large".to_string(),
            3 => "Huge".to_string(),
            n => format!("{n}×{n}"),
        };
        lines.push(format!(
            "  {} at ({},{}) — {} ({})",
            entity.name, entity.x, entity.y, size_label, entity.faction
        ));
    }

    lines.join("\n")
}

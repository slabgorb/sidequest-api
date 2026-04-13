//! Tactical grid maps — ASCII-based room geometry for dungeon crawl combat.
//!
//! Implements ADR-071: deterministic tactical maps from ASCII grids stored
//! in `rooms.yaml`. Provides a parser, typed cells, exit extraction,
//! legend resolution, and entity positioning.

mod entity;
mod grid;
pub mod layout;
pub(crate) mod parser;

pub use entity::{EntitySize, Faction, TacticalEntity};
pub use grid::{
    CardinalDirection, CellProperties, ExitGap, FeatureDef, FeatureType, GridParseError, GridPos,
    TacticalCell, TacticalGrid,
};

//! Tactical grid maps — ASCII-based room geometry for dungeon crawl combat.
//!
//! Implements ADR-071: deterministic tactical maps from ASCII grids stored
//! in `rooms.yaml`. Provides a parser, typed cells, exit extraction, and
//! legend resolution.

mod grid;
pub(crate) mod parser;

pub use grid::{
    CardinalDirection, CellProperties, ExitGap, FeatureDef, FeatureType, GridParseError, GridPos,
    TacticalCell, TacticalGrid,
};

//! Archetype resolution on the Layered Content Model framework.
//!
//! Replaces the legacy `archetype_resolve` module. Resolves
//! `(jungian, rpg_role)` axis pairs into a named archetype with lore,
//! faction, and cultural flavor — routing through the four-tier Resolver
//! framework for observability (`content.resolve` OTEL span) while
//! preserving the axis-lookup logic that funnels depend on.
//!
//! The file-based [`crate::resolver::Resolver`] is not yet wired through
//! this module; Phase 2 content migration will populate per-archetype
//! fragment YAML files that the Resolver can merge per tier. Until then,
//! the axis-lookup path below uses the pre-loaded `BaseArchetypes`,
//! `ArchetypeConstraints`, and `ArchetypeFunnels` structures that
//! `load_genre_pack` already produces, and emits a `content.resolve` span
//! with the resolved provenance so the GM panel sees every resolution.

/// The resolved archetype value type used by the Layered framework.
pub mod resolved;
/// Axis-lookup shim — replaces the legacy `resolve_archetype` entry point.
pub mod shim;

pub use resolved::ArchetypeResolved;
pub use shim::{resolve_archetype, ArchetypeResolution, ResolutionSource};

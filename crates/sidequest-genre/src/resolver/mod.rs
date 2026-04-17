//! Four-tier content resolver: Global -> Genre -> World -> Culture.
//!
//! Every resolution emits an OTEL `content.resolve` span and produces a
//! `Resolved<T>` carrying full provenance.

/// Resolved value + provenance types.
pub mod resolved;

pub use resolved::{ContributionKind, MergeStep, Provenance, Resolved, Span, Tier};

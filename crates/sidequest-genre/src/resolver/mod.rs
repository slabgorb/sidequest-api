//! Four-tier content resolver: Global -> Genre -> World -> Culture.
//!
//! Every resolution emits an OTEL `content.resolve` span and produces a
//! `Resolved<T>` carrying full provenance.

/// Resolved value + provenance types.
pub mod resolved;

pub use resolved::{ContributionKind, MergeStep, Provenance, Resolved, Span, Tier};

/// Per-field merge strategies for the four-tier content chain.
pub mod merge;

pub use merge::{apply_append, apply_strategy, MergeStrategy};

/// What to resolve against. Chain is walked in order: Global → Genre → World → Culture.
#[derive(Debug, Clone)]
pub struct ResolutionContext {
    /// Genre pack identifier (e.g. `"heavy_metal"`).
    pub genre: String,
    /// World identifier within the genre pack, if any.
    pub world: Option<String>,
    /// Culture identifier within the world, if any.
    pub culture: Option<String>,
}

/// Trait implemented by every struct with `#[derive(Layered)]`.
/// Allows the resolver to walk per-field merges across the four-tier chain.
pub trait LayeredMerge {
    /// Merge `other` (deeper tier) into `self` (shallower tier), producing the combined value.
    fn merge(self, other: Self) -> Self;
}

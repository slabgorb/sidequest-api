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

/// File-based tier loader with merge walk support.
pub mod load;

pub use load::{LayeredMerge, Resolver};

/// `content.resolve` OTEL span emission for the resolver.
pub mod otel;

pub use otel::emit_content_resolve_span;

//! Resolved value + provenance types.
//!
//! The provenance vocabulary (`Tier`, `Span`, `ContributionKind`, `MergeStep`,
//! `Provenance`) lives in `sidequest-protocol` so it can ride on
//! `GameMessage` payloads without creating a `protocol -> genre` dependency
//! cycle. They are re-exported here for backward compatibility — every
//! existing `sidequest_genre::resolver::Provenance` import keeps working.
//!
//! `Resolved<T>` stays here because it is generic over the value type,
//! which is a genre-crate concern (e.g. `ArchetypeResolved`).

pub use sidequest_protocol::{ContributionKind, MergeStep, Provenance, Span, Tier};

/// A resolved content value paired with its full provenance.
#[derive(Debug, Clone)]
pub struct Resolved<T> {
    /// The resolved value.
    pub value: T,
    /// Where the value came from and how it was assembled.
    pub provenance: Provenance,
}

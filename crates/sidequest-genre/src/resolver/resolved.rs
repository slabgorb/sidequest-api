use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Content-inheritance tier. Always walked in this order: Global, Genre, World, Culture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    /// Base defaults shared across all genre packs.
    Global,
    /// Genre-pack–level overrides (e.g. `caverns_and_claudes/`).
    Genre,
    /// World-level overrides within a genre pack.
    World,
    /// Culture-level overrides within a world.
    Culture,
}

/// Line range in a YAML source file (1-based lines, 0-based cols).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    /// First line of the range (1-based).
    pub start_line: u32,
    /// First column of the range (0-based).
    pub start_col: u32,
    /// Last line of the range (1-based, inclusive).
    pub end_line: u32,
    /// Last column of the range (0-based, exclusive).
    pub end_col: u32,
}

/// How a later tier's value relates to the value introduced by an earlier tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContributionKind {
    /// This tier introduced the value for the first time.
    Initial,
    /// This tier replaced the value wholesale.
    Replaced,
    /// This tier appended entries to a list value.
    Appended,
    /// This tier deep-merged a map value.
    Merged,
}

/// One step in the merge trail — records which tier and file contributed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeStep {
    /// The tier that made this contribution.
    pub tier: Tier,
    /// Source file path relative to the genre pack root.
    pub file: PathBuf,
    /// Location within the file, if available.
    pub span: Option<Span>,
    /// How this tier's value related to the previous value.
    pub contribution: ContributionKind,
}

/// Full provenance for a resolved content value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    /// The tier that produced the final resolved value.
    pub source_tier: Tier,
    /// Source file path relative to the genre pack root.
    pub source_file: PathBuf,
    /// Location within the source file, if available.
    pub source_span: Option<Span>,
    /// Ordered list of tier contributions that produced the final value.
    pub merge_trail: Vec<MergeStep>,
}

/// A resolved content value paired with its full provenance.
#[derive(Debug, Clone)]
pub struct Resolved<T> {
    /// The resolved value.
    pub value: T,
    /// Where the value came from and how it was assembled.
    pub provenance: Provenance,
}

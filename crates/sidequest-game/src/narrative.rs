//! Narrative tracking — append-only, immutable entries.
//!
//! NarrativeEntry is a simple data struct. The narrative log is a Vec
//! of entries, queried via reverse iteration (newest first).
//! No edits or deletes — append only.

use serde::{Deserialize, Serialize};

/// A single narrative entry in the game log.
///
/// Entries are immutable once created. The narrative log is a `Vec<NarrativeEntry>`
/// that only grows via `push`. Query via `.iter().rev()` for newest-first.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NarrativeEntry {
    /// Milliseconds since game start.
    pub timestamp: u64,
    /// Which game round this occurred in.
    pub round: u32,
    /// Source of the narration (e.g., "narrator", "combat", "chase").
    pub author: String,
    /// The narration text.
    pub content: String,
    /// Tags for scene filtering.
    pub tags: Vec<String>,
}

//! Lore mark tool (ADR-057 Phase 4).
//!
//! Validates footnote input and produces a typed `Footnote` struct.
//! Replaces the narrator's `footnotes` JSON field with a typed tool call.

use sidequest_protocol::{FactCategory, Footnote};

/// Input for the `lore_mark` tool call.
#[derive(Debug, Clone)]
pub struct LoreMarkInput {
    /// Marker number matching `[N]` in prose (optional — narrator may omit).
    pub marker: Option<u32>,
    /// One-sentence description of the fact.
    pub summary: String,
    /// Category string — validated against FactCategory enum (case-insensitive).
    pub category: String,
    /// True if this is a new revelation, false if referencing prior knowledge.
    pub is_new: bool,
}

/// Error returned when lore mark input is invalid.
#[derive(Debug, thiserror::Error)]
pub enum LoreMarkError {
    /// Category is not one of the valid FactCategory values.
    #[error(
        "invalid footnote category: \"{0}\" — expected one of: Lore, Place, Person, Quest, Ability"
    )]
    InvalidCategory(String),
    /// Summary is empty.
    #[error("footnote summary must not be empty")]
    EmptySummary,
}

/// Parse a category string (case-insensitive) into a `FactCategory`.
fn parse_category(input: &str) -> Option<FactCategory> {
    match input.to_lowercase().as_str() {
        "lore" => Some(FactCategory::Lore),
        "place" => Some(FactCategory::Place),
        "person" => Some(FactCategory::Person),
        "quest" => Some(FactCategory::Quest),
        "ability" => Some(FactCategory::Ability),
        _ => None,
    }
}

/// Validate lore mark input and produce a `Footnote` struct.
///
/// Category is validated case-insensitively against the five `FactCategory` variants.
/// Summary must not be empty. Marker is optional.
#[tracing::instrument(name = "tool.lore_mark", skip_all, fields(
    marker = ?input.marker,
    category = %input.category,
    is_new = input.is_new,
))]
fn acquire_footnote(input: LoreMarkInput) -> Result<Footnote, LoreMarkError> {
    // Validate summary
    if input.summary.is_empty() {
        tracing::warn!(valid = false, "lore mark rejected: empty summary");
        return Err(LoreMarkError::EmptySummary);
    }

    // Validate category
    let category = parse_category(&input.category).ok_or_else(|| {
        tracing::warn!(valid = false, category = %input.category, "lore mark rejected: invalid category");
        LoreMarkError::InvalidCategory(input.category.clone())
    })?;

    let footnote = Footnote {
        marker: input.marker,
        fact_id: None,
        summary: input.summary,
        category,
        is_new: input.is_new,
    };

    tracing::info!(
        valid = true,
        marker = ?footnote.marker,
        category = ?footnote.category,
        is_new = footnote.is_new,
        summary_len = footnote.summary.len(),
        "lore mark validated"
    );

    Ok(footnote)
}

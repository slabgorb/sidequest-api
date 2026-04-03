//! Lore mark validation tool (ADR-057 Phase 4).
//!
//! Validates a lore_mark tool call from the narrator sidecar.
//! Accepts text with category (world, npc, faction, location, quest, custom)
//! and confidence level (high, medium, low).
//! Rejects empty/whitespace fields and invalid categories/confidence.
//! Produces a lore text string for the `lore_established` field.

/// Valid lore categories.
const VALID_CATEGORIES: &[&str] = &["world", "npc", "faction", "location", "quest", "custom"];

/// Valid confidence levels.
const VALID_CONFIDENCE: &[&str] = &["high", "medium", "low"];

/// Validated result of a `lore_mark` tool call.
///
/// Fields are private with getters to enforce validation invariants
/// (non-empty, trimmed, valid category/confidence). Constructed only
/// through `validate_lore_mark`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LoreMarkResult {
    text: String,
    category: String,
    confidence: String,
}

impl LoreMarkResult {
    /// The lore fact text.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The lore category (world, npc, faction, location, quest, custom).
    pub fn category(&self) -> &str {
        &self.category
    }

    /// The confidence level (high, medium, low).
    pub fn confidence(&self) -> &str {
        &self.confidence
    }

    /// Convert to a lore text string for the `lore_established` vector.
    pub fn to_lore_text(&self) -> String {
        self.text.clone()
    }
}

/// Error returned when a lore_mark tool call has invalid fields.
#[derive(Debug, thiserror::Error)]
#[error("invalid lore_mark: {0}")]
pub struct InvalidLoreMark(String);

/// Validate a lore_mark tool call.
///
/// `text` must be non-empty after trimming.
/// `category` must be one of: world, npc, faction, location, quest, custom (case-insensitive).
/// `confidence` must be one of: high, medium, low (case-insensitive).
#[tracing::instrument(name = "tool.lore_mark", skip_all, fields(
    text = %text,
    category = %category,
    confidence = %confidence,
))]
pub fn validate_lore_mark(
    text: &str,
    category: &str,
    confidence: &str,
) -> Result<LoreMarkResult, InvalidLoreMark> {
    let text = text.trim();
    let category = category.trim().to_lowercase();
    let confidence = confidence.trim().to_lowercase();

    if text.is_empty() {
        tracing::warn!(valid = false, "text is empty");
        return Err(InvalidLoreMark("text is empty".to_string()));
    }
    if category.is_empty() {
        tracing::warn!(valid = false, "category is empty");
        return Err(InvalidLoreMark("category is empty".to_string()));
    }
    if !VALID_CATEGORIES.contains(&category.as_str()) {
        tracing::warn!(valid = false, category = %category, "invalid category");
        return Err(InvalidLoreMark(format!(
            "category must be one of {:?}, got '{category}'",
            VALID_CATEGORIES
        )));
    }
    if confidence.is_empty() {
        tracing::warn!(valid = false, "confidence is empty");
        return Err(InvalidLoreMark("confidence is empty".to_string()));
    }
    if !VALID_CONFIDENCE.contains(&confidence.as_str()) {
        tracing::warn!(valid = false, confidence = %confidence, "invalid confidence");
        return Err(InvalidLoreMark(format!(
            "confidence must be one of {:?}, got '{confidence}'",
            VALID_CONFIDENCE
        )));
    }

    let result = LoreMarkResult {
        text: text.to_string(),
        category,
        confidence,
    };

    tracing::info!(
        valid = true,
        text = result.text(),
        category = result.category(),
        confidence = result.confidence(),
        "lore_mark validated"
    );

    Ok(result)
}

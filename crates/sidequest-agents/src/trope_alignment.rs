//! Story 3-8: Trope alignment check — validates narration references trope themes
//! when beats fire.
//!
//! When the TropeEngine advances a trope past a threshold and a beat fires,
//! the Narrator agent should weave that beat's theme into the narration.
//! This validator performs case-insensitive keyword matching to flag turns
//! where a beat fired but the narration shows no thematic connection.
//!
//! Deliberately simple: no stemming, no NLP, no synonym expansion.
//! The goal is to catch obvious misses for human inspection.

use crate::patch_legality::ValidationResult;
use crate::turn_record::TurnRecord;

/// Contextual data for a trope beat that fired this turn.
///
/// Populated from the genre pack's trope definition when assembling
/// the TurnRecord. Contains everything the alignment check needs
/// to verify narration references the beat's theme.
#[derive(Debug, Clone)]
pub struct TropeContext {
    /// Name of the trope (e.g., "suspicion").
    pub trope_name: String,
    /// Name of the specific beat (e.g., "seeds_of_doubt").
    pub beat_name: String,
    /// Progression threshold that triggered this beat (0.0–1.0).
    pub threshold: f32,
    /// Beat description from the genre pack.
    pub description: String,
    /// Tags from the trope definition (e.g., ["paranoia", "distrust"]).
    pub keywords: Vec<String>,
}

/// Extract keywords from a TropeContext for alignment matching.
///
/// Combines the trope's tags with significant words (4+ characters) from the
/// beat description. Returns a deduplicated, lowercased keyword list.
///
/// # Arguments
/// * `ctx` — The trope context to extract keywords from.
///
/// # Returns
/// Lowercased keywords suitable for substring matching against narration.
pub fn extract_keywords(ctx: &TropeContext) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut keywords = Vec::new();

    // Add all tags (lowercased, punctuation stripped).
    // Tags are curated by the genre pack author, so no length filter.
    for tag in &ctx.keywords {
        let clean: String = tag.chars().filter(|c| c.is_alphanumeric()).collect();
        let lower = clean.to_lowercase();
        if !lower.is_empty() && seen.insert(lower.clone()) {
            keywords.push(lower);
        }
    }

    // Add description words that are 4+ characters (after punctuation stripping).
    for word in ctx.description.split_whitespace() {
        let clean: String = word.chars().filter(|c| c.is_alphanumeric()).collect();
        let lower = clean.to_lowercase();
        if lower.len() >= 4 && seen.insert(lower.clone()) {
            keywords.push(lower);
        }
    }

    keywords
}

/// Check whether narration text references the themes of fired trope beats.
///
/// For each TropeContext, extracts keywords and performs case-insensitive
/// substring matching against the narration. If no keywords match, emits
/// a `ValidationResult::Warning` and a `tracing::warn!` event tagged
/// with `component="watcher"`, `check="trope_alignment"`.
///
/// # Arguments
/// * `record` — The TurnRecord containing the narration text.
/// * `trope_contexts` — Beat contexts for tropes that fired this turn.
///
/// # Returns
/// A list of `ValidationResult` entries (empty if all beats are aligned
/// or no beats fired).
pub fn check_trope_alignment(
    record: &TurnRecord,
    trope_contexts: &[TropeContext],
) -> Vec<ValidationResult> {
    let narration_lower = record.narration.to_lowercase();
    let mut results = Vec::new();

    for ctx in trope_contexts {
        let keywords = extract_keywords(ctx);

        let matched = keywords.iter().any(|kw| narration_lower.contains(kw.as_str()));

        if matched {
            tracing::debug!(
                component = "watcher",
                check = "trope_alignment",
                trope = %ctx.trope_name,
                beat = %ctx.beat_name,
                "trope beat aligned with narration",
            );
        } else {
            let threshold_pct = (ctx.threshold * 100.0).round() as u32;
            let msg = format!(
                "Trope '{}' beat '{}' fired at {}% but narration shows no thematic connection",
                ctx.trope_name, ctx.beat_name, threshold_pct,
            );

            let narration_excerpt = if record.narration.len() > 100 {
                &record.narration[..100]
            } else {
                &record.narration
            };

            tracing::warn!(
                component = "watcher",
                check = "trope_alignment",
                trope = %ctx.trope_name,
                beat = %ctx.beat_name,
                narration_excerpt = %narration_excerpt,
                keywords_sought = ?keywords,
                "trope beat fired but narration shows no thematic connection",
            );

            results.push(ValidationResult::Warning(msg));
        }
    }

    results
}

//! Lore retrieval — budget-aware fragment selection, telemetry, and prompt formatting.
//!
//! Story 11-4: budget-aware selection with category priority + semantic search.
//! Story 18-4: selected-vs-rejected telemetry summary.

use serde::{Deserialize, Serialize};

use super::similarity::cosine_similarity;
use super::store::{LoreCategory, LoreFragment, LoreStore};

// ---------------------------------------------------------------------------
// Lore injection — select & format fragments for prompt context (story 11-4)
// ---------------------------------------------------------------------------

/// Select lore fragments from the store that fit within the given token budget.
///
/// Prioritizes by category relevance: Geography and Faction fragments come first
/// (most universally relevant for scene-setting), then History, then game events
/// (most recent first by turn_created), then everything else.
///
/// The `priority_categories` parameter allows the caller to boost specific
/// categories (e.g., Event fragments during combat, Character fragments during
/// dialogue). Fragments in priority categories get sort priority 0, Geography/Faction
/// get priority 1, and everything else gets priority 2.
pub fn select_lore_for_prompt<'a>(
    store: &'a LoreStore,
    budget: usize,
    priority_categories: Option<&[LoreCategory]>,
    query_embedding: Option<&[f32]>,
) -> Vec<&'a LoreFragment> {
    if store.is_empty() || budget == 0 {
        return Vec::new();
    }

    // Check if semantic search is viable: we have a query embedding AND at least
    // one fragment has an embedding.
    let use_semantic = query_embedding.is_some()
        && store
            .fragments
            .values()
            .any(|f| f.embedding().is_some());

    let mut fragments: Vec<&LoreFragment> = store.fragments.values().collect();

    if use_semantic {
        let qe = query_embedding.unwrap();
        // Sort by descending cosine similarity. Fragments without embeddings
        // get similarity -1.0 so they rank last.
        fragments.sort_by(|a, b| {
            let sim_a = a
                .embedding()
                .map(|e| cosine_similarity(qe, e))
                .unwrap_or(-1.0);
            let sim_b = b
                .embedding()
                .map(|e| cosine_similarity(qe, e))
                .unwrap_or(-1.0);
            sim_b
                .partial_cmp(&sim_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    } else {
        // Category-based fallback
        // Stable sort by id first for determinism
        fragments.sort_by_key(|f| f.id().to_string());

        // Priority: caller-specified categories first, then geo/faction, then recency
        fragments.sort_by(|a, b| {
            let priority_a = fragment_priority(a, priority_categories);
            let priority_b = fragment_priority(b, priority_categories);
            priority_a.cmp(&priority_b)
                .then_with(|| {
                    // Within same priority, prefer more recent game events
                    b.turn_created().unwrap_or(0).cmp(&a.turn_created().unwrap_or(0))
                })
        });
    }

    let mut selected = Vec::new();
    let mut remaining = budget;
    for frag in fragments {
        let cost = frag.token_estimate();
        if cost <= remaining {
            selected.push(frag);
            remaining -= cost;
        }
    }
    selected
}

/// Assign a sort priority to a fragment based on its category.
fn fragment_priority(frag: &LoreFragment, priority_categories: Option<&[LoreCategory]>) -> u8 {
    // Caller-specified priority categories get highest priority
    if let Some(cats) = priority_categories {
        if cats.contains(frag.category()) {
            return 0;
        }
    }
    // Geography and Faction are universally relevant for scene-setting
    match frag.category() {
        LoreCategory::Geography | LoreCategory::Faction => 1,
        LoreCategory::History => 2,
        _ => 3,
    }
}

// ---------------------------------------------------------------------------
// Lore retrieval telemetry — summarize selected vs rejected (story 18-4)
// ---------------------------------------------------------------------------

/// Summary of a single lore fragment for telemetry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FragmentSummary {
    /// Fragment identifier.
    pub id: String,
    /// Category as a string (e.g., "history", "geography").
    pub category: String,
    /// Estimated token count.
    pub tokens: usize,
}

/// Structured summary of a lore retrieval operation for the OTEL dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoreRetrievalSummary {
    /// Token budget for this retrieval.
    pub budget: usize,
    /// Total tokens used by selected fragments.
    pub tokens_used: usize,
    /// Fragments that were selected for the prompt.
    pub selected: Vec<FragmentSummary>,
    /// Fragments that were rejected (didn't fit budget or lower priority).
    pub rejected: Vec<FragmentSummary>,
    /// Context hint used for prioritization, if any.
    pub context_hint: Option<String>,
    /// Total fragments in the store.
    pub total_fragments: usize,
}

fn fragment_to_summary(frag: &LoreFragment) -> FragmentSummary {
    let category = match frag.category() {
        LoreCategory::History => "history",
        LoreCategory::Geography => "geography",
        LoreCategory::Faction => "faction",
        LoreCategory::Character => "character",
        LoreCategory::Item => "item",
        LoreCategory::Event => "event",
        LoreCategory::Language => "language",
        LoreCategory::Custom(s) => s.as_str(),
    };
    FragmentSummary {
        id: frag.id().to_string(),
        category: category.to_string(),
        tokens: frag.token_estimate(),
    }
}

/// Produce a telemetry summary comparing selected vs rejected fragments.
pub fn summarize_lore_retrieval<'a>(
    store: &'a LoreStore,
    selected: &[&'a LoreFragment],
    budget: usize,
    priority_categories: Option<&[LoreCategory]>,
) -> LoreRetrievalSummary {
    let selected_ids: std::collections::HashSet<&str> =
        selected.iter().map(|f| f.id()).collect();

    let rejected: Vec<FragmentSummary> = store
        .fragments
        .values()
        .filter(|f| !selected_ids.contains(f.id()))
        .map(|f| fragment_to_summary(f))
        .collect();

    let tokens_used: usize = selected.iter().map(|f| f.token_estimate()).sum();

    LoreRetrievalSummary {
        budget,
        tokens_used,
        selected: selected.iter().map(|f| fragment_to_summary(f)).collect(),
        rejected,
        context_hint: priority_categories.map(|cats| {
            cats.iter().map(|c| format!("{c}")).collect::<Vec<_>>().join(", ")
        }),
        total_fragments: store.len(),
    }
}

/// Format selected lore fragments into a prompt-ready string, grouped by
/// category with markdown section headers (e.g. `## History`).
///
/// Returns an empty string when `fragments` is empty.
pub fn format_lore_context(fragments: &[&LoreFragment]) -> String {
    if fragments.is_empty() {
        return String::new();
    }

    // Group fragments by category, preserving input order within each group.
    let mut groups: Vec<(&LoreCategory, Vec<&LoreFragment>)> = Vec::new();
    for frag in fragments {
        if let Some((_cat, group)) = groups.iter_mut().find(|(c, _)| *c == frag.category()) {
            group.push(frag);
        } else {
            groups.push((frag.category(), vec![frag]));
        }
    }

    let mut output = String::new();
    for (i, (category, frags)) in groups.iter().enumerate() {
        if i > 0 {
            output.push('\n');
        }
        output.push_str(&format!("## {category}\n"));
        for frag in frags {
            output.push_str(frag.content());
            output.push('\n');
        }
    }
    output
}

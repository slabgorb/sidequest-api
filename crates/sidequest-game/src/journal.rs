//! Journal browse view — KnownFact to JournalEntry conversion.
//!
//! Story 9-13: Converts a character's accumulated KnownFacts into
//! wire-format JournalEntry structs for the React browse view.

use sidequest_protocol::{FactCategory, JournalEntry, JournalSortOrder};

use crate::known_fact::KnownFact;

/// Filter and sort options for journal queries.
#[derive(Debug, Clone)]
pub struct JournalFilter {
    /// Optional category filter. None = all categories.
    pub category: Option<FactCategory>,
    /// Sort order for results.
    pub sort_by: JournalSortOrder,
}

impl Default for JournalFilter {
    fn default() -> Self {
        Self {
            category: None,
            sort_by: JournalSortOrder::Time,
        }
    }
}

/// Convert KnownFacts to JournalEntries, applying filter and sort.
///
/// Each fact gets a unique `fact_id` derived from its index. Source and
/// confidence are converted to their Display strings for wire format.
pub fn build_journal_entries(facts: &[KnownFact], filter: &JournalFilter) -> Vec<JournalEntry> {
    let mut entries: Vec<JournalEntry> = facts
        .iter()
        .enumerate()
        .filter(|(_, fact)| match filter.category {
            Some(cat) => fact.category == cat,
            None => true,
        })
        .map(|(idx, fact)| JournalEntry {
            fact_id: format!("kf-{}", idx),
            content: fact.content.clone(),
            category: fact.category,
            source: fact.source.to_string(),
            confidence: fact.confidence.to_string(),
            learned_turn: fact.learned_turn,
        })
        .collect();

    match filter.sort_by {
        JournalSortOrder::Time => {
            entries.sort_by(|a, b| b.learned_turn.cmp(&a.learned_turn));
        }
        JournalSortOrder::Category => {
            entries.sort_by(|a, b| {
                let cat_cmp = category_order(&a.category).cmp(&category_order(&b.category));
                if cat_cmp != std::cmp::Ordering::Equal {
                    cat_cmp
                } else {
                    b.learned_turn.cmp(&a.learned_turn)
                }
            });
        }
    }

    entries
}

/// Stable ordering for FactCategory grouping.
fn category_order(cat: &FactCategory) -> u8 {
    match cat {
        FactCategory::Lore => 0,
        FactCategory::Place => 1,
        FactCategory::Person => 2,
        FactCategory::Quest => 3,
        FactCategory::Ability => 4,
        _ => 255,
    }
}

//! Footnote extraction — convert narrator footnotes to discovered facts.
//!
//! Story 9-11: Maps `is_new: true` footnotes to `DiscoveredFact` entries
//! for accumulation into character knowledge via `WorldStatePatch`.

use sidequest_game::known_fact::{Confidence, DiscoveredFact, FactSource, KnownFact};
use sidequest_protocol::Footnote;

/// Convert narrator footnotes into discovered facts for a character.
///
/// Only `is_new: true` footnotes are converted — callbacks (`is_new: false`)
/// reference existing knowledge and do not create new entries.
///
/// Each new footnote becomes a `DiscoveredFact` with:
/// - `character_name`: the character who learned this fact
/// - `fact.content`: the footnote summary
/// - `fact.learned_turn`: the current game turn
/// - `fact.source`: provided by caller (Discovery for world facts, Backstory for character history)
/// - `fact.confidence`: `Confidence::Certain` (narrator is authoritative)
pub fn footnotes_to_discovered_facts(
    footnotes: &[Footnote],
    character_name: &str,
    turn: u64,
    source: FactSource,
) -> Vec<DiscoveredFact> {
    footnotes
        .iter()
        .filter(|f| f.is_new)
        .map(|f| DiscoveredFact {
            character_name: character_name.to_string(),
            fact: KnownFact {
                content: f.summary.clone(),
                learned_turn: turn,
                source: source.clone(),
                confidence: Confidence::Certain,
            },
        })
        .collect()
}

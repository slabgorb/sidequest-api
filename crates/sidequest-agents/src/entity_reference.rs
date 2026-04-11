//! Entity reference validation — heuristic check for phantom entity references in narration.
//!
//! Story 3-4: Scans narration text against the game state snapshot to detect
//! references to entities (characters, NPCs, items, locations, regions) that
//! don't exist. Emits `tracing::warn!` for each unresolved reference.
//!
//! This is a heuristic flag for human review, not a deterministic verdict.
//! See ADR-031 ("God lifting rocks" principle).

use std::collections::HashSet;

use crate::patch_legality::ValidationResult;
use crate::turn_record::TurnRecord;
use sidequest_game::GameSnapshot;

/// Registry of all known entity names extracted from a GameSnapshot.
///
/// Built fresh per turn from `snapshot_after`. Names are owned strings
/// because the registry may outlive the snapshot borrow (see context doc
/// on owned-vs-borrowed trade-off).
pub struct EntityRegistry {
    /// Player character names.
    pub character_names: HashSet<String>,
    /// Non-player character names.
    pub npc_names: HashSet<String>,
    /// Item names from all inventories.
    pub item_names: HashSet<String>,
    /// Current location name.
    pub location_names: HashSet<String>,
    /// Discovered region names.
    pub region_names: HashSet<String>,
}

impl EntityRegistry {
    /// Build an entity registry from a game snapshot.
    ///
    /// Extracts character names, NPC names, item names (from all inventories),
    /// the current location, and all discovered regions.
    pub fn from_snapshot(snapshot: &GameSnapshot) -> Self {
        let character_names: HashSet<String> = snapshot
            .characters
            .iter()
            .map(|c| c.core.name.as_str().to_string())
            .collect();

        let npc_names: HashSet<String> = snapshot
            .npcs
            .iter()
            .map(|n| n.core.name.as_str().to_string())
            .collect();

        let item_names: HashSet<String> = snapshot
            .characters
            .iter()
            .flat_map(|c| c.core.inventory.carried())
            .chain(
                snapshot
                    .npcs
                    .iter()
                    .flat_map(|n| n.core.inventory.carried()),
            )
            .map(|item| item.name.as_str().to_string())
            .collect();

        let mut location_names = HashSet::new();
        location_names.insert(snapshot.location.clone());

        let region_names: HashSet<String> = snapshot.discovered_regions.iter().cloned().collect();

        EntityRegistry {
            character_names,
            npc_names,
            item_names,
            location_names,
            region_names,
        }
    }

    /// Check if a candidate string matches any known entity (case-insensitive).
    ///
    /// Returns true if the candidate is a substring of any known name, or vice versa.
    /// This bidirectional substring match handles compound names like "Old Grimjaw"
    /// matching NPC "Grimjaw".
    pub fn matches(&self, candidate: &str) -> bool {
        let candidate_lower = candidate.to_lowercase();
        for known in self.all_names() {
            let known_lower = known.to_lowercase();
            // Bidirectional substring: candidate contains known name, or known name contains candidate
            if candidate_lower.contains(&known_lower) || known_lower.contains(&candidate_lower) {
                return true;
            }
        }
        false
    }

    /// Iterator over all known names across all categories.
    fn all_names(&self) -> impl Iterator<Item = &String> {
        self.character_names
            .iter()
            .chain(self.npc_names.iter())
            .chain(self.item_names.iter())
            .chain(self.location_names.iter())
            .chain(self.region_names.iter())
    }
}

/// Extract capitalized phrases from narration that might be entity references.
///
/// Filters out common English words (stop words) and sentence-initial
/// capitalization. Consecutive capitalized words are grouped into phrases
/// (e.g., "Old Grimjaw" → one candidate, not two).
pub fn extract_potential_references(narration: &str) -> Vec<String> {
    if narration.is_empty() {
        return Vec::new();
    }

    // Stop words that should not be treated as entity references even when capitalized.
    const STOP_WORDS: &[&str] = &[
        "The", "A", "An", "And", "Or", "But", "In", "On", "At", "To", "For", "Of", "With", "By",
        "From", "Up", "About", "Into", "Through", "During", "Before", "After", "Above", "Below",
        "Between", "Under", "Again", "Further", "Then", "Once", "He", "She", "It", "They", "We",
        "You", "I", "His", "Her", "Its", "Their", "Our", "Your", "My", "This", "That", "These",
        "Those", "Is", "Was", "Were", "Are", "Be", "Been", "Being", "Have", "Has", "Had", "Do",
        "Does", "Did", "Will", "Would", "Could", "Should", "May", "Might", "Must", "Shall", "Not",
        "No", "Nor", "So", "If", "As", "Each", "Which", "Who", "Whom", "What", "When", "Where",
        "Why", "How", "All", "Both", "Few", "More", "Most", "Other", "Some", "Such", "Than", "Too",
        "Very",
    ];

    let stop_set: HashSet<&str> = STOP_WORDS.iter().copied().collect();

    let mut results = Vec::new();
    let words: Vec<&str> = narration.split_whitespace().collect();

    // Track sentence boundaries: a word is sentence-initial if it's the first word
    // or follows a sentence-ending punctuation mark.
    let mut sentence_start = true;
    let mut current_phrase: Vec<&str> = Vec::new();

    for word in &words {
        // Strip trailing punctuation for analysis
        let clean = word.trim_end_matches(|c: char| c.is_ascii_punctuation());
        let has_trailing_sentence_end =
            word.ends_with('.') || word.ends_with('!') || word.ends_with('?');

        let is_capitalized = clean.chars().next().is_some_and(|c| c.is_uppercase());

        if is_capitalized && !sentence_start && !stop_set.contains(clean) {
            current_phrase.push(clean);
        } else {
            // Flush any accumulated phrase
            if !current_phrase.is_empty() {
                results.push(current_phrase.join(" "));
                current_phrase.clear();
            }
        }

        // Update sentence boundary tracking
        sentence_start = has_trailing_sentence_end;
    }

    // Flush trailing phrase
    if !current_phrase.is_empty() {
        results.push(current_phrase.join(" "));
    }

    results
}

/// Validate that narration only references entities present in the game state.
///
/// Builds an `EntityRegistry` from `record.snapshot_after`, extracts potential
/// entity references from `record.narration`, and flags any that don't match
/// a known entity. Each unresolved reference emits a `tracing::warn!` and
/// produces a `ValidationResult::Warning`.
pub fn check_entity_references(record: &TurnRecord) -> Vec<ValidationResult> {
    let registry = EntityRegistry::from_snapshot(&record.snapshot_after);
    let references = extract_potential_references(&record.narration);

    let mut results = Vec::new();

    for reference in &references {
        if !registry.matches(reference) {
            let msg = format!(
                "Narration references unknown entity '{}' not found in game state",
                reference
            );
            tracing::warn!(
                component = "watcher",
                check = "entity_reference",
                unresolved = %reference,
                "{}",
                msg,
            );
            results.push(ValidationResult::Warning(msg));
        }
    }

    results
}

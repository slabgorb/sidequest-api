//! Story 9-13: Journal browse view — KnownFact to JournalEntry conversion
//!
//! RED phase — these tests reference types and functions that don't exist yet.
//! They will fail to compile until Dev implements:
//!   - FactCategory field on KnownFact (or a mapping from FactSource)
//!   - journal::build_journal_entries() function
//!   - Filtering by FactCategory
//!   - Sorting by time (newest first) and by category
//!
//! ACs tested: AC2 (server handler reads KnownFacts), AC5 (pipeline wiring)
//! Also tests: category field on KnownFact, filter/sort logic

use sidequest_game::character::Character;
use sidequest_game::creature_core::CreatureCore;
use sidequest_game::inventory::Inventory;
use sidequest_game::journal::{build_journal_entries, JournalFilter};
use sidequest_game::known_fact::{Confidence, FactSource, KnownFact};
use sidequest_protocol::NonBlankString;
use sidequest_protocol::{FactCategory, JournalSortOrder};
use std::collections::HashMap;

// ============================================================================
// Test helpers
// ============================================================================

fn make_fact(
    content: &str,
    turn: u64,
    source: FactSource,
    confidence: Confidence,
    category: FactCategory,
) -> KnownFact {
    KnownFact {
        content: content.to_string(),
        learned_turn: turn,
        source,
        confidence,
        category,
    }
}

fn make_character_with_facts(name: &str, facts: Vec<KnownFact>) -> Character {
    Character {
        core: CreatureCore {
            name: NonBlankString::new(name).unwrap(),
            description: NonBlankString::new("A test character").unwrap(),
            personality: NonBlankString::new("Bold").unwrap(),
            level: 3,
            hp: 20,
            max_hp: 20,
            ac: 14,
            xp: 0,
            inventory: Inventory::default(),
            statuses: vec![],
        },
        backstory: NonBlankString::new("Born in the test realm").unwrap(),
        narrative_state: "Exploring".to_string(),
        hooks: vec![],
        char_class: NonBlankString::new("Ranger").unwrap(),
        race: NonBlankString::new("Elf").unwrap(),
        pronouns: String::new(),
        stats: HashMap::from([("STR".to_string(), 12), ("DEX".to_string(), 16)]),
        abilities: vec![],
        known_facts: facts,
        affinities: vec![],
        is_friendly: true,
        resolved_archetype: None,
        archetype_provenance: None,
    }
}

fn sample_facts() -> Vec<KnownFact> {
    vec![
        make_fact(
            "The grove's oldest tree radiates corruption",
            3,
            FactSource::Observation,
            Confidence::Certain,
            FactCategory::Place,
        ),
        make_fact(
            "Elder Mirova guards a secret beneath the well",
            5,
            FactSource::Dialogue,
            Confidence::Suspected,
            FactCategory::Person,
        ),
        make_fact(
            "The ancient runes pulse with a ward against shadow creatures",
            7,
            FactSource::Discovery,
            Confidence::Certain,
            FactCategory::Lore,
        ),
        make_fact(
            "Find the source of corruption before the harvest moon",
            2,
            FactSource::Dialogue,
            Confidence::Certain,
            FactCategory::Quest,
        ),
        make_fact(
            "Root-bonding allows you to sense corruption in living wood",
            1,
            FactSource::Discovery,
            Confidence::Certain,
            FactCategory::Ability,
        ),
    ]
}

// ============================================================================
// AC2: KnownFact now carries FactCategory
// ============================================================================

#[test]
fn known_fact_has_category_field() {
    let fact = make_fact(
        "The mayor is a cultist",
        14,
        FactSource::Dialogue,
        Confidence::Certain,
        FactCategory::Person,
    );
    assert_eq!(fact.category, FactCategory::Person);
}

#[test]
fn known_fact_category_survives_serde() {
    let fact = make_fact(
        "Ancient library under the mountain",
        8,
        FactSource::Discovery,
        Confidence::Certain,
        FactCategory::Place,
    );
    let json = serde_json::to_string(&fact).expect("serialize");
    let restored: KnownFact = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(restored.category, FactCategory::Place);
}

// ============================================================================
// AC2: build_journal_entries — converts KnownFacts to JournalEntries
// ============================================================================

#[test]
fn build_journal_entries_converts_all_facts() {
    let facts = sample_facts();
    let entries = build_journal_entries(&facts, &JournalFilter::default());
    assert_eq!(
        entries.len(),
        5,
        "all 5 facts should produce journal entries"
    );
}

#[test]
fn build_journal_entries_preserves_content() {
    let facts = sample_facts();
    let entries = build_journal_entries(&facts, &JournalFilter::default());
    let contents: Vec<&str> = entries.iter().map(|e| e.content.as_str()).collect();
    assert!(contents.contains(&"The grove's oldest tree radiates corruption"));
    assert!(contents.contains(&"Elder Mirova guards a secret beneath the well"));
}

#[test]
fn build_journal_entries_preserves_category() {
    let facts = sample_facts();
    let entries = build_journal_entries(&facts, &JournalFilter::default());
    let lore_entry = entries
        .iter()
        .find(|e| e.content.as_str().contains("ancient runes"))
        .unwrap();
    assert_eq!(lore_entry.category, FactCategory::Lore);
}

#[test]
fn build_journal_entries_preserves_source() {
    let facts = sample_facts();
    let entries = build_journal_entries(&facts, &JournalFilter::default());
    let grove_entry = entries
        .iter()
        .find(|e| e.content.as_str().contains("grove"))
        .unwrap();
    assert_eq!(grove_entry.source, "Observation");
}

#[test]
fn build_journal_entries_preserves_confidence() {
    let facts = sample_facts();
    let entries = build_journal_entries(&facts, &JournalFilter::default());
    let mirova_entry = entries
        .iter()
        .find(|e| e.content.as_str().contains("Mirova"))
        .unwrap();
    assert_eq!(mirova_entry.confidence, "Suspected");
}

#[test]
fn build_journal_entries_preserves_learned_turn() {
    let facts = sample_facts();
    let entries = build_journal_entries(&facts, &JournalFilter::default());
    let quest_entry = entries
        .iter()
        .find(|e| e.content.as_str().contains("harvest moon"))
        .unwrap();
    assert_eq!(quest_entry.learned_turn, 2);
}

#[test]
fn build_journal_entries_assigns_fact_ids() {
    let facts = sample_facts();
    let entries = build_journal_entries(&facts, &JournalFilter::default());
    // Every entry should have a non-empty fact_id. Trivially true after the
    // NonBlankString protocol sweep — the type system enforces it — but
    // kept as a regression guard in case the field type ever relaxes.
    for entry in &entries {
        assert!(
            !entry.fact_id.as_str().is_empty(),
            "fact_id must not be empty"
        );
    }
    // fact_ids should be unique
    let ids: Vec<&str> = entries.iter().map(|e| e.fact_id.as_str()).collect();
    let unique: std::collections::HashSet<&str> = ids.iter().copied().collect();
    assert_eq!(ids.len(), unique.len(), "fact_ids must be unique");
}

// ============================================================================
// AC2: Category filtering
// ============================================================================

#[test]
fn filter_by_category_lore() {
    let facts = sample_facts();
    let filter = JournalFilter {
        category: Some(FactCategory::Lore),
        sort_by: JournalSortOrder::Time,
    };
    let entries = build_journal_entries(&facts, &filter);
    assert_eq!(entries.len(), 1);
    assert!(entries[0].content.as_str().contains("ancient runes"));
}

#[test]
fn filter_by_category_person() {
    let facts = sample_facts();
    let filter = JournalFilter {
        category: Some(FactCategory::Person),
        sort_by: JournalSortOrder::Time,
    };
    let entries = build_journal_entries(&facts, &filter);
    assert_eq!(entries.len(), 1);
    assert!(entries[0].content.as_str().contains("Mirova"));
}

#[test]
fn filter_by_category_returns_empty_when_none_match() {
    let facts = vec![make_fact(
        "Only a lore fact",
        1,
        FactSource::Observation,
        Confidence::Certain,
        FactCategory::Lore,
    )];
    let filter = JournalFilter {
        category: Some(FactCategory::Quest),
        sort_by: JournalSortOrder::Time,
    };
    let entries = build_journal_entries(&facts, &filter);
    assert!(entries.is_empty());
}

#[test]
fn no_category_filter_returns_all() {
    let facts = sample_facts();
    let filter = JournalFilter {
        category: None,
        sort_by: JournalSortOrder::Time,
    };
    let entries = build_journal_entries(&facts, &filter);
    assert_eq!(entries.len(), 5);
}

// ============================================================================
// AC4: Sort — chronological (newest first) and by category
// ============================================================================

#[test]
fn sort_by_time_returns_newest_first() {
    let facts = sample_facts();
    let filter = JournalFilter {
        category: None,
        sort_by: JournalSortOrder::Time,
    };
    let entries = build_journal_entries(&facts, &filter);
    // Newest is turn 7 (ancient runes), oldest is turn 1 (root-bonding)
    assert_eq!(entries[0].learned_turn, 7, "newest fact should be first");
    assert_eq!(
        entries[entries.len() - 1].learned_turn,
        1,
        "oldest fact should be last"
    );
    // Verify monotonically non-increasing
    for window in entries.windows(2) {
        assert!(
            window[0].learned_turn >= window[1].learned_turn,
            "entries should be sorted newest-first: {} >= {}",
            window[0].learned_turn,
            window[1].learned_turn,
        );
    }
}

#[test]
fn sort_by_category_groups_entries() {
    let facts = sample_facts();
    let filter = JournalFilter {
        category: None,
        sort_by: JournalSortOrder::Category,
    };
    let entries = build_journal_entries(&facts, &filter);
    assert_eq!(entries.len(), 5);
    // Entries should be grouped by category — within each group, newest first
    // We don't prescribe category ordering, but entries of the same category must be adjacent
    let mut seen_categories: Vec<FactCategory> = vec![];
    for entry in &entries {
        if seen_categories.last() != Some(&entry.category) {
            assert!(
                !seen_categories.contains(&entry.category),
                "category {:?} appeared non-contiguously — entries must be grouped",
                entry.category,
            );
            seen_categories.push(entry.category);
        }
    }
}

// ============================================================================
// AC2 + AC8: Empty state
// ============================================================================

#[test]
fn build_journal_entries_with_no_facts() {
    let facts: Vec<KnownFact> = vec![];
    let entries = build_journal_entries(&facts, &JournalFilter::default());
    assert!(entries.is_empty());
}

#[test]
fn build_journal_entries_from_character_with_no_facts() {
    let character = make_character_with_facts("Reva", vec![]);
    let entries = build_journal_entries(&character.known_facts, &JournalFilter::default());
    assert!(entries.is_empty());
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn single_fact_produces_single_entry() {
    let facts = vec![make_fact(
        "A lone discovery",
        42,
        FactSource::Discovery,
        Confidence::Rumored,
        FactCategory::Lore,
    )];
    let entries = build_journal_entries(&facts, &JournalFilter::default());
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].confidence, "Rumored");
    assert_eq!(entries[0].learned_turn, 42);
}

#[test]
fn duplicate_content_facts_both_appear() {
    let facts = vec![
        make_fact(
            "Same content",
            1,
            FactSource::Dialogue,
            Confidence::Rumored,
            FactCategory::Lore,
        ),
        make_fact(
            "Same content",
            5,
            FactSource::Observation,
            Confidence::Certain,
            FactCategory::Lore,
        ),
    ];
    let entries = build_journal_entries(&facts, &JournalFilter::default());
    assert_eq!(
        entries.len(),
        2,
        "duplicate content facts should both appear"
    );
    // Should have different fact_ids
    assert_ne!(entries[0].fact_id, entries[1].fact_id);
}

#[test]
fn backstory_source_included_in_journal() {
    let fact = KnownFact {
        content: "You grew up in the shadow of the Iron Spire".to_string(),
        learned_turn: 0,
        source: FactSource::Backstory,
        confidence: Confidence::Certain,
        category: FactCategory::Lore,
    };
    let entries = build_journal_entries(&[fact], &JournalFilter::default());
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].source, "Backstory");
}

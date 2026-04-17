//! Story F3: Session Resume Recap ("Previously On...") tests
//!
//! Tests the narrative log data model enrichments (EncounterTag, new NarrativeEntry fields)
//! and the generate_recap() algorithm.
//!
//! ACs tested:
//!   1. NarrativeEntry has encounter_tags, speaker, entry_type fields
//!   2. EncounterTag struct with npc_id, encounter_type, archetype_id, notes
//!   3. generate_recap() returns None for empty entries
//!   4. generate_recap() includes "Previously On..." header
//!   5. generate_recap() includes party intro with character names
//!   6. generate_recap() includes entry bullets
//!   7. generate_recap() includes location footer
//!   8. generate_recap() handles no characters gracefully
//!   9. generate_recap() handles empty location gracefully
//!  10. Recap integrates with SqliteStore load (rich recap)

use sidequest_game::narrative::{generate_recap, EncounterTag, NarrativeEntry};
use sidequest_game::persistence::{SessionStore, SqliteStore};

// ============================================================================
// Test fixtures
// ============================================================================

fn test_entry(round: u32, content: &str) -> NarrativeEntry {
    NarrativeEntry {
        timestamp: round as u64 * 1000,
        round,
        author: "narrator".to_string(),
        content: content.to_string(),
        tags: vec![],
        encounter_tags: vec![],
        speaker: None,
        entry_type: None,
    }
}

fn test_entry_with_tags(
    round: u32,
    content: &str,
    encounter_tags: Vec<EncounterTag>,
) -> NarrativeEntry {
    NarrativeEntry {
        timestamp: round as u64 * 1000,
        round,
        author: "narrator".to_string(),
        content: content.to_string(),
        tags: vec![],
        encounter_tags,
        speaker: Some("Thorn".to_string()),
        entry_type: Some("dialogue".to_string()),
    }
}

// ============================================================================
// AC-1: NarrativeEntry has new fields
// ============================================================================

#[test]
fn narrative_entry_has_encounter_tags() {
    let entry = test_entry(1, "test");
    assert!(entry.encounter_tags.is_empty());
}

#[test]
fn narrative_entry_has_speaker() {
    let mut entry = test_entry(1, "test");
    entry.speaker = Some("Thorn".to_string());
    assert_eq!(entry.speaker.unwrap(), "Thorn");
}

#[test]
fn narrative_entry_has_entry_type() {
    let mut entry = test_entry(1, "test");
    entry.entry_type = Some("combat".to_string());
    assert_eq!(entry.entry_type.unwrap(), "combat");
}

// ============================================================================
// AC-2: EncounterTag struct
// ============================================================================

#[test]
fn encounter_tag_has_required_fields() {
    let tag = EncounterTag {
        npc_id: "goblin_chief".to_string(),
        encounter_type: "combat".to_string(),
        archetype_id: Some("warrior".to_string()),
        notes: Some("First encounter".to_string()),
    };
    assert_eq!(tag.npc_id, "goblin_chief");
    assert_eq!(tag.encounter_type, "combat");
    assert_eq!(tag.archetype_id.unwrap(), "warrior");
    assert_eq!(tag.notes.unwrap(), "First encounter");
}

#[test]
fn encounter_tag_optional_fields_default_to_none() {
    let tag = EncounterTag {
        npc_id: "merchant".to_string(),
        encounter_type: "dialogue".to_string(),
        archetype_id: None,
        notes: None,
    };
    assert!(tag.archetype_id.is_none());
    assert!(tag.notes.is_none());
}

// ============================================================================
// AC-3: Empty entries → None
// ============================================================================

#[test]
fn generate_recap_returns_none_for_empty_entries() {
    let result = generate_recap(&[], &["Thorn".to_string()], "Town Square");
    assert!(result.is_none());
}

// ============================================================================
// AC-4: Header
// ============================================================================

#[test]
fn generate_recap_includes_header() {
    let entries = vec![test_entry(1, "The adventure begins.")];
    let recap = generate_recap(&entries, &["Thorn".to_string()], "Town Square").unwrap();
    assert!(
        recap.starts_with("## Previously On"),
        "Recap should start with markdown heading '## Previously On'. Got: {}",
        recap
    );
}

// ============================================================================
// AC-5: Party intro
// ============================================================================

#[test]
fn generate_recap_includes_party_intro() {
    let entries = vec![test_entry(1, "The adventure begins.")];
    let names = vec![
        "Thorn".to_string(),
        "Luna".to_string(),
        "Grimjaw".to_string(),
    ];
    let recap = generate_recap(&entries, &names, "Town Square").unwrap();
    assert!(
        recap.contains("The party"),
        "Recap should mention the party. Got: {}",
        recap
    );
    assert!(
        recap.contains("Thorn"),
        "Recap should include character name Thorn. Got: {}",
        recap
    );
    assert!(
        recap.contains("Luna"),
        "Recap should include character name Luna. Got: {}",
        recap
    );
    assert!(
        recap.contains("Grimjaw"),
        "Recap should include character name Grimjaw. Got: {}",
        recap
    );
    assert!(
        recap.contains("had been adventuring"),
        "Recap should include adventuring text. Got: {}",
        recap
    );
}

// ============================================================================
// AC-6: Entry bullets
// ============================================================================

#[test]
fn generate_recap_includes_entry_bullets() {
    let entries = vec![
        test_entry(1, "The party entered the tavern."),
        test_entry(2, "A stranger approached with a map."),
        test_entry(3, "They set out for the dungeon."),
    ];
    let recap = generate_recap(&entries, &["Thorn".to_string()], "Dungeon Entrance").unwrap();
    assert!(
        recap.contains("- The party entered the tavern."),
        "Got: {}",
        recap
    );
    assert!(
        recap.contains("- A stranger approached with a map."),
        "Got: {}",
        recap
    );
    assert!(
        recap.contains("- They set out for the dungeon."),
        "Got: {}",
        recap
    );
}

// ============================================================================
// AC-7: Location footer
// ============================================================================

#[test]
fn generate_recap_includes_location_footer() {
    let entries = vec![test_entry(1, "Something happened.")];
    let recap = generate_recap(&entries, &["Thorn".to_string()], "Dark Cave").unwrap();
    assert!(
        recap.contains("The party now finds themselves at Dark Cave."),
        "Recap should include location footer. Got: {}",
        recap
    );
}

// ============================================================================
// AC-8: No characters → skip party intro
// ============================================================================

#[test]
fn generate_recap_handles_no_characters() {
    let entries = vec![test_entry(1, "Something happened.")];
    let recap = generate_recap(&entries, &[], "Town").unwrap();
    assert!(
        !recap.contains("had been adventuring"),
        "Recap should NOT include party intro when no characters. Got: {}",
        recap
    );
    // Should still have header and entries
    assert!(recap.contains("Previously On"));
    assert!(recap.contains("- Something happened."));
}

// ============================================================================
// AC-9: Empty location → skip footer
// ============================================================================

#[test]
fn generate_recap_handles_empty_location() {
    let entries = vec![test_entry(1, "Something happened.")];
    let recap = generate_recap(&entries, &["Thorn".to_string()], "").unwrap();
    assert!(
        !recap.contains("finds themselves"),
        "Recap should NOT include location footer when empty. Got: {}",
        recap
    );
}

// ============================================================================
// AC-10: Full recap format
// ============================================================================

#[test]
fn generate_recap_full_format() {
    let entries = vec![
        test_entry(1, "The party entered the tavern."),
        test_entry(2, "A brawl erupted."),
    ];
    let names = vec!["Thorn".to_string(), "Luna".to_string()];
    let recap = generate_recap(&entries, &names, "Town Square").unwrap();

    // Verify order: header, party intro, entries, footer
    let header_pos = recap.find("Previously On").unwrap();
    let party_pos = recap.find("The party").unwrap();
    let entry1_pos = recap.find("- The party entered the tavern.").unwrap();
    let entry2_pos = recap.find("- A brawl erupted.").unwrap();
    let footer_pos = recap
        .find("The party now finds themselves at Town Square.")
        .unwrap();

    assert!(header_pos < party_pos, "Header before party intro");
    assert!(party_pos < entry1_pos, "Party intro before entries");
    assert!(entry1_pos < entry2_pos, "Entries in order");
    assert!(entry2_pos < footer_pos, "Entries before footer");
}

// ============================================================================
// AC-11: Serde round-trip with new fields
// ============================================================================

#[test]
fn narrative_entry_serde_roundtrip_with_new_fields() {
    let entry = test_entry_with_tags(
        1,
        "Dialogue with the chief.",
        vec![EncounterTag {
            npc_id: "chief".to_string(),
            encounter_type: "dialogue".to_string(),
            archetype_id: Some("leader".to_string()),
            notes: None,
        }],
    );
    let json = serde_json::to_string(&entry).unwrap();
    let deserialized: NarrativeEntry = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.encounter_tags.len(), 1);
    assert_eq!(deserialized.encounter_tags[0].npc_id, "chief");
    assert_eq!(deserialized.speaker.unwrap(), "Thorn");
    assert_eq!(deserialized.entry_type.unwrap(), "dialogue");
}

#[test]
fn narrative_entry_backward_compat_deserialization() {
    // Old format without new fields should still deserialize (serde default)
    let old_json = r#"{
        "timestamp": 1000,
        "round": 1,
        "author": "narrator",
        "content": "Old entry",
        "tags": ["combat"]
    }"#;
    let entry: NarrativeEntry = serde_json::from_str(old_json).unwrap();
    assert!(entry.encounter_tags.is_empty());
    assert!(entry.speaker.is_none());
    assert!(entry.entry_type.is_none());
}

// ============================================================================
// AC-12: SqliteStore integration — rich recap via load()
// ============================================================================

#[test]
fn sqlite_load_produces_rich_recap() {
    use sidequest_game::character::Character;
    use sidequest_game::creature_core::CreatureCore;
    use sidequest_game::inventory::Inventory;
    use sidequest_game::state::GameSnapshot;
    use sidequest_protocol::NonBlankString;
    use std::collections::HashMap;

    let store = SqliteStore::open_in_memory().unwrap();
    store
        .init_session("mutant_wasteland", "flickering_reach")
        .unwrap();

    let snapshot = GameSnapshot {
        genre_slug: "mutant_wasteland".to_string(),
        world_slug: "flickering_reach".to_string(),
        characters: vec![Character {
            core: CreatureCore {
                name: NonBlankString::new("Thorn").unwrap(),
                description: NonBlankString::new("A scarred warrior").unwrap(),
                personality: NonBlankString::new("Gruff").unwrap(),
                level: 1,
                hp: 10,
                max_hp: 10,
                ac: 10,
                xp: 0,
                inventory: Inventory::default(),
                statuses: vec![],
            },
            backstory: NonBlankString::new("Born in the wastes").unwrap(),
            narrative_state: "Exploring".to_string(),
            hooks: vec![],
            char_class: NonBlankString::new("Fighter").unwrap(),
            race: NonBlankString::new("Human").unwrap(),
            pronouns: String::new(),
            stats: HashMap::from([("STR".to_string(), 14), ("DEX".to_string(), 12)]),
            abilities: vec![],
            known_facts: vec![],
            affinities: vec![],
            is_friendly: true,
            resolved_archetype: None,
        }],
        location: "Town Square".to_string(),
        ..GameSnapshot::default()
    };

    store.save(&snapshot).unwrap();

    // Append narrative entries
    store
        .append_narrative(&test_entry(1, "The party entered the tavern."))
        .unwrap();
    store
        .append_narrative(&test_entry(2, "A stranger approached."))
        .unwrap();

    // Load — should get rich recap
    let session = store.load().unwrap().unwrap();
    let recap = session.recap.unwrap();

    assert!(
        recap.contains("Previously On"),
        "Has header. Got: {}",
        recap
    );
    assert!(
        recap.contains("Thorn"),
        "Has character name. Got: {}",
        recap
    );
    assert!(
        recap.contains("had been adventuring"),
        "Has party intro. Got: {}",
        recap
    );
    assert!(
        recap.contains("- The party entered the tavern."),
        "Has entry 1. Got: {}",
        recap
    );
    assert!(
        recap.contains("- A stranger approached."),
        "Has entry 2. Got: {}",
        recap
    );
    assert!(
        recap.contains("Town Square"),
        "Has location. Got: {}",
        recap
    );
}

#[test]
fn sqlite_load_no_entries_no_recap() {
    let store = SqliteStore::open_in_memory().unwrap();
    store
        .init_session("mutant_wasteland", "flickering_reach")
        .unwrap();

    let snapshot = GameSnapshot {
        genre_slug: "mutant_wasteland".to_string(),
        world_slug: "flickering_reach".to_string(),
        ..GameSnapshot::default()
    };
    store.save(&snapshot).unwrap();

    let session = store.load().unwrap().unwrap();
    assert!(session.recap.is_none(), "No entries means no recap");
}

use sidequest_game::state::GameSnapshot;

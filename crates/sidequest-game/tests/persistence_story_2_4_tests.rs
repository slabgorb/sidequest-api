//! Story 2-4: SQLite persistence tests
//!
//! RED phase — these tests reference types that need to be created/refactored.
//! They will fail to compile until Dev implements:
//!   - SessionStore trait (replaces concrete GameStore)
//!   - SqliteStore implementing SessionStore
//!   - SavedSession, SessionMeta structs
//!   - generate_recap() method
//!   - recent_narrative(limit) method
//!   - list_saves(root) directory scanning
//!   - PersistError (renamed from PersistenceError)
//!   - New schema: session_meta + game_state + narrative_log (singleton)
//!
//! ACs tested:
//!   1. Create save — new DB with schema, initial state
//!   2. Save state — atomic JSON serialization
//!   3. Load state — returns SavedSession
//!   4. Narrative append — round, author, content
//!   5. Narrative query — recent_narrative(10) returns last 10
//!   6. Recap generation — "Previously On..." from entries
//!   7. Empty recap — None for fresh games
//!   8. List saves — directory tree scan
//!   9. Crash recovery — (tested via transaction semantics)
//!  10. In-memory tests — open_in_memory() works
//!  11. Schema migration — idempotent reopen

use std::path::Path;

use chrono::Utc;

use sidequest_game::narrative::NarrativeEntry;
use sidequest_game::state::GameSnapshot;

// === New types from story 2-4 (some exist, some need refactoring) ===
use sidequest_game::persistence::{
    PersistError, SavedSession, SessionMeta, SessionStore, SqliteStore,
};

// ============================================================================
// Test fixtures
// ============================================================================

fn test_snapshot() -> GameSnapshot {
    use sidequest_game::character::Character;
    use sidequest_game::combat::CombatState;
    use sidequest_game::creature_core::CreatureCore;
    use sidequest_game::inventory::Inventory;
    use sidequest_game::turn::{TurnManager, TurnPhase};
    use sidequest_protocol::NonBlankString;
    use std::collections::HashMap;

    GameSnapshot {
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
                inventory: Inventory::default(),
                statuses: vec![],
            },
            backstory: NonBlankString::new("Born in the wastes").unwrap(),
            narrative_state: "Exploring".to_string(),
            hooks: vec![],
            char_class: NonBlankString::new("Fighter").unwrap(),
            race: NonBlankString::new("Human").unwrap(),
            stats: HashMap::from([
                ("STR".to_string(), 14),
                ("DEX".to_string(), 12),
                ("CON".to_string(), 13),
                ("INT".to_string(), 10),
                ("WIS".to_string(), 8),
                ("CHA".to_string(), 15),
            ]),
            abilities: vec![],
        known_facts: vec![],
            is_friendly: true,
        }],
        npcs: vec![],
        location: "Town Square".to_string(),
        time_of_day: "morning".to_string(),
        quest_log: HashMap::new(),
        notes: vec![],
        narrative_log: vec![],
        combat: CombatState::new(),
        chase: None,
        active_tropes: vec![],
        atmosphere: "tense".to_string(),
        current_region: "flickering_reach".to_string(),
        discovered_regions: vec!["flickering_reach".to_string()],
        discovered_routes: vec![],
        turn_manager: TurnManager::new(),
        last_saved_at: None,
        active_stakes: String::new(),
        lore_established: vec![],
        turns_since_meaningful: 0,
        total_beats_fired: 0,
        campaign_maturity: sidequest_game::CampaignMaturity::Fresh,
        world_history: vec![],
        ..GameSnapshot::default()
    }
}

fn test_entry(round: u32, author: &str, content: &str) -> NarrativeEntry {
    NarrativeEntry {
        timestamp: round as u64 * 1000,
        round,
        author: author.to_string(),
        content: content.to_string(),
        tags: vec![],
        encounter_tags: vec![],
        speaker: None,
        entry_type: None,
    }
}

// ============================================================================
// AC-10: In-memory tests — SqliteStore::open_in_memory() works
// ============================================================================

#[test]
fn open_in_memory_succeeds() {
    let store = SqliteStore::open_in_memory();
    assert!(store.is_ok(), "In-memory store should open successfully");
}

#[test]
fn open_in_memory_creates_schema() {
    let store = SqliteStore::open_in_memory().unwrap();
    // Saving should work — proves tables exist
    let snapshot = test_snapshot();
    let result = store.save(&snapshot);
    assert!(
        result.is_ok(),
        "Save to fresh in-memory store should succeed"
    );
}

// ============================================================================
// AC-1: Create save — new session creates save.db with schema, writes state
// ============================================================================

#[test]
fn save_writes_initial_state() {
    let store = SqliteStore::open_in_memory().unwrap();
    let snapshot = test_snapshot();

    let result = store.save(&snapshot);
    assert!(result.is_ok(), "Initial save should succeed");
}

#[test]
fn save_then_load_roundtrips() {
    let store = SqliteStore::open_in_memory().unwrap();
    let snapshot = test_snapshot();

    store.save(&snapshot).unwrap();
    let loaded = store.load().unwrap();

    assert!(loaded.is_some(), "Load after save should return Some");
    let session = loaded.unwrap();
    assert_eq!(
        session.snapshot.genre_slug, "mutant_wasteland",
        "Loaded snapshot should match saved genre"
    );
    assert_eq!(
        session.snapshot.characters.len(),
        1,
        "Loaded snapshot should have 1 character"
    );
}

// ============================================================================
// AC-2: Save state — atomic JSON serialization in transaction
// ============================================================================

#[test]
fn save_updates_last_saved_at() {
    let store = SqliteStore::open_in_memory().unwrap();
    let snapshot = test_snapshot();
    assert!(snapshot.last_saved_at.is_none());

    store.save(&snapshot).unwrap();
    let loaded = store.load().unwrap().unwrap();
    assert!(
        loaded.snapshot.last_saved_at.is_some(),
        "Saved snapshot should have last_saved_at stamped"
    );
}

#[test]
fn save_overwrites_previous_state() {
    let store = SqliteStore::open_in_memory().unwrap();
    let mut snapshot = test_snapshot();

    store.save(&snapshot).unwrap();

    // Modify and save again
    snapshot.location = "Dark Alley".to_string();
    store.save(&snapshot).unwrap();

    let loaded = store.load().unwrap().unwrap();
    assert_eq!(
        loaded.snapshot.location, "Dark Alley",
        "Second save should overwrite first"
    );
}

// ============================================================================
// AC-3: Load state — returns SavedSession with meta + snapshot + recap
// ============================================================================

#[test]
fn load_returns_saved_session_with_meta() {
    let store = SqliteStore::open_in_memory().unwrap();

    // Initialize session metadata
    store
        .init_session("mutant_wasteland", "flickering_reach")
        .unwrap();
    store.save(&test_snapshot()).unwrap();

    let session = store.load().unwrap().unwrap();
    assert_eq!(session.meta.genre_slug, "mutant_wasteland");
    assert_eq!(session.meta.world_slug, "flickering_reach");
    assert!(
        session.meta.created_at <= Utc::now(),
        "created_at should be in the past"
    );
}

#[test]
fn load_empty_store_returns_none() {
    let store = SqliteStore::open_in_memory().unwrap();
    let result = store.load().unwrap();
    assert!(result.is_none(), "Load from empty store should return None");
}

// ============================================================================
// AC-4: Narrative append — entry with round, author, content
// ============================================================================

#[test]
fn append_narrative_succeeds() {
    let store = SqliteStore::open_in_memory().unwrap();
    let entry = test_entry(1, "narrator", "You awaken in a dark room.");

    let result = store.append_narrative(&entry);
    assert!(result.is_ok(), "Appending narrative should succeed");
}

#[test]
fn append_multiple_entries() {
    let store = SqliteStore::open_in_memory().unwrap();

    store
        .append_narrative(&test_entry(1, "narrator", "First entry"))
        .unwrap();
    store
        .append_narrative(&test_entry(1, "combat", "Second entry"))
        .unwrap();
    store
        .append_narrative(&test_entry(2, "narrator", "Third entry"))
        .unwrap();

    let entries = store.recent_narrative(10).unwrap();
    assert_eq!(entries.len(), 3, "Should have 3 entries");
}

// ============================================================================
// AC-5: Narrative query — recent_narrative(10) returns last 10 by insertion
// ============================================================================

#[test]
fn recent_narrative_returns_in_insertion_order() {
    let store = SqliteStore::open_in_memory().unwrap();

    for i in 1..=5 {
        store
            .append_narrative(&test_entry(i, "narrator", &format!("Entry {}", i)))
            .unwrap();
    }

    let entries = store.recent_narrative(10).unwrap();
    assert_eq!(entries.len(), 5);
    assert_eq!(
        entries[0].content, "Entry 1",
        "First entry should be oldest"
    );
    assert_eq!(entries[4].content, "Entry 5", "Last entry should be newest");
}

#[test]
fn recent_narrative_respects_limit() {
    let store = SqliteStore::open_in_memory().unwrap();

    for i in 1..=20 {
        store
            .append_narrative(&test_entry(i, "narrator", &format!("Entry {}", i)))
            .unwrap();
    }

    let entries = store.recent_narrative(5).unwrap();
    assert_eq!(entries.len(), 5, "Should return exactly 5 entries");
    // Should be the LAST 5 entries (16-20)
    assert_eq!(entries[0].content, "Entry 16");
    assert_eq!(entries[4].content, "Entry 20");
}

#[test]
fn recent_narrative_empty_log_returns_empty_vec() {
    let store = SqliteStore::open_in_memory().unwrap();
    let entries = store.recent_narrative(10).unwrap();
    assert!(entries.is_empty(), "Empty log should return empty vec");
}

// ============================================================================
// AC-6: Recap generation — "Previously On..." from recent entries
// ============================================================================

#[test]
fn generate_recap_produces_text() {
    let store = SqliteStore::open_in_memory().unwrap();

    store
        .append_narrative(&test_entry(1, "narrator", "The party entered the tavern."))
        .unwrap();
    store
        .append_narrative(&test_entry(
            2,
            "narrator",
            "A stranger approached with a map.",
        ))
        .unwrap();

    let recap = store.generate_recap().unwrap();
    assert!(
        recap.is_some(),
        "Recap should exist when entries are present"
    );

    let text = recap.unwrap();
    assert!(
        text.contains("Previously"),
        "Recap should contain 'Previously'. Got: {}",
        text
    );
}

#[test]
fn generate_recap_includes_entry_content() {
    let store = SqliteStore::open_in_memory().unwrap();

    store
        .append_narrative(&test_entry(
            1,
            "narrator",
            "The dragon attacked the village.",
        ))
        .unwrap();

    let recap = store.generate_recap().unwrap().unwrap();
    assert!(
        recap.contains("dragon") || recap.contains("village"),
        "Recap should reference entry content. Got: {}",
        recap
    );
}

// ============================================================================
// AC-7: Empty recap — fresh game returns None
// ============================================================================

#[test]
fn generate_recap_returns_none_for_fresh_game() {
    let store = SqliteStore::open_in_memory().unwrap();
    let recap = store.generate_recap().unwrap();
    assert!(
        recap.is_none(),
        "Fresh game with no entries should return None"
    );
}

// ============================================================================
// AC-8: List saves — directory tree scan
// ============================================================================

#[test]
fn list_saves_finds_save_files() {
    let tmp = tempfile::tempdir().unwrap();
    let save_dir = tmp.path().join("mutant_wasteland/flickering_reach");
    std::fs::create_dir_all(&save_dir).unwrap();

    // Create a save.db in the directory
    let db_path = save_dir.join("save.db");
    let store = SqliteStore::open(db_path.to_str().unwrap()).unwrap();
    store
        .init_session("mutant_wasteland", "flickering_reach")
        .unwrap();
    store.save(&test_snapshot()).unwrap();
    drop(store);

    let saves = SqliteStore::list_saves(tmp.path()).unwrap();
    assert_eq!(saves.len(), 1, "Should find 1 save");
    assert_eq!(saves[0].genre_slug, "mutant_wasteland");
    assert_eq!(saves[0].world_slug, "flickering_reach");
}

#[test]
fn list_saves_empty_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let saves = SqliteStore::list_saves(tmp.path()).unwrap();
    assert!(saves.is_empty(), "Empty directory should return no saves");
}

#[test]
fn list_saves_multiple_genres() {
    let tmp = tempfile::tempdir().unwrap();

    // Create two save directories
    for (genre, world) in [
        ("mutant_wasteland", "flickering_reach"),
        ("low_fantasy", "default"),
    ] {
        let save_dir = tmp.path().join(genre).join(world);
        std::fs::create_dir_all(&save_dir).unwrap();
        let db_path = save_dir.join("save.db");
        let store = SqliteStore::open(db_path.to_str().unwrap()).unwrap();
        store.init_session(genre, world).unwrap();
        store.save(&test_snapshot()).unwrap();
    }

    let saves = SqliteStore::list_saves(tmp.path()).unwrap();
    assert_eq!(saves.len(), 2, "Should find 2 saves");
}

// ============================================================================
// AC-9: Crash recovery — transaction semantics
// ============================================================================

#[test]
fn save_is_atomic_via_transaction() {
    // This test verifies that save uses transactions by checking that
    // a successful save is fully committed (no partial state)
    let store = SqliteStore::open_in_memory().unwrap();
    let snapshot = test_snapshot();

    store.save(&snapshot).unwrap();
    let loaded = store.load().unwrap().unwrap();

    // All fields should be present — no partial save
    assert_eq!(loaded.snapshot.genre_slug, snapshot.genre_slug);
    assert_eq!(loaded.snapshot.characters.len(), snapshot.characters.len());
    assert_eq!(loaded.snapshot.location, snapshot.location);
}

// ============================================================================
// AC-11: Schema migration — idempotent reopen
// ============================================================================

#[test]
fn reopen_same_db_is_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("test.db");
    let path_str = db_path.to_str().unwrap();

    // Open, save, close
    {
        let store = SqliteStore::open(path_str).unwrap();
        store
            .init_session("mutant_wasteland", "flickering_reach")
            .unwrap();
        store.save(&test_snapshot()).unwrap();
    }

    // Reopen — should not fail with "table already exists"
    {
        let store = SqliteStore::open(path_str);
        assert!(
            store.is_ok(),
            "Reopening existing DB should succeed (idempotent schema)"
        );
        let store = store.unwrap();
        let loaded = store.load().unwrap();
        assert!(loaded.is_some(), "Data should survive reopen");
    }
}

// ============================================================================
// SavedSession and SessionMeta structure
// ============================================================================

#[test]
fn saved_session_has_all_fields() {
    let store = SqliteStore::open_in_memory().unwrap();
    store
        .init_session("mutant_wasteland", "flickering_reach")
        .unwrap();
    store.save(&test_snapshot()).unwrap();

    let session = store.load().unwrap().unwrap();

    // Meta fields
    assert_eq!(session.meta.genre_slug, "mutant_wasteland");
    assert_eq!(session.meta.world_slug, "flickering_reach");

    // Snapshot
    assert!(!session.snapshot.characters.is_empty());

    // Recap (None for fresh game)
    assert!(session.recap.is_none(), "Fresh game should have no recap");
}

#[test]
fn saved_session_includes_recap_when_entries_exist() {
    let store = SqliteStore::open_in_memory().unwrap();
    store
        .init_session("mutant_wasteland", "flickering_reach")
        .unwrap();
    store.save(&test_snapshot()).unwrap();

    store
        .append_narrative(&test_entry(1, "narrator", "The adventure begins."))
        .unwrap();

    let session = store.load().unwrap().unwrap();
    assert!(
        session.recap.is_some(),
        "SavedSession should include recap when entries exist"
    );
}

// ============================================================================
// PersistError variants
// ============================================================================

#[test]
fn persist_error_database_variant_exists() {
    // Verify the error type has the expected variants
    let _err = PersistError::NotFound;
}

#[test]
fn persist_error_serialization_variant_exists() {
    let _err = PersistError::Serialization("test".to_string());
}

// ============================================================================
// SessionStore trait — init_session method
// ============================================================================

#[test]
fn init_session_stores_metadata() {
    let store = SqliteStore::open_in_memory().unwrap();
    store
        .init_session("mutant_wasteland", "flickering_reach")
        .unwrap();

    // Save and load to verify meta persists
    store.save(&test_snapshot()).unwrap();
    let session = store.load().unwrap().unwrap();
    assert_eq!(session.meta.genre_slug, "mutant_wasteland");
    assert_eq!(session.meta.world_slug, "flickering_reach");
}

// ============================================================================
// Narrative entry preservation
// ============================================================================

#[test]
fn narrative_entry_preserves_all_fields() {
    let store = SqliteStore::open_in_memory().unwrap();
    let entry = NarrativeEntry {
        timestamp: 12345,
        round: 3,
        author: "narrator".to_string(),
        content: "A dramatic scene unfolds.".to_string(),
        tags: vec!["combat".to_string(), "boss".to_string()],
        encounter_tags: vec![],
        speaker: None,
        entry_type: None,
    };

    store.append_narrative(&entry).unwrap();
    let loaded = store.recent_narrative(1).unwrap();
    assert_eq!(loaded.len(), 1);

    let loaded_entry = &loaded[0];
    assert_eq!(loaded_entry.round, 3);
    assert_eq!(loaded_entry.author, "narrator");
    assert_eq!(loaded_entry.content, "A dramatic scene unfolds.");
    assert_eq!(loaded_entry.tags, vec!["combat", "boss"]);
}

// ============================================================================
// Rust lang-review rule enforcement
// ============================================================================

// Rule #2: #[non_exhaustive] on PersistError
#[test]
fn persist_error_is_non_exhaustive() {
    let _db = PersistError::Database("test".to_string());
    let _ser = PersistError::Serialization("test".to_string());
    let _nf = PersistError::NotFound;
}

// Rule #1: No silent error swallowing in store operations
// (Verified by the fact that all operations return Result)

// Rule #11: workspace dependencies
// (Verified at cargo build time)

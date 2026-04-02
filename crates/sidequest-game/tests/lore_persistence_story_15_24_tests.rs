//! Tests for story 15-24: Persist LoreFragments to SQLite.
//!
//! Verifies that lore fragments accumulated during gameplay survive
//! session close/reopen via the lore_fragments table in save.db.

use std::collections::HashMap;

use sidequest_game::{
    LoreCategory, LoreFragment, LoreSource, SqliteStore,
};

fn temp_store() -> (SqliteStore, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let store = SqliteStore::open(db_path.to_str().unwrap()).unwrap();
    store.init_session("test_genre", "test_world").unwrap();
    (store, dir)
}

fn test_fragment(id: &str, category: LoreCategory, content: &str, source: LoreSource, turn: Option<u64>) -> LoreFragment {
    LoreFragment::new(
        id.to_string(),
        category,
        content.to_string(),
        source,
        turn,
        HashMap::new(),
    )
}

#[test]
fn append_and_load_single_fragment() {
    let (store, _dir) = temp_store();
    let fragment = test_fragment("evt-1-abc", LoreCategory::Event, "The hero slew the dragon", LoreSource::GameEvent, Some(5));

    store.append_lore_fragment(&fragment).unwrap();
    let loaded = store.load_lore_fragments().unwrap();

    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].id(), "evt-1-abc");
    assert_eq!(loaded[0].content(), "The hero slew the dragon");
    assert_eq!(loaded[0].category(), &LoreCategory::Event);
    assert_eq!(loaded[0].source(), &LoreSource::GameEvent);
    assert_eq!(loaded[0].turn_created(), Some(5));
}

#[test]
fn append_multiple_fragments_preserves_all() {
    let (store, _dir) = temp_store();

    store.append_lore_fragment(&test_fragment("f1", LoreCategory::History, "The kingdom fell", LoreSource::GenrePack, None)).unwrap();
    store.append_lore_fragment(&test_fragment("f2", LoreCategory::Geography, "A mountain pass", LoreSource::GameEvent, Some(3))).unwrap();
    store.append_lore_fragment(&test_fragment("f3", LoreCategory::Character, "A mysterious stranger", LoreSource::CharacterCreation, Some(1))).unwrap();

    let loaded = store.load_lore_fragments().unwrap();
    assert_eq!(loaded.len(), 3);
}

#[test]
fn duplicate_id_silently_ignored() {
    let (store, _dir) = temp_store();
    let fragment = test_fragment("dup-1", LoreCategory::Event, "First version", LoreSource::GameEvent, Some(1));

    store.append_lore_fragment(&fragment).unwrap();
    // Same ID, different content — INSERT OR IGNORE should skip it
    let dup = test_fragment("dup-1", LoreCategory::Faction, "Second version", LoreSource::GameEvent, Some(2));
    store.append_lore_fragment(&dup).unwrap();

    let loaded = store.load_lore_fragments().unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].content(), "First version"); // original preserved
}

#[test]
fn empty_table_returns_empty_vec() {
    let (store, _dir) = temp_store();
    let loaded = store.load_lore_fragments().unwrap();
    assert!(loaded.is_empty());
}

#[test]
fn metadata_roundtrips_through_json() {
    let (store, _dir) = temp_store();
    let mut meta = HashMap::new();
    meta.insert("location".to_string(), "dark_forest".to_string());
    meta.insert("event_type".to_string(), "combat".to_string());

    let fragment = LoreFragment::new(
        "meta-1".to_string(),
        LoreCategory::Event,
        "A battle in the forest".to_string(),
        LoreSource::GameEvent,
        Some(10),
        meta,
    );

    store.append_lore_fragment(&fragment).unwrap();
    let loaded = store.load_lore_fragments().unwrap();

    assert_eq!(loaded[0].metadata().get("location").unwrap(), "dark_forest");
    assert_eq!(loaded[0].metadata().get("event_type").unwrap(), "combat");
}

#[test]
fn all_categories_roundtrip() {
    let (store, _dir) = temp_store();
    let categories = vec![
        ("h", LoreCategory::History),
        ("g", LoreCategory::Geography),
        ("f", LoreCategory::Faction),
        ("c", LoreCategory::Character),
        ("i", LoreCategory::Item),
        ("e", LoreCategory::Event),
        ("l", LoreCategory::Language),
        ("x", LoreCategory::Custom("Prophecy".to_string())),
    ];

    for (id, cat) in &categories {
        store.append_lore_fragment(&test_fragment(id, cat.clone(), "test", LoreSource::GameEvent, None)).unwrap();
    }

    let loaded = store.load_lore_fragments().unwrap();
    assert_eq!(loaded.len(), 8);

    // Custom category should survive roundtrip
    let custom = loaded.iter().find(|f| f.id() == "x").unwrap();
    assert_eq!(custom.category(), &LoreCategory::Custom("Prophecy".to_string()));
}

#[test]
fn all_sources_roundtrip() {
    let (store, _dir) = temp_store();

    store.append_lore_fragment(&test_fragment("s1", LoreCategory::Event, "a", LoreSource::GenrePack, None)).unwrap();
    store.append_lore_fragment(&test_fragment("s2", LoreCategory::Event, "b", LoreSource::CharacterCreation, None)).unwrap();
    store.append_lore_fragment(&test_fragment("s3", LoreCategory::Event, "c", LoreSource::GameEvent, None)).unwrap();

    let loaded = store.load_lore_fragments().unwrap();
    assert_eq!(loaded[0].source(), &LoreSource::GenrePack);
    assert_eq!(loaded[1].source(), &LoreSource::CharacterCreation);
    assert_eq!(loaded[2].source(), &LoreSource::GameEvent);
}

#[test]
fn fragments_survive_store_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("reopen.db");

    // First session: write a fragment
    {
        let store = SqliteStore::open(db_path.to_str().unwrap()).unwrap();
        store.init_session("genre", "world").unwrap();
        store.append_lore_fragment(&test_fragment("persist-1", LoreCategory::History, "The war began", LoreSource::GameEvent, Some(1))).unwrap();
    }

    // Second session: reopen and verify
    {
        let store = SqliteStore::open(db_path.to_str().unwrap()).unwrap();
        store.init_session("genre", "world").unwrap();
        let loaded = store.load_lore_fragments().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id(), "persist-1");
        assert_eq!(loaded[0].content(), "The war began");
    }
}

#[test]
fn token_estimate_recomputed_on_load() {
    let (store, _dir) = temp_store();
    let content = "a".repeat(100); // 100 chars → 25 tokens
    store.append_lore_fragment(&test_fragment("tok-1", LoreCategory::Event, &content, LoreSource::GameEvent, Some(1))).unwrap();

    let loaded = store.load_lore_fragments().unwrap();
    assert_eq!(loaded[0].token_estimate(), 25);
}

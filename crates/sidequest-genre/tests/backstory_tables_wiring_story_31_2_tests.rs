//! Story 31-2: Wiring tests for backstory_tables.yaml loading.
//!
//! These tests verify that the genre loader correctly reads backstory_tables.yaml
//! from real genre pack directories and populates the GenrePack::backstory_tables field.
//! This is the integration/wiring test that the builder-level tests defer to.

use std::path::PathBuf;

fn genre_packs_path() -> PathBuf {
    if let Ok(p) = std::env::var("GENRE_PACKS_PATH") {
        PathBuf::from(p)
    } else {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest.join("../../../sidequest-content/genre_packs")
    }
}

// ============================================================================
// Wiring: C&C genre pack loads backstory_tables
// ============================================================================

#[test]
fn caverns_and_claudes_has_backstory_tables() {
    let path = genre_packs_path().join("caverns_and_claudes");
    let pack = sidequest_genre::load_genre_pack(&path)
        .expect("caverns_and_claudes should load successfully");

    assert!(
        pack.backstory_tables.is_some(),
        "caverns_and_claudes must have backstory_tables loaded from backstory_tables.yaml"
    );
}

#[test]
fn caverns_backstory_tables_has_template() {
    let path = genre_packs_path().join("caverns_and_claudes");
    let pack = sidequest_genre::load_genre_pack(&path).unwrap();
    let tables = pack.backstory_tables.as_ref().expect("backstory_tables must be Some");

    assert!(
        !tables.template.is_empty(),
        "backstory_tables.template must not be empty"
    );
    assert!(
        tables.template.contains('{'),
        "backstory_tables.template must contain at least one placeholder. Got: {}",
        tables.template
    );
}

#[test]
fn caverns_backstory_tables_has_expected_table_keys() {
    let path = genre_packs_path().join("caverns_and_claudes");
    let pack = sidequest_genre::load_genre_pack(&path).unwrap();
    let tables = pack.backstory_tables.as_ref().expect("backstory_tables must be Some");

    // The C&C backstory_tables.yaml has trade, feature, reason tables
    assert!(
        tables.tables.contains_key("trade"),
        "backstory_tables must contain 'trade' table. Keys present: {:?}",
        tables.tables.keys().collect::<Vec<_>>()
    );
    assert!(
        tables.tables.contains_key("feature"),
        "backstory_tables must contain 'feature' table. Keys present: {:?}",
        tables.tables.keys().collect::<Vec<_>>()
    );
    assert!(
        tables.tables.contains_key("reason"),
        "backstory_tables must contain 'reason' table. Keys present: {:?}",
        tables.tables.keys().collect::<Vec<_>>()
    );
}

#[test]
fn caverns_backstory_tables_entries_are_nonempty() {
    let path = genre_packs_path().join("caverns_and_claudes");
    let pack = sidequest_genre::load_genre_pack(&path).unwrap();
    let tables = pack.backstory_tables.as_ref().expect("backstory_tables must be Some");

    for (key, entries) in &tables.tables {
        assert!(
            !entries.is_empty(),
            "Table '{}' must have at least one entry",
            key
        );
        for (i, entry) in entries.iter().enumerate() {
            assert!(
                !entry.trim().is_empty(),
                "Table '{}' entry {} must not be blank",
                key, i
            );
        }
    }
}

// ============================================================================
// Wiring: genre pack without backstory_tables.yaml loads cleanly
// ============================================================================

#[test]
fn genre_without_backstory_tables_yaml_loads_as_none() {
    // mutant_wasteland doesn't have backstory_tables.yaml (spoilable test fixture)
    let path = genre_packs_path().join("mutant_wasteland");
    let pack = sidequest_genre::load_genre_pack(&path)
        .expect("mutant_wasteland should load successfully");

    assert!(
        pack.backstory_tables.is_none(),
        "Genre pack without backstory_tables.yaml should have backstory_tables = None"
    );
}

// ============================================================================
// Deserializer: non-string entries should error, not silently drop
// ============================================================================

#[test]
fn backstory_tables_deserializer_rejects_non_string_table_entries() {
    // A table with a mix of strings and integers — the deserializer should
    // either error or at minimum preserve the valid string entries.
    // Current behavior: silently drops non-strings. This test asserts that
    // non-string entries cause a deserialization error (the fix Avasarala demanded).
    let yaml = r#"
template: "Former {trade}."
trade:
  - ratcatcher
  - 42
  - gravedigger
"#;

    let result: Result<sidequest_genre::BackstoryTables, _> = serde_yaml::from_str(yaml);

    // After the fix: this should be Err because 42 is not a string.
    // If this test passes (result is Err), the silent-drop bug is fixed.
    assert!(
        result.is_err(),
        "Deserializing backstory_tables with non-string entries (42) should error, \
         not silently drop them. Got: {:?}",
        result.unwrap()
    );
}

// ============================================================================
// Template-table key consistency validation
// ============================================================================

#[test]
fn caverns_backstory_tables_template_keys_match_table_keys() {
    let path = genre_packs_path().join("caverns_and_claudes");
    let pack = sidequest_genre::load_genre_pack(&path).unwrap();
    let tables = pack.backstory_tables.as_ref().expect("backstory_tables must be Some");

    // Extract placeholder keys from the template (anything between { and })
    let mut template_keys: Vec<String> = Vec::new();
    let mut chars = tables.template.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            let key: String = chars.by_ref().take_while(|&c| c != '}').collect();
            if !key.is_empty() {
                template_keys.push(key);
            }
        }
    }

    for key in &template_keys {
        assert!(
            tables.tables.contains_key(key),
            "Template placeholder '{{{}}}' has no matching table. \
             Template keys: {:?}, Table keys: {:?}",
            key,
            template_keys,
            tables.tables.keys().collect::<Vec<_>>()
        );
    }
}

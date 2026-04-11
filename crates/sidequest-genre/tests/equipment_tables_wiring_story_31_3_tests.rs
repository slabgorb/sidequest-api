//! Story 31-3: Wiring tests for equipment_tables.yaml loading.
//!
//! These tests verify that the genre loader correctly reads equipment_tables.yaml
//! from real genre pack directories and populates GenrePack::equipment_tables.
//! This is the integration/wiring test that the builder-level tests defer to.
//!
//! Parallels the 31-2 backstory_tables wiring test structure.

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
// Wiring: C&C genre pack loads equipment_tables
// ============================================================================

#[test]
fn caverns_and_claudes_has_equipment_tables() {
    let path = genre_packs_path().join("caverns_and_claudes");
    let pack = sidequest_genre::load_genre_pack(&path)
        .expect("caverns_and_claudes should load successfully");

    assert!(
        pack.equipment_tables.is_some(),
        "caverns_and_claudes must have equipment_tables loaded from equipment_tables.yaml — \
         the `the_kit` scene directive `equipment_generation: random_table` needs data to consume"
    );
}

#[test]
fn caverns_equipment_tables_is_nonempty() {
    let path = genre_packs_path().join("caverns_and_claudes");
    let pack = sidequest_genre::load_genre_pack(&path).unwrap();
    let tables = pack
        .equipment_tables
        .as_ref()
        .expect("equipment_tables must be Some");

    assert!(
        !tables.tables.is_empty(),
        "equipment_tables.tables must have at least one slot defined"
    );
}

#[test]
fn caverns_equipment_tables_slot_entries_are_nonempty_and_nonblank() {
    let path = genre_packs_path().join("caverns_and_claudes");
    let pack = sidequest_genre::load_genre_pack(&path).unwrap();
    let tables = pack
        .equipment_tables
        .as_ref()
        .expect("equipment_tables must be Some");

    for (slot, entries) in &tables.tables {
        assert!(
            !entries.is_empty(),
            "Slot '{}' must have at least one candidate item_id",
            slot
        );
        for (i, entry) in entries.iter().enumerate() {
            assert!(
                !entry.trim().is_empty(),
                "Slot '{}' entry {} must not be blank",
                slot,
                i
            );
        }
    }
}

// ============================================================================
// Wiring: every referenced item_id exists in the inventory.item_catalog
// This is the cross-file consistency check — tables that reference nonexistent
// items would silently produce blank-named inventory on the builder side.
// ============================================================================

#[test]
fn caverns_equipment_tables_reference_only_catalog_items() {
    let path = genre_packs_path().join("caverns_and_claudes");
    let pack = sidequest_genre::load_genre_pack(&path).unwrap();

    let inventory = pack
        .inventory
        .as_ref()
        .expect("caverns_and_claudes must have inventory.yaml loaded");
    let catalog_ids: std::collections::HashSet<&str> = inventory
        .item_catalog
        .iter()
        .map(|item| item.id.as_str())
        .collect();

    let tables = pack
        .equipment_tables
        .as_ref()
        .expect("equipment_tables must be Some");

    let mut missing: Vec<(String, String)> = Vec::new();
    for (slot, entries) in &tables.tables {
        for entry in entries {
            if !catalog_ids.contains(entry.as_str()) {
                missing.push((slot.clone(), entry.clone()));
            }
        }
    }

    assert!(
        missing.is_empty(),
        "equipment_tables references item_ids that are not in inventory.item_catalog: {:?}. \
         Every table entry must resolve to a real catalog item.",
        missing
    );
}

// ============================================================================
// Wiring: every rolls_per_slot key must correspond to an existing slot
// ============================================================================

#[test]
fn caverns_equipment_tables_rolls_per_slot_keys_match_slots() {
    let path = genre_packs_path().join("caverns_and_claudes");
    let pack = sidequest_genre::load_genre_pack(&path).unwrap();
    let tables = pack
        .equipment_tables
        .as_ref()
        .expect("equipment_tables must be Some");

    for key in tables.rolls_per_slot.keys() {
        assert!(
            tables.tables.contains_key(key),
            "rolls_per_slot key '{}' does not match any slot in tables. Slots: {:?}",
            key,
            tables.tables.keys().collect::<Vec<_>>()
        );
    }
}

// ============================================================================
// Wiring: genre pack without equipment_tables.yaml loads cleanly as None
// ============================================================================

#[test]
fn genre_without_equipment_tables_yaml_loads_as_none() {
    // mutant_wasteland is used as the "no backstory_tables" fixture for 31-2.
    // It likewise must not have equipment_tables.yaml — so this test verifies
    // the optional-file pattern holds for the new loader field.
    let path = genre_packs_path().join("mutant_wasteland");
    let pack =
        sidequest_genre::load_genre_pack(&path).expect("mutant_wasteland should load successfully");

    assert!(
        pack.equipment_tables.is_none(),
        "Genre pack without equipment_tables.yaml should have equipment_tables = None, \
         not an empty-tables Some(..). Silent fallbacks violate project rules."
    );
}

// ============================================================================
// Deserializer: missing 'tables' field must fail loudly, not silently default
// ============================================================================

#[test]
fn equipment_tables_deserializer_rejects_missing_tables_field() {
    let yaml = r#"
rolls_per_slot:
  utility: 2
"#;
    let result: Result<sidequest_genre::EquipmentTables, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_err(),
        "EquipmentTables without 'tables' field must fail to deserialize, \
         not silently construct an empty-tables value. Got: {:?}",
        result
    );
}

#[test]
fn equipment_tables_deserializer_accepts_missing_rolls_per_slot() {
    // rolls_per_slot is optional (defaults to empty map) — tables is required.
    let yaml = r#"
tables:
  weapon:
    - dagger
"#;
    let result: Result<sidequest_genre::EquipmentTables, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_ok(),
        "EquipmentTables without 'rolls_per_slot' must deserialize (field is optional). Got: {:?}",
        result.err()
    );
    let parsed = result.unwrap();
    assert!(
        parsed.rolls_per_slot.is_empty(),
        "Missing rolls_per_slot should default to empty HashMap"
    );
}

// ============================================================================
// REWORK (after Reviewer rejection 2026-04-10): AudioConfig/MixerConfig
// must reject unknown fields to honor CLAUDE.md "No Silent Fallbacks".
//
// The initial Dev fix (making voice_volume/duck_music_for_voice/
// creature_voice_presets optional with defaults) was correct for content
// that legitimately dropped those fields, but both parent structs lack
// #[serde(deny_unknown_fields)]. Typos in adjacent keys silently drop.
// These tests will fail until Dev adds the attribute.
// ============================================================================

#[test]
fn mixer_config_rejects_unknown_fields() {
    // A mixer with a typo'd key ("musik_volume" instead of "music_volume")
    // must hard-fail, not silently default music_volume to 0.0 and drop the
    // typo. This is the failure mode CLAUDE.md "No Silent Fallbacks" targets.
    let yaml = r#"
music_volume: 0.8
sfx_volume: 0.9
duck_amount_db: -6.0
crossfade_default_ms: 2000
musik_volume: 0.3
"#;
    let result: Result<sidequest_genre::MixerConfig, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_err(),
        "MixerConfig must reject unknown fields via #[serde(deny_unknown_fields)]. \
         Typo'd 'musik_volume' should fail to deserialize, not silently default. Got: {:?}",
        result
    );
}

#[test]
fn audio_config_rejects_unknown_fields() {
    // Top-level AudioConfig must also reject unknown fields. A typo at the
    // audio.yaml root (e.g., "mod_tracks" instead of "mood_tracks") would
    // silently wipe every mood track binding in the genre pack.
    let yaml = r#"
mood_tracks: {}
sfx_library: {}
mixer:
  music_volume: 0.8
  sfx_volume: 0.9
  duck_amount_db: -6.0
  crossfade_default_ms: 2000
mod_tracks: {}
"#;
    let result: Result<sidequest_genre::AudioConfig, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_err(),
        "AudioConfig must reject unknown fields via #[serde(deny_unknown_fields)]. \
         Typo'd 'mod_tracks' should fail to deserialize. Got: {:?}",
        result
    );
}

#[test]
fn mixer_config_still_accepts_missing_voice_volume() {
    // Regression guard: after adding deny_unknown_fields, the TTS-removal
    // schema-drift fix must still work — a mixer without voice_volume,
    // duck_music_for_voice, or creature_voice_presets must still deserialize.
    let yaml = r#"
music_volume: 0.3
sfx_volume: 0.7
duck_amount_db: -15.0
crossfade_default_ms: 2000
"#;
    let result: Result<sidequest_genre::MixerConfig, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_ok(),
        "MixerConfig must still deserialize without voice_volume / duck_music_for_voice \
         (the TTS-removal schema-drift fix). Got: {:?}",
        result.err()
    );
    let mixer = result.unwrap();
    assert_eq!(
        mixer.voice_volume, 1.0,
        "voice_volume should default to 1.0"
    );
    assert!(
        !mixer.duck_music_for_voice,
        "duck_music_for_voice should default to false"
    );
}

#[test]
fn audio_config_still_accepts_missing_creature_voice_presets() {
    let yaml = r#"
mood_tracks: {}
sfx_library: {}
mixer:
  music_volume: 0.8
  sfx_volume: 0.9
  duck_amount_db: -6.0
  crossfade_default_ms: 2000
"#;
    let result: Result<sidequest_genre::AudioConfig, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_ok(),
        "AudioConfig must still deserialize without creature_voice_presets (TTS removal). \
         Got: {:?}",
        result.err()
    );
    let config = result.unwrap();
    assert!(
        config.creature_voice_presets.is_empty(),
        "creature_voice_presets should default to empty HashMap"
    );
}

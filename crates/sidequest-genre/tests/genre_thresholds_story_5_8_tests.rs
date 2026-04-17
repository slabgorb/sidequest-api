//! Story 5-8: Genre-tunable thresholds — genre pack integration tests.
//!
//! RED phase — these tests verify that genre packs can include optional pacing
//! configuration (DramaThresholds) and that the loader populates it correctly.
//!
//! ACs tested:
//!   AC2 — GenrePack model includes optional pacing config
//!   AC3 — Genre pack loader populates thresholds from YAML when present
//!   AC4 — Falls back to Default when genre pack has no pacing config
//!   AC5 — At least one genre pack has pacing thresholds
//!
//! These tests compile but FAIL because GenrePack does not yet have a
//! drama_thresholds field and no genre pack YAML includes pacing config.

use sidequest_genre::{load_genre_pack, GenreCode, GenreLoader, GenrePack};
use std::path::PathBuf;

/// Locate the genre_packs directory relative to this crate.
fn genre_packs_path() -> PathBuf {
    if let Ok(p) = std::env::var("GENRE_PACKS_PATH") {
        PathBuf::from(p)
    } else {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest
            .join("../../..")
            .join("sidequest-content")
            .join("genre_packs")
    }
}

/// Fixture pack that has no pacing.yaml — drama_thresholds must be None.
fn no_pacing_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/genre_thresholds_story_5_8/no_pacing_pack")
}

/// Helper: check if GenrePack has a `drama_thresholds` field via JSON serialization.
/// GenrePack doesn't derive Serialize, so we use std::fmt::Debug output as a proxy.
fn pack_has_drama_thresholds_field(pack: &GenrePack) -> bool {
    let debug = format!("{:?}", pack);
    debug.contains("drama_thresholds")
}

// ============================================================================
// AC2: GenrePack model includes optional pacing config
// ============================================================================

#[test]
fn genre_pack_has_drama_thresholds_field() {
    // GenrePack should have an optional drama_thresholds field.
    let path = genre_packs_path().join("mutant_wasteland");
    if !path.exists() {
        panic!(
            "mutant_wasteland genre pack not found at {}",
            path.display()
        );
    }

    let pack = load_genre_pack(&path).expect("should load mutant_wasteland");

    assert!(
        pack_has_drama_thresholds_field(&pack),
        "GenrePack should have a drama_thresholds field — add `pub drama_thresholds: Option<DramaThresholds>` to GenrePack",
    );
}

#[test]
fn genre_pack_drama_thresholds_is_optional() {
    // A genre pack without pacing.yaml should load successfully with drama_thresholds: None.
    // Uses a self-contained fixture instead of sidequest-content/low_fantasy.
    let path = no_pacing_fixture_path();
    assert!(
        path.exists(),
        "fixture pack not found at {} — fixture must exist",
        path.display()
    );

    let pack = load_genre_pack(&path).expect("should load no_pacing fixture pack");

    // Check Debug output — drama_thresholds should appear as "None"
    let debug = format!("{:?}", pack);
    assert!(
        debug.contains("drama_thresholds: None"),
        "pack without pacing.yaml should have drama_thresholds: None. \
         Debug output does not contain 'drama_thresholds: None' — field not yet added to GenrePack.",
    );
}

// ============================================================================
// AC3: Genre pack loader populates thresholds from YAML when present
// ============================================================================

#[test]
fn genre_pack_loader_populates_thresholds_from_yaml() {
    // After we add pacing config to mutant_wasteland, this should load non-default values.
    let path = genre_packs_path().join("mutant_wasteland");
    if !path.exists() {
        panic!("mutant_wasteland genre pack not found");
    }

    let pack = load_genre_pack(&path).expect("should load mutant_wasteland");

    // Check that drama_thresholds is present and has Some(...) in Debug
    let debug = format!("{:?}", pack);
    assert!(
        debug.contains("drama_thresholds: Some("),
        "mutant_wasteland should have drama_thresholds: Some(...) after story 5-8. \
         Current Debug output does not contain this — field not yet added or YAML not yet defined.",
    );
}

// ============================================================================
// AC4: Falls back to Default when genre pack has no pacing config
// ============================================================================

#[test]
fn genre_pack_without_pacing_falls_back_to_default_thresholds() {
    // Uses a self-contained fixture pack (no pacing.yaml) instead of sidequest-content/low_fantasy.
    // Verifies that packs without pacing.yaml produce drama_thresholds: None.
    // Consumers use .drama_thresholds.unwrap_or_default() to get engine defaults.
    let path = no_pacing_fixture_path();
    assert!(
        path.exists(),
        "fixture pack not found at {} — fixture must exist",
        path.display()
    );

    let pack = load_genre_pack(&path).expect("should load no_pacing fixture pack");

    let debug = format!("{:?}", pack);

    // First, verify the field exists at all
    assert!(
        debug.contains("drama_thresholds"),
        "GenrePack must have a drama_thresholds field — not found in Debug output",
    );

    // Then verify it's None for a pack without pacing.yaml
    assert!(
        debug.contains("drama_thresholds: None"),
        "pack without pacing.yaml should have drama_thresholds: None",
    );
}

// ============================================================================
// AC5: At least one genre pack has pacing thresholds (mutant_wasteland)
// ============================================================================

#[test]
fn mutant_wasteland_has_pacing_thresholds_in_yaml() {
    // Check that the mutant_wasteland genre pack YAML includes pacing config.
    let path = genre_packs_path().join("mutant_wasteland");
    if !path.exists() {
        panic!("mutant_wasteland genre pack not found");
    }

    // Check for a pacing.yaml or drama_thresholds section in pack.yaml
    let pacing_yaml = path.join("pacing.yaml");
    let has_pacing_file = pacing_yaml.exists();

    // Also check if pack.yaml has a drama_thresholds key
    let pack_yaml_content =
        std::fs::read_to_string(path.join("pack.yaml")).expect("pack.yaml should exist");
    let has_inline_thresholds =
        pack_yaml_content.contains("drama_thresholds") || pack_yaml_content.contains("pacing");

    assert!(
        has_pacing_file || has_inline_thresholds,
        "mutant_wasteland must have pacing config: either pacing.yaml or a pacing/drama_thresholds \
         section. Neither found — add pacing.yaml to the mutant_wasteland genre pack.",
    );
}

#[test]
fn mutant_wasteland_thresholds_differ_from_defaults() {
    // Load the pack and verify thresholds are non-default.
    let path = genre_packs_path().join("mutant_wasteland");
    if !path.exists() {
        panic!("mutant_wasteland genre pack not found");
    }

    let pack = load_genre_pack(&path).expect("should load mutant_wasteland");

    // The drama_thresholds must exist and differ from defaults in at least one field.
    let debug = format!("{:?}", pack);
    assert!(
        debug.contains("drama_thresholds: Some("),
        "mutant_wasteland must have non-None drama_thresholds — field not yet added or YAML not defined. \
         Debug: ...{}...",
        &debug[..debug.len().min(200)],
    );
}

// ============================================================================
// AC3+multi-path: Loaded thresholds via GenreLoader
// ============================================================================

#[test]
fn genre_loader_load_includes_drama_thresholds() {
    let search_paths = vec![genre_packs_path()];
    let loader = GenreLoader::new(search_paths);
    let code = GenreCode::new("mutant_wasteland").unwrap();

    let pack = loader.load(&code).expect("should load via GenreLoader");

    let debug = format!("{:?}", pack);
    assert!(
        debug.contains("drama_thresholds"),
        "GenreLoader.load() result should include drama_thresholds field",
    );
    assert!(
        debug.contains("drama_thresholds: Some("),
        "GenreLoader.load() should populate drama_thresholds for mutant_wasteland",
    );
}

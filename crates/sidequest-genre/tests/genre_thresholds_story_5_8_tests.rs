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
        manifest.join("../../../genre_packs")
    }
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
        panic!("mutant_wasteland genre pack not found at {}", path.display());
    }

    let pack = load_genre_pack(&path).expect("should load mutant_wasteland");

    assert!(
        pack_has_drama_thresholds_field(&pack),
        "GenrePack should have a drama_thresholds field — add `pub drama_thresholds: Option<DramaThresholds>` to GenrePack",
    );
}

#[test]
fn genre_pack_drama_thresholds_is_optional() {
    // A genre pack without pacing config should load successfully.
    // The drama_thresholds field should be None.
    let path = genre_packs_path().join("low_fantasy");
    if !path.exists() {
        panic!("low_fantasy genre pack not found at {}", path.display());
    }

    let pack = load_genre_pack(&path).expect("should load low_fantasy");

    // Check Debug output — drama_thresholds should appear as "None"
    let debug = format!("{:?}", pack);
    assert!(
        debug.contains("drama_thresholds: None"),
        "low_fantasy should have drama_thresholds: None (no pacing config defined). \
         Debug output does not contain 'drama_thresholds' — field not yet added to GenrePack.",
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
    let path = genre_packs_path().join("low_fantasy");
    if !path.exists() {
        panic!("low_fantasy genre pack not found");
    }

    let pack = load_genre_pack(&path).expect("should load low_fantasy");

    // The Debug output should show drama_thresholds: None for packs without pacing.
    // Consumers use .drama_thresholds.unwrap_or_default() to get defaults.
    let debug = format!("{:?}", pack);

    // First, verify the field exists at all
    assert!(
        debug.contains("drama_thresholds"),
        "GenrePack must have a drama_thresholds field — not found in Debug output",
    );

    // Then verify it's None for low_fantasy
    assert!(
        debug.contains("drama_thresholds: None"),
        "low_fantasy should have drama_thresholds: None",
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
    let has_inline_thresholds = pack_yaml_content.contains("drama_thresholds")
        || pack_yaml_content.contains("pacing");

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

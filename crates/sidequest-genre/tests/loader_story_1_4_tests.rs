//! Failing tests for Story 1-4: Genre loader — unified loading, two-phase validation.
//!
//! These tests exercise the features that 1-4 must implement:
//! - GenreCode newtype with validated construction
//! - Error aggregation (collect all errors, report all at once)
//! - Multi-path file search (local → home → install)
//! - GenreCache (load once, return same Arc for same code)
//! - GenreLoader trait abstraction
//!
//! All tests are expected to FAIL (RED state) until Dev implements the features.

use sidequest_genre::{GenreCache, GenreCode, GenreError, GenreLoader, ValidationErrors};
use std::path::PathBuf;
use std::sync::Arc;

/// Locate the genre_packs directory relative to this crate.
fn genre_packs_path() -> PathBuf {
    if let Ok(p) = std::env::var("GENRE_PACKS_PATH") {
        PathBuf::from(p)
    } else {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest.join("../../../genre_packs")
    }
}

// ═══════════════════════════════════════════════════════════
// AC: GenreCode newtype — validated by GenreCode::new()
// ═══════════════════════════════════════════════════════════

// Rule #5: Unvalidated constructors at trust boundaries
// Rule #9: Public fields on types with invariants

#[test]
fn genre_code_new_accepts_valid_snake_case_code() {
    let code = GenreCode::new("mutant_wasteland").expect("valid snake_case genre code");
    assert_eq!(code.as_str(), "mutant_wasteland");
}

#[test]
fn genre_code_new_accepts_single_word_code() {
    let code = GenreCode::new("fantasy").expect("single word is valid");
    assert_eq!(code.as_str(), "fantasy");
}

#[test]
fn genre_code_new_rejects_empty_string() {
    let result = GenreCode::new("");
    assert!(result.is_err(), "empty string should be rejected");
}

#[test]
fn genre_code_new_rejects_whitespace_only() {
    let result = GenreCode::new("   ");
    assert!(result.is_err(), "whitespace-only should be rejected");
}

#[test]
fn genre_code_new_rejects_string_with_spaces() {
    let result = GenreCode::new("mutant wasteland");
    assert!(
        result.is_err(),
        "spaces are not valid in genre codes (must be snake_case)"
    );
}

#[test]
fn genre_code_new_rejects_uppercase() {
    let result = GenreCode::new("MutantWasteland");
    assert!(
        result.is_err(),
        "uppercase characters are not valid in genre codes"
    );
}

#[test]
fn genre_code_new_rejects_special_characters() {
    let result = GenreCode::new("mutant-wasteland");
    assert!(
        result.is_err(),
        "hyphens are not valid in genre codes (use underscores)"
    );
}

#[test]
fn genre_code_new_rejects_leading_underscore() {
    let result = GenreCode::new("_mutant_wasteland");
    assert!(result.is_err(), "leading underscore should be rejected");
}

#[test]
fn genre_code_new_rejects_trailing_underscore() {
    let result = GenreCode::new("mutant_wasteland_");
    assert!(result.is_err(), "trailing underscore should be rejected");
}

// Rule #8: #[derive(Deserialize)] bypassing validated constructors
// Rule #13: Constructor/Deserialize validation consistency

#[test]
fn genre_code_deserialize_rejects_empty_string() {
    let json = r#""""#;
    let result: Result<GenreCode, _> = serde_json::from_str(json);
    assert!(
        result.is_err(),
        "deserializing empty string should fail — must go through validation"
    );
}

#[test]
fn genre_code_deserialize_rejects_uppercase() {
    let json = r#""MutantWasteland""#;
    let result: Result<GenreCode, _> = serde_json::from_str(json);
    assert!(
        result.is_err(),
        "deserializing uppercase code should fail — same rules as new()"
    );
}

#[test]
fn genre_code_deserialize_accepts_valid_code() {
    let json = r#""low_fantasy""#;
    let code: GenreCode = serde_json::from_str(json).expect("valid code should deserialize");
    assert_eq!(code.as_str(), "low_fantasy");
}

#[test]
fn genre_code_serializes_as_plain_string() {
    let code = GenreCode::new("mutant_wasteland").unwrap();
    let json = serde_json::to_string(&code).unwrap();
    assert_eq!(json, r#""mutant_wasteland""#);
}

// ═══════════════════════════════════════════════════════════
// AC: Error aggregation — collect all errors, report all at once
// ═══════════════════════════════════════════════════════════

#[test]
fn validation_errors_collects_multiple_errors() {
    let mut errors = ValidationErrors::new();
    errors.push(GenreError::ValidationError {
        message: "error one".into(),
    });
    errors.push(GenreError::ValidationError {
        message: "error two".into(),
    });
    assert_eq!(errors.len(), 2, "should hold multiple errors");
    assert!(!errors.is_empty());
}

#[test]
fn validation_errors_into_result_returns_ok_when_empty() {
    let errors = ValidationErrors::new();
    let result: Result<(), ValidationErrors> = errors.into_result();
    assert!(result.is_ok(), "empty errors should produce Ok(())");
}

#[test]
fn validation_errors_into_result_returns_err_when_nonempty() {
    let mut errors = ValidationErrors::new();
    errors.push(GenreError::ValidationError {
        message: "some error".into(),
    });
    let result: Result<(), ValidationErrors> = errors.into_result();
    assert!(result.is_err(), "non-empty errors should produce Err");
}

#[test]
fn validate_reports_all_errors_not_just_first() {
    // Load a valid pack, inject TWO bad cross-references,
    // and verify validate() reports BOTH (not just the first)
    let path = genre_packs_path().join("mutant_wasteland");
    if !path.exists() {
        panic!("mutant_wasteland not found — required for this test");
    }

    let mut pack = sidequest_genre::load_genre_pack(&path).unwrap();

    // Inject bad achievement reference
    let bad_achievement: sidequest_genre::Achievement = serde_yaml::from_str(
        r#"
id: bad_ach_1
name: Bad Achievement
description: References nonexistent trope
trope_id: totally_fake_trope
trigger_status: activated
emoji: "❌"
"#,
    )
    .unwrap();
    pack.achievements.push(bad_achievement);

    // Inject bad cartography reference
    if let Some(world) = pack.worlds.get_mut("flickering_reach") {
        if let Some(region) = world.cartography.regions.get_mut("toods_dome") {
            region.adjacent.push("phantom_region_999".to_string());
        }
    }

    // validate() should return ALL errors, not fail-on-first
    let result = pack.validate();
    assert!(result.is_err(), "should detect validation errors");

    // The error type should contain multiple errors
    match result {
        Err(errors) => {
            assert!(
                errors.len() >= 2,
                "should report at least 2 errors (achievement + cartography), got {}",
                errors.len()
            );
        }
        Ok(()) => panic!("expected validation errors"),
    }
}

// ═══════════════════════════════════════════════════════════
// AC: File search — try local, home, install paths in order
// ═══════════════════════════════════════════════════════════

#[test]
fn genre_loader_finds_pack_in_first_search_path() {
    let search_paths = vec![genre_packs_path()];
    let loader = GenreLoader::new(search_paths);
    let code = GenreCode::new("mutant_wasteland").unwrap();

    let result = loader.find(&code);
    assert!(
        result.is_ok(),
        "should find mutant_wasteland in genre_packs/"
    );

    let found_path = result.unwrap();
    assert!(
        found_path.ends_with("mutant_wasteland"),
        "found path should end with genre code"
    );
}

#[test]
fn genre_loader_searches_paths_in_order() {
    // First path doesn't have the pack, second does
    let search_paths = vec![PathBuf::from("/nonexistent/first/path"), genre_packs_path()];
    let loader = GenreLoader::new(search_paths);
    let code = GenreCode::new("mutant_wasteland").unwrap();

    let result = loader.find(&code);
    assert!(
        result.is_ok(),
        "should find pack in second search path after first fails"
    );
}

#[test]
fn genre_loader_returns_error_when_not_found_in_any_path() {
    let search_paths = vec![
        PathBuf::from("/nonexistent/path/a"),
        PathBuf::from("/nonexistent/path/b"),
    ];
    let loader = GenreLoader::new(search_paths);
    let code = GenreCode::new("mutant_wasteland").unwrap();

    let result = loader.find(&code);
    assert!(
        result.is_err(),
        "should error when genre not found in any search path"
    );
}

#[test]
fn genre_loader_returns_error_with_all_searched_paths() {
    let search_paths = vec![
        PathBuf::from("/path/a"),
        PathBuf::from("/path/b"),
        PathBuf::from("/path/c"),
    ];
    let loader = GenreLoader::new(search_paths);
    let code = GenreCode::new("totally_nonexistent").unwrap();

    let result = loader.find(&code);
    let err = result.unwrap_err();
    let err_msg = err.to_string();

    // Error should mention all searched paths
    assert!(
        err_msg.contains("/path/a"),
        "error should list searched path /path/a: {err_msg}"
    );
    assert!(
        err_msg.contains("/path/b"),
        "error should list searched path /path/b: {err_msg}"
    );
}

#[test]
fn genre_loader_load_performs_full_load_and_validate() {
    let search_paths = vec![genre_packs_path()];
    let loader = GenreLoader::new(search_paths);
    let code = GenreCode::new("mutant_wasteland").unwrap();

    let pack = loader.load(&code).expect("should load and validate pack");
    assert_eq!(pack.meta.name.as_str(), "Mutant Wasteland");
}

// ═══════════════════════════════════════════════════════════
// AC: Caching — same GenreCode returns same object (immutable)
// ═══════════════════════════════════════════════════════════

#[test]
fn genre_cache_returns_same_arc_for_same_code() {
    let search_paths = vec![genre_packs_path()];
    let loader = GenreLoader::new(search_paths);
    let cache = GenreCache::new();
    let code = GenreCode::new("mutant_wasteland").unwrap();

    let pack1 = cache.get_or_load(&code, &loader).expect("first load");
    let pack2 = cache.get_or_load(&code, &loader).expect("second load");

    // Both should be the same Arc (pointer equality)
    assert!(
        Arc::ptr_eq(&pack1, &pack2),
        "cache should return the same Arc<GenrePack> for the same code"
    );
}

#[test]
fn genre_cache_stores_different_packs_for_different_codes() {
    let search_paths = vec![genre_packs_path()];
    let loader = GenreLoader::new(search_paths);
    let cache = GenreCache::new();

    let mw_code = GenreCode::new("mutant_wasteland").unwrap();
    let lf_code = GenreCode::new("low_fantasy").unwrap();

    let mw_pack = cache
        .get_or_load(&mw_code, &loader)
        .expect("load mutant_wasteland");
    let lf_pack = cache
        .get_or_load(&lf_code, &loader)
        .expect("load low_fantasy");

    assert!(
        !Arc::ptr_eq(&mw_pack, &lf_pack),
        "different codes should produce different Arc pointers"
    );
    assert_eq!(mw_pack.meta.name.as_str(), "Mutant Wasteland");
    assert_eq!(lf_pack.meta.name.as_str(), "Low Fantasy");
}

#[test]
fn genre_cache_propagates_load_error() {
    let search_paths = vec![PathBuf::from("/nonexistent")];
    let loader = GenreLoader::new(search_paths);
    let cache = GenreCache::new();
    let code = GenreCode::new("mutant_wasteland").unwrap();

    let result = cache.get_or_load(&code, &loader);
    assert!(
        result.is_err(),
        "cache should propagate errors from failed loads"
    );
}

// ═══════════════════════════════════════════════════════════
// AC: Success path — Load valid genre → GenrePack with all fields
// ═══════════════════════════════════════════════════════════

#[test]
fn genre_loader_loads_three_different_packs() {
    let search_paths = vec![genre_packs_path()];
    let loader = GenreLoader::new(search_paths);

    for code_str in &["mutant_wasteland", "low_fantasy", "elemental_harmony"] {
        let code = GenreCode::new(code_str).unwrap();
        let pack = loader
            .load(&code)
            .unwrap_or_else(|e| panic!("should load {code_str}: {e}"));
        // Verify the pack name is populated (not empty/default)
        assert!(
            !pack.meta.name.as_str().is_empty(),
            "{code_str} should have a non-empty name"
        );
        // Verify rules are populated (ability_score_names is always present)
        assert!(
            !pack.rules.ability_score_names.is_empty(),
            "{code_str} should have ability score names"
        );
    }
}

// ═══════════════════════════════════════════════════════════
// AC: Failure path — invalid enum/conflict → multi-error report
// ═══════════════════════════════════════════════════════════

#[test]
fn validate_catches_multiple_cartography_errors_at_once() {
    let path = genre_packs_path().join("mutant_wasteland");
    if !path.exists() {
        panic!("mutant_wasteland required");
    }

    let mut pack = sidequest_genre::load_genre_pack(&path).unwrap();

    // Inject TWO bad adjacent refs in different regions
    if let Some(world) = pack.worlds.get_mut("flickering_reach") {
        if let Some(region) = world.cartography.regions.get_mut("toods_dome") {
            region.adjacent.push("ghost_region_1".to_string());
        }
        if let Some(region) = world.cartography.regions.get_mut("glass_flat") {
            region.adjacent.push("ghost_region_2".to_string());
        }
    }

    let result = pack.validate();
    assert!(result.is_err());
    match result {
        Err(errors) => {
            assert!(
                errors.len() >= 2,
                "should catch both bad adjacent refs, got {} error(s)",
                errors.len()
            );
        }
        Ok(()) => panic!("expected errors"),
    }
}

// ═══════════════════════════════════════════════════════════
// Rule coverage: Rust lang-review rules
// ═══════════════════════════════════════════════════════════

// Rule #2: #[non_exhaustive] is a compile-time guarantee on GenreError.
// It cannot be verified at runtime — the attribute is on the enum definition.

// Rule #11: Workspace dependency compliance
// (This is verified by reading Cargo.toml — all deps use { workspace = true })
// The existing Cargo.toml already uses workspace deps correctly.

// Rule #6: Test quality self-check
// All tests in this file use assert!, assert_eq!, or pattern matching with panic!.
// No `let _ = result;` or `assert!(true)` patterns.

//! Story 16-16: Content audit — all genre packs declare confrontations and resources
//!
//! Validates that every shipped genre pack loads without error and each declares
//! at least one confrontation type. Documents which packs have resource declarations.
//!
//! The pack list tracks `sidequest-content/genre_packs/` — the authoritative
//! inventory of shipped packs. Incomplete packs live in `genre_workshopping/`
//! and are intentionally excluded (see sidequest-content commit 6c28431 / PR #83
//! from 2026-04-16, which moved `low_fantasy`, `neon_dystopia`, `pulp_noir`,
//! `road_warrior`, `spaghetti_western`, and `victoria` to the workshop).

use std::path::PathBuf;

/// Shipped genre pack directory names (mirrors `sidequest-content/genre_packs/`).
const ALL_GENRES: &[&str] = &[
    "caverns_and_claudes",
    "elemental_harmony",
    "heavy_metal",
    "mutant_wasteland",
    "space_opera",
];

/// Genre packs expected to have resource declarations.
///
/// Empty until a shipped pack declares resources. The packs that previously
/// populated this table (`neon_dystopia`, `pulp_noir`, `road_warrior`,
/// `spaghetti_western`, `victoria`) were moved to `genre_workshopping/` and
/// are no longer part of the shipped inventory.
const GENRES_WITH_RESOURCES: &[(&str, &[&str])] = &[];

/// Genre packs expected to have genre-specific confrontation types (beyond negotiation).
const GENRES_WITH_SPECIFIC_CONFRONTATIONS: &[(&str, &[&str])] =
    &[("space_opera", &["ship_combat"])];

fn genre_packs_path() -> PathBuf {
    if let Ok(path) = std::env::var("GENRE_PACKS_PATH") {
        return PathBuf::from(path);
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.join("../../../sidequest-content/genre_packs")
}

// ═══════════════════════════════════════════════════════════
// All shipped genre packs load without error
// ═══════════════════════════════════════════════════════════

#[test]
fn all_genre_packs_load_successfully() {
    let packs_dir = genre_packs_path();
    for genre in ALL_GENRES {
        let pack = sidequest_genre::load_genre_pack(&packs_dir.join(genre));
        assert!(
            pack.is_ok(),
            "genre pack '{}' failed to load: {:?}",
            genre,
            pack.err()
        );
    }
}

// ═══════════════════════════════════════════════════════════
// Every genre pack has at least one confrontation type
// ═══════════════════════════════════════════════════════════

#[test]
fn every_genre_pack_has_at_least_one_confrontation() {
    let packs_dir = genre_packs_path();
    for genre in ALL_GENRES {
        let pack = sidequest_genre::load_genre_pack(&packs_dir.join(genre))
            .unwrap_or_else(|e| panic!("{} failed to load: {e}", genre));
        assert!(
            !pack.rules.confrontations.is_empty(),
            "genre pack '{}' has no confrontation types declared",
            genre
        );
    }
}

// ═══════════════════════════════════════════════════════════
// Every confrontation has valid structure
// ═══════════════════════════════════════════════════════════

#[test]
fn all_confrontations_have_required_fields() {
    let packs_dir = genre_packs_path();
    for genre in ALL_GENRES {
        let pack = sidequest_genre::load_genre_pack(&packs_dir.join(genre))
            .unwrap_or_else(|e| panic!("{} failed to load: {e}", genre));
        for conf in &pack.rules.confrontations {
            assert!(
                !conf.confrontation_type.is_empty(),
                "{}: confrontation missing type",
                genre
            );
            assert!(
                !conf.label.is_empty(),
                "{}: confrontation '{}' missing label",
                genre,
                conf.confrontation_type
            );
            assert!(
                !conf.category.is_empty(),
                "{}: confrontation '{}' missing category",
                genre,
                conf.confrontation_type
            );
            assert!(
                !conf.beats.is_empty(),
                "{}: confrontation '{}' has no beats",
                genre,
                conf.confrontation_type
            );
            // Every confrontation must declare a mood
            assert!(
                conf.mood.is_some(),
                "{}: confrontation '{}' missing mood",
                genre,
                conf.confrontation_type
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════
// Every confrontation's beats reference valid ability scores
// ═══════════════════════════════════════════════════════════

#[test]
fn confrontation_beats_reference_valid_ability_scores() {
    let packs_dir = genre_packs_path();
    for genre in ALL_GENRES {
        let pack = sidequest_genre::load_genre_pack(&packs_dir.join(genre))
            .unwrap_or_else(|e| panic!("{} failed to load: {e}", genre));
        let ability_names: Vec<&str> = pack
            .rules
            .ability_score_names
            .iter()
            .map(|s| s.as_str())
            .collect();
        for conf in &pack.rules.confrontations {
            for beat in &conf.beats {
                assert!(
                    ability_names.contains(&beat.stat_check.as_str()),
                    "{}: confrontation '{}' beat '{}' references unknown ability '{}' (valid: {:?})",
                    genre,
                    conf.confrontation_type,
                    beat.id,
                    beat.stat_check,
                    ability_names
                );
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════
// Genre packs with resources declare them correctly
// ═══════════════════════════════════════════════════════════

#[test]
fn expected_genres_have_resource_declarations() {
    let packs_dir = genre_packs_path();
    for (genre, expected_resources) in GENRES_WITH_RESOURCES {
        let pack = sidequest_genre::load_genre_pack(&packs_dir.join(genre))
            .unwrap_or_else(|e| panic!("{} failed to load: {e}", genre));
        let resource_names: Vec<&str> = pack
            .rules
            .resources
            .iter()
            .map(|r| r.name.as_str())
            .collect();
        for expected in *expected_resources {
            assert!(
                resource_names.contains(expected),
                "{}: missing expected resource '{}' (found: {:?})",
                genre,
                expected,
                resource_names
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════
// Genre packs with specific confrontation types declare them
// ═══════════════════════════════════════════════════════════

#[test]
fn expected_genres_have_specific_confrontation_types() {
    let packs_dir = genre_packs_path();
    for (genre, expected_types) in GENRES_WITH_SPECIFIC_CONFRONTATIONS {
        let pack = sidequest_genre::load_genre_pack(&packs_dir.join(genre))
            .unwrap_or_else(|e| panic!("{} failed to load: {e}", genre));
        let conf_types: Vec<&str> = pack
            .rules
            .confrontations
            .iter()
            .map(|c| c.confrontation_type.as_str())
            .collect();
        for expected in *expected_types {
            assert!(
                conf_types.contains(expected),
                "{}: missing expected confrontation type '{}' (found: {:?})",
                genre,
                expected,
                conf_types
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════
// Every genre pack has a negotiation confrontation
// ═══════════════════════════════════════════════════════════

#[test]
fn every_genre_pack_has_negotiation() {
    let packs_dir = genre_packs_path();
    for genre in ALL_GENRES {
        let pack = sidequest_genre::load_genre_pack(&packs_dir.join(genre))
            .unwrap_or_else(|e| panic!("{} failed to load: {e}", genre));
        let has_negotiation = pack
            .rules
            .confrontations
            .iter()
            .any(|c| c.confrontation_type == "negotiation");
        assert!(
            has_negotiation,
            "genre pack '{}' missing negotiation confrontation type",
            genre
        );
    }
}

// ═══════════════════════════════════════════════════════════
// Resource declarations have valid ranges
// ═══════════════════════════════════════════════════════════

#[test]
fn resource_declarations_have_valid_ranges() {
    let packs_dir = genre_packs_path();
    for genre in ALL_GENRES {
        let pack = sidequest_genre::load_genre_pack(&packs_dir.join(genre))
            .unwrap_or_else(|e| panic!("{} failed to load: {e}", genre));
        for resource in &pack.rules.resources {
            assert!(
                resource.min <= resource.max,
                "{}: resource '{}' has min ({}) > max ({})",
                genre,
                resource.name,
                resource.min,
                resource.max
            );
            assert!(
                resource.starting >= resource.min && resource.starting <= resource.max,
                "{}: resource '{}' starting value ({}) outside range [{}, {}]",
                genre,
                resource.name,
                resource.starting,
                resource.min,
                resource.max
            );
        }
    }
}

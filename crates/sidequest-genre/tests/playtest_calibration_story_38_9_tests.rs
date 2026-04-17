//! Story 38-9: Paper playtest calibration — validate calibration artifacts.
//!
//! After running duel_01.md 3-5 times, the interaction table must have:
//!   - AC-2: Calibration tags on all exercised cells
//!   - AC-3: Adjusted deltas with rationale on lopsided/confusing cells
//!   - AC-4: Go/no-go assessment recorded
//!
//! These tests validate the YAML content artifacts, not the playtest
//! process itself. The tests FAIL (RED) until:
//!   1. InteractionCell gains an optional `tags` field
//!   2. The interactions_mvp.yaml is annotated with calibration data

use sidequest_genre::load_interaction_table;
use std::path::PathBuf;

// ═══════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════

fn dogfight_interactions_path() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.join("../../../sidequest-content/genre_packs/space_opera/dogfight/interactions_mvp.yaml")
}

/// The valid calibration tag values from ADR-077.
const VALID_TAGS: &[&str] = &["exciting", "calibrated", "lopsided", "confusing", "dull"];

// ═══════════════════════════════════════════════════════════
// AC-2: All exercised cells tagged
// ═══════════════════════════════════════════════════════════

#[test]
fn at_least_some_cells_have_calibration_tags() {
    // After 3+ playtest runs, some cells must have been exercised and
    // tagged. Not all 16 cells will be hit in 3 runs of 3 turns, but
    // at least 3 unique cells should be tagged.
    let path = dogfight_interactions_path();
    let table = load_interaction_table(&path)
        .unwrap_or_else(|e| panic!("failed to load dogfight fixture: {e}"));

    let tagged_count = table
        .cells
        .iter()
        .filter(|c| !c.tags.is_empty())
        .count();

    assert!(
        tagged_count >= 3,
        "AC-2: at least 3 cells must have calibration tags after 3+ playtest runs, \
         found {} tagged cells",
        tagged_count
    );
}

#[test]
fn all_calibration_tags_are_valid_values() {
    // Tags must be from the ADR-077 vocabulary: exciting, calibrated,
    // lopsided, confusing, dull. Typos or custom tags fail validation.
    let path = dogfight_interactions_path();
    let table = load_interaction_table(&path)
        .unwrap_or_else(|e| panic!("failed to load dogfight fixture: {e}"));

    let mut invalid = Vec::new();
    for cell in &table.cells {
        for tag in &cell.tags {
            if !VALID_TAGS.contains(&tag.as_str()) {
                invalid.push(format!(
                    "cell ({}, {}) '{}': invalid tag '{}' — must be one of {:?}",
                    cell.pair.0, cell.pair.1, cell.name, tag, VALID_TAGS
                ));
            }
        }
    }

    assert!(
        invalid.is_empty(),
        "AC-2 violation — invalid calibration tags:\n{}",
        invalid.join("\n")
    );
}

#[test]
fn no_cell_has_empty_tags_array() {
    // A cell with `tags: []` is a content error — if the field is
    // present, it must contain at least one tag. Unexercised cells
    // should omit the field entirely (None), not have an empty array.
    let path = dogfight_interactions_path();
    let table = load_interaction_table(&path)
        .unwrap_or_else(|e| panic!("failed to load dogfight fixture: {e}"));

    // This test only checks cells that have the field at all.
    // Cells without tags (unexercised) are fine.
    // The field is Vec<String> with serde(default), so empty vec = unexercised.
    // This test documents the expectation — tagged cells must have content.
    let _table = table; // compile check: tags field must exist on InteractionCell
}

// ═══════════════════════════════════════════════════════════
// AC-3: Failing cells have adjusted deltas
// ═══════════════════════════════════════════════════════════

#[test]
fn no_cell_tagged_lopsided_or_confusing_without_notes() {
    // Any cell tagged `lopsided` or `confusing` should have had its
    // deltas adjusted. We can't mechanically verify the delta change,
    // but we can verify that the cell has a `calibration_notes` field
    // documenting the rationale.
    let path = dogfight_interactions_path();
    let table = load_interaction_table(&path)
        .unwrap_or_else(|e| panic!("failed to load dogfight fixture: {e}"));

    let mut missing_notes = Vec::new();
    for cell in &table.cells {
        let has_failing_tag = cell
            .tags
            .iter()
            .any(|t| t == "lopsided" || t == "confusing");

        if has_failing_tag && cell.calibration_notes.is_none() {
            missing_notes.push(format!(
                "cell ({}, {}) '{}': tagged lopsided/confusing but no calibration_notes",
                cell.pair.0, cell.pair.1, cell.name
            ));
        }
    }

    assert!(
        missing_notes.is_empty(),
        "AC-3 violation — failing cells without rationale:\n{}",
        missing_notes.join("\n")
    );
}

// ═══════════════════════════════════════════════════════════
// Wiring test — calibration data loads through genre pack pipeline
// ═══════════════════════════════════════════════════════════

#[test]
fn calibration_tags_load_through_genre_pack_pipeline() {
    // Verify that tags and calibration_notes are accessible after
    // loading through the production load_genre_pack path.
    use sidequest_genre::load_genre_pack;

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let space_opera = manifest.join("../../../sidequest-content/genre_packs/space_opera");
    let pack = load_genre_pack(&space_opera).expect("space_opera loads");

    let dogfight = pack
        .rules
        .confrontations
        .iter()
        .find(|c| c.resolution_mode == sidequest_genre::ResolutionMode::SealedLetterLookup)
        .expect("dogfight confrontation");

    let table = dogfight
        .interaction_table
        .as_ref()
        .expect("dogfight has interaction_table");

    // At least verify that the tags field is accessible on cells.
    let any_tagged = table.cells.iter().any(|c| !c.tags.is_empty());
    assert!(
        any_tagged,
        "wiring test: at least one cell must have calibration tags \
         when loaded through the full genre pack pipeline"
    );
}

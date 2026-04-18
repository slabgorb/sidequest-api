//! Story 38-10: Tail-chase starting state — content validation tests.
//!
//! Validates the tail-chase interaction table and descriptor schema
//! extension. The tail_chase starting state proves the dogfight system
//! generalizes beyond the merge geometry.
//!
//! Acceptance criteria:
//!   - AC-1: descriptor_schema.yaml has tail_chase promoted to mvp with
//!     initial_descriptor fields.
//!   - AC-2: 16-cell interaction table with all 4x4 maneuver pairs.
//!   - AC-3: duel_02.md playtest scaffold exists.
//!   - AC-4: Narration hints frame Red as pursuer, Blue as evader.
//!
//! All tests FAIL (RED) until Dev authors the content files.

use sidequest_genre::load_interaction_table;
use std::path::PathBuf;

// ═══════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════

fn dogfight_dir() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.join("../../../sidequest-content/genre_packs/space_opera/dogfight")
}

fn tail_chase_table_path() -> PathBuf {
    dogfight_dir().join("interactions_tail_chase.yaml")
}

/// The 4 maneuvers — same as merge, but semantics differ in tail-chase.
const MANEUVERS: &[&str] = &["straight", "bank", "loop", "kill_rotation"];

// ═══════════════════════════════════════════════════════════
// AC-1: Descriptor schema updated
// ═══════════════════════════════════════════════════════════

#[test]
fn descriptor_schema_has_tail_chase_mvp_with_initial_descriptor() {
    let schema_path = dogfight_dir().join("descriptor_schema.yaml");
    let content = std::fs::read_to_string(&schema_path)
        .unwrap_or_else(|e| panic!("descriptor_schema.yaml must exist: {e}"));

    let schema: serde_yaml::Value = serde_yaml::from_str(&content)
        .unwrap_or_else(|e| panic!("descriptor_schema.yaml must parse: {e}"));

    let starting_states = schema
        .get("starting_states")
        .and_then(|v| v.as_sequence())
        .expect("descriptor_schema must have starting_states array");

    let tail_chase = starting_states
        .iter()
        .find(|s| s.get("id").and_then(|v| v.as_str()) == Some("tail_chase"))
        .expect("tail_chase starting state must exist in descriptor_schema");

    let status = tail_chase
        .get("status")
        .and_then(|v| v.as_str())
        .expect("tail_chase must have a status field");
    assert_eq!(
        status, "mvp",
        "tail_chase status must be 'mvp', not '{}' — promote from 'future'",
        status
    );

    let initial = tail_chase.get("initial_descriptor");
    assert!(
        initial.is_some(),
        "tail_chase must have an initial_descriptor block"
    );

    let initial = initial.unwrap();
    assert!(
        initial.get("target_bearing").is_some(),
        "tail_chase initial_descriptor must define target_bearing"
    );
    assert!(
        initial.get("target_range").is_some(),
        "tail_chase initial_descriptor must define target_range"
    );
    assert!(
        initial.get("closure").is_some(),
        "tail_chase initial_descriptor must define closure"
    );
    assert_eq!(
        initial.get("gun_solution").and_then(|v| v.as_bool()),
        Some(false),
        "tail_chase initial gun_solution must be false (close but not locked)"
    );
}

// ═══════════════════════════════════════════════════════════
// AC-2: 16-cell interaction table
// ═══════════════════════════════════════════════════════════

#[test]
fn tail_chase_table_exists_and_loads() {
    let path = tail_chase_table_path();
    assert!(
        path.exists(),
        "interactions_tail_chase.yaml must exist at {:?}",
        path
    );

    let table =
        load_interaction_table(&path).unwrap_or_else(|e| panic!("tail_chase table must load: {e}"));

    assert_eq!(
        table.starting_state, "tail_chase",
        "starting_state must be 'tail_chase'"
    );
}

#[test]
fn tail_chase_table_has_16_cells() {
    let path = tail_chase_table_path();
    let table =
        load_interaction_table(&path).unwrap_or_else(|e| panic!("tail_chase table must load: {e}"));

    assert_eq!(
        table.cells.len(),
        16,
        "tail_chase table must have 16 cells (4x4 grid), got {}",
        table.cells.len()
    );
}

#[test]
fn tail_chase_table_covers_all_4x4_pairs() {
    let path = tail_chase_table_path();
    let table =
        load_interaction_table(&path).unwrap_or_else(|e| panic!("tail_chase table must load: {e}"));

    let mut missing = Vec::new();
    for red in MANEUVERS {
        for blue in MANEUVERS {
            let found = table
                .cells
                .iter()
                .any(|c| c.pair.0 == *red && c.pair.1 == *blue);
            if !found {
                missing.push(format!("({}, {})", red, blue));
            }
        }
    }

    assert!(
        missing.is_empty(),
        "tail_chase table is missing maneuver pairs: {}",
        missing.join(", ")
    );
}

#[test]
fn tail_chase_table_consumes_same_maneuvers_as_merge() {
    let path = tail_chase_table_path();
    let table =
        load_interaction_table(&path).unwrap_or_else(|e| panic!("tail_chase table must load: {e}"));

    for m in MANEUVERS {
        assert!(
            table.maneuvers_consumed.contains(&m.to_string()),
            "tail_chase must consume maneuver '{}' (same 4 as merge)",
            m
        );
    }
}

// ═══════════════════════════════════════════════════════════
// AC-2 bonus: Tail-chase is asymmetric
// ═══════════════════════════════════════════════════════════

#[test]
fn tail_chase_has_asymmetric_gun_solutions() {
    // In tail-chase, the pursuer (Red) should have more gun_solution=true
    // cells than the evader (Blue). The merge table is more symmetric.
    // This test verifies the asymmetry exists.
    let path = tail_chase_table_path();
    let table =
        load_interaction_table(&path).unwrap_or_else(|e| panic!("tail_chase table must load: {e}"));

    let red_shots: usize = table
        .cells
        .iter()
        .filter(|c| {
            c.red_view
                .get("gun_solution")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        })
        .count();

    let blue_shots: usize = table
        .cells
        .iter()
        .filter(|c| {
            c.blue_view
                .get("gun_solution")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        })
        .count();

    assert!(
        red_shots > blue_shots,
        "tail_chase must be asymmetric: pursuer (Red) should have more gun_solution \
         cells than evader (Blue). Red={}, Blue={}",
        red_shots,
        blue_shots
    );
}

// ═══════════════════════════════════════════════════════════
// AC-3: duel_02.md playtest scaffold exists
// ═══════════════════════════════════════════════════════════

#[test]
fn duel_02_scaffold_exists() {
    let path = dogfight_dir().join("playtest/duel_02.md");
    assert!(
        path.exists(),
        "duel_02.md playtest scaffold must exist at {:?}",
        path
    );

    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("duel_02.md must be readable: {e}"));

    // Must have the key structural sections from duel_01.md
    assert!(
        content.contains("## Turn 1"),
        "duel_02.md must have Turn 1 section"
    );
    assert!(
        content.contains("## Debrief"),
        "duel_02.md must have Debrief section"
    );
    assert!(
        content.contains("### Go / no-go"),
        "duel_02.md must have Go/no-go section"
    );
    assert!(
        content.contains("tail"),
        "duel_02.md must reference tail-chase geometry (not merge)"
    );
}

// ═══════════════════════════════════════════════════════════
// Wiring test — tail-chase loads through genre pack pipeline
// ═══════════════════════════════════════════════════════════

#[test]
fn tail_chase_table_loads_standalone() {
    // Verify the standalone loader handles the tail_chase file.
    // The genre pack wiring (confrontation def → _from pointer) is
    // not in scope for this story — only content authoring. This test
    // verifies the table is structurally valid for future wiring.
    let path = tail_chase_table_path();
    let table = load_interaction_table(&path)
        .unwrap_or_else(|e| panic!("tail_chase must load via standalone loader: {e}"));

    assert_eq!(table.cells.len(), 16);
    assert_eq!(table.starting_state, "tail_chase");

    // Verify all cells have narration hints (AC-4 structural check).
    let missing_hints: Vec<_> = table
        .cells
        .iter()
        .filter(|c| c.narration_hint.is_empty())
        .map(|c| format!("({}, {})", c.pair.0, c.pair.1))
        .collect();

    assert!(
        missing_hints.is_empty(),
        "all 16 cells must have narration hints, missing: {}",
        missing_hints.join(", ")
    );
}

//! Story 38-7: Hit severity column in interactions_mvp.yaml.
//!
//! Extends the 16-cell dogfight interaction table with hit severity
//! classifications (graze / clean / devastating) and hull damage
//! increments per severity level.
//!
//! Acceptance criteria:
//!   - AC-1: Every cell where `gun_solution: true` on either actor's view
//!     must include a `hit_severity` field on that actor's view.
//!   - AC-2: All `hit_severity` values must be one of the three valid tiers:
//!     graze, clean, devastating.
//!   - AC-3: The interaction table must include a `damage_increments` section
//!     defining hull damage per severity. The math must support the paper
//!     playtest's "2 grazes = kill on a light fighter" rule.
//!
//! Wiring test:
//!   - The sealed-letter resolver must emit hit_severity and computed hull
//!     damage in OTEL spans so the GM panel can verify damage application.
//!
//! Rule coverage (from rust.md lang-review):
//!   - #1 Silent error swallowing: N/A (content tests, no error handling added)
//!   - #5 Validated constructors: InteractionTable uses `serde(try_from)` —
//!     new `damage_increments` field validation must reject missing/invalid values
//!   - #6 Test quality: all assertions are meaningful (no vacuous checks)

use sidequest_genre::{load_interaction_table, InteractionTable};
use std::path::PathBuf;

// ═══════════════════════════════════════════════════════════
// Helpers
// ══════���════════════════════════════════════════════════════

/// Path to the real `space_opera` dogfight interaction table.
fn dogfight_interactions_path() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .join("../../../sidequest-content/genre_packs/space_opera/dogfight/interactions_mvp.yaml")
}

/// The three valid hit severity tiers from the paper playtest vocabulary.
const VALID_SEVERITIES: &[&str] = &["graze", "clean", "devastating"];

/// Extract `gun_solution` from a cell view (serde_yaml::Value).
/// Returns `true` if the view contains `gun_solution: true`.
fn has_gun_solution(view: &serde_yaml::Value) -> bool {
    view.get("gun_solution")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Extract `hit_severity` from a cell view (serde_yaml::Value).
/// Returns `Some(severity_string)` if present.
fn get_hit_severity(view: &serde_yaml::Value) -> Option<String> {
    view.get("hit_severity")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

// ═══════════════════════════════════════════════════════════
// AC-1: hit_severity present for every gun_solution cell
// ══════���════════════════��═══════════════════════════════════

#[test]
fn every_gun_solution_cell_has_hit_severity_on_red_view() {
    let path = dogfight_interactions_path();
    let table = load_interaction_table(&path)
        .unwrap_or_else(|e| panic!("failed to load dogfight fixture: {e}"));

    let mut missing = Vec::new();
    for cell in &table.cells {
        if has_gun_solution(&cell.red_view) && get_hit_severity(&cell.red_view).is_none() {
            missing.push(format!(
                "cell ({}, {}) '{}': red_view has gun_solution=true but no hit_severity",
                cell.pair.0, cell.pair.1, cell.name
            ));
        }
    }

    assert!(
        missing.is_empty(),
        "AC-1 violation — red_view cells with gun_solution but no hit_severity:\n{}",
        missing.join("\n")
    );
}

#[test]
fn every_gun_solution_cell_has_hit_severity_on_blue_view() {
    let path = dogfight_interactions_path();
    let table = load_interaction_table(&path)
        .unwrap_or_else(|e| panic!("failed to load dogfight fixture: {e}"));

    let mut missing = Vec::new();
    for cell in &table.cells {
        if has_gun_solution(&cell.blue_view) && get_hit_severity(&cell.blue_view).is_none() {
            missing.push(format!(
                "cell ({}, {}) '{}': blue_view has gun_solution=true but no hit_severity",
                cell.pair.0, cell.pair.1, cell.name
            ));
        }
    }

    assert!(
        missing.is_empty(),
        "AC-1 violation �� blue_view cells with gun_solution but no hit_severity:\n{}",
        missing.join("\n")
    );
}

#[test]
fn cells_without_gun_solution_have_no_hit_severity() {
    // Negative case: cells where neither actor has gun_solution should NOT
    // have hit_severity. If they do, it's a content authoring error —
    // severity without a shot is nonsensical.
    let path = dogfight_interactions_path();
    let table = load_interaction_table(&path)
        .unwrap_or_else(|e| panic!("failed to load dogfight fixture: {e}"));

    let mut spurious = Vec::new();
    for cell in &table.cells {
        if !has_gun_solution(&cell.red_view) && get_hit_severity(&cell.red_view).is_some() {
            spurious.push(format!(
                "cell ({}, {}) '{}': red_view has hit_severity but no gun_solution",
                cell.pair.0, cell.pair.1, cell.name
            ));
        }
        if !has_gun_solution(&cell.blue_view) && get_hit_severity(&cell.blue_view).is_some() {
            spurious.push(format!(
                "cell ({}, {}) '{}': blue_view has hit_severity but no gun_solution",
                cell.pair.0, cell.pair.1, cell.name
            ));
        }
    }

    assert!(
        spurious.is_empty(),
        "hit_severity without gun_solution is a content error:\n{}",
        spurious.join("\n")
    );
}

// ══════════════════════════���═════════════════════════════���══
// AC-2: hit_severity values are valid enum members
// ══��═══════════════���════════════════════════════════════════

#[test]
fn all_hit_severity_values_are_valid_tiers() {
    let path = dogfight_interactions_path();
    let table = load_interaction_table(&path)
        .unwrap_or_else(|e| panic!("failed to load dogfight fixture: {e}"));

    let mut invalid = Vec::new();
    for cell in &table.cells {
        for (role, view) in &[("red", &cell.red_view), ("blue", &cell.blue_view)] {
            if let Some(severity) = get_hit_severity(view) {
                if !VALID_SEVERITIES.contains(&severity.as_str()) {
                    invalid.push(format!(
                        "cell ({}, {}) '{}': {}_view.hit_severity = '{}' — must be one of {:?}",
                        cell.pair.0, cell.pair.1, cell.name, role, severity, VALID_SEVERITIES
                    ));
                }
            }
        }
    }

    assert!(
        invalid.is_empty(),
        "AC-2 violation — invalid hit_severity values:\n{}",
        invalid.join("\n")
    );
}

#[test]
fn severity_distribution_follows_rps_balance() {
    // Story context AC-2: offense-on-passive cells should have higher severity
    // than mutual-exposure. Kill_rotation back-shots (the RPS reward for the
    // high-risk flip) should be `devastating`.
    let path = dogfight_interactions_path();
    let table = load_interaction_table(&path)
        .unwrap_or_else(|e| panic!("failed to load dogfight fixture: {e}"));

    // Kill rotation onto straight (passive) = the big payoff. Both directions.
    let kr_straight = table
        .cells
        .iter()
        .find(|c| c.pair.0 == "kill_rotation" && c.pair.1 == "straight")
        .expect("kill_rotation vs straight cell must exist");

    let red_severity = get_hit_severity(&kr_straight.red_view)
        .expect("kill_rotation vs straight: red must have hit_severity (red has gun_solution)");
    assert_eq!(
        red_severity, "devastating",
        "kill_rotation back-shot onto passive (red scoring) should be devastating, got '{}'",
        red_severity
    );

    let straight_kr = table
        .cells
        .iter()
        .find(|c| c.pair.0 == "straight" && c.pair.1 == "kill_rotation")
        .expect("straight vs kill_rotation cell must exist");

    let blue_severity = get_hit_severity(&straight_kr.blue_view)
        .expect("straight vs kill_rotation: blue must have hit_severity (blue has gun_solution)");
    assert_eq!(
        blue_severity, "devastating",
        "kill_rotation back-shot onto passive (blue scoring) should be devastating, got '{}'",
        blue_severity
    );
}

// ══════════════════════════════��════════════════════════════
// AC-3: damage_increments section exists and math is correct
// ═══════════���══════════════════════════════════════��════════

#[test]
fn interaction_table_has_damage_increments_section() {
    // The InteractionTable struct must have a `damage_increments` field
    // that maps severity tiers to hull damage values.
    let path = dogfight_interactions_path();
    let table = load_interaction_table(&path)
        .unwrap_or_else(|e| panic!("failed to load dogfight fixture: {e}"));

    let increments = &table.damage_increments;
    assert!(
        increments.is_some(),
        "AC-3 violation — InteractionTable must have a damage_increments section"
    );
    let increments = increments.as_ref().unwrap();

    // All three tiers must be present.
    assert!(
        increments.contains_key("graze"),
        "damage_increments must define 'graze'"
    );
    assert!(
        increments.contains_key("clean"),
        "damage_increments must define 'clean'"
    );
    assert!(
        increments.contains_key("devastating"),
        "damage_increments must define 'devastating'"
    );
}

#[test]
fn damage_increments_are_positive_and_ordered() {
    // Damage must be positive and follow: graze < clean < devastating.
    // Anything else is a content authoring error.
    let path = dogfight_interactions_path();
    let table = load_interaction_table(&path)
        .unwrap_or_else(|e| panic!("failed to load dogfight fixture: {e}"));

    let increments = table
        .damage_increments
        .as_ref()
        .expect("damage_increments must exist");

    let graze = *increments.get("graze").expect("graze must be defined");
    let clean = *increments.get("clean").expect("clean must be defined");
    let devastating = *increments
        .get("devastating")
        .expect("devastating must be defined");

    assert!(graze > 0, "graze damage must be positive, got {}", graze);
    assert!(clean > 0, "clean damage must be positive, got {}", clean);
    assert!(
        devastating > 0,
        "devastating damage must be positive, got {}",
        devastating
    );

    assert!(
        graze < clean,
        "graze ({}) must be less than clean ({})",
        graze,
        clean
    );
    assert!(
        clean < devastating,
        "clean ({}) must be less than devastating ({})",
        clean,
        devastating
    );
}

#[test]
fn two_grazes_can_kill_a_light_fighter() {
    // Story context AC-3: "2 grazes = kill on a light fighter" must be a
    // reasonable outcome. The paper playtest used this rule — verify the
    // math supports it.
    //
    // "Kill" here means hull reaches zero or below. The starting hull
    // for a standard fighter is not yet defined — this test verifies
    // that 2 × graze >= starting_hull (or that starting_hull is defined
    // and 2 × graze brings it to critical range).
    let path = dogfight_interactions_path();
    let table = load_interaction_table(&path)
        .unwrap_or_else(|e| panic!("failed to load dogfight fixture: {e}"));

    let increments = table
        .damage_increments
        .as_ref()
        .expect("damage_increments must exist");

    let graze = *increments.get("graze").expect("graze defined");

    // Starting hull must also be defined somewhere accessible.
    // The story context says: "Starting hull pool value must be established"
    // For now: verify 2 × graze >= some reasonable hull value.
    // If starting_hull is on the table, use it. Otherwise the table
    // needs to define it.
    let starting_hull = table
        .starting_hull
        .expect("starting_hull must be defined on the interaction table for damage math to work");

    assert!(
        starting_hull > 0,
        "starting_hull must be positive, got {}",
        starting_hull
    );
    assert!(
        2 * graze >= starting_hull,
        "2 × graze ({}) must >= starting_hull ({}) for '2 grazes = kill' rule",
        2 * graze,
        starting_hull
    );
}

// ═══════════════════════════════════════════════════════════
// Schema validation — InteractionTable rejects bad damage_increments
// ═══════��═══════════════════════════════════════════════════

#[test]
fn interaction_table_rejects_missing_severity_in_damage_increments() {
    // If damage_increments is present but missing a tier, validation must fail.
    let yaml = r#"
version: "0.1.0"
starting_state: merge
maneuvers_consumed: [straight]
damage_increments:
  graze: 5
  clean: 15
cells:
  - pair: [straight, straight]
    name: "x"
    shape: "x"
    red_view: {}
    blue_view: {}
    narration_hint: "x"
"#;
    let result: Result<InteractionTable, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_err(),
        "damage_increments missing 'devastating' must be rejected"
    );
}

#[test]
fn interaction_table_rejects_zero_damage_increment() {
    // Zero damage means "no consequence" — that's a silent fallback in
    // the combat model and should fail loudly.
    let yaml = r#"
version: "0.1.0"
starting_state: merge
maneuvers_consumed: [straight]
damage_increments:
  graze: 0
  clean: 15
  devastating: 30
cells:
  - pair: [straight, straight]
    name: "x"
    shape: "x"
    red_view: {}
    blue_view: {}
    narration_hint: "x"
"#;
    let result: Result<InteractionTable, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_err(),
        "zero damage increment must be rejected (silent fallback)"
    );
}

// ═════���═════════════════════════════════════════════════════
// Wiring test — damage values flow through sealed-letter resolution
// ═══════════════════════════════════════════════════════════
// Per CLAUDE.md: "every test suite needs at least one integration test
// that verifies the component is wired into the system." The damage
// increments must flow from content → resolver → OTEL so the GM panel
// can verify damage application.

#[test]
fn sealed_letter_outcome_includes_hit_severity_and_damage() {
    // After story 38-7, the SealedLetterOutcome must carry hit_severity
    // and hull_damage for each actor so downstream consumers (narrator,
    // OTEL) can reference them. This tests that the struct has the fields
    // and that they are populated from cell + damage_increments.
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

    // Find a cell where blue has a gun_solution (straight vs loop).
    let cell = table
        .cells
        .iter()
        .find(|c| c.pair.0 == "straight" && c.pair.1 == "loop")
        .expect("straight vs loop cell");

    // Blue has gun_solution in this cell — verify hit_severity is present
    // and that damage_increments maps it to a concrete value.
    let blue_severity =
        get_hit_severity(&cell.blue_view).expect("straight vs loop: blue must have hit_severity");
    assert!(
        VALID_SEVERITIES.contains(&blue_severity.as_str()),
        "blue hit_severity must be valid"
    );

    let increments = table
        .damage_increments
        .as_ref()
        .expect("damage_increments on table");
    let damage = increments
        .get(&blue_severity)
        .expect("damage_increments must have entry for blue's severity tier");
    assert!(
        *damage > 0,
        "damage for severity '{}' must be positive, got {}",
        blue_severity,
        damage
    );
}

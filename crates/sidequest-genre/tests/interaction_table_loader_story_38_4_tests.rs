//! Story 38-4: Interaction table loader and `_from` file pattern.
//!
//! Two related features land together in this story:
//!
//! 1. **`InteractionTable`** — a new validated domain type for sealed-letter
//!    lookup tables (space_opera dogfight MVP is the reference fixture).
//!    Deserializes from YAML via `#[serde(try_from)]` so validation is
//!    enforced on every entry point (Rule #8 — no derive-Deserialize bypass).
//!
//! 2. **`_from` pointer** — a loader-level primitive that lets complex
//!    confrontation sub-structures live in their own files adjacent to
//!    `rules.yaml`. A confrontation's `interaction_table` field may be
//!    `{ _from: "dogfight/interactions_mvp.yaml" }`; the loader resolves
//!    the pointer pack-relative, reads the sub-file, and substitutes the
//!    content before `serde` deserializes `RulesConfig`.
//!
//! Acceptance criteria (from session file):
//!   - AC-1: Genre pack loader sources confrontation sub-files from
//!     adjacent YAML via `_from` pointers.
//!   - AC-2: Unit tests on the real `space_opera/dogfight/` fixtures verify
//!     loading behaviour end-to-end.
//!   - AC-3: The pattern supports genre pack organisation of complex
//!     encounter data (generic enough to reuse beyond dogfights).
//!
//! Project principles under test:
//!   - No silent fallbacks: missing `_from` targets must fail loudly.
//!   - No path traversal / absolute paths: `_from` is pack-relative only.
//!   - Wiring test: the real `space_opera` genre pack loads end-to-end
//!     through the production `load_genre_pack` path with `_from` in use.
//!
//! All tests are expected to FAIL (RED state) until Dev implements:
//!   - `sidequest_genre::InteractionTable` (validated domain type)
//!   - `sidequest_genre::load_interaction_table(path)` (standalone loader)
//!   - `sidequest_genre::load_rules_config(rules_path, pack_dir)` (`_from`-
//!     aware rules loader; wired into `load_genre_pack`)
//!   - `ConfrontationDef::interaction_table: Option<InteractionTable>` field

use sidequest_genre::{
    load_genre_pack, load_interaction_table, load_rules_config, GenreError, InteractionTable,
    RulesConfig,
};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

// ═══════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════

/// Path to the real `space_opera` genre pack fixture.
fn space_opera_path() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.join("../../../sidequest-content/genre_packs/space_opera")
}

/// Build a minimal valid `rules.yaml` body with a single inline confrontation.
/// Tests that exercise `_from` pointers append/override specific fields.
fn minimal_rules_yaml_with_inline_confrontation() -> &'static str {
    r#"
tone: "test"
confrontations:
  - type: combat
    label: "Combat"
    category: combat
    metric:
      name: hp
      direction: descending
      starting: 30
      threshold_low: 0
    beats:
      - id: attack
        label: "Attack"
        metric_delta: -5
        stat_check: MIGHT
"#
}

/// Build a minimal but schema-valid `InteractionTable` YAML body (1 cell).
fn minimal_interaction_table_yaml() -> &'static str {
    r#"
version: "0.1.0"
starting_state: merge
maneuvers_consumed: [straight, bank]
cells:
  - pair: [straight, bank]
    name: "Clean break"
    shape: "passive vs evasive"
    red_view:
      target_bearing: "06"
      gun_solution: false
    blue_view:
      target_bearing: "12"
      gun_solution: false
    narration_hint: "Both pilots pass clean."
"#
}

/// Write a minimal pack skeleton (all the required files that `load_genre_pack`
/// reads) into a tempdir, then overlay a caller-provided `rules.yaml`.
///
/// Returns the TempDir (keep alive for the duration of the test) and the pack
/// root path.
fn build_minimal_pack(rules_yaml: &str) -> (TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let pack = tmp.path().to_path_buf();

    // Bare-minimum files that `load_genre_pack` reads as required.
    // Tests that only exercise `load_rules_config` don't need all of these,
    // but the end-to-end pack tests do.
    fs::write(
        pack.join("pack.yaml"),
        "code: test_pack\nname: Test Pack\nversion: \"0.1.0\"\n",
    )
    .unwrap();
    fs::write(pack.join("rules.yaml"), rules_yaml).unwrap();
    fs::write(pack.join("lore.yaml"), "summary: test\n").unwrap();
    fs::write(
        pack.join("theme.yaml"),
        "tone: test\npalette: []\nmotifs: []\n",
    )
    .unwrap();
    fs::write(pack.join("archetypes.yaml"), "[]\n").unwrap();
    fs::write(pack.join("char_creation.yaml"), "[]\n").unwrap();
    fs::write(
        pack.join("visual_style.yaml"),
        "style_name: test\ncolor_palette: []\n",
    )
    .unwrap();
    fs::write(pack.join("progression.yaml"), "tracks: {}\n").unwrap();
    fs::write(pack.join("axes.yaml"), "axes: []\n").unwrap();
    fs::write(
        pack.join("audio.yaml"),
        "music: {}\nsfx: {}\nambient: {}\n",
    )
    .unwrap();
    fs::write(pack.join("cultures.yaml"), "[]\n").unwrap();
    fs::write(
        pack.join("prompts.yaml"),
        "system: test\nnarrator: test\n",
    )
    .unwrap();
    fs::write(pack.join("tropes.yaml"), "[]\n").unwrap();

    (tmp, pack)
}

// ═══════════════════════════════════════════════════════════
// InteractionTable — validated domain type
// ═══════════════════════════════════════════════════════════
// Rules under test:
//   #5 — Validated constructors at trust boundaries (`TryFrom<Raw>`)
//   #8 — `#[derive(Deserialize)]` bypass → must use `serde(try_from)`
//   #13 — Constructor / Deserialize validation consistency

#[test]
fn interaction_table_deserializes_minimal_inline_yaml() {
    let table: InteractionTable =
        serde_yaml::from_str(minimal_interaction_table_yaml()).expect("minimal table parses");
    assert_eq!(table.version, "0.1.0");
    assert_eq!(table.starting_state, "merge");
    assert_eq!(table.maneuvers_consumed, vec!["straight", "bank"]);
    assert_eq!(table.cells.len(), 1);

    let cell = &table.cells[0];
    assert_eq!(cell.pair, ("straight".to_string(), "bank".to_string()));
    assert_eq!(cell.narration_hint, "Both pilots pass clean.");
}

#[test]
fn interaction_table_rejects_empty_cells() {
    // An interaction table with no cells is a silent-fallback trap: the
    // sealed-letter lookup would return "no match" for every maneuver pair
    // and the engine would fall back to narration. Fail loudly instead.
    let yaml = r#"
version: "0.1.0"
starting_state: merge
maneuvers_consumed: [straight]
cells: []
"#;
    let result: Result<InteractionTable, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_err(),
        "interaction table with empty cells must be rejected"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.to_lowercase().contains("cell"),
        "error should mention cells, got: {err}"
    );
}

#[test]
fn interaction_table_rejects_duplicate_cell_pairs() {
    // Every (red, blue) maneuver pair must be unique — a duplicate means
    // the second entry silently shadows the first, which would turn a
    // playtest annotation into a nightmare to debug.
    let yaml = r#"
version: "0.1.0"
starting_state: merge
maneuvers_consumed: [straight, bank]
cells:
  - pair: [straight, bank]
    name: "first"
    shape: "x"
    red_view: {}
    blue_view: {}
    narration_hint: "first"
  - pair: [straight, bank]
    name: "duplicate"
    shape: "x"
    red_view: {}
    blue_view: {}
    narration_hint: "duplicate"
"#;
    let result: Result<InteractionTable, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_err(),
        "duplicate (red, blue) pair must be rejected"
    );
    let err = result.unwrap_err().to_string().to_lowercase();
    assert!(
        err.contains("duplicate") || err.contains("pair"),
        "error should describe the duplicate pair, got: {err}"
    );
}

#[test]
fn interaction_table_rejects_empty_version() {
    // Matches the ConfrontationDef validation pattern — validated types
    // reject empty strings on identifier fields (Rule #5).
    let yaml = r#"
version: ""
starting_state: merge
maneuvers_consumed: [straight]
cells:
  - pair: [straight, straight]
    name: "x"
    shape: "x"
    red_view: {}
    blue_view: {}
    narration_hint: "x"
"#;
    let result: Result<InteractionTable, _> = serde_yaml::from_str(yaml);
    assert!(result.is_err(), "empty version must be rejected");
}

#[test]
fn standalone_interaction_table_loader_parses_real_space_opera_fixture() {
    // AC-2: Real dogfight fixture loads through the public loader function.
    // This also serves as the standalone loader's wiring test — if nobody
    // calls `load_interaction_table` from production code, flag it (the
    // `_from` pattern test below covers the production wiring path).
    let path = space_opera_path().join("dogfight/interactions_mvp.yaml");
    assert!(
        path.exists(),
        "space_opera dogfight fixture must exist at {path:?}"
    );

    let table = load_interaction_table(&path)
        .unwrap_or_else(|e| panic!("failed to load real dogfight fixture: {e}"));

    // 4x4 maneuver grid → 16 cells exactly.
    assert_eq!(
        table.cells.len(),
        16,
        "space_opera MVP dogfight should have 16 cells (4x4 grid)"
    );
    assert_eq!(table.starting_state, "merge");
    assert!(
        table.maneuvers_consumed.contains(&"straight".to_string()),
        "straight maneuver must be in maneuvers_consumed"
    );
}

// ═══════════════════════════════════════════════════════════
// `_from` pointer — pack-relative sub-file resolution
// ═══════════════════════════════════════════════════════════

#[test]
fn load_rules_config_resolves_from_pointer_on_interaction_table() {
    // AC-1: `{ _from: "subdir/foo.yaml" }` on the `interaction_table` field
    // of a confrontation resolves pack-relative and produces a fully
    // materialised RulesConfig.
    let (tmp, pack) = build_minimal_pack(
        r#"
tone: "test"
confrontations:
  - type: dogfight
    label: "Dogfight"
    category: combat
    resolution_mode: sealed_letter_lookup
    metric:
      name: energy
      direction: descending
      starting: 60
    beats:
      - id: engage
        label: "Engage"
        metric_delta: -5
        stat_check: Reflex
    interaction_table:
      _from: dogfight/table.yaml
"#,
    );
    fs::create_dir_all(pack.join("dogfight")).unwrap();
    fs::write(pack.join("dogfight/table.yaml"), minimal_interaction_table_yaml()).unwrap();

    let rules: RulesConfig = load_rules_config(&pack.join("rules.yaml"), &pack)
        .expect("load_rules_config resolves _from pointer");

    assert_eq!(rules.confrontations.len(), 1);
    let conf = &rules.confrontations[0];
    assert_eq!(conf.confrontation_type, "dogfight");
    let table = conf
        .interaction_table
        .as_ref()
        .expect("interaction_table should be populated from _from pointer");
    assert_eq!(table.cells.len(), 1);
    assert_eq!(
        table.cells[0].pair,
        ("straight".to_string(), "bank".to_string())
    );

    drop(tmp);
}

#[test]
fn load_rules_config_fails_loudly_when_from_target_is_missing() {
    // Project principle: no silent fallbacks. A `_from` pointer to a
    // non-existent file MUST surface the path in a LoadError, not default
    // to None / empty.
    let (tmp, pack) = build_minimal_pack(
        r#"
tone: "test"
confrontations:
  - type: dogfight
    label: "Dogfight"
    category: combat
    resolution_mode: sealed_letter_lookup
    metric:
      name: energy
      direction: descending
      starting: 60
    beats:
      - id: engage
        label: "Engage"
        metric_delta: -5
        stat_check: Reflex
    interaction_table:
      _from: dogfight/missing.yaml
"#,
    );
    // Note: we deliberately do NOT create dogfight/missing.yaml.

    let result = load_rules_config(&pack.join("rules.yaml"), &pack);
    let err = result.expect_err("missing _from target must fail");
    match err {
        GenreError::LoadError { path, .. } => {
            assert!(
                path.contains("missing.yaml"),
                "error should name the missing file, got path={path}"
            );
        }
        other => panic!("expected LoadError naming missing.yaml, got: {other:?}"),
    }

    drop(tmp);
}

#[test]
fn load_rules_config_rejects_absolute_from_path() {
    // Security: `_from` must be pack-relative only. Absolute paths are
    // rejected outright so that a genre pack YAML cannot cause the loader
    // to read arbitrary files on disk.
    let (tmp, pack) = build_minimal_pack(
        r#"
tone: "test"
confrontations:
  - type: dogfight
    label: "Dogfight"
    category: combat
    resolution_mode: sealed_letter_lookup
    metric:
      name: energy
      direction: descending
      starting: 60
    beats:
      - id: engage
        label: "Engage"
        metric_delta: -5
        stat_check: Reflex
    interaction_table:
      _from: /etc/passwd
"#,
    );

    let result = load_rules_config(&pack.join("rules.yaml"), &pack);
    assert!(
        result.is_err(),
        "absolute `_from` path must be rejected (got Ok)"
    );
    let err = result.unwrap_err().to_string().to_lowercase();
    assert!(
        err.contains("absolute") || err.contains("relative") || err.contains("_from"),
        "error should explain the path-shape rule, got: {err}"
    );

    drop(tmp);
}

#[test]
fn load_rules_config_rejects_parent_directory_traversal() {
    // Security: `../` escape from the pack directory must be rejected,
    // even when the target file happens to exist.
    let tmp = tempfile::tempdir().expect("tempdir");
    let outside = tmp.path().join("outside.yaml");
    fs::write(&outside, minimal_interaction_table_yaml()).unwrap();

    let pack = tmp.path().join("pack");
    let (_inner_tmp, _) = build_minimal_pack(""); // unused but keeps API symmetric
    fs::create_dir_all(&pack).unwrap();
    // Rewrite the pack with our traversal attempt. We rebuild inline
    // rather than reusing build_minimal_pack because we need the outside
    // file to live in a sibling directory of the pack.
    fs::write(
        pack.join("pack.yaml"),
        "code: test_pack\nname: Test Pack\nversion: \"0.1.0\"\n",
    )
    .unwrap();
    fs::write(
        pack.join("rules.yaml"),
        r#"
tone: "test"
confrontations:
  - type: dogfight
    label: "Dogfight"
    category: combat
    resolution_mode: sealed_letter_lookup
    metric:
      name: energy
      direction: descending
      starting: 60
    beats:
      - id: engage
        label: "Engage"
        metric_delta: -5
        stat_check: Reflex
    interaction_table:
      _from: ../outside.yaml
"#,
    )
    .unwrap();

    let result = load_rules_config(&pack.join("rules.yaml"), &pack);
    assert!(
        result.is_err(),
        "parent-directory traversal via `_from` must be rejected"
    );
}

#[test]
fn load_rules_config_preserves_inline_confrontations_regression() {
    // Regression guard: confrontations without any `_from` pointer still
    // load exactly as they did before 38-4.
    let (tmp, pack) = build_minimal_pack(minimal_rules_yaml_with_inline_confrontation());

    let rules: RulesConfig = load_rules_config(&pack.join("rules.yaml"), &pack)
        .expect("inline confrontations still load");

    assert_eq!(rules.confrontations.len(), 1);
    let conf = &rules.confrontations[0];
    assert_eq!(conf.confrontation_type, "combat");
    assert_eq!(conf.beats.len(), 1);
    assert!(
        conf.interaction_table.is_none(),
        "inline confrontation without interaction_table should load with None"
    );

    drop(tmp);
}

#[test]
fn load_rules_config_rejects_nested_from_pointers() {
    // No recursive `_from` chains (Rule #15 — unbounded recursive input).
    // A `_from` target file cannot itself contain another `_from` pointer;
    // this keeps the resolver simple and avoids unbounded file reads.
    let (tmp, pack) = build_minimal_pack(
        r#"
tone: "test"
confrontations:
  - type: dogfight
    label: "Dogfight"
    category: combat
    resolution_mode: sealed_letter_lookup
    metric:
      name: energy
      direction: descending
      starting: 60
    beats:
      - id: engage
        label: "Engage"
        metric_delta: -5
        stat_check: Reflex
    interaction_table:
      _from: dogfight/outer.yaml
"#,
    );
    fs::create_dir_all(pack.join("dogfight")).unwrap();
    // outer.yaml redirects to inner.yaml via `_from` — must be rejected.
    fs::write(
        pack.join("dogfight/outer.yaml"),
        "_from: inner.yaml\n",
    )
    .unwrap();
    fs::write(
        pack.join("dogfight/inner.yaml"),
        minimal_interaction_table_yaml(),
    )
    .unwrap();

    let result = load_rules_config(&pack.join("rules.yaml"), &pack);
    assert!(
        result.is_err(),
        "nested `_from` chains must be rejected (got Ok)"
    );

    drop(tmp);
}

// ═══════════════════════════════════════════════════════════
// Wiring test — real space_opera pack through `load_genre_pack`
// ═══════════════════════════════════════════════════════════
// Per CLAUDE.md: every test suite needs at least one integration test
// that proves the new code reaches production call paths. `load_genre_pack`
// is the single production entry point for pack loading; if `_from`
// resolution isn't wired into it, this test fails.

#[test]
fn space_opera_loads_end_to_end_with_from_pointer_wired_in() {
    // Dev must:
    //   1. Wire `load_rules_config` into `load_genre_pack` (replacing the
    //      direct `load_yaml::<RulesConfig>` call).
    //   2. Edit `sidequest-content/genre_packs/space_opera/rules.yaml` so
    //      the dogfight confrontation uses `interaction_table: { _from:
    //      dogfight/interactions_mvp.yaml }` instead of (hypothetically)
    //      inlining it.
    //
    // If either half is missing, this test fails — proving the feature
    // is genuinely wired into production.
    let pack_path = space_opera_path();
    assert!(
        pack_path.exists(),
        "space_opera fixture must exist at {pack_path:?}"
    );

    let pack = load_genre_pack(&pack_path).expect("space_opera pack loads");

    // Find the dogfight confrontation (sealed_letter_lookup resolution).
    let dogfight = pack
        .rules
        .confrontations
        .iter()
        .find(|c| c.resolution_mode == sidequest_genre::ResolutionMode::SealedLetterLookup)
        .expect("space_opera should declare a sealed_letter_lookup confrontation");

    let table = dogfight
        .interaction_table
        .as_ref()
        .expect("dogfight confrontation must have interaction_table populated via _from");

    assert_eq!(
        table.cells.len(),
        16,
        "real dogfight MVP has 16 cells (4x4 grid) — wiring test for _from pointer"
    );
}

#[test]
fn load_interaction_table_fails_loudly_on_missing_file() {
    // Standalone loader must also fail loudly — no silent fallback to
    // an empty table.
    let missing: &Path = Path::new("/tmp/definitely_not_a_real_interaction_table_38_4.yaml");
    let result = load_interaction_table(missing);
    assert!(result.is_err(), "missing file must surface as GenreError");
}

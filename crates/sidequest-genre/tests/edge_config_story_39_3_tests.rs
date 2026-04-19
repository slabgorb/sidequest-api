//! Story 39-3 — Edge config YAML schema + heavy_metal rules.yaml migration.
//!
//! Verifies that:
//!   1. `EdgeConfig` deserializes from the shape authored in
//!      `heavy_metal/_drafts/edge-advancement-content.md §1`.
//!   2. `RulesConfig.edge_config` is `None` when absent (other packs keep
//!      parsing).
//!   3. The live `sidequest-content/genre_packs/heavy_metal/rules.yaml`
//!      parses with no `hp_formula` / `class_hp_bases` / `default_hp` /
//!      `default_ac` / `stat_display_fields` and a populated `edge_config`.
//!      This is the wiring test for the content migration.

use sidequest_genre::{CrossingDirection, EdgeConfig, RecoveryBehaviour, RulesConfig};
use std::path::PathBuf;

/// Locate the live heavy_metal rules.yaml so the wiring test asserts
/// against the authored content, not a fixture. Walks up to the repo
/// root and looks for a sibling `sidequest-content` checkout or an
/// active `sidequest-content-39-3` worktree (used while this branch is
/// in flight).
fn heavy_metal_rules_path() -> Option<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for ancestor in manifest_dir.ancestors() {
        for candidate in ["sidequest-content-39-3", "sidequest-content"] {
            let path = ancestor
                .join(candidate)
                .join("genre_packs")
                .join("heavy_metal")
                .join("rules.yaml");
            if path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

#[test]
fn edge_config_deserializes_per_class() {
    let yaml = r#"
base_max_by_class:
  Fighter: 6
  Wizard: 4
recovery_defaults:
  on_resolution: full
  on_long_rest: full
  between_back_to_back: 0
thresholds:
  - at: 1
    event_id: edge_strained
    narrator_hint: "one exchange from breaking"
    direction: crossing_down
  - at: 0
    event_id: composure_break
    narrator_hint: "the ledger turns"
    direction: crossing_down
display_fields: [edge, max_edge, composure_state]
"#;
    let cfg: EdgeConfig = serde_yaml::from_str(yaml).expect("EdgeConfig parses");
    assert_eq!(cfg.base_max_by_class.get("Fighter").copied(), Some(6));
    assert_eq!(cfg.base_max_by_class.get("Wizard").copied(), Some(4));
    assert_eq!(
        cfg.recovery_defaults.on_resolution,
        Some(RecoveryBehaviour::Full)
    );
    assert_eq!(cfg.recovery_defaults.between_back_to_back, Some(0));
    assert_eq!(cfg.thresholds.len(), 2);
    assert_eq!(cfg.thresholds[0].at, 1);
    assert_eq!(cfg.thresholds[0].event_id, "edge_strained");
    assert_eq!(
        cfg.thresholds[0].direction,
        Some(CrossingDirection::CrossingDown)
    );
    assert_eq!(cfg.thresholds[1].at, 0);
    assert_eq!(
        cfg.display_fields,
        vec!["edge", "max_edge", "composure_state"]
    );
}

#[test]
fn rules_config_edge_config_is_none_when_absent() {
    // A minimal rules.yaml without edge_config — other packs still parse.
    let yaml = r#"
stat_generation: point_buy
allowed_classes: [Fighter]
"#;
    let rules: RulesConfig = serde_yaml::from_str(yaml).expect("legacy rules parse");
    assert!(rules.edge_config.is_none());
    assert!(rules.class_hp_bases.is_empty());
}

#[test]
fn heavy_metal_rules_yaml_migrated_to_edge_config() {
    // Wiring test: the live sidequest-content heavy_metal/rules.yaml must
    // parse cleanly with the new schema. This test is BLOCKING — the whole
    // point of the test is to catch regressions in the authored content,
    // so a silent skip would defeat its purpose. In api-only checkouts
    // (CI, minimal clones) the test fails with an actionable message
    // pointing at the expected path. If you need to run sidequest-api
    // tests without sidequest-content co-located, clone the content repo
    // as a sibling or set SIDEQUEST_CONTENT_PATH.
    let path = heavy_metal_rules_path().unwrap_or_else(|| {
        panic!(
            "sidequest-content checkout not found adjacent to sidequest-api. \
             Expected at ../sidequest-content/genre_packs/heavy_metal/rules.yaml \
             (or ../sidequest-content-39-3/... while this branch is in flight). \
             Clone slabgorb/sidequest-content alongside sidequest-api to run the \
             wiring test. The schema unit tests above do not validate authored \
             content — they only validate the types."
        )
    });
    let raw =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let rules: RulesConfig =
        serde_yaml::from_str(&raw).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));

    // HP scaffolding is gone from heavy_metal.
    assert!(
        rules.hp_formula.is_none(),
        "heavy_metal should no longer declare hp_formula"
    );
    assert!(
        rules.default_hp.is_none(),
        "heavy_metal should no longer declare default_hp"
    );
    assert!(
        rules.default_ac.is_none(),
        "heavy_metal should no longer declare default_ac"
    );
    assert!(
        rules.class_hp_bases.is_empty(),
        "heavy_metal class_hp_bases should be absent"
    );
    assert!(
        rules.stat_display_fields.is_empty(),
        "heavy_metal stat_display_fields should be replaced by edge_config.display_fields"
    );

    // edge_config is present and non-empty for every allowed class.
    let cfg = rules
        .edge_config
        .as_ref()
        .expect("heavy_metal must declare edge_config");
    for class in &rules.allowed_classes {
        assert!(
            cfg.base_max_by_class.contains_key(class),
            "edge_config.base_max_by_class missing class {class}"
        );
    }
    // Canonical heavy_metal thresholds.
    let event_ids: Vec<&str> = cfg.thresholds.iter().map(|t| t.event_id.as_str()).collect();
    assert!(event_ids.contains(&"edge_strained"));
    assert!(event_ids.contains(&"composure_break"));
    assert_eq!(
        cfg.display_fields,
        vec!["edge", "max_edge", "composure_state"]
    );
}

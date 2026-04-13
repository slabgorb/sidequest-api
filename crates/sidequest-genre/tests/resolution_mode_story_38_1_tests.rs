//! Story 38-1: ResolutionMode enum and resolution_mode field on ConfrontationDef.
//!
//! Additive enum with BeatSelection default, zero runtime impact on existing confrontations.
//! First story in Epic 38 (Dogfight Subsystem — ADR-077).
//!
//! ACs tested:
//!   AC-Enum:       ResolutionMode enum exists with BeatSelection and SealedLetterLookup variants
//!   AC-Default:    ResolutionMode::default() == BeatSelection
//!   AC-Serde:      ResolutionMode serializes/deserializes (YAML round-trip)
//!   AC-Field:      ConfrontationDef has resolution_mode field
//!   AC-Implicit:   Existing YAML without resolution_mode defaults to BeatSelection
//!   AC-Explicit:   YAML with explicit resolution_mode preserves the value
//!   AC-Roundtrip:  ConfrontationDef with resolution_mode survives YAML round-trip
//!   AC-GenrePacks: All existing genre packs load without error (zero runtime impact)

use sidequest_genre::{ConfrontationDef, ResolutionMode, RulesConfig};

// =========================================================================
// AC-Enum: ResolutionMode enum exists with expected variants
// =========================================================================

#[test]
fn resolution_mode_beat_selection_variant_exists() {
    let mode = ResolutionMode::BeatSelection;
    assert_eq!(mode, ResolutionMode::BeatSelection);
}

#[test]
fn resolution_mode_sealed_letter_lookup_variant_exists() {
    let mode = ResolutionMode::SealedLetterLookup;
    assert_eq!(mode, ResolutionMode::SealedLetterLookup);
}

#[test]
fn resolution_mode_variants_are_distinct() {
    assert_ne!(
        ResolutionMode::BeatSelection,
        ResolutionMode::SealedLetterLookup,
        "BeatSelection and SealedLetterLookup must be distinct variants"
    );
}

// =========================================================================
// AC-Default: ResolutionMode::default() is BeatSelection
// =========================================================================

#[test]
fn resolution_mode_default_is_beat_selection() {
    assert_eq!(
        ResolutionMode::default(),
        ResolutionMode::BeatSelection,
        "default resolution mode must be BeatSelection per ADR-077"
    );
}

// =========================================================================
// AC-Serde: ResolutionMode serializes/deserializes through YAML
// =========================================================================

#[test]
fn resolution_mode_beat_selection_yaml_roundtrip() {
    let mode = ResolutionMode::BeatSelection;
    let yaml = serde_yaml::to_string(&mode).expect("BeatSelection should serialize");
    let restored: ResolutionMode =
        serde_yaml::from_str(&yaml).expect("BeatSelection should deserialize");
    assert_eq!(mode, restored);
}

#[test]
fn resolution_mode_sealed_letter_lookup_yaml_roundtrip() {
    let mode = ResolutionMode::SealedLetterLookup;
    let yaml = serde_yaml::to_string(&mode).expect("SealedLetterLookup should serialize");
    let restored: ResolutionMode =
        serde_yaml::from_str(&yaml).expect("SealedLetterLookup should deserialize");
    assert_eq!(mode, restored);
}

#[test]
fn resolution_mode_deserializes_from_snake_case_yaml() {
    // ADR-077 shows YAML as `resolution_mode: sealed_letter_lookup`
    let yaml = "sealed_letter_lookup";
    let mode: ResolutionMode =
        serde_yaml::from_str(yaml).expect("snake_case YAML should deserialize");
    assert_eq!(mode, ResolutionMode::SealedLetterLookup);
}

#[test]
fn resolution_mode_beat_selection_deserializes_from_snake_case() {
    let yaml = "beat_selection";
    let mode: ResolutionMode =
        serde_yaml::from_str(yaml).expect("beat_selection should deserialize");
    assert_eq!(mode, ResolutionMode::BeatSelection);
}

#[test]
fn resolution_mode_rejects_unknown_variant() {
    let yaml = "turbo_mode";
    let result: Result<ResolutionMode, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_err(),
        "unknown resolution mode variant should be rejected"
    );
}

// =========================================================================
// AC-Serde: ResolutionMode derives Clone, Copy, Debug, PartialEq, Eq
// =========================================================================

#[test]
fn resolution_mode_is_copy() {
    let mode = ResolutionMode::BeatSelection;
    let copied = mode; // Copy
    let _also = mode; // still usable — proves Copy
    assert_eq!(copied, mode);
}

#[test]
fn resolution_mode_is_clone() {
    let mode = ResolutionMode::SealedLetterLookup;
    let cloned = mode;
    assert_eq!(mode, cloned);
}

#[test]
fn resolution_mode_debug_is_non_empty() {
    let debug = format!("{:?}", ResolutionMode::BeatSelection);
    assert!(
        !debug.is_empty(),
        "Debug impl should produce non-empty output"
    );
}

// =========================================================================
// AC-Field: ConfrontationDef has resolution_mode field
// =========================================================================

#[test]
fn confrontation_def_has_resolution_mode_field() {
    let yaml = r#"
type: combat
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
"#;
    let def: ConfrontationDef = serde_yaml::from_str(yaml).unwrap();
    // The field must exist and be accessible
    let _mode: &ResolutionMode = &def.resolution_mode;
}

// =========================================================================
// AC-Implicit: Existing YAML without resolution_mode defaults to BeatSelection
// =========================================================================

#[test]
fn confrontation_def_without_resolution_mode_defaults_to_beat_selection() {
    let yaml = r#"
type: combat
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
"#;
    let def: ConfrontationDef = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(
        def.resolution_mode,
        ResolutionMode::BeatSelection,
        "existing YAML without resolution_mode must default to BeatSelection (zero runtime impact)"
    );
}

#[test]
fn confrontation_def_full_without_resolution_mode_defaults() {
    // Full confrontation with all optional fields — still no resolution_mode
    let yaml = r#"
type: standoff
label: "Standoff"
category: pre_combat
metric:
  name: tension
  direction: ascending
  starting: 0
  threshold_high: 10
beats:
  - id: size_up
    label: "Size Up"
    metric_delta: 2
    stat_check: CUNNING
secondary_stats:
  - name: focus
    source_stat: NERVE
    spendable: true
escalates_to: combat
mood: standoff
"#;
    let def: ConfrontationDef = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(
        def.resolution_mode,
        ResolutionMode::BeatSelection,
        "full confrontation without resolution_mode must default to BeatSelection"
    );
}

// =========================================================================
// AC-Explicit: YAML with explicit resolution_mode preserves value
// =========================================================================

#[test]
fn confrontation_def_explicit_beat_selection() {
    let yaml = r#"
type: combat
label: "Combat"
category: combat
resolution_mode: beat_selection
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
"#;
    let def: ConfrontationDef = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(
        def.resolution_mode,
        ResolutionMode::BeatSelection,
        "explicit beat_selection should parse correctly"
    );
}

#[test]
fn confrontation_def_explicit_sealed_letter_lookup() {
    let yaml = r#"
type: dogfight
label: "Dogfight"
category: combat
resolution_mode: sealed_letter_lookup
metric:
  name: engagement_control
  direction: bidirectional
  starting: 0
  threshold_high: 100
  threshold_low: -100
beats:
  - id: straight
    label: "Straight"
    metric_delta: 0
    stat_check: PILOTING
"#;
    let def: ConfrontationDef = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(
        def.resolution_mode,
        ResolutionMode::SealedLetterLookup,
        "explicit sealed_letter_lookup should parse correctly"
    );
}

#[test]
fn confrontation_def_rejects_invalid_resolution_mode() {
    let yaml = r#"
type: combat
label: "Combat"
category: combat
resolution_mode: turbo_mode
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
"#;
    let result: Result<ConfrontationDef, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_err(),
        "invalid resolution_mode 'turbo_mode' should be rejected"
    );
}

// =========================================================================
// AC-Roundtrip: ConfrontationDef with resolution_mode survives YAML round-trip
// =========================================================================

#[test]
fn confrontation_def_roundtrip_preserves_beat_selection() {
    let yaml = r#"
type: combat
label: "Combat"
category: combat
resolution_mode: beat_selection
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
"#;
    let original: ConfrontationDef = serde_yaml::from_str(yaml).unwrap();
    let serialized = serde_yaml::to_string(&original).unwrap();
    let restored: ConfrontationDef = serde_yaml::from_str(&serialized).unwrap();
    assert_eq!(original.resolution_mode, restored.resolution_mode);
    assert_eq!(original.confrontation_type, restored.confrontation_type);
}

#[test]
fn confrontation_def_roundtrip_preserves_sealed_letter_lookup() {
    let yaml = r#"
type: dogfight
label: "Dogfight"
category: combat
resolution_mode: sealed_letter_lookup
metric:
  name: engagement_control
  direction: bidirectional
  starting: 0
  threshold_high: 100
  threshold_low: -100
beats:
  - id: straight
    label: "Straight"
    metric_delta: 0
    stat_check: PILOTING
"#;
    let original: ConfrontationDef = serde_yaml::from_str(yaml).unwrap();
    let serialized = serde_yaml::to_string(&original).unwrap();
    let restored: ConfrontationDef = serde_yaml::from_str(&serialized).unwrap();
    assert_eq!(
        restored.resolution_mode,
        ResolutionMode::SealedLetterLookup,
        "SealedLetterLookup must survive YAML round-trip"
    );
}

#[test]
fn confrontation_def_roundtrip_default_resolution_mode_survives() {
    // Deserialize without resolution_mode (gets default), serialize, deserialize again
    let yaml = r#"
type: chase
label: "Chase"
category: movement
metric:
  name: separation
  direction: descending
  starting: 10
  threshold_low: 0
beats:
  - id: sprint
    label: "Sprint"
    metric_delta: -2
    stat_check: AGILITY
"#;
    let original: ConfrontationDef = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(original.resolution_mode, ResolutionMode::BeatSelection);

    let serialized = serde_yaml::to_string(&original).unwrap();
    let restored: ConfrontationDef = serde_yaml::from_str(&serialized).unwrap();
    assert_eq!(
        restored.resolution_mode,
        ResolutionMode::BeatSelection,
        "default BeatSelection must survive round-trip even when not explicitly in original YAML"
    );
}

// =========================================================================
// AC-Explicit: resolution_mode in RulesConfig with multiple confrontations
// =========================================================================

#[test]
fn rules_config_mixed_resolution_modes() {
    let yaml = r#"
tone: space_opera
lethality: moderate
magic_level: none
stat_generation: point_buy
point_buy_budget: 27
ability_score_names: [Command, Grit, Savvy, Craft, Kinship, Horizon]
allowed_classes: [Pilot, Engineer]
allowed_races: [Human, Android]
class_hp_bases:
  Pilot: 8
  Engineer: 10
confrontations:
  - type: ship_combat
    label: "Ship Combat"
    category: combat
    metric:
      name: hull
      direction: descending
      starting: 100
      threshold_low: 0
    beats:
      - id: broadside
        label: "Broadside"
        metric_delta: -15
        stat_check: COMMAND
  - type: dogfight
    label: "Dogfight"
    category: combat
    resolution_mode: sealed_letter_lookup
    metric:
      name: engagement_control
      direction: bidirectional
      starting: 0
      threshold_high: 100
      threshold_low: -100
    beats:
      - id: straight
        label: "Straight"
        metric_delta: 0
        stat_check: GRIT
"#;
    let rules: RulesConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(rules.confrontations.len(), 2);

    // ship_combat: no explicit resolution_mode → BeatSelection default
    assert_eq!(
        rules.confrontations[0].resolution_mode,
        ResolutionMode::BeatSelection,
        "ship_combat without resolution_mode should default to BeatSelection"
    );

    // dogfight: explicit sealed_letter_lookup
    assert_eq!(
        rules.confrontations[1].resolution_mode,
        ResolutionMode::SealedLetterLookup,
        "dogfight with explicit resolution_mode should be SealedLetterLookup"
    );
}

// =========================================================================
// AC-GenrePacks: All existing genre packs load without error
// =========================================================================

/// Helper to locate genre packs directory.
fn genre_packs_path() -> std::path::PathBuf {
    if let Ok(path) = std::env::var("GENRE_PACKS_PATH") {
        return std::path::PathBuf::from(path);
    }
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.join("../../../sidequest-content/genre_packs")
}

#[test]
fn all_genre_packs_load_after_resolution_mode_addition() {
    let packs_dir = genre_packs_path();
    let entries: Vec<_> = std::fs::read_dir(&packs_dir)
        .expect("should read genre_packs directory")
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .collect();

    assert!(
        !entries.is_empty(),
        "should find at least one genre pack in {}",
        packs_dir.display()
    );

    // Skip packs with in-progress schemas
    const SKIP_PACKS: &[&str] = &["caverns_and_claudes"];

    for entry in &entries {
        let pack_path = entry.path();
        let pack_name = pack_path.file_name().unwrap().to_string_lossy();

        if SKIP_PACKS.contains(&pack_name.as_ref()) {
            continue;
        }

        let pack = sidequest_genre::load_genre_pack(&pack_path).unwrap_or_else(|e| {
            panic!(
                "genre pack '{}' must load without error after resolution_mode addition: {:?}",
                pack_name, e
            )
        });


        // Most confrontations should default to BeatSelection, except those
        // explicitly configured with sealed_letter_lookup (story 38-4)
        for conf in &pack.rules.confrontations {
            let expected = if pack_name.as_ref() == "space_opera" && conf.confrontation_type == "dogfight" {
                ResolutionMode::SealedLetterLookup
            } else {
                ResolutionMode::BeatSelection
            };
            assert_eq!(
                conf.resolution_mode,
                expected,
                "genre pack '{}' confrontation '{}' has unexpected resolution_mode",
                pack_name,
                conf.confrontation_type
            );
        }
    }
}

// =========================================================================
// Wiring test: ResolutionMode is re-exported from sidequest_genre
// =========================================================================

#[test]
fn resolution_mode_is_publicly_exported() {
    // This test verifies that ResolutionMode is accessible through the
    // public API of sidequest_genre (via `pub use models::*`).
    // If this compiles, the export is wired.
    let _: sidequest_genre::ResolutionMode = sidequest_genre::ResolutionMode::BeatSelection;
}

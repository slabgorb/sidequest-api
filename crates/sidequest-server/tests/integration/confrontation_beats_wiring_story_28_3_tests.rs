//! Story 28-3: Populate beats in Confrontation protocol message
//!
//! The Confrontation message must carry genre-defined beat options so the UI
//! ConfrontationOverlay can render player choices. Before 28-3, `beats: vec![]`
//! was hardcoded — the overlay had no buttons.
//!
//! ACs tested:
//!   AC-Beats-Populated: Beats field is non-empty when ConfrontationDef exists
//!   AC-1:1-Mapping:     BeatDef fields map to ConfrontationBeat fields
//!   AC-Graceful:        Unknown encounter_type → empty beats
//!   AC-OTEL:            encounter.beats_sent event emitted
//!   AC-Wiring:          beats: vec![] removed from dispatch/mod.rs

use sidequest_genre::ConfrontationDef;
use sidequest_protocol::NonBlankString;
use sidequest_server::find_confrontation_def;

fn nbs(s: &str) -> NonBlankString {
    NonBlankString::new(s).expect("test literal must be non-blank")
}

// =========================================================================
// AC-Beats-Populated: Beats field is non-empty when def exists
// =========================================================================

/// When a ConfrontationDef exists for the encounter_type, find_confrontation_def
/// returns it and its beats vec is non-empty.
#[test]
fn beats_populated_from_known_def() {
    let standoff_yaml = r#"
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
  - id: draw
    label: "Draw"
    metric_delta: 5
    stat_check: REFLEX
    resolution: true
    risk: "Miss and take damage"
"#;
    let defs: Vec<ConfrontationDef> = vec![serde_yaml::from_str(standoff_yaml).unwrap()];
    let def = find_confrontation_def(&defs, "standoff");
    assert!(def.is_some(), "Must find standoff def");
    let def = def.unwrap();
    assert!(
        !def.beats.is_empty(),
        "Beats must be non-empty for a known def"
    );
    assert_eq!(def.beats.len(), 2, "standoff has exactly 2 beats");
}

// =========================================================================
// AC-1:1-Mapping: BeatDef fields map to ConfrontationBeat fields
// =========================================================================

/// Every BeatDef field must map correctly to sidequest_protocol::ConfrontationBeat.
/// Tests the field-by-field mapping that dispatch/mod.rs performs.
#[test]
fn beat_def_maps_to_confrontation_beat_fields() {
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
  - id: draw
    label: "Draw"
    metric_delta: 5
    stat_check: REFLEX
    risk: "Miss and take damage"
    resolution: true
"#;
    let def: ConfrontationDef = serde_yaml::from_str(yaml).unwrap();
    let beat = &def.beats[0];

    // Map exactly as dispatch/mod.rs does
    let protocol_beat = sidequest_protocol::ConfrontationBeat {
        id: nbs(&beat.id),
        label: nbs(&beat.label),
        metric_delta: beat.metric_delta,
        stat_check: beat.stat_check.clone(),
        risk: beat.risk.clone(),
        resolution: beat.resolution.unwrap_or(false),
    };

    assert_eq!(protocol_beat.id.as_str(), "draw");
    assert_eq!(protocol_beat.label.as_str(), "Draw");
    assert_eq!(protocol_beat.metric_delta, 5);
    assert_eq!(protocol_beat.stat_check, "REFLEX");
    assert_eq!(protocol_beat.risk, Some("Miss and take damage".to_string()));
    assert!(protocol_beat.resolution, "resolution=true must map to true");
}

/// When resolution is not set in BeatDef (None), it maps to false.
#[test]
fn beat_def_resolution_none_maps_to_false() {
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
    let beat = &def.beats[0];
    assert!(beat.resolution.is_none(), "No resolution field set in YAML");

    let protocol_beat = sidequest_protocol::ConfrontationBeat {
        id: nbs(&beat.id),
        label: nbs(&beat.label),
        metric_delta: beat.metric_delta,
        stat_check: beat.stat_check.clone(),
        risk: beat.risk.clone(),
        resolution: beat.resolution.unwrap_or(false),
    };
    assert!(
        !protocol_beat.resolution,
        "None resolution must map to false"
    );
}

/// When risk is not set in BeatDef, it maps to None.
#[test]
fn beat_def_risk_none_maps_to_none() {
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
  - id: block
    label: "Block"
    metric_delta: 0
    stat_check: MIGHT
"#;
    let def: ConfrontationDef = serde_yaml::from_str(yaml).unwrap();
    let beat = &def.beats[0];
    assert!(beat.risk.is_none(), "No risk field set in YAML");

    let protocol_beat = sidequest_protocol::ConfrontationBeat {
        id: nbs(&beat.id),
        label: nbs(&beat.label),
        metric_delta: beat.metric_delta,
        stat_check: beat.stat_check.clone(),
        risk: beat.risk.clone(),
        resolution: beat.resolution.unwrap_or(false),
    };
    assert!(protocol_beat.risk.is_none(), "None risk must stay None");
}

// =========================================================================
// AC-Graceful: Unknown encounter_type → empty beats
// =========================================================================

/// When no ConfrontationDef matches the encounter_type, find_confrontation_def
/// returns None. The dispatch code falls back to empty beats via unwrap_or_default().
#[test]
fn unknown_encounter_type_yields_no_def() {
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
"#;
    let defs: Vec<ConfrontationDef> = vec![serde_yaml::from_str(yaml).unwrap()];

    // "combat" doesn't exist in defs — graceful miss
    let result = find_confrontation_def(&defs, "combat");
    assert!(result.is_none(), "Unknown encounter_type must return None");

    // The dispatch code does: def.map(...).unwrap_or_default()
    let beats: Vec<sidequest_protocol::ConfrontationBeat> = result
        .map(|d| {
            d.beats
                .iter()
                .map(|b| sidequest_protocol::ConfrontationBeat {
                    id: nbs(&b.id),
                    label: nbs(&b.label),
                    metric_delta: b.metric_delta,
                    stat_check: b.stat_check.clone(),
                    risk: b.risk.clone(),
                    resolution: b.resolution.unwrap_or(false),
                })
                .collect()
        })
        .unwrap_or_default();
    assert!(
        beats.is_empty(),
        "Unknown type must produce empty beats vec"
    );
}

// =========================================================================
// AC-Wiring: beats: vec![] removed from dispatch/mod.rs
// =========================================================================

/// Verify that the hardcoded `beats: vec![]` no longer exists in the
/// Confrontation message builder. This is a file-level wiring test.
#[test]
fn beats_vec_empty_removed_from_dispatch() {
    let dispatch_mod = crate::test_helpers::dispatch_source_combined();

    // The old hardcoded empty beats
    assert!(
        !dispatch_mod.contains("beats: vec![]"),
        "dispatch/mod.rs must not contain 'beats: vec![]' — beats should come from ConfrontationDef"
    );

    // Verify the wiring: find_confrontation_def must be called in the file
    assert!(
        dispatch_mod.contains("find_confrontation_def"),
        "dispatch/mod.rs must call find_confrontation_def to look up beats"
    );
}

// =========================================================================
// AC-OTEL: encounter.beats_sent event emitted
// =========================================================================

/// The dispatch must emit an encounter.beats_sent OTEL event when beats are
/// populated, so the GM panel can verify beats are flowing.
/// This test scans the source for the WatcherEventBuilder call.
#[test]
fn otel_beats_sent_event_exists_in_dispatch() {
    let dispatch_mod = crate::test_helpers::dispatch_source_combined();

    assert!(
        dispatch_mod.contains("beats_sent"),
        "dispatch/mod.rs must emit an encounter.beats_sent OTEL event when beats are populated"
    );
}

// =========================================================================
// AC-1:1-Mapping: Label and category come from ConfrontationDef, not hardcoded
// =========================================================================

/// When a ConfrontationDef exists, label and category should come from the def,
/// not from naive string manipulation of encounter_type.
#[test]
fn label_and_category_from_def_not_hardcoded() {
    let yaml = r#"
type: standoff
label: "Tense Standoff"
category: pre_combat
metric:
  name: tension
  direction: ascending
  starting: 0
  threshold_high: 10
beats:
  - id: draw
    label: "Draw"
    metric_delta: 5
    stat_check: REFLEX
"#;
    let defs: Vec<ConfrontationDef> = vec![serde_yaml::from_str(yaml).unwrap()];
    let def = find_confrontation_def(&defs, "standoff").unwrap();

    // Label comes from def, not encounter_type.replace('_', " ")
    assert_eq!(def.label, "Tense Standoff");
    assert_ne!(
        def.label, "standoff",
        "Label must not be raw encounter_type"
    );

    // Category comes from def, not cloned encounter_type
    assert_eq!(def.category, "pre_combat");
    assert_ne!(
        def.category, "standoff",
        "Category must not be raw encounter_type"
    );
}

// =========================================================================
// Integration: Real genre pack beats are mappable
// =========================================================================

/// spaghetti_western's standoff confrontation type has beats that must map
/// cleanly to ConfrontationBeat protocol structs.
#[test]
fn spaghetti_western_standoff_beats_map_to_protocol() {
    let genre_packs_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("sidequest-content")
        .join("genre_packs");

    let sw_rules_path = genre_packs_path
        .join("spaghetti_western")
        .join("rules.yaml");
    assert!(
        sw_rules_path.exists(),
        "spaghetti_western/rules.yaml must exist at {:?}",
        sw_rules_path
    );

    let rules_yaml = std::fs::read_to_string(&sw_rules_path).unwrap();
    let rules: sidequest_genre::RulesConfig = serde_yaml::from_str(&rules_yaml).unwrap();

    let standoff = rules
        .confrontations
        .iter()
        .find(|c| c.confrontation_type == "standoff")
        .expect("spaghetti_western must have a standoff confrontation type");

    assert!(!standoff.beats.is_empty(), "standoff must have beats");

    // Map every beat to the protocol type — must not panic
    let protocol_beats: Vec<sidequest_protocol::ConfrontationBeat> = standoff
        .beats
        .iter()
        .map(|b| sidequest_protocol::ConfrontationBeat {
            id: nbs(&b.id),
            label: nbs(&b.label),
            metric_delta: b.metric_delta,
            stat_check: b.stat_check.clone(),
            risk: b.risk.clone(),
            resolution: b.resolution.unwrap_or(false),
        })
        .collect();

    assert!(
        !protocol_beats.is_empty(),
        "Mapped protocol beats must be non-empty"
    );

    // Every beat must have a non-empty id and label — now enforced by the
    // NonBlankString type at construction (see nbs() helper and the protocol
    // crate's NonBlankString newtype). A blank literal would panic inside
    // nbs() before reaching these assertions.
    for beat in &protocol_beats {
        assert!(!beat.id.as_str().is_empty(), "Beat id must not be empty");
        assert!(!beat.label.as_str().is_empty(), "Beat label must not be empty");
        assert!(
            !beat.stat_check.is_empty(),
            "Beat stat_check must not be empty"
        );
    }
}

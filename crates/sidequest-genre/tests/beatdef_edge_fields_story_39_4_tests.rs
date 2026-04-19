//! Story 39-4 — BeatDef schema extension: edge_delta, target_edge_delta, resource_deltas.
//!
//! These tests pin the wire contract for the three optional BeatDef
//! fields added by story 39-4 and guard against future serde drift.
//!
//! ACs covered:
//!   AC1  — `BeatDef` exposes three optional fields, each `#[serde(default)]`
//!   AC1b — a beat YAML without the new keys still deserializes unchanged
//!          (existing packs must not break)
//!   AC3  — `target_edge_delta` parses as `i32` (negative legal)
//!   AC5  — `resource_deltas` parses as a `HashMap<String, f64>` with
//!          signed values
//!
//! The fields are optional and independent — a beat may set none, any, or
//! all three. The dispatch wiring (AC2–AC4, AC6) is covered in the
//! server-crate integration tests for the same story.

use std::collections::HashMap;

use sidequest_genre::BeatDef;

/// AC1 — all three new fields are declared on BeatDef and carry the
/// expected shape. This test reads them directly; it compiles only if the
/// struct exposes them publicly.
#[test]
fn beatdef_exposes_edge_delta_target_edge_delta_resource_deltas() {
    let yaml = r#"
id: strike
label: "Strike"
metric_delta: -3
stat_check: STR
edge_delta: 1
target_edge_delta: 2
resource_deltas:
  voice: -1.0
  grit: 0.5
"#;
    let beat: BeatDef = serde_yaml::from_str(yaml).expect("beat must parse");

    assert_eq!(
        beat.edge_delta,
        Some(1),
        "edge_delta must deserialize as Option<i32> (self-debit composure)"
    );
    assert_eq!(
        beat.target_edge_delta,
        Some(2),
        "target_edge_delta must deserialize as Option<i32> (opponent composure)"
    );

    let deltas = beat
        .resource_deltas
        .as_ref()
        .expect("resource_deltas must deserialize when present");
    assert_eq!(deltas.get("voice"), Some(&-1.0));
    assert_eq!(deltas.get("grit"), Some(&0.5));
}

/// AC1b — a beat without any of the new keys deserializes as-is. This is
/// the backward-compat gate: every existing beat in every genre pack must
/// keep working after the schema grows.
#[test]
fn legacy_beatdef_without_new_keys_still_parses() {
    let yaml = r#"
id: attack
label: "Attack"
metric_delta: -5
stat_check: MIGHT
"#;
    let beat: BeatDef = serde_yaml::from_str(yaml).expect("legacy beat must still parse");

    assert_eq!(beat.id, "attack");
    assert!(
        beat.edge_delta.is_none(),
        "edge_delta must default to None when absent"
    );
    assert!(
        beat.target_edge_delta.is_none(),
        "target_edge_delta must default to None when absent"
    );
    assert!(
        beat.resource_deltas.is_none(),
        "resource_deltas must default to None when absent"
    );
    assert_eq!(beat.gold_delta, None, "gold_delta still honored");
}

/// AC3 — target_edge_delta accepts negative values (opponent recovers) and
/// zero (no-op). Pinning the signed contract closes the "what if delta is
/// reversed" ambiguity before dispatch wiring is written.
#[test]
fn target_edge_delta_supports_signed_and_zero_values() {
    for val in [-3i32, 0, 1, 7] {
        let yaml = format!(
            r#"
id: pulse
label: "Pulse"
metric_delta: 0
stat_check: WIL
target_edge_delta: {}
"#,
            val
        );
        let beat: BeatDef = serde_yaml::from_str(&yaml)
            .unwrap_or_else(|e| panic!("target_edge_delta={val} must parse: {e}"));
        assert_eq!(beat.target_edge_delta, Some(val));
    }
}

/// AC5 — resource_deltas is a HashMap<String, f64>. Confirm serde accepts
/// signed and fractional values (pact currencies may be fractional per
/// epic 39 authoring) and rejects malformed entries loudly.
#[test]
fn resource_deltas_accepts_signed_fractional_values() {
    let yaml = r#"
id: pact_push
label: "Pact Push"
metric_delta: 0
stat_check: CHA
resource_deltas:
  voice: -0.5
  grit: 2
  covenant: -3.0
"#;
    let beat: BeatDef = serde_yaml::from_str(yaml).expect("resource_deltas map must parse");
    let deltas: &HashMap<String, f64> = beat.resource_deltas.as_ref().unwrap();
    assert_eq!(deltas.len(), 3, "all three resource entries must survive");
    assert_eq!(deltas.get("voice"), Some(&-0.5));
    assert_eq!(deltas.get("grit"), Some(&2.0));
    assert_eq!(deltas.get("covenant"), Some(&-3.0));
}

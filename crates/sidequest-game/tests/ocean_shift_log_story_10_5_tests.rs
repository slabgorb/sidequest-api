//! Story 10-5: OCEAN shift log — track personality changes with cause attribution.
//!
//! RED phase: tests compile against stub types but FAIL because the logic is
//! not yet implemented.
//!
//! Acceptance criteria:
//!   AC-1: OceanShift struct with dimension, old/new values, cause, turn
//!   AC-2: OceanShiftLog — collection with push and query
//!   AC-3: apply_shift() modifies OceanProfile, clamps, and logs
//!   AC-4: Clamping — values stay within 0.0–10.0
//!   AC-5: Serde — OceanShift and OceanShiftLog round-trip through YAML
//!   AC-6: Query by dimension — filter log for a specific trait

use sidequest_game::{OceanDimension, OceanProfile, OceanShift, OceanShiftLog};

// ─── AC-1: OceanShift struct fields ─────────────────────────

#[test]
fn ocean_shift_has_required_fields() {
    let shift = OceanShift {
        dimension: OceanDimension::Extraversion,
        old_value: 5.0,
        new_value: 7.0,
        cause: "rallied the crowd".to_string(),
        turn: 3,
    };
    assert_eq!(shift.dimension, OceanDimension::Extraversion);
    assert!((shift.old_value - 5.0).abs() < f64::EPSILON);
    assert!((shift.new_value - 7.0).abs() < f64::EPSILON);
    assert_eq!(shift.cause, "rallied the crowd");
    assert_eq!(shift.turn, 3);
}

#[test]
fn ocean_dimension_enum_has_all_five() {
    // Ensure all five variants exist and are distinct.
    let dims = [
        OceanDimension::Openness,
        OceanDimension::Conscientiousness,
        OceanDimension::Extraversion,
        OceanDimension::Agreeableness,
        OceanDimension::Neuroticism,
    ];
    for (i, a) in dims.iter().enumerate() {
        for (j, b) in dims.iter().enumerate() {
            if i != j {
                assert_ne!(a, b);
            }
        }
    }
}

// ─── AC-2: OceanShiftLog push and access ────────────────────

#[test]
fn shift_log_starts_empty() {
    let log = OceanShiftLog::default();
    assert!(log.shifts().is_empty());
}

#[test]
fn shift_log_push_appends_entry() {
    let mut log = OceanShiftLog::default();
    let shift = OceanShift {
        dimension: OceanDimension::Openness,
        old_value: 5.0,
        new_value: 6.5,
        cause: "discovered ancient library".to_string(),
        turn: 1,
    };
    log.push(shift.clone());
    assert_eq!(log.shifts().len(), 1, "push should append one entry");
    assert_eq!(log.shifts()[0], shift);
}

// ─── AC-3: apply_shift modifies profile and logs ────────────

#[test]
fn apply_shift_changes_dimension_value() {
    let mut profile = OceanProfile::default(); // all 5.0
    let mut log = OceanShiftLog::default();

    profile.apply_shift(
        OceanDimension::Agreeableness,
        2.0,
        "helped a stranger".to_string(),
        1,
        &mut log,
    );

    let new_val = profile.get(OceanDimension::Agreeableness);
    assert!(
        (new_val - 7.0).abs() < f64::EPSILON,
        "agreeableness should be 5.0 + 2.0 = 7.0, got {new_val}"
    );
}

#[test]
fn apply_shift_logs_old_and_new_values() {
    let mut profile = OceanProfile::default();
    let mut log = OceanShiftLog::default();

    profile.apply_shift(
        OceanDimension::Neuroticism,
        -1.5,
        "meditation retreat".to_string(),
        4,
        &mut log,
    );

    assert_eq!(log.shifts().len(), 1, "one shift should be logged");
    let entry = &log.shifts()[0];
    assert_eq!(entry.dimension, OceanDimension::Neuroticism);
    assert!((entry.old_value - 5.0).abs() < f64::EPSILON);
    assert!((entry.new_value - 3.5).abs() < f64::EPSILON);
    assert_eq!(entry.cause, "meditation retreat");
    assert_eq!(entry.turn, 4);
}

#[test]
fn apply_shift_returns_new_value() {
    let mut profile = OceanProfile::default();
    let mut log = OceanShiftLog::default();

    let result = profile.apply_shift(
        OceanDimension::Conscientiousness,
        1.0,
        "adopted a routine".to_string(),
        2,
        &mut log,
    );
    assert!(
        (result - 6.0).abs() < f64::EPSILON,
        "apply_shift should return new value 6.0, got {result}"
    );
}

// ─── AC-4: Clamping ─────────────────────────────────────────

#[test]
fn apply_shift_clamps_above_ten() {
    let mut profile = OceanProfile::default();
    let mut log = OceanShiftLog::default();

    // 5.0 + 7.0 = 12.0 → should clamp to 10.0
    profile.apply_shift(
        OceanDimension::Extraversion,
        7.0,
        "became legendary orator".to_string(),
        5,
        &mut log,
    );

    let val = profile.get(OceanDimension::Extraversion);
    assert!(
        (val - 10.0).abs() < f64::EPSILON,
        "should clamp to 10.0, got {val}"
    );
    assert!(
        (log.shifts()[0].new_value - 10.0).abs() < f64::EPSILON,
        "logged new_value should also be clamped"
    );
}

#[test]
fn apply_shift_clamps_below_zero() {
    let mut profile = OceanProfile {
        openness: 2.0,
        ..Default::default()
    };
    let mut log = OceanShiftLog::default();

    // 2.0 - 5.0 = -3.0 → should clamp to 0.0
    profile.apply_shift(
        OceanDimension::Openness,
        -5.0,
        "traumatic betrayal".to_string(),
        7,
        &mut log,
    );

    let val = profile.get(OceanDimension::Openness);
    assert!(val.abs() < f64::EPSILON, "should clamp to 0.0, got {val}");
    assert!(
        log.shifts()[0].new_value.abs() < f64::EPSILON,
        "logged new_value should also be clamped to 0.0"
    );
}

// ─── AC-5: Serde round-trip ─────────────────────────────────

#[test]
fn ocean_shift_yaml_round_trip() {
    let shift = OceanShift {
        dimension: OceanDimension::Conscientiousness,
        old_value: 4.0,
        new_value: 6.0,
        cause: "kept a promise under duress".to_string(),
        turn: 10,
    };
    let yaml = serde_yaml::to_string(&shift).expect("serialize OceanShift");
    let back: OceanShift = serde_yaml::from_str(&yaml).expect("deserialize OceanShift");
    assert_eq!(shift, back);
}

#[test]
fn ocean_shift_log_yaml_round_trip() {
    let mut log = OceanShiftLog::default();
    log.push(OceanShift {
        dimension: OceanDimension::Openness,
        old_value: 5.0,
        new_value: 7.0,
        cause: "explored the unknown".to_string(),
        turn: 1,
    });
    log.push(OceanShift {
        dimension: OceanDimension::Neuroticism,
        old_value: 5.0,
        new_value: 3.0,
        cause: "survived the ordeal".to_string(),
        turn: 2,
    });

    let yaml = serde_yaml::to_string(&log).expect("serialize OceanShiftLog");
    let back: OceanShiftLog = serde_yaml::from_str(&yaml).expect("deserialize OceanShiftLog");
    assert_eq!(log, back);
}

// ─── AC-6: Query by dimension ───────────────────────────────

#[test]
fn shifts_for_filters_by_dimension() {
    let mut profile = OceanProfile::default();
    let mut log = OceanShiftLog::default();

    profile.apply_shift(
        OceanDimension::Openness,
        1.0,
        "read a book".to_string(),
        1,
        &mut log,
    );
    profile.apply_shift(
        OceanDimension::Neuroticism,
        -0.5,
        "calming music".to_string(),
        2,
        &mut log,
    );
    profile.apply_shift(
        OceanDimension::Openness,
        0.5,
        "met a traveler".to_string(),
        3,
        &mut log,
    );

    let openness_shifts = log.shifts_for(OceanDimension::Openness);
    assert_eq!(
        openness_shifts.len(),
        2,
        "should find 2 Openness shifts, got {}",
        openness_shifts.len()
    );
    for s in &openness_shifts {
        assert_eq!(s.dimension, OceanDimension::Openness);
    }

    let neuro_shifts = log.shifts_for(OceanDimension::Neuroticism);
    assert_eq!(neuro_shifts.len(), 1);
    assert_eq!(neuro_shifts[0].dimension, OceanDimension::Neuroticism);
}

#[test]
fn shifts_for_returns_empty_when_no_matches() {
    let log = OceanShiftLog::default();
    let result = log.shifts_for(OceanDimension::Agreeableness);
    assert!(result.is_empty());
}

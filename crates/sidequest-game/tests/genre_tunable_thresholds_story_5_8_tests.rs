//! Story 5-8: Genre-tunable thresholds — drama_weight breakpoints configurable per genre pack
//!
//! RED phase — these tests verify that DramaThresholds can be deserialized from YAML
//! and that genre-loaded thresholds change pacing_hint output vs defaults.
//!
//! ACs tested:
//!   AC1 — DramaThresholds is deserializable from YAML (via YAML shape validation)
//!   AC6 — Thresholds flow through to pacing_hint()

use sidequest_game::tension_tracker::{
    CombatEvent, DeliveryMode, DramaThresholds, TensionTracker,
};

// ============================================================================
// AC1: DramaThresholds is deserializable from YAML
// ============================================================================

/// Helper: parse YAML into DramaThresholds fields via serde_yaml::Value.
/// This validates the expected YAML shape. Once Deserialize is derived,
/// the genre-crate tests handle typed deserialization.
fn thresholds_from_yaml_value(yaml: &str) -> DramaThresholds {
    let value: serde_yaml::Value =
        serde_yaml::from_str(yaml).expect("YAML should parse");
    let map = value.as_mapping().expect("top-level should be a mapping");

    let get_f64 = |key: &str, default: f64| -> f64 {
        map.get(serde_yaml::Value::String(key.to_string()))
            .and_then(|v| v.as_f64())
            .unwrap_or(default)
    };
    let get_u32 = |key: &str, default: u32| -> u32 {
        map.get(serde_yaml::Value::String(key.to_string()))
            .and_then(|v| v.as_u64())
            .map(|v| v as u32)
            .unwrap_or(default)
    };

    let defaults = DramaThresholds::default();
    DramaThresholds {
        sentence_delivery_min: get_f64("sentence_delivery_min", defaults.sentence_delivery_min),
        streaming_delivery_min: get_f64("streaming_delivery_min", defaults.streaming_delivery_min),
        render_threshold: get_f64("render_threshold", defaults.render_threshold),
        escalation_streak: get_u32("escalation_streak", defaults.escalation_streak),
        ramp_length: get_u32("ramp_length", defaults.ramp_length),
    }
}

#[test]
fn drama_thresholds_yaml_shape_has_all_fields() {
    let yaml = r#"
sentence_delivery_min: 0.20
streaming_delivery_min: 0.60
render_threshold: 0.35
escalation_streak: 3
ramp_length: 6
"#;
    let thresholds = thresholds_from_yaml_value(yaml);

    assert!(
        (thresholds.sentence_delivery_min - 0.20).abs() < f64::EPSILON,
        "sentence_delivery_min should be 0.20, got {}",
        thresholds.sentence_delivery_min,
    );
    assert!(
        (thresholds.streaming_delivery_min - 0.60).abs() < f64::EPSILON,
        "streaming_delivery_min should be 0.60",
    );
    assert!(
        (thresholds.render_threshold - 0.35).abs() < f64::EPSILON,
        "render_threshold should be 0.35",
    );
    assert_eq!(thresholds.escalation_streak, 3);
    assert_eq!(thresholds.ramp_length, 6);
}

#[test]
fn drama_thresholds_partial_yaml_fills_defaults() {
    let yaml = r#"
sentence_delivery_min: 0.15
escalation_streak: 10
"#;
    let thresholds = thresholds_from_yaml_value(yaml);
    let defaults = DramaThresholds::default();

    assert!(
        (thresholds.sentence_delivery_min - 0.15).abs() < f64::EPSILON,
        "overridden field should be 0.15",
    );
    assert_eq!(thresholds.escalation_streak, 10);
    // Non-overridden fields should match defaults
    assert!(
        (thresholds.streaming_delivery_min - defaults.streaming_delivery_min).abs() < f64::EPSILON,
        "non-overridden streaming_delivery_min should use default",
    );
    assert!(
        (thresholds.render_threshold - defaults.render_threshold).abs() < f64::EPSILON,
        "non-overridden render_threshold should use default",
    );
    assert_eq!(thresholds.ramp_length, defaults.ramp_length);
}

#[test]
fn drama_thresholds_implements_serde_deserialize() {
    // AC1: DramaThresholds must derive Deserialize.
    // This is a compile-time trait bound check done at runtime via a generic helper.
    fn assert_deserializable<'de, T: serde::Deserialize<'de>>() {}
    // FAILS at compile time until `derive(Deserialize)` is added to DramaThresholds.
    // For now, we check the concept via the YAML shape tests above,
    // and this test serves as documentation of the requirement.
    //
    // Uncomment the line below once Deserialize is derived — it will then compile:
    // assert_deserializable::<DramaThresholds>();

    // Until then, this test FAILS to signal RED:
    panic!(
        "DramaThresholds does not yet derive serde::Deserialize. \
         Add `#[derive(Serialize, Deserialize)]` and `#[serde(default)]` to DramaThresholds."
    );
}

#[test]
fn drama_thresholds_implements_serde_serialize() {
    // AC1: DramaThresholds must derive Serialize.
    // Same pattern as above — fails until Serialize is derived.
    panic!(
        "DramaThresholds does not yet derive serde::Serialize. \
         Add `#[derive(Serialize, Deserialize)]` to DramaThresholds."
    );
}

// ============================================================================
// AC6: Thresholds flow through to pacing_hint()
// ============================================================================

#[test]
fn custom_thresholds_change_delivery_mode_boundary() {
    // With default thresholds, sentence_delivery_min=0.30.
    // If we lower it to 0.01, a tracker with any non-zero drama should get Sentence.
    let mut tracker = TensionTracker::new();
    tracker.record_event(CombatEvent::Boring);
    tracker.record_event(CombatEvent::Boring);

    let dw = tracker.drama_weight();
    assert!(dw > 0.0, "two boring turns should produce non-zero drama_weight");

    let custom = DramaThresholds {
        sentence_delivery_min: 0.01,
        streaming_delivery_min: 0.95,
        ..DramaThresholds::default()
    };

    let custom_hint = tracker.pacing_hint(&custom);
    assert_eq!(
        custom_hint.delivery_mode,
        DeliveryMode::Sentence,
        "low sentence_delivery_min=0.01 should produce Sentence for drama_weight={dw:.3}",
    );
}

#[test]
fn custom_thresholds_raise_streaming_boundary() {
    // With default streaming_delivery_min=0.70, high drama triggers Streaming.
    // Raise it to 0.99 — even high drama should stay at Sentence.
    let mut tracker = TensionTracker::new();
    tracker.inject_spike(0.85);

    let dw = tracker.drama_weight();
    assert!(dw > 0.70, "spike of 0.85 should produce drama_weight > 0.70");

    // Default thresholds → should be Streaming
    let default_hint = tracker.pacing_hint(&DramaThresholds::default());
    assert_eq!(
        default_hint.delivery_mode,
        DeliveryMode::Streaming,
        "default thresholds with dw={dw:.3} should be Streaming",
    );

    let custom = DramaThresholds {
        streaming_delivery_min: 0.99,
        ..DramaThresholds::default()
    };

    let custom_hint = tracker.pacing_hint(&custom);
    assert_eq!(
        custom_hint.delivery_mode,
        DeliveryMode::Sentence,
        "streaming_delivery_min=0.99 should keep dw={dw:.3} at Sentence, not Streaming",
    );
}

#[test]
fn custom_escalation_streak_triggers_earlier() {
    let mut tracker = TensionTracker::new();
    for _ in 0..3 {
        tracker.record_event(CombatEvent::Boring);
    }

    // Default escalation_streak=5 → no escalation beat after 3 boring turns
    let default_hint = tracker.pacing_hint(&DramaThresholds::default());
    assert!(
        default_hint.escalation_beat.is_none(),
        "default thresholds should NOT escalate after 3 boring turns",
    );

    let custom = DramaThresholds {
        escalation_streak: 2,
        ..DramaThresholds::default()
    };

    let custom_hint = tracker.pacing_hint(&custom);
    assert!(
        custom_hint.escalation_beat.is_some(),
        "custom escalation_streak=2 should trigger escalation after 3 boring turns",
    );
}

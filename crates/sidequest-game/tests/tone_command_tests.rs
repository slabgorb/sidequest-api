//! Tests for /tone command (F2/F10 — narrative axis values).
//!
//! Covers: show, preset, set, case-insensitive lookup, out-of-range rejection,
//! format_tone_context prompt injection.

use std::collections::HashMap;

use sidequest_game::axis::{format_tone_context, AxisValue, ToneCommand};
use sidequest_game::slash_router::{CommandHandler, CommandResult};
use sidequest_game::state::GameSnapshot;
use sidequest_genre::{AxesConfig, AxisDefinition, AxisPreset};

fn test_axes_config() -> AxesConfig {
    AxesConfig {
        definitions: vec![
            AxisDefinition {
                id: "weirdness".to_string(),
                name: "Weirdness".to_string(),
                description: "How gonzo".to_string(),
                poles: vec!["grounded".to_string(), "gonzo".to_string()],
                default: 0.5,
            },
            AxisDefinition {
                id: "hope".to_string(),
                name: "Hope".to_string(),
                description: "Recovery vs decline".to_string(),
                poles: vec!["declining".to_string(), "recovering".to_string()],
                default: 0.5,
            },
        ],
        modifiers: {
            let mut m = HashMap::new();
            let mut weirdness = HashMap::new();
            weirdness.insert("grounded".to_string(), "Keep it realistic.".to_string());
            weirdness.insert("gonzo".to_string(), "Embrace the weird.".to_string());
            m.insert("weirdness".to_string(), weirdness);
            let mut hope = HashMap::new();
            hope.insert(
                "declining".to_string(),
                "World is getting worse.".to_string(),
            );
            hope.insert("recovering".to_string(), "Signs of recovery.".to_string());
            m.insert("hope".to_string(), hope);
            m
        },
        presets: vec![
            AxisPreset {
                name: "Caves of Qud".to_string(),
                description: "Maximum weirdness, cautiously hopeful.".to_string(),
                values: {
                    let mut v = HashMap::new();
                    v.insert("weirdness".to_string(), 0.9);
                    v.insert("hope".to_string(), 0.6);
                    v
                },
            },
            AxisPreset {
                name: "The Road".to_string(),
                description: "Minimal weirdness, nearly hopeless.".to_string(),
                values: {
                    let mut v = HashMap::new();
                    v.insert("weirdness".to_string(), 0.1);
                    v.insert("hope".to_string(), 0.15);
                    v
                },
            },
        ],
    }
}

fn test_snapshot() -> GameSnapshot {
    GameSnapshot {
        genre_slug: "mutant_wasteland".to_string(),
        ..Default::default()
    }
}

fn snapshot_with_axes(values: Vec<AxisValue>) -> GameSnapshot {
    GameSnapshot {
        genre_slug: "mutant_wasteland".to_string(),
        axis_values: values,
        ..Default::default()
    }
}

// ============================================================================
// /tone show
// ============================================================================

#[test]
fn tone_show_displays_axes_with_defaults() {
    let cmd = ToneCommand::new(test_axes_config());
    let snap = test_snapshot();
    let result = cmd.handle(&snap, "");
    match result {
        CommandResult::Display(text) => {
            assert!(text.contains("NARRATIVE AXES:"), "Should have header");
            assert!(text.contains("Weirdness"), "Should show weirdness axis");
            assert!(text.contains("Hope"), "Should show hope axis");
            assert!(text.contains("0.50"), "Should show default value");
            assert!(text.contains("PRESETS:"), "Should show presets section");
            assert!(text.contains("Caves of Qud"), "Should show preset name");
        }
        other => panic!("Expected Display, got {:?}", other),
    }
}

#[test]
fn tone_show_with_custom_values() {
    let cmd = ToneCommand::new(test_axes_config());
    let snap = snapshot_with_axes(vec![AxisValue {
        axis_id: "weirdness".to_string(),
        value: 0.9,
    }]);
    let result = cmd.handle(&snap, "show");
    match result {
        CommandResult::Display(text) => {
            assert!(
                text.contains("0.90"),
                "Should show custom value for weirdness"
            );
            assert!(text.contains("0.50"), "Should show default for hope");
        }
        other => panic!("Expected Display, got {:?}", other),
    }
}

// ============================================================================
// /tone preset
// ============================================================================

#[test]
fn tone_preset_applies_values() {
    let cmd = ToneCommand::new(test_axes_config());
    let snap = test_snapshot();
    let result = cmd.handle(&snap, "preset Caves of Qud");
    match result {
        CommandResult::ToneChange(values) => {
            assert_eq!(values.len(), 2, "Should set all axes");
            let weirdness = values.iter().find(|v| v.axis_id == "weirdness").unwrap();
            assert!((weirdness.value - 0.9).abs() < f64::EPSILON);
            let hope = values.iter().find(|v| v.axis_id == "hope").unwrap();
            assert!((hope.value - 0.6).abs() < f64::EPSILON);
        }
        other => panic!("Expected ToneChange, got {:?}", other),
    }
}

#[test]
fn tone_preset_case_insensitive() {
    let cmd = ToneCommand::new(test_axes_config());
    let snap = test_snapshot();
    let result = cmd.handle(&snap, "preset caves of qud");
    match result {
        CommandResult::ToneChange(values) => {
            assert_eq!(values.len(), 2);
        }
        other => panic!("Expected ToneChange, got {:?}", other),
    }
}

#[test]
fn tone_preset_unknown_returns_error() {
    let cmd = ToneCommand::new(test_axes_config());
    let snap = test_snapshot();
    let result = cmd.handle(&snap, "preset Nonexistent");
    match result {
        CommandResult::Error(e) => {
            assert!(e.contains("Unknown preset"), "Error: {}", e);
            assert!(e.contains("Caves of Qud"), "Should list available presets");
        }
        other => panic!("Expected Error, got {:?}", other),
    }
}

#[test]
fn tone_preset_missing_axes_use_defaults() {
    // Preset only sets weirdness, not hope
    let config = AxesConfig {
        definitions: test_axes_config().definitions,
        modifiers: HashMap::new(),
        presets: vec![AxisPreset {
            name: "partial".to_string(),
            description: "Only sets weirdness".to_string(),
            values: {
                let mut v = HashMap::new();
                v.insert("weirdness".to_string(), 0.8);
                v
            },
        }],
    };
    let cmd = ToneCommand::new(config);
    let snap = test_snapshot();
    let result = cmd.handle(&snap, "preset partial");
    match result {
        CommandResult::ToneChange(values) => {
            let hope = values.iter().find(|v| v.axis_id == "hope").unwrap();
            assert!(
                (hope.value - 0.5).abs() < f64::EPSILON,
                "Missing axis should use default 0.5"
            );
        }
        other => panic!("Expected ToneChange, got {:?}", other),
    }
}

// ============================================================================
// /tone set
// ============================================================================

#[test]
fn tone_set_single_axis() {
    let cmd = ToneCommand::new(test_axes_config());
    let snap = test_snapshot();
    let result = cmd.handle(&snap, "set weirdness 0.8");
    match result {
        CommandResult::ToneChange(values) => {
            let weirdness = values.iter().find(|v| v.axis_id == "weirdness").unwrap();
            assert!((weirdness.value - 0.8).abs() < f64::EPSILON);
        }
        other => panic!("Expected ToneChange, got {:?}", other),
    }
}

#[test]
fn tone_set_case_insensitive_axis() {
    let cmd = ToneCommand::new(test_axes_config());
    let snap = test_snapshot();
    let result = cmd.handle(&snap, "set WEIRDNESS 0.3");
    match result {
        CommandResult::ToneChange(values) => {
            let weirdness = values.iter().find(|v| v.axis_id == "weirdness").unwrap();
            assert!((weirdness.value - 0.3).abs() < f64::EPSILON);
        }
        other => panic!("Expected ToneChange, got {:?}", other),
    }
}

#[test]
fn tone_set_rejects_out_of_range() {
    let cmd = ToneCommand::new(test_axes_config());
    let snap = test_snapshot();

    let result = cmd.handle(&snap, "set weirdness 1.5");
    assert!(matches!(result, CommandResult::Error(_)));

    let result = cmd.handle(&snap, "set weirdness -0.1");
    assert!(matches!(result, CommandResult::Error(_)));
}

#[test]
fn tone_set_rejects_invalid_number() {
    let cmd = ToneCommand::new(test_axes_config());
    let snap = test_snapshot();
    let result = cmd.handle(&snap, "set weirdness abc");
    assert!(matches!(result, CommandResult::Error(_)));
}

#[test]
fn tone_set_unknown_axis() {
    let cmd = ToneCommand::new(test_axes_config());
    let snap = test_snapshot();
    let result = cmd.handle(&snap, "set nonexistent 0.5");
    match result {
        CommandResult::Error(e) => {
            assert!(e.contains("Unknown axis"), "Error: {}", e);
            assert!(e.contains("weirdness"), "Should list available axes");
        }
        other => panic!("Expected Error, got {:?}", other),
    }
}

#[test]
fn tone_set_preserves_other_axes() {
    let cmd = ToneCommand::new(test_axes_config());
    let snap = snapshot_with_axes(vec![
        AxisValue {
            axis_id: "weirdness".to_string(),
            value: 0.7,
        },
        AxisValue {
            axis_id: "hope".to_string(),
            value: 0.3,
        },
    ]);
    let result = cmd.handle(&snap, "set weirdness 0.2");
    match result {
        CommandResult::ToneChange(values) => {
            let weirdness = values.iter().find(|v| v.axis_id == "weirdness").unwrap();
            assert!((weirdness.value - 0.2).abs() < f64::EPSILON);
            let hope = values.iter().find(|v| v.axis_id == "hope").unwrap();
            assert!(
                (hope.value - 0.3).abs() < f64::EPSILON,
                "Hope should be unchanged"
            );
        }
        other => panic!("Expected ToneChange, got {:?}", other),
    }
}

// ============================================================================
// format_tone_context (prompt injection)
// ============================================================================

#[test]
fn tone_context_low_value_uses_low_pole() {
    let config = test_axes_config();
    let values = vec![
        AxisValue {
            axis_id: "weirdness".to_string(),
            value: 0.2,
        },
        AxisValue {
            axis_id: "hope".to_string(),
            value: 0.5,
        },
    ];
    let context = format_tone_context(&config, &values);
    assert!(context.contains("[TONE]"), "Should have TONE block");
    assert!(
        context.contains("grounded"),
        "Low weirdness should use grounded pole"
    );
    assert!(
        context.contains("Keep it realistic"),
        "Should include low pole modifier"
    );
    assert!(
        !context.contains("Embrace the weird"),
        "Should not include high pole modifier"
    );
}

#[test]
fn tone_context_high_value_uses_high_pole() {
    let config = test_axes_config();
    let values = vec![
        AxisValue {
            axis_id: "weirdness".to_string(),
            value: 0.8,
        },
        AxisValue {
            axis_id: "hope".to_string(),
            value: 0.5,
        },
    ];
    let context = format_tone_context(&config, &values);
    assert!(
        context.contains("gonzo"),
        "High weirdness should use gonzo pole"
    );
    assert!(
        context.contains("Embrace the weird"),
        "Should include high pole modifier"
    );
}

#[test]
fn tone_context_mid_value_blends_both_poles() {
    let config = test_axes_config();
    let values = vec![AxisValue {
        axis_id: "weirdness".to_string(),
        value: 0.5,
    }];
    let context = format_tone_context(&config, &values);
    assert!(context.contains("balanced"), "Mid value should blend");
    assert!(
        context.contains("Keep it realistic"),
        "Should include low modifier in blend"
    );
    assert!(
        context.contains("Embrace the weird"),
        "Should include high modifier in blend"
    );
}

#[test]
fn tone_context_empty_definitions_returns_empty() {
    let config = AxesConfig {
        definitions: vec![],
        modifiers: HashMap::new(),
        presets: vec![],
    };
    let context = format_tone_context(&config, &[]);
    assert!(
        context.is_empty(),
        "Empty definitions should produce empty context"
    );
}

#[test]
fn tone_context_uses_defaults_when_no_values() {
    let config = test_axes_config();
    // No axis values set — should use defaults (0.5 for both)
    let context = format_tone_context(&config, &[]);
    // Both at 0.5, which is in the blend zone
    assert!(
        context.contains("balanced"),
        "Default 0.5 should be in blend zone"
    );
}

// ============================================================================
// /tone subcommand errors
// ============================================================================

#[test]
fn tone_unknown_subcommand() {
    let cmd = ToneCommand::new(test_axes_config());
    let snap = test_snapshot();
    let result = cmd.handle(&snap, "invalid");
    assert!(matches!(result, CommandResult::Error(_)));
}

#[test]
fn tone_preset_empty_args() {
    let cmd = ToneCommand::new(test_axes_config());
    let snap = test_snapshot();
    let result = cmd.handle(&snap, "preset");
    assert!(matches!(result, CommandResult::Error(_)));
}

#[test]
fn tone_set_missing_value() {
    let cmd = ToneCommand::new(test_axes_config());
    let snap = test_snapshot();
    let result = cmd.handle(&snap, "set weirdness");
    assert!(matches!(result, CommandResult::Error(_)));
}

// ============================================================================
// axis_values persist in GameSnapshot
// ============================================================================

#[test]
fn axis_values_default_empty() {
    let snap = GameSnapshot::default();
    assert!(
        snap.axis_values.is_empty(),
        "Default snapshot should have empty axis values"
    );
}

#[test]
fn axis_values_serializes() {
    let snap = snapshot_with_axes(vec![AxisValue {
        axis_id: "weirdness".to_string(),
        value: 0.7,
    }]);
    let json = serde_json::to_string(&snap).unwrap();
    let restored: GameSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.axis_values.len(), 1);
    assert!((restored.axis_values[0].value - 0.7).abs() < f64::EPSILON);
}

//! Story 39-3 wiring test — `CharacterBuilder` consumes `edge_config`.
//!
//! Drives the production chargen path (`CharacterBuilder::build`) with a
//! heavy_metal-shaped `RulesConfig` that declares `edge_config` and asserts
//! the produced `Character.core.edge` is seeded from YAML (`base_max` matches
//! `edge_config.base_max_by_class[class]`, authored thresholds are attached).
//!
//! Also asserts the loud-failure path: a class missing from
//! `edge_config.base_max_by_class` yields `BuilderError::EdgeConfigMissingClass`
//! — no silent fallback to the placeholder.

use std::collections::HashMap;

use sidequest_game::builder::{BuilderError, CharacterBuilder};
use sidequest_game::combatant::Combatant;
use sidequest_genre::{
    CharCreationChoice, CharCreationScene, EdgeConfig, EdgeRecoveryDefaults, EdgeThresholdDecl,
    MechanicalEffects, RulesConfig,
};

fn heavy_metal_edge_config() -> EdgeConfig {
    EdgeConfig {
        base_max_by_class: HashMap::from([
            ("Fighter".to_string(), 6),
            ("Wizard".to_string(), 4),
        ]),
        recovery_defaults: EdgeRecoveryDefaults {
            on_resolution: Some("full".into()),
            on_long_rest: Some("full".into()),
            between_back_to_back: Some(0),
        },
        thresholds: vec![
            EdgeThresholdDecl {
                at: 1,
                event_id: "edge_strained".into(),
                narrator_hint: "one exchange from breaking".into(),
                direction: Some("crossing_down".into()),
            },
            EdgeThresholdDecl {
                at: 0,
                event_id: "composure_break".into(),
                narrator_hint: "the ledger turns".into(),
                direction: Some("crossing_down".into()),
            },
        ],
        display_fields: vec!["edge".into(), "max_edge".into(), "composure_state".into()],
    }
}

fn rules_with_edge_config(class: &str) -> RulesConfig {
    RulesConfig {
        stat_generation: "standard_array".into(),
        ability_score_names: vec![
            "STR".into(),
            "DEX".into(),
            "CON".into(),
            "INT".into(),
            "WIS".into(),
            "CHA".into(),
        ],
        allowed_classes: vec![class.to_string()],
        allowed_races: vec!["Human".into()],
        default_class: Some(class.to_string()),
        default_race: Some("Human".into()),
        default_location: Some("An ending house".into()),
        default_time_of_day: Some("dusk".into()),
        edge_config: Some(heavy_metal_edge_config()),
        ..Default::default()
    }
}

fn one_scene(class: &str) -> Vec<CharCreationScene> {
    vec![CharCreationScene {
        id: "origin".into(),
        title: "Origin".into(),
        narration: "Choose.".into(),
        choices: vec![CharCreationChoice {
            label: class.into(),
            description: "The path.".into(),
            mechanical_effects: MechanicalEffects {
                class_hint: Some(class.into()),
                race_hint: Some("Human".into()),
                ..Default::default()
            },
        }],
        allows_freeform: Some(false),
        hook_prompt: None,
        loading_text: None,
        mechanical_effects: None,
    }]
}

#[test]
fn builder_seeds_fighter_edge_from_config() {
    let rules = rules_with_edge_config("Fighter");
    let mut builder = CharacterBuilder::new(one_scene("Fighter"), &rules, None);
    builder.apply_choice(0).expect("choice applies");
    let character = builder.build("Halvard").expect("fighter builds");

    assert_eq!(
        character.core.edge.base_max, 6,
        "Fighter base_max should match edge_config.base_max_by_class"
    );
    assert_eq!(character.core.edge.current, 6, "starts at full composure");
    assert_eq!(character.core.edge.max, 6);
    assert_eq!(
        Combatant::max_edge(&character),
        6,
        "Combatant trait reads through to YAML-seeded pool"
    );
    let events: Vec<&str> = character
        .core
        .edge
        .thresholds
        .iter()
        .map(|t| t.event_id.as_str())
        .collect();
    assert!(events.contains(&"edge_strained"));
    assert!(events.contains(&"composure_break"));
}

#[test]
fn builder_seeds_wizard_edge_from_config() {
    let rules = rules_with_edge_config("Wizard");
    let mut builder = CharacterBuilder::new(one_scene("Wizard"), &rules, None);
    builder.apply_choice(0).expect("choice applies");
    let character = builder.build("Lio").expect("wizard builds");

    assert_eq!(
        character.core.edge.base_max, 4,
        "Wizard base_max reflects lower edge capacity"
    );
}

#[test]
fn builder_fails_loudly_when_class_missing_from_edge_config() {
    // Allowed class is not in edge_config.base_max_by_class — must fail,
    // not silently fall back to the placeholder pool.
    let rules = rules_with_edge_config("Bard");
    let mut builder = CharacterBuilder::new(one_scene("Bard"), &rules, None);
    builder.apply_choice(0).expect("choice applies");
    let err = builder.build("Rime").expect_err("should fail");
    match err {
        BuilderError::EdgeConfigMissingClass(class) => assert_eq!(class, "Bard"),
        other => panic!("expected EdgeConfigMissingClass, got {other:?}"),
    }
}

#[test]
fn builder_uses_placeholder_when_edge_config_absent() {
    // Legacy packs (no edge_config) still build — they get the placeholder
    // pool. This is not a silent fallback because we emit an OTEL event
    // flagging the placeholder path.
    let rules = RulesConfig {
        stat_generation: "standard_array".into(),
        ability_score_names: vec![
            "STR".into(),
            "DEX".into(),
            "CON".into(),
            "INT".into(),
            "WIS".into(),
            "CHA".into(),
        ],
        allowed_classes: vec!["Fighter".into()],
        allowed_races: vec!["Human".into()],
        default_class: Some("Fighter".into()),
        default_race: Some("Human".into()),
        edge_config: None,
        ..Default::default()
    };
    let mut builder = CharacterBuilder::new(one_scene("Fighter"), &rules, None);
    builder.apply_choice(0).expect("choice applies");
    let character = builder.build("Grog").expect("builds with placeholder");
    assert_eq!(character.core.edge.base_max, 10, "placeholder base_max");
}

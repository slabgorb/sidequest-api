//! Story 37-21: scene narration placeholder interpolation.
//!
//! Genre packs (space_opera, heavy_metal) author confirmation-scene narration
//! with `{name}`, `{class}`, `{race}` placeholders intended to be substituted
//! from the builder's accumulated state. Before the fix those tokens rendered
//! literally because nothing interpolated `scene.narration` before it was
//! cloned into the `CharacterCreation` payload.
//!
//! These tests exercise the interpolation through the real public
//! `to_scene_message()` entry point — the exact same call the server dispatch
//! loop uses when it sends the scene to the client. A passing unit test on an
//! isolated helper would not prove the payload path is wired correctly.

use std::collections::HashMap;

use sidequest_game::builder::CharacterBuilder;
use sidequest_genre::{CharCreationChoice, CharCreationScene, MechanicalEffects, RulesConfig};
use sidequest_protocol::GameMessage;

fn rules() -> RulesConfig {
    RulesConfig {
        tone: "heroic".to_string(),
        lethality: "medium".to_string(),
        magic_level: "high".to_string(),
        stat_generation: "standard_array".to_string(),
        point_buy_budget: 27,
        ability_score_names: vec![
            "STR".to_string(),
            "DEX".to_string(),
            "CON".to_string(),
            "INT".to_string(),
            "WIS".to_string(),
            "CHA".to_string(),
        ],
        allowed_classes: vec!["Drifter".to_string(), "Spacer".to_string()],
        allowed_races: vec!["Outer Rim".to_string(), "Belt".to_string()],
        class_hp_bases: HashMap::new(),
        default_class: None,
        default_race: None,
        default_hp: Some(10),
        default_ac: Some(10),
        default_location: None,
        default_time_of_day: None,
        hp_formula: None,
        banned_spells: vec![],
        custom_rules: HashMap::new(),
        stat_display_fields: vec![],
        encounter_base_tension: HashMap::new(),
        race_label: None,
        class_label: None,
        confrontations: vec![],
        resources: vec![],
        xp_affinity: None,
        initiative_rules: HashMap::new(),
    }
}

/// Two-scene flow: (1) class choice, (2) freeform name entry with placeholder-bearing narration.
fn scenes_with_placeholders() -> Vec<CharCreationScene> {
    vec![
        CharCreationScene {
            id: "class_choice".to_string(),
            title: "Pick a Path".to_string(),
            narration: "Choose your calling.".to_string(),
            choices: vec![CharCreationChoice {
                label: "Drifter".to_string(),
                description: "Someone drifting between stars.".to_string(),
                mechanical_effects: MechanicalEffects {
                    class_hint: Some("Drifter".to_string()),
                    race_hint: Some("Outer Rim".to_string()),
                    ..Default::default()
                },
            }],
            allows_freeform: Some(false),
            loading_text: None,
            hook_prompt: None,
            mechanical_effects: None,
        },
        // Final scene: freeform name entry. After apply_freeform("Naomi"),
        // builder advances to BuilderPhase::Confirmation — BUT only AFTER
        // to_scene_message() renders this scene's narration first. That is
        // the render we need to exercise.
        CharCreationScene {
            id: "confirmation".to_string(),
            title: "Welcome Aboard".to_string(),
            narration: "Welcome aboard, {name}. The {class} from {race} space.".to_string(),
            choices: vec![],
            allows_freeform: Some(true),
            loading_text: None,
            hook_prompt: None,
            mechanical_effects: None,
        },
    ]
}

fn extract_prompt(msg: &GameMessage) -> String {
    match msg {
        GameMessage::CharacterCreation { payload, .. } => payload
            .prompt
            .clone()
            .expect("scene message must carry a prompt"),
        other => panic!("expected CharacterCreation, got {:?}", other),
    }
}

#[test]
fn confirmation_scene_interpolates_accumulated_state() {
    let scenes = scenes_with_placeholders();
    let rules = rules();
    let mut builder = CharacterBuilder::new(scenes, &rules, None);

    // Scene 0: pick Drifter (populates class_hint + race_hint).
    builder.apply_choice(0).expect("class choice applies");

    // Seed the character_name directly for the interpolation test — the name
    // resolver reads from the last freeform result, which we inject via
    // apply_freeform on the confirmation scene. But the scene narration
    // renders BEFORE the name is captured into results, so we need a separate
    // assertion path. Here we assert class + race interpolate cleanly on the
    // confirmation-scene render; the empty-name case is exercised below.
    let msg_before_name = builder.to_scene_message("player-1");
    let prompt = extract_prompt(&msg_before_name);

    // Class and race must resolve from the class-choice scene results.
    assert!(
        prompt.contains("Drifter"),
        "class placeholder should interpolate: got {prompt:?}"
    );
    assert!(
        prompt.contains("Outer Rim"),
        "race placeholder should interpolate: got {prompt:?}"
    );

    // No literal placeholders may remain in the rendered prompt.
    assert!(
        !prompt.contains("{class}"),
        "rendered prompt must not contain literal {{class}}: {prompt:?}"
    );
    assert!(
        !prompt.contains("{race}"),
        "rendered prompt must not contain literal {{race}}: {prompt:?}"
    );
}

#[test]
fn missing_name_substitutes_empty_string_rather_than_leaking_literal() {
    // The confirmation-scene narration renders before the player types their
    // name. `{name}` therefore resolves to an empty string. The important
    // invariant: a literal "{name}" must NOT leak to the client.
    let scenes = scenes_with_placeholders();
    let rules = rules();
    let mut builder = CharacterBuilder::new(scenes, &rules, None);
    builder.apply_choice(0).expect("class choice applies");

    let msg = builder.to_scene_message("player-1");
    let prompt = extract_prompt(&msg);

    assert!(
        !prompt.contains("{name}"),
        "literal {{name}} must never reach the client: {prompt:?}"
    );
    // Welcome should read "Welcome aboard, ." — awkward punctuation is
    // acceptable for unresolved placeholders; leaking "{name}" is not.
    assert!(
        prompt.contains("Welcome aboard,"),
        "surrounding prose must still render: {prompt:?}"
    );
}

#[test]
fn scene_without_placeholders_is_returned_verbatim() {
    // A scene whose narration contains no curly braces must pass through
    // byte-for-byte — no accidental rewrites, no stray substitutions.
    let scenes = vec![CharCreationScene {
        id: "plain".to_string(),
        title: "Plain".to_string(),
        narration: "The wind shifts. The earth hums. The water remembers.".to_string(),
        choices: vec![CharCreationChoice {
            label: "Continue".to_string(),
            description: "Continue.".to_string(),
            mechanical_effects: MechanicalEffects::default(),
        }],
        allows_freeform: Some(false),
        loading_text: None,
        hook_prompt: None,
        mechanical_effects: None,
    }];
    let rules = rules();
    let builder = CharacterBuilder::new(scenes, &rules, None);

    let msg = builder.to_scene_message("player-1");
    let prompt = extract_prompt(&msg);
    assert_eq!(prompt, "The wind shifts. The earth hums. The water remembers.");
}

/// Wiring check: verify the production dispatch path (to_scene_message)
/// actually invokes the interpolation, not merely that a helper exists on
/// the builder. Satisfies the project rule "Every Test Suite Needs a Wiring
/// Test" — a unit test on a private helper would not catch a regression
/// where the payload constructor forgets to call it.
#[test]
fn wiring_scene_message_payload_routes_through_interpolator() {
    // Two scenes so to_scene_message stays in InProgress after the first
    // choice — the second scene (with the placeholder narration) is the one
    // we assert against.
    let scenes = vec![
        CharCreationScene {
            id: "class_pick".to_string(),
            title: "Pick".to_string(),
            narration: "Choose.".to_string(),
            choices: vec![CharCreationChoice {
                label: "Spacer".to_string(),
                description: "A spacer.".to_string(),
                mechanical_effects: MechanicalEffects {
                    class_hint: Some("Spacer".to_string()),
                    race_hint: Some("Belt".to_string()),
                    ..Default::default()
                },
            }],
            allows_freeform: Some(false),
            loading_text: None,
            hook_prompt: None,
            mechanical_effects: None,
        },
        CharCreationScene {
            id: "token_check".to_string(),
            title: "Token Check".to_string(),
            narration: "classname={class}|racename={race}|playername={name}".to_string(),
            choices: vec![CharCreationChoice {
                label: "Continue".to_string(),
                description: "Continue.".to_string(),
                mechanical_effects: MechanicalEffects::default(),
            }],
            allows_freeform: Some(false),
            loading_text: None,
            hook_prompt: None,
            mechanical_effects: None,
        },
    ];
    let rules = rules();
    let mut builder = CharacterBuilder::new(scenes, &rules, None);
    builder.apply_choice(0).expect("choice applies");

    let msg = builder.to_scene_message("player-1");
    let prompt = extract_prompt(&msg);

    // Every token must be interpolated where its data exists. The payload
    // constructor is what gets this right — not an isolated helper.
    assert!(prompt.contains("classname=Spacer"), "got {prompt:?}");
    assert!(prompt.contains("racename=Belt"), "got {prompt:?}");
    // Name is unset → empty substitution, but no literal leak.
    assert!(prompt.contains("playername="), "got {prompt:?}");
    assert!(!prompt.contains("{"), "no literal braces: {prompt:?}");
}

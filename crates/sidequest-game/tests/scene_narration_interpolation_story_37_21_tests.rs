//! Scene narration placeholder interpolation tests.
//!
//! Covers the `CharacterBuilder` interpolator that substitutes
//! `{name}`, `{class}`, and `{race}` placeholders in `scene.narration`
//! before the text is cloned into the `CharacterCreation` payload.
//!
//! Invariants asserted:
//! - Known placeholders are substituted from accumulated builder state.
//! - A literal `{name}` / `{class}` / `{race}` never leaks to the client,
//!   even when the backing value is unset (substitute empty string).
//! - Text containing no curly braces is returned byte-for-byte.
//! - Unrecognized brace tokens (`{origin}`, typos) pass through to the
//!   output AND fire a Warn OTEL event so a GM can catch author typos.
//! - The payload constructor (`to_scene_message`) routes through the
//!   interpolator, not just the private helper in isolation.
//! - The OTEL `chargen.StateTransition` event fires on every interpolation
//!   and carries the correct resolved / missing fields.

use std::collections::HashMap;

use sidequest_game::builder::CharacterBuilder;
use sidequest_genre::{CharCreationChoice, CharCreationScene, MechanicalEffects, RulesConfig};
use sidequest_protocol::GameMessage;
use sidequest_telemetry::{
    init_global_channel, subscribe_global, Severity, WatcherEvent, WatcherEventType,
};
use tokio::sync::broadcast::error::TryRecvError;

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

/// Two-scene flow: class choice, then a placeholder-bearing scene so the
/// second scene's narration renders while the builder is still in InProgress.
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

    // Name is not set at this point (no freeform name entry has run). The
    // class-choice scene populates class_hint and race_hint; those must
    // substitute, while `{name}` substitutes as empty. This test covers
    // class + race resolution; the empty-name case is exercised separately.
    builder.apply_choice(0).expect("class choice applies");

    let msg = builder.to_scene_message("player-1");
    let prompt = extract_prompt(&msg);

    assert!(
        prompt.contains("Drifter"),
        "class placeholder should interpolate: got {prompt:?}"
    );
    assert!(
        prompt.contains("Outer Rim"),
        "race placeholder should interpolate: got {prompt:?}"
    );

    // No literal known placeholders may remain.
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
    // The placeholder-bearing scene renders before any freeform name entry.
    // `{name}` resolves to empty string. The invariant: a literal "{name}"
    // must NOT leak to the client.
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
    assert!(
        prompt.contains("Welcome aboard,"),
        "surrounding prose must still render: {prompt:?}"
    );
}

#[test]
fn scene_without_placeholders_is_returned_verbatim() {
    // Byte-for-byte passthrough when no curly braces are present.
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
    assert_eq!(
        prompt,
        "The wind shifts. The earth hums. The water remembers."
    );
}

/// Wiring check: the production dispatch path (`to_scene_message`) must
/// actually invoke the interpolator. A substring check on a rendered token
/// name would pass against an uninterpolated input (`"playername={name}"`
/// also contains `"playername="`), so the assertion here is the full
/// expected post-interpolation output.
#[test]
fn wiring_scene_message_payload_routes_through_interpolator() {
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

    // Full-string equality — a no-op interpolator would leave `{class}` etc.
    // in the output and fail this exact assertion, unlike a substring check
    // that would pass on uninterpolated input.
    assert_eq!(prompt, "classname=Spacer|racename=Belt|playername=");
}

#[test]
fn placeholders_appearing_more_than_once_all_substitute() {
    // str::replace is defined to replace all occurrences; this test locks
    // that behavior so a future refactor to a hand-rolled scanner cannot
    // silently drop duplicates.
    let scenes = vec![
        CharCreationScene {
            id: "class_pick".to_string(),
            title: "Pick".to_string(),
            narration: "Choose.".to_string(),
            choices: vec![CharCreationChoice {
                label: "Drifter".to_string(),
                description: "A drifter.".to_string(),
                mechanical_effects: MechanicalEffects {
                    class_hint: Some("Drifter".to_string()),
                    ..Default::default()
                },
            }],
            allows_freeform: Some(false),
            loading_text: None,
            hook_prompt: None,
            mechanical_effects: None,
        },
        CharCreationScene {
            id: "dup".to_string(),
            title: "Dup".to_string(),
            narration: "{class} is a {class}.".to_string(),
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
    assert_eq!(prompt, "Drifter is a Drifter.");
}

/// Unrecognized brace tokens (author typos, unsupported keys) must pass
/// through verbatim AND fire a separate Warn OTEL event so a GM can spot
/// them instead of watching them render silently to the player.
#[tokio::test]
async fn unrecognized_token_passes_through_and_fires_warn_event() {
    init_global_channel();
    let mut rx = subscribe_global().expect("channel should be initialized");

    let scenes = vec![
        CharCreationScene {
            id: "class_pick".to_string(),
            title: "Pick".to_string(),
            narration: "Choose.".to_string(),
            choices: vec![CharCreationChoice {
                label: "Drifter".to_string(),
                description: "A drifter.".to_string(),
                mechanical_effects: MechanicalEffects {
                    class_hint: Some("Drifter".to_string()),
                    ..Default::default()
                },
            }],
            allows_freeform: Some(false),
            loading_text: None,
            hook_prompt: None,
            mechanical_effects: None,
        },
        CharCreationScene {
            id: "typo".to_string(),
            title: "Typo".to_string(),
            narration: "Hello {nmae} from {origin}.".to_string(),
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

    // Passthrough: unrecognized tokens reach the client literally.
    assert_eq!(prompt, "Hello {nmae} from {origin}.");

    // Per-token emission: BOTH unrecognized tokens must fire a Warn event.
    // A first-only scan would satisfy the old test but lose the second typo;
    // asserting both events is what locks the multi-token contract.
    let events = drain_events(&mut rx);
    let warns: Vec<&WatcherEvent> = events
        .iter()
        .filter(|ev| {
            ev.component == "chargen"
                && ev.fields.get("action").and_then(|v| v.as_str())
                    == Some("scene_narration_unrecognized_placeholder")
        })
        .collect();

    // Every warn must be Warn severity and carry a `token` field.
    for warn in &warns {
        assert!(
            matches!(warn.severity, Severity::Warn),
            "unrecognized-token event must be Warn severity"
        );
    }
    let tokens: Vec<&str> = warns
        .iter()
        .map(|w| {
            w.fields
                .get("token")
                .and_then(|v| v.as_str())
                .expect("token field present")
        })
        .collect();

    assert!(
        tokens.contains(&"{nmae}"),
        "first typo '{{nmae}}' must fire a Warn event, got tokens: {tokens:?}"
    );
    assert!(
        tokens.contains(&"{origin}"),
        "second typo '{{origin}}' must fire a Warn event, got tokens: {tokens:?}"
    );
}

/// OTEL coverage: a successful interpolation must emit a StateTransition
/// with per-key resolved fields so the GM panel can distinguish "placeholder
/// was present and resolved" from "placeholder was present but value was
/// empty" from "placeholder was absent entirely."
#[tokio::test]
async fn interpolation_emits_state_transition_event() {
    init_global_channel();
    let mut rx = subscribe_global().expect("channel should be initialized");

    let scenes = scenes_with_placeholders();
    let rules = rules();
    let mut builder = CharacterBuilder::new(scenes, &rules, None);
    builder.apply_choice(0).expect("class choice applies");

    let _msg = builder.to_scene_message("player-1");

    let events = drain_events(&mut rx);
    let event = events
        .iter()
        .find(|ev| {
            ev.component == "chargen"
                && matches!(ev.event_type, WatcherEventType::StateTransition)
                && ev.fields.get("action").and_then(|v| v.as_str())
                    == Some("scene_narration_interpolated")
        })
        .unwrap_or_else(|| {
            panic!(
                "expected scene_narration_interpolated event, got {} events",
                events.len()
            )
        });

    // Name placeholder WAS present but not resolved (no name entered yet)
    // → Warn severity, name_resolved = Some(false).
    assert!(
        matches!(event.severity, Severity::Warn),
        "event severity must be Warn when a placeholder is unresolved"
    );
    assert_eq!(
        event.fields.get("name_resolved").and_then(|v| v.as_bool()),
        Some(false),
        "name_resolved must be false (placeholder present, value empty): {:?}",
        event.fields
    );
    // Class + race were resolved from the class-choice scene.
    assert_eq!(
        event.fields.get("class_resolved").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(
        event.fields.get("race_resolved").and_then(|v| v.as_bool()),
        Some(true)
    );
}

/// Drain every currently-buffered event from the broadcast receiver.
///
/// `broadcast::Sender::send` is synchronous and returns after the event is
/// in the ring buffer, so by the time the test's call to the code-under-test
/// returns, every event it emitted is already visible to `try_recv`. No
/// sleep is needed — polling stops at the first `Empty`.
fn drain_events(rx: &mut tokio::sync::broadcast::Receiver<WatcherEvent>) -> Vec<WatcherEvent> {
    let mut out = Vec::new();
    loop {
        match rx.try_recv() {
            Ok(ev) => out.push(ev),
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Closed) => break,
            Err(TryRecvError::Lagged(_)) => continue,
        }
    }
    out
}

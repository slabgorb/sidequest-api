//! Story 34-11: OTEL dice spans — WatcherEvent emissions at dice dispatch points.
//!
//! RED phase tests. These verify that the three dice dispatch decision points
//! emit structured WatcherEvents visible on the GM panel's "dice" channel.
//!
//! ACs tested:
//! 1. dice.request_sent — emitted when DiceRequest is broadcast (via SharedGameSession)
//! 2. dice.throw_received — emitted when DiceThrow arrives from rolling player
//! 3. dice.result_broadcast — emitted when DiceResult is resolved and broadcast
//! 4. All events use "dice" channel
//! 5. Correct WatcherEventType per event
//! 6. Required fields present on each event
//!
//! Pattern: matches otel_subsystems_story_35_10_tests.rs — init global channel,
//! drain stale events, exercise code, assert emitted events.

use std::num::NonZeroU8;

use sidequest_game::dice::resolve_dice;
use sidequest_protocol::{DiceRequestPayload, DieSides, DieSpec, ThrowParams};
use sidequest_telemetry::{WatcherEvent, WatcherEventType};

use crate::dice_dispatch::{compose_dice_result, generate_dice_seed, validate_dice_inputs};
use crate::test_support::telemetry::{drain_events, fresh_subscriber};

/// Find events by component and event field value.
fn find_dice_events(events: &[WatcherEvent], event_name: &str) -> Vec<WatcherEvent> {
    events
        .iter()
        .filter(|e| {
            e.component == "dice"
                && e.fields.get("event").and_then(serde_json::Value::as_str) == Some(event_name)
        })
        .cloned()
        .collect()
}

/// Build a standard test DiceRequest.
fn test_dice_request() -> DiceRequestPayload {
    DiceRequestPayload {
        request_id: "test-req-001".to_string(),
        rolling_player_id: "player-1".to_string(),
        character_name: "Kael Ashblade".to_string(),
        stat: sidequest_protocol::Stat::new("strength").unwrap(),
        modifier: 3,
        difficulty: std::num::NonZeroU32::new(15).unwrap(),
        dice: vec![DieSpec {
            sides: DieSides::D20,
            count: NonZeroU8::new(1).unwrap(),
        }],
        context: "Strength check to break down the door".to_string(),
    }
}

// ============================================================
// AC-1: dice.request_sent WatcherEvent
// ============================================================

#[test]
fn dice_request_sent_emits_watcher_event() {
    let (_guard, mut rx) = fresh_subscriber();
    let request = test_dice_request();

    // Exercise: the code path that broadcasts DiceRequest should emit a WatcherEvent.
    // This is called from the dispatch pipeline when a beat triggers a stat check.
    // For now we call the emit site directly — it lives alongside the DiceRequest broadcast.
    crate::emit_dice_request_sent(&request);

    let events = drain_events(&mut rx);
    let dice_events = find_dice_events(&events, "dice.request_sent");

    assert_eq!(
        dice_events.len(),
        1,
        "Exactly one dice.request_sent event expected, found {}",
        dice_events.len()
    );

    let event = &dice_events[0];
    assert_eq!(event.component, "dice");

    // Required fields
    assert_eq!(
        event.fields.get("request_id").and_then(|v| v.as_str()),
        Some("test-req-001"),
        "request_id field must be present"
    );
    assert_eq!(
        event.fields.get("rolling_player").and_then(|v| v.as_str()),
        Some("player-1"),
        "rolling_player field must be present"
    );
    assert_eq!(
        event.fields.get("stat").and_then(|v| v.as_str()),
        Some("strength"),
        "stat field must be present"
    );
    assert!(
        event.fields.contains_key("difficulty"),
        "difficulty field must be present"
    );
    assert!(
        event.fields.contains_key("dice_count"),
        "dice_count field must be present"
    );
}

#[test]
fn dice_request_sent_uses_correct_event_type() {
    let (_guard, mut rx) = fresh_subscriber();
    let request = test_dice_request();

    crate::emit_dice_request_sent(&request);

    let events = drain_events(&mut rx);
    let dice_events = find_dice_events(&events, "dice.request_sent");

    assert_eq!(dice_events.len(), 1);
    assert!(
        matches!(
            dice_events[0].event_type,
            WatcherEventType::SubsystemExerciseSummary
        ),
        "dice.request_sent must use SubsystemExerciseSummary type"
    );
}

// ============================================================
// AC-2: dice.throw_received WatcherEvent
// ============================================================

#[test]
fn dice_throw_received_emits_watcher_event() {
    let (_guard, mut rx) = fresh_subscriber();

    let throw_params = ThrowParams {
        velocity: [1.0, 2.0, 3.0],
        angular: [0.5, 0.5, 0.5],
        position: [0.0, 1.0],
    };

    crate::emit_dice_throw_received("test-req-001", "player-1", &throw_params);

    let events = drain_events(&mut rx);
    let dice_events = find_dice_events(&events, "dice.throw_received");

    assert_eq!(
        dice_events.len(),
        1,
        "Exactly one dice.throw_received event expected"
    );

    let event = &dice_events[0];
    assert_eq!(event.component, "dice");
    assert_eq!(
        event.fields.get("request_id").and_then(|v| v.as_str()),
        Some("test-req-001"),
    );
    assert_eq!(
        event.fields.get("rolling_player").and_then(|v| v.as_str()),
        Some("player-1"),
    );
    assert!(
        event.fields.contains_key("has_throw_params"),
        "has_throw_params field must be present"
    );
}

#[test]
fn dice_throw_received_uses_correct_event_type() {
    let (_guard, mut rx) = fresh_subscriber();
    let throw_params = ThrowParams {
        velocity: [1.0, 2.0, 3.0],
        angular: [0.5, 0.5, 0.5],
        position: [0.0, 1.0],
    };

    crate::emit_dice_throw_received("test-req-001", "player-1", &throw_params);

    let events = drain_events(&mut rx);
    let dice_events = find_dice_events(&events, "dice.throw_received");

    assert_eq!(dice_events.len(), 1);
    assert!(
        matches!(
            dice_events[0].event_type,
            WatcherEventType::SubsystemExerciseSummary
        ),
        "dice.throw_received must use SubsystemExerciseSummary type"
    );
}

// ============================================================
// AC-3: dice.result_broadcast WatcherEvent
// ============================================================

#[test]
fn dice_result_broadcast_emits_watcher_event() {
    let (_guard, mut rx) = fresh_subscriber();

    let request = test_dice_request();
    let seed = generate_dice_seed("test-session", 1);
    let _ = validate_dice_inputs(&request.dice, request.modifier, request.difficulty);
    let resolved = resolve_dice(&request.dice, request.modifier, request.difficulty, seed)
        .expect("resolve_dice should succeed");

    let result_payload = compose_dice_result(
        &request.request_id,
        &request.rolling_player_id,
        &request.character_name,
        &resolved,
        request.modifier,
        request.difficulty,
        seed,
        &ThrowParams {
            velocity: [1.0, 0.0, 0.0],
            angular: [0.0, 0.0, 0.0],
            position: [0.0, 0.0],
        },
    );

    crate::emit_dice_result_broadcast(&result_payload, &resolved);

    let events = drain_events(&mut rx);
    let dice_events = find_dice_events(&events, "dice.result_broadcast");

    assert_eq!(
        dice_events.len(),
        1,
        "Exactly one dice.result_broadcast event expected"
    );

    let event = &dice_events[0];
    assert_eq!(event.component, "dice");
    assert_eq!(
        event.fields.get("request_id").and_then(|v| v.as_str()),
        Some("test-req-001"),
    );
    assert_eq!(
        event.fields.get("rolling_player").and_then(|v| v.as_str()),
        Some("player-1"),
    );
    assert!(
        event.fields.contains_key("total"),
        "total field must be present"
    );
    assert!(
        event.fields.contains_key("outcome"),
        "outcome field must be present"
    );
    assert!(
        event.fields.contains_key("seed"),
        "seed field must be present"
    );
}

#[test]
fn dice_result_broadcast_uses_state_transition_type() {
    let (_guard, mut rx) = fresh_subscriber();

    let request = test_dice_request();
    let seed = generate_dice_seed("test-session", 1);
    let resolved = resolve_dice(&request.dice, request.modifier, request.difficulty, seed)
        .expect("resolve_dice should succeed");
    let result_payload = compose_dice_result(
        &request.request_id,
        &request.rolling_player_id,
        &request.character_name,
        &resolved,
        request.modifier,
        request.difficulty,
        seed,
        &ThrowParams {
            velocity: [0.0; 3],
            angular: [0.0; 3],
            position: [0.0; 2],
        },
    );

    crate::emit_dice_result_broadcast(&result_payload, &resolved);

    let events = drain_events(&mut rx);
    let dice_events = find_dice_events(&events, "dice.result_broadcast");

    assert_eq!(dice_events.len(), 1);
    assert!(
        matches!(dice_events[0].event_type, WatcherEventType::StateTransition),
        "dice.result_broadcast must use StateTransition type (outcome changes narrator context)"
    );
}

// ============================================================
// AC-4: All events use "dice" channel
// ============================================================

#[test]
fn all_dice_events_use_dice_channel() {
    let (_guard, mut rx) = fresh_subscriber();

    let request = test_dice_request();
    let seed = generate_dice_seed("test-session", 1);
    let resolved = resolve_dice(&request.dice, request.modifier, request.difficulty, seed)
        .expect("resolve_dice should succeed");
    let result_payload = compose_dice_result(
        &request.request_id,
        &request.rolling_player_id,
        &request.character_name,
        &resolved,
        request.modifier,
        request.difficulty,
        seed,
        &ThrowParams {
            velocity: [0.0; 3],
            angular: [0.0; 3],
            position: [0.0; 2],
        },
    );
    let throw_params = ThrowParams {
        velocity: [1.0, 2.0, 3.0],
        angular: [0.5, 0.5, 0.5],
        position: [0.0, 1.0],
    };

    crate::emit_dice_request_sent(&request);
    crate::emit_dice_throw_received("test-req-001", "player-1", &throw_params);
    crate::emit_dice_result_broadcast(&result_payload, &resolved);

    let events = drain_events(&mut rx);
    let dice_events: Vec<_> = events.iter().filter(|e| e.component == "dice").collect();

    assert_eq!(
        dice_events.len(),
        3,
        "All three dice events must use 'dice' channel, found {} events",
        dice_events.len()
    );
}

// ============================================================
// AC-6: Outcome field contains variant name, not Debug repr
// ============================================================

#[test]
fn dice_result_outcome_field_is_variant_name() {
    let (_guard, mut rx) = fresh_subscriber();

    let request = test_dice_request();
    let seed = generate_dice_seed("test-session", 1);
    let resolved = resolve_dice(&request.dice, request.modifier, request.difficulty, seed)
        .expect("resolve_dice should succeed");
    let result_payload = compose_dice_result(
        &request.request_id,
        &request.rolling_player_id,
        &request.character_name,
        &resolved,
        request.modifier,
        request.difficulty,
        seed,
        &ThrowParams {
            velocity: [0.0; 3],
            angular: [0.0; 3],
            position: [0.0; 2],
        },
    );

    crate::emit_dice_result_broadcast(&result_payload, &resolved);

    let events = drain_events(&mut rx);
    let dice_events = find_dice_events(&events, "dice.result_broadcast");
    assert_eq!(dice_events.len(), 1);

    let outcome_val = dice_events[0]
        .fields
        .get("outcome")
        .and_then(|v| v.as_str())
        .expect("outcome must be a string field");

    // Must be one of the known variant names, not Debug format like "CritSuccess" vs "RollOutcome::CritSuccess"
    let valid_outcomes = ["CritSuccess", "Success", "Fail", "CritFail", "Unknown"];
    assert!(
        valid_outcomes.contains(&outcome_val),
        "outcome field must be a clean variant name, got: '{}'",
        outcome_val
    );
}

// ============================================================
// Wiring test: emit functions exist and are pub
// ============================================================

#[test]
fn emit_functions_are_accessible() {
    // Compile-time wiring test: these functions must exist as pub in the server crate.
    // If any is missing, this test fails to compile.
    let _f1: fn(&DiceRequestPayload) = crate::emit_dice_request_sent;
    let _f2: fn(&str, &str, &ThrowParams) = crate::emit_dice_throw_received;
    let _f3: fn(&sidequest_protocol::DiceResultPayload, &sidequest_game::dice::ResolvedRoll) =
        crate::emit_dice_result_broadcast;
}

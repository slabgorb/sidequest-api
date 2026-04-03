//! Story 15-17: Wire chase cinematography — chase_depth context never injected into narrator prompt.
//!
//! Tests that:
//! 1. ChaseState::format_context() produces meaningful narrator context with all sections
//! 2. format_context() output contains chase phase, rig status, and cinematography
//! 3. Vehicle chases include terrain danger and camera guidance
//! 4. The dispatch/prompt.rs build_prompt_context() calls format_context() (wiring test)
//! 5. OTEL event chase.context_injected can be constructed with required fields

use std::collections::HashMap;

// ============================================================================
// AC-1: ChaseState::format_context() produces meaningful narrator context
// ============================================================================

#[test]
fn format_context_produces_chase_sequence_header() {
    // format_context() must produce a [CHASE SEQUENCE] section that the narrator
    // can use to write chase-aware prose.
    let chase = sidequest_game::ChaseState::new_vehicle_chase(
        sidequest_game::chase::ChaseType::Footrace,
        0.5,
        sidequest_game::chase_depth::RigType::Frankenstein,
        10,
    );
    let context = chase.format_context(vec![]);

    assert!(
        context.contains("[CHASE SEQUENCE]"),
        "format_context() must produce a [CHASE SEQUENCE] header for narrator. Got: {}",
        &context[..context.len().min(200)]
    );
}

#[test]
fn format_context_includes_phase_and_beat() {
    let chase = sidequest_game::ChaseState::new_vehicle_chase(
        sidequest_game::chase::ChaseType::Footrace,
        0.5,
        sidequest_game::chase_depth::RigType::Frankenstein,
        10,
    );
    let context = chase.format_context(vec![]);

    // Must include the phase name (Setup for a new chase) and beat number
    assert!(
        context.contains("Phase:"),
        "format_context() must include Phase label. Got: {}",
        context
    );
    assert!(
        context.contains("Beat:"),
        "format_context() must include Beat number. Got: {}",
        context
    );
}

#[test]
fn format_context_includes_rig_status() {
    // C1: Rig stats must be visible to the narrator
    let chase = sidequest_game::ChaseState::new_vehicle_chase(
        sidequest_game::chase::ChaseType::Footrace,
        0.5,
        sidequest_game::chase_depth::RigType::Frankenstein,
        10,
    );
    let context = chase.format_context(vec![]);

    assert!(
        context.contains("Rig:"),
        "format_context() must include Rig status line. Got: {}",
        context
    );
    assert!(
        context.contains("HP:"),
        "format_context() must include rig HP. Got: {}",
        context
    );
    assert!(
        context.contains("Speed:"),
        "format_context() must include rig Speed. Got: {}",
        context
    );
}

#[test]
fn format_context_includes_terrain_danger() {
    // C4: Terrain danger and effective stats must appear
    let chase = sidequest_game::ChaseState::new_vehicle_chase(
        sidequest_game::chase::ChaseType::Footrace,
        0.5,
        sidequest_game::chase_depth::RigType::Frankenstein,
        10,
    );
    let context = chase.format_context(vec![]);

    assert!(
        context.contains("Terrain danger:"),
        "format_context() must include terrain danger. Got: {}",
        context
    );
    assert!(
        context.contains("Effective speed:"),
        "format_context() must include effective speed after terrain. Got: {}",
        context
    );
}

#[test]
fn format_context_includes_cinematography_directives() {
    // C5: Prose style directives must be present so narrator writes phase-appropriate prose
    let chase = sidequest_game::ChaseState::new_vehicle_chase(
        sidequest_game::chase::ChaseType::Footrace,
        0.5,
        sidequest_game::chase_depth::RigType::Frankenstein,
        10,
    );
    let context = chase.format_context(vec![]);

    // Cinematography section includes camera, pacing, sentence guidance
    assert!(
        context.contains("Camera:") || context.contains("camera"),
        "format_context() must include camera guidance. Got: {}",
        context
    );
    assert!(
        context.contains("sentences") || context.contains("Sentences"),
        "format_context() must include sentence range guidance. Got: {}",
        context
    );
}

#[test]
fn format_context_with_actors_includes_crew() {
    // C2: When actors are assigned, crew roles appear in context
    let mut chase = sidequest_game::ChaseState::new_vehicle_chase(
        sidequest_game::chase::ChaseType::Footrace,
        0.5,
        sidequest_game::chase_depth::RigType::Frankenstein,
        10,
    );
    chase.set_actors(vec![sidequest_game::chase_depth::ChaseActor {
        name: "Rex".to_string(),
        role: sidequest_game::chase_depth::ChaseRole::Driver,
    }]);

    let context = chase.format_context(vec![]);

    assert!(
        context.contains("Rex") && context.contains("Crew:"),
        "format_context() must list crew when actors are present. Got: {}",
        context
    );
}

#[test]
fn format_context_with_decisions_includes_choices() {
    // C3: When beat decisions are provided, they appear as numbered choices
    let chase = sidequest_game::ChaseState::new_vehicle_chase(
        sidequest_game::chase::ChaseType::Footrace,
        0.5,
        sidequest_game::chase_depth::RigType::Frankenstein,
        10,
    );
    let decisions = vec![sidequest_game::chase_depth::BeatDecision {
        description: "Gun the engine through the debris field".to_string(),
        separation_delta: 3,
        risk: "moderate".to_string(),
    }];

    let context = chase.format_context(decisions);

    assert!(
        context.contains("Gun the engine"),
        "format_context() must include decision descriptions. Got: {}",
        context
    );
    assert!(
        context.contains("Decisions:"),
        "format_context() must have a Decisions header. Got: {}",
        context
    );
}

#[test]
fn format_context_is_non_empty_for_basic_chase() {
    // Even a basic chase (no vehicle) must produce some context
    let chase = sidequest_game::ChaseState::new(
        sidequest_game::chase::ChaseType::Stealth,
        0.6,
    );
    let context = chase.format_context(vec![]);

    assert!(
        !context.is_empty(),
        "format_context() must produce non-empty output even for non-vehicle chases"
    );
    assert!(
        context.contains("[CHASE SEQUENCE]"),
        "Non-vehicle chase must still have [CHASE SEQUENCE] header"
    );
}

// ============================================================================
// AC-2: OTEL event chase.context_injected — fields constructible
// ============================================================================

#[test]
fn chase_context_otel_event_has_required_fields() {
    // The OTEL event must carry phase, danger_level, camera, and sentence_range.
    // We test that WatcherEventBuilder can construct this event shape.
    let event = sidequest_server::WatcherEvent {
        timestamp: chrono::Utc::now(),
        component: "chase".to_string(),
        event_type: sidequest_server::WatcherEventType::StateTransition,
        severity: sidequest_server::Severity::Info,
        fields: {
            let mut f = HashMap::new();
            f.insert("event".to_string(), serde_json::json!("chase.context_injected"));
            f.insert("phase".to_string(), serde_json::json!("Setup"));
            f.insert("danger_level".to_string(), serde_json::json!(0.3));
            f.insert("camera".to_string(), serde_json::json!("Wide"));
            f.insert("sentence_range".to_string(), serde_json::json!("3–5"));
            f
        },
    };

    // Verify all required fields are present
    assert_eq!(event.component, "chase");
    assert!(event.fields.contains_key("event"));
    assert_eq!(event.fields["event"], serde_json::json!("chase.context_injected"));
    assert!(event.fields.contains_key("phase"), "OTEL event must include chase phase");
    assert!(event.fields.contains_key("danger_level"), "OTEL event must include danger_level");
    assert!(event.fields.contains_key("camera"), "OTEL event must include camera mode");
    assert!(event.fields.contains_key("sentence_range"), "OTEL event must include sentence_range");
}

#[test]
fn chase_context_otel_event_roundtrips_through_json() {
    // GM panel receives events as JSON — verify roundtrip
    let event = sidequest_server::WatcherEvent {
        timestamp: chrono::Utc::now(),
        component: "chase".to_string(),
        event_type: sidequest_server::WatcherEventType::StateTransition,
        severity: sidequest_server::Severity::Info,
        fields: {
            let mut f = HashMap::new();
            f.insert("event".to_string(), serde_json::json!("chase.context_injected"));
            f.insert("phase".to_string(), serde_json::json!("Rising"));
            f.insert("danger_level".to_string(), serde_json::json!(0.7));
            f.insert("camera".to_string(), serde_json::json!("Close"));
            f.insert("sentence_range".to_string(), serde_json::json!("5–8"));
            f
        },
    };

    let json = serde_json::to_string(&event).unwrap();
    let deserialized: sidequest_server::WatcherEvent = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.component, "chase");
    assert_eq!(deserialized.fields["phase"], serde_json::json!("Rising"));
    assert_eq!(deserialized.fields["danger_level"], serde_json::json!(0.7));
}

// ============================================================================
// AC-3: WIRING TEST — format_context() is called from dispatch/prompt.rs
// ============================================================================

#[test]
fn prompt_rs_calls_format_context_on_chase_state() {
    // Wiring test: verify that dispatch/prompt.rs contains a call to
    // format_context() when chase_state is present. This catches the
    // "implemented but never wired" pattern that is this project's
    // most expensive defect category.
    //
    // We scan the source file for the call pattern. If this test fails,
    // it means format_context() is not being called from the prompt builder,
    // which means chase cinematography context is NOT being injected into
    // the narrator prompt — the narrator will improvise chase scenes with
    // zero mechanical backing.
    let prompt_source = include_str!("../src/dispatch/prompt.rs");

    assert!(
        prompt_source.contains("format_context"),
        "dispatch/prompt.rs MUST call format_context() to inject chase \
         cinematography into the narrator prompt. Without this call, the \
         narrator has no chase depth context (phase, camera, rig status, \
         terrain) and will improvise chase scenes with zero mechanical backing."
    );
}

#[test]
fn prompt_rs_chase_context_goes_into_state_summary() {
    // The chase context must be pushed into state_summary, not silently discarded.
    // Pattern: state_summary.push_str(&chase_context) or similar
    let prompt_source = include_str!("../src/dispatch/prompt.rs");

    // Look for the pattern where chase context is appended to state_summary
    assert!(
        prompt_source.contains("state_summary.push_str") && prompt_source.contains("chase"),
        "dispatch/prompt.rs must push chase context into state_summary. \
         It's not enough to call format_context() — the result must be \
         appended to the state_summary string that becomes the narrator prompt."
    );
}

#[test]
fn prompt_rs_emits_chase_otel_event() {
    // OTEL event must be emitted when chase context is injected.
    // Without this, the GM panel has no visibility into whether
    // chase cinematography is actually engaged.
    let prompt_source = include_str!("../src/dispatch/prompt.rs");

    assert!(
        prompt_source.contains("chase.context_injected") || prompt_source.contains("chase_context_injected"),
        "dispatch/prompt.rs must emit a chase.context_injected OTEL event \
         so the GM panel can verify chase cinematography is engaged, not \
         just Claude improvising chase scenes."
    );
}

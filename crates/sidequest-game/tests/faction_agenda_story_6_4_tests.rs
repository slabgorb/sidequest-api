//! Story 6-4 RED tests — FactionAgenda model: faction goals, urgency, and
//! scene injection rules.
//!
//! These tests reference APIs that do NOT yet exist:
//! - FactionAgenda struct with validated construction
//! - AgendaUrgency enum with DirectivePriority mapping
//! - FactionAgendaError for validation failures
//! - Serde Deserialize with try_from validation
//! - Scene injection text generation
//!
//! Dev must implement the faction_agenda module to make these pass.

use sidequest_game::faction_agenda::{AgendaUrgency, FactionAgenda, FactionAgendaError};
use sidequest_game::scene_directive::DirectivePriority;

// ===========================================================================
// Section 1: AgendaUrgency enum
// ===========================================================================

#[test]
fn urgency_has_four_levels() {
    // Dormant < Simmering < Pressing < Critical
    let levels = [
        AgendaUrgency::Dormant,
        AgendaUrgency::Simmering,
        AgendaUrgency::Pressing,
        AgendaUrgency::Critical,
    ];
    // Each level is distinct
    for i in 0..levels.len() {
        for j in (i + 1)..levels.len() {
            assert_ne!(levels[i], levels[j]);
        }
    }
}

#[test]
fn urgency_is_ordered() {
    assert!(AgendaUrgency::Dormant < AgendaUrgency::Simmering);
    assert!(AgendaUrgency::Simmering < AgendaUrgency::Pressing);
    assert!(AgendaUrgency::Pressing < AgendaUrgency::Critical);
}

#[test]
fn urgency_maps_to_directive_priority() {
    assert_eq!(
        AgendaUrgency::Dormant.to_directive_priority(),
        None,
        "Dormant agendas should not produce directive elements"
    );
    assert_eq!(
        AgendaUrgency::Simmering.to_directive_priority(),
        Some(DirectivePriority::Low)
    );
    assert_eq!(
        AgendaUrgency::Pressing.to_directive_priority(),
        Some(DirectivePriority::Medium)
    );
    assert_eq!(
        AgendaUrgency::Critical.to_directive_priority(),
        Some(DirectivePriority::High)
    );
}

#[test]
fn urgency_is_debug_clone_copy() {
    let u = AgendaUrgency::Pressing;
    let u2 = u; // Copy
    let _debug = format!("{:?}", u2);
    assert_eq!(u, u2);
}

#[test]
fn urgency_default_is_dormant() {
    assert_eq!(AgendaUrgency::default(), AgendaUrgency::Dormant);
}

// ===========================================================================
// Section 2: FactionAgenda validated construction — Rule #5
// ===========================================================================

#[test]
fn try_new_accepts_valid_agenda() {
    let agenda = FactionAgenda::try_new(
        "Iron Brotherhood".to_string(),
        "Control the northern trade route".to_string(),
        AgendaUrgency::Pressing,
        "The Iron Brotherhood tightens its grip on the northern pass.".to_string(),
    );
    assert!(agenda.is_ok());
    let agenda = agenda.unwrap();
    assert_eq!(agenda.faction_name(), "Iron Brotherhood");
    assert_eq!(agenda.goal(), "Control the northern trade route");
    assert_eq!(agenda.urgency(), AgendaUrgency::Pressing);
    assert_eq!(
        agenda.event_text(),
        "The Iron Brotherhood tightens its grip on the northern pass."
    );
}

#[test]
fn try_new_rejects_empty_faction_name() {
    let result = FactionAgenda::try_new(
        String::new(),
        "goal".to_string(),
        AgendaUrgency::Pressing,
        "event".to_string(),
    );
    assert!(result.is_err());
}

#[test]
fn try_new_rejects_whitespace_faction_name() {
    let result = FactionAgenda::try_new(
        "   ".to_string(),
        "goal".to_string(),
        AgendaUrgency::Pressing,
        "event".to_string(),
    );
    assert!(result.is_err());
}

#[test]
fn try_new_rejects_empty_goal() {
    let result = FactionAgenda::try_new(
        "faction".to_string(),
        String::new(),
        AgendaUrgency::Pressing,
        "event".to_string(),
    );
    assert!(result.is_err());
}

#[test]
fn try_new_rejects_empty_event_text() {
    let result = FactionAgenda::try_new(
        "faction".to_string(),
        "goal".to_string(),
        AgendaUrgency::Pressing,
        String::new(),
    );
    assert!(result.is_err());
}

// ===========================================================================
// Section 3: Private fields with getters — Rule #9
// ===========================================================================

#[test]
fn fields_accessed_through_getters() {
    let agenda = FactionAgenda::try_new(
        "Crimson Order".to_string(),
        "Purify the temple".to_string(),
        AgendaUrgency::Critical,
        "Crimson Order zealots march toward the temple gates.".to_string(),
    )
    .unwrap();

    // All accessors return expected types
    let _name: &str = agenda.faction_name();
    let _goal: &str = agenda.goal();
    let _urgency: AgendaUrgency = agenda.urgency();
    let _event: &str = agenda.event_text();

    // Verify actual values
    assert_eq!(agenda.faction_name(), "Crimson Order");
    assert_eq!(agenda.goal(), "Purify the temple");
    assert_eq!(agenda.urgency(), AgendaUrgency::Critical);
}

// ===========================================================================
// Section 4: Scene injection — AC: scene injection rules
// ===========================================================================

#[test]
fn scene_injection_text_includes_event() {
    let agenda = FactionAgenda::try_new(
        "Shadow Guild".to_string(),
        "Infiltrate the court".to_string(),
        AgendaUrgency::Pressing,
        "Rumors of Shadow Guild operatives circulate in the court.".to_string(),
    )
    .unwrap();

    let injection = agenda.scene_injection();
    assert!(
        injection.is_some(),
        "non-dormant agenda should produce scene injection"
    );
    let text = injection.unwrap();
    assert!(
        text.contains("Shadow Guild"),
        "injection should mention faction: got '{}'",
        text
    );
}

#[test]
fn dormant_agenda_produces_no_injection() {
    let agenda = FactionAgenda::try_new(
        "Sleeping Council".to_string(),
        "Wait for the prophecy".to_string(),
        AgendaUrgency::Dormant,
        "The council watches from the shadows.".to_string(),
    )
    .unwrap();

    assert!(
        agenda.scene_injection().is_none(),
        "dormant agenda should not inject into scene"
    );
}

#[test]
fn critical_agenda_produces_injection() {
    let agenda = FactionAgenda::try_new(
        "Dragon Cult".to_string(),
        "Summon the elder wyrm".to_string(),
        AgendaUrgency::Critical,
        "The Dragon Cult begins the summoning ritual at the caldera.".to_string(),
    )
    .unwrap();

    let injection = agenda.scene_injection();
    assert!(injection.is_some());
    assert!(injection.unwrap().contains("Dragon Cult"));
}

// ===========================================================================
// Section 5: Urgency mutation — agenda escalation
// ===========================================================================

#[test]
fn urgency_can_be_escalated() {
    let mut agenda = FactionAgenda::try_new(
        "faction".to_string(),
        "goal".to_string(),
        AgendaUrgency::Simmering,
        "event".to_string(),
    )
    .unwrap();

    agenda.set_urgency(AgendaUrgency::Critical);
    assert_eq!(agenda.urgency(), AgendaUrgency::Critical);
}

#[test]
fn urgency_can_be_deescalated() {
    let mut agenda = FactionAgenda::try_new(
        "faction".to_string(),
        "goal".to_string(),
        AgendaUrgency::Critical,
        "event".to_string(),
    )
    .unwrap();

    agenda.set_urgency(AgendaUrgency::Dormant);
    assert_eq!(agenda.urgency(), AgendaUrgency::Dormant);
}

// ===========================================================================
// Section 6: Serde Deserialize — Rule #8 (try_from validation)
// ===========================================================================

#[test]
fn deserialize_valid_yaml() {
    let yaml = r#"
faction_name: "Iron Brotherhood"
goal: "Control the northern trade route"
urgency: pressing
event_text: "The Iron Brotherhood tightens its grip."
"#;
    let agenda: Result<FactionAgenda, _> = serde_yaml::from_str(yaml);
    assert!(
        agenda.is_ok(),
        "valid YAML should deserialize: {:?}",
        agenda.err()
    );
    let agenda = agenda.unwrap();
    assert_eq!(agenda.faction_name(), "Iron Brotherhood");
    assert_eq!(agenda.urgency(), AgendaUrgency::Pressing);
}

#[test]
fn deserialize_rejects_empty_faction_name() {
    let yaml = r#"
faction_name: ""
goal: "goal"
urgency: pressing
event_text: "event"
"#;
    let result: Result<FactionAgenda, _> = serde_yaml::from_str(yaml);
    assert!(
        result.is_err(),
        "empty faction_name should fail deserialization (Rule #8)"
    );
}

#[test]
fn deserialize_rejects_empty_goal() {
    let yaml = r#"
faction_name: "faction"
goal: ""
urgency: pressing
event_text: "event"
"#;
    let result: Result<FactionAgenda, _> = serde_yaml::from_str(yaml);
    assert!(result.is_err());
}

#[test]
fn deserialize_all_urgency_levels() {
    for (level_str, expected) in [
        ("dormant", AgendaUrgency::Dormant),
        ("simmering", AgendaUrgency::Simmering),
        ("pressing", AgendaUrgency::Pressing),
        ("critical", AgendaUrgency::Critical),
    ] {
        let yaml = format!(
            r#"
faction_name: "test"
goal: "test"
urgency: {}
event_text: "test"
"#,
            level_str
        );
        let agenda: FactionAgenda = serde_yaml::from_str(&yaml)
            .unwrap_or_else(|e| panic!("failed to deserialize urgency '{}': {}", level_str, e));
        assert_eq!(
            agenda.urgency(),
            expected,
            "urgency '{}' mismatch",
            level_str
        );
    }
}

// ===========================================================================
// Section 7: Error type — Rule #2 (non_exhaustive)
// ===========================================================================

#[test]
fn error_is_debug_and_display() {
    let result = FactionAgenda::try_new(
        String::new(),
        "goal".to_string(),
        AgendaUrgency::Pressing,
        "event".to_string(),
    );
    let err = result.unwrap_err();
    let debug = format!("{:?}", err);
    let display = format!("{}", err);
    assert!(!debug.is_empty());
    assert!(!display.is_empty());
}

// ===========================================================================
// Section 8: Edge cases
// ===========================================================================

#[test]
fn multiple_agendas_per_faction_allowed() {
    let a1 = FactionAgenda::try_new(
        "Iron Brotherhood".to_string(),
        "Control trade".to_string(),
        AgendaUrgency::Pressing,
        "Trade event".to_string(),
    )
    .unwrap();

    let a2 = FactionAgenda::try_new(
        "Iron Brotherhood".to_string(),
        "Recruit soldiers".to_string(),
        AgendaUrgency::Simmering,
        "Recruitment event".to_string(),
    )
    .unwrap();

    assert_eq!(a1.faction_name(), a2.faction_name());
    assert_ne!(a1.goal(), a2.goal());
}

#[test]
fn agenda_with_long_text_accepted() {
    let long_goal = "A".repeat(500);
    let long_event = "B".repeat(1000);
    let result = FactionAgenda::try_new(
        "faction".to_string(),
        long_goal,
        AgendaUrgency::Pressing,
        long_event,
    );
    assert!(result.is_ok(), "long text should be accepted");
}

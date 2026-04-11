//! Story 6-5 RED tests — Wire faction agendas into scene directive.
//!
//! Tests verify that FactionAgenda instances produce faction_events
//! in the SceneDirective output from format_scene_directive().

use sidequest_game::faction_agenda::{AgendaUrgency, FactionAgenda};
use sidequest_game::scene_directive::{
    format_scene_directive, ActiveStake, DirectivePriority, DirectiveSource, SceneDirective,
};

// ===========================================================================
// Helpers
// ===========================================================================

fn pressing_agenda(faction: &str, event: &str) -> FactionAgenda {
    FactionAgenda::try_new(
        faction.to_string(),
        "goal".to_string(),
        AgendaUrgency::Pressing,
        event.to_string(),
    )
    .unwrap()
}

fn dormant_agenda(faction: &str) -> FactionAgenda {
    FactionAgenda::try_new(
        faction.to_string(),
        "dormant goal".to_string(),
        AgendaUrgency::Dormant,
        "should not appear".to_string(),
    )
    .unwrap()
}

// ===========================================================================
// Section 1: format_scene_directive accepts faction agendas
// ===========================================================================

#[test]
fn format_directive_accepts_faction_agendas_parameter() {
    let agendas = vec![pressing_agenda(
        "Iron Brotherhood",
        "The Brotherhood marches north.",
    )];

    let directive = format_scene_directive(&[], &[], &[], &agendas);

    assert_eq!(directive.faction_events.len(), 1);
    assert_eq!(
        directive.faction_events[0],
        "The Brotherhood marches north."
    );
}

#[test]
fn format_directive_with_no_agendas_has_empty_faction_events() {
    let directive = format_scene_directive(&[], &[], &[], &[]);
    assert!(directive.faction_events.is_empty());
}

#[test]
fn format_directive_with_empty_agenda_slice() {
    let agendas: Vec<FactionAgenda> = vec![];
    let directive = format_scene_directive(&[], &[], &[], &agendas);
    assert!(directive.faction_events.is_empty());
}

// ===========================================================================
// Section 2: Dormant agendas filtered out
// ===========================================================================

#[test]
fn dormant_agendas_excluded_from_faction_events() {
    let agendas = vec![
        dormant_agenda("Sleeping Council"),
        pressing_agenda("Shadow Guild", "Guild agents infiltrate the market."),
    ];

    let directive = format_scene_directive(&[], &[], &[], &agendas);

    assert_eq!(directive.faction_events.len(), 1);
    assert!(directive.faction_events[0].contains("Guild agents"));
}

#[test]
fn all_dormant_agendas_produce_empty_events() {
    let agendas = vec![dormant_agenda("Council A"), dormant_agenda("Council B")];

    let directive = format_scene_directive(&[], &[], &[], &agendas);
    assert!(directive.faction_events.is_empty());
}

// ===========================================================================
// Section 3: Multiple active agendas
// ===========================================================================

#[test]
fn multiple_active_agendas_all_appear() {
    let agendas = vec![
        pressing_agenda("Iron Brotherhood", "Brotherhood event."),
        FactionAgenda::try_new(
            "Shadow Guild".to_string(),
            "infiltrate".to_string(),
            AgendaUrgency::Critical,
            "Guild event.".to_string(),
        )
        .unwrap(),
        FactionAgenda::try_new(
            "Merchant League".to_string(),
            "corner market".to_string(),
            AgendaUrgency::Simmering,
            "League event.".to_string(),
        )
        .unwrap(),
    ];

    let directive = format_scene_directive(&[], &[], &[], &agendas);

    assert_eq!(directive.faction_events.len(), 3);
    assert!(directive
        .faction_events
        .contains(&"Brotherhood event.".to_string()));
    assert!(directive
        .faction_events
        .contains(&"Guild event.".to_string()));
    assert!(directive
        .faction_events
        .contains(&"League event.".to_string()));
}

#[test]
fn mixed_dormant_and_active_agendas() {
    let agendas = vec![
        dormant_agenda("Dormant A"),
        pressing_agenda("Active B", "B event."),
        dormant_agenda("Dormant C"),
        FactionAgenda::try_new(
            "Active D".to_string(),
            "goal".to_string(),
            AgendaUrgency::Critical,
            "D event.".to_string(),
        )
        .unwrap(),
    ];

    let directive = format_scene_directive(&[], &[], &[], &agendas);

    assert_eq!(directive.faction_events.len(), 2);
    assert!(directive.faction_events.contains(&"B event.".to_string()));
    assert!(directive.faction_events.contains(&"D event.".to_string()));
}

// ===========================================================================
// Section 4: Faction events coexist with other directive elements
// ===========================================================================

#[test]
fn faction_events_coexist_with_stakes_and_hints() {
    let stakes = vec![ActiveStake {
        description: "The bridge is collapsing.".to_string(),
    }];
    let hints = vec!["The wind carries a foul scent.".to_string()];
    let agendas = vec![pressing_agenda(
        "Dragon Cult",
        "Cultists chant in the distance.",
    )];

    let directive = format_scene_directive(&[], &stakes, &hints, &agendas);

    // Stakes produce mandatory elements
    assert!(!directive.mandatory_elements.is_empty());
    // Hints pass through
    assert_eq!(directive.narrative_hints.len(), 1);
    // Faction events present
    assert_eq!(directive.faction_events.len(), 1);
    assert_eq!(
        directive.faction_events[0],
        "Cultists chant in the distance."
    );
}

// ===========================================================================
// Section 5: DirectiveSource::FactionEvent variant exists
// ===========================================================================

#[test]
fn directive_source_has_faction_event_variant() {
    // The FactionEvent variant should exist on DirectiveSource
    let source = DirectiveSource::FactionEvent;
    assert_eq!(source, DirectiveSource::FactionEvent);
}

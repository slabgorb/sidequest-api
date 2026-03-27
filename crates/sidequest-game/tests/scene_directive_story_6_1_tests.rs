//! Story 6-1: Scene directive formatter
//!
//! Tests for `format_scene_directive()` — composing fired beats, narrative
//! hints, and active stakes into a MUST-weave SceneDirective for the narrator.

use sidequest_game::scene_directive::{
    format_scene_directive, ActiveStake, DirectiveElement, DirectivePriority, DirectiveSource,
    SceneDirective,
};
use sidequest_game::trope::FiredBeat;
use sidequest_genre::TropeEscalation;

// =========================================================================
// Helpers
// =========================================================================

fn fired_beat(event: &str, at: f64, stakes: &str) -> FiredBeat {
    FiredBeat {
        trope_id: "trope-1".into(),
        trope_name: "Test Trope".into(),
        beat: TropeEscalation {
            at,
            event: event.into(),
            npcs_involved: vec![],
            stakes: stakes.into(),
        },
    }
}

fn active_stake(description: &str) -> ActiveStake {
    ActiveStake {
        description: description.into(),
    }
}

// =========================================================================
// AC: Pure function — takes refs, returns owned SceneDirective
// =========================================================================

#[test]
fn format_scene_directive_returns_owned_scene_directive() {
    let beats = vec![fired_beat("a rival appears", 0.5, "territory")];
    let stakes: Vec<ActiveStake> = vec![];
    let hints: Vec<String> = vec![];

    let directive: SceneDirective = format_scene_directive(&beats, &stakes, &hints);

    // Verifies the function takes refs and returns an owned value
    assert!(!directive.mandatory_elements.is_empty());
}

// =========================================================================
// AC: Beat conversion — FiredBeat → DirectiveElement with TropeBeat source
// =========================================================================

#[test]
fn fired_beat_becomes_trope_beat_element() {
    let beats = vec![fired_beat("the faction makes a power play", 0.5, "control")];
    let stakes: Vec<ActiveStake> = vec![];
    let hints: Vec<String> = vec![];

    let directive = format_scene_directive(&beats, &stakes, &hints);

    assert_eq!(directive.mandatory_elements.len(), 1);
    let elem = &directive.mandatory_elements[0];
    assert_eq!(elem.source, DirectiveSource::TropeBeat);
    assert!(
        elem.content.contains("the faction makes a power play"),
        "Content should include the beat event text, got: {}",
        elem.content
    );
}

#[test]
fn multiple_beats_each_produce_element() {
    let beats = vec![
        fired_beat("first event", 0.3, "low stakes"),
        fired_beat("second event", 0.7, "high stakes"),
    ];
    let stakes: Vec<ActiveStake> = vec![];
    let hints: Vec<String> = vec![];

    let directive = format_scene_directive(&beats, &stakes, &hints);

    let trope_elements: Vec<&DirectiveElement> = directive
        .mandatory_elements
        .iter()
        .filter(|e| e.source == DirectiveSource::TropeBeat)
        .collect();
    assert_eq!(trope_elements.len(), 2);
}

// =========================================================================
// AC: Stake conversion — ActiveStake → DirectiveElement with ActiveStake source
// =========================================================================

#[test]
fn active_stake_becomes_active_stake_element() {
    let beats: Vec<FiredBeat> = vec![];
    let stakes = vec![active_stake("the village could be destroyed")];
    let hints: Vec<String> = vec![];

    let directive = format_scene_directive(&beats, &stakes, &hints);

    assert_eq!(directive.mandatory_elements.len(), 1);
    let elem = &directive.mandatory_elements[0];
    assert_eq!(elem.source, DirectiveSource::ActiveStake);
    assert_eq!(elem.content, "the village could be destroyed");
}

#[test]
fn multiple_stakes_each_produce_element() {
    let beats: Vec<FiredBeat> = vec![];
    let stakes = vec![
        active_stake("alliance is fragile"),
        active_stake("supply lines threatened"),
    ];
    let hints: Vec<String> = vec![];

    let directive = format_scene_directive(&beats, &stakes, &hints);

    let stake_elements: Vec<&DirectiveElement> = directive
        .mandatory_elements
        .iter()
        .filter(|e| e.source == DirectiveSource::ActiveStake)
        .collect();
    assert_eq!(stake_elements.len(), 2);
}

// =========================================================================
// AC: Priority ordering — elements sorted by priority descending
// =========================================================================

#[test]
fn elements_sorted_by_priority_descending() {
    // Low-urgency beat (at: 0.2) and high-urgency beat (at: 0.9)
    let beats = vec![
        fired_beat("minor event", 0.2, "minor"),
        fired_beat("critical event", 0.9, "critical"),
    ];
    let stakes: Vec<ActiveStake> = vec![];
    let hints: Vec<String> = vec![];

    let directive = format_scene_directive(&beats, &stakes, &hints);

    // Higher urgency (0.9) → higher priority → should appear first
    assert!(directive.mandatory_elements.len() >= 2);
    assert!(
        directive.mandatory_elements[0].priority >= directive.mandatory_elements[1].priority,
        "Elements should be sorted by priority descending"
    );
}

#[test]
fn beats_and_stakes_mixed_sorted_by_priority() {
    let beats = vec![fired_beat("low urgency beat", 0.1, "minor")];
    let stakes = vec![active_stake("critical stake")];
    let hints: Vec<String> = vec![];

    let directive = format_scene_directive(&beats, &stakes, &hints);

    // Should be sorted regardless of source type
    for window in directive.mandatory_elements.windows(2) {
        assert!(
            window[0].priority >= window[1].priority,
            "Elements must be in priority-descending order"
        );
    }
}

// =========================================================================
// AC: Element cap — no more than configurable max mandatory elements (default 3)
// =========================================================================

#[test]
fn caps_mandatory_elements_at_default_three() {
    let beats = vec![
        fired_beat("event 1", 0.9, "high"),
        fired_beat("event 2", 0.7, "medium"),
        fired_beat("event 3", 0.5, "medium"),
        fired_beat("event 4", 0.3, "low"),
    ];
    let stakes = vec![active_stake("extra stake")];
    let hints: Vec<String> = vec![];

    let directive = format_scene_directive(&beats, &stakes, &hints);

    assert!(
        directive.mandatory_elements.len() <= 3,
        "Default cap is 3, got {}",
        directive.mandatory_elements.len()
    );
}

#[test]
fn cap_keeps_highest_priority_elements() {
    let beats = vec![
        fired_beat("high urgency", 0.9, "critical"),
        fired_beat("medium urgency", 0.5, "medium"),
        fired_beat("low urgency", 0.1, "minor"),
        fired_beat("very low urgency", 0.05, "trivial"),
    ];
    let stakes: Vec<ActiveStake> = vec![];
    let hints: Vec<String> = vec![];

    let directive = format_scene_directive(&beats, &stakes, &hints);

    // Should keep the 3 highest-priority elements, dropping the lowest
    assert!(directive.mandatory_elements.len() <= 3);
    // The highest urgency element should be first
    assert!(
        directive.mandatory_elements[0]
            .content
            .contains("high urgency"),
        "Highest priority element should be retained, got: {}",
        directive.mandatory_elements[0].content
    );
}

#[test]
fn exactly_three_elements_not_capped() {
    let beats = vec![
        fired_beat("event 1", 0.9, "stakes"),
        fired_beat("event 2", 0.5, "stakes"),
        fired_beat("event 3", 0.3, "stakes"),
    ];
    let stakes: Vec<ActiveStake> = vec![];
    let hints: Vec<String> = vec![];

    let directive = format_scene_directive(&beats, &stakes, &hints);

    assert_eq!(
        directive.mandatory_elements.len(),
        3,
        "Exactly 3 elements should pass through uncapped"
    );
}

// =========================================================================
// AC: Empty inputs — returns SceneDirective with empty vecs
// =========================================================================

#[test]
fn empty_inputs_returns_empty_directive() {
    let beats: Vec<FiredBeat> = vec![];
    let stakes: Vec<ActiveStake> = vec![];
    let hints: Vec<String> = vec![];

    let directive = format_scene_directive(&beats, &stakes, &hints);

    assert!(directive.mandatory_elements.is_empty());
    assert!(directive.faction_events.is_empty());
    assert!(directive.narrative_hints.is_empty());
}

// =========================================================================
// AC: Narrative hints — passed through as-is
// =========================================================================

#[test]
fn narrative_hints_passed_through_as_is() {
    let beats: Vec<FiredBeat> = vec![];
    let stakes: Vec<ActiveStake> = vec![];
    let hints = vec![
        "The air is thick with tension".to_string(),
        "A storm brews on the horizon".to_string(),
    ];

    let directive = format_scene_directive(&beats, &stakes, &hints);

    assert_eq!(directive.narrative_hints.len(), 2);
    assert_eq!(directive.narrative_hints[0], "The air is thick with tension");
    assert_eq!(
        directive.narrative_hints[1],
        "A storm brews on the horizon"
    );
}

#[test]
fn narrative_hints_independent_of_elements() {
    let beats = vec![fired_beat("some event", 0.5, "some stake")];
    let stakes: Vec<ActiveStake> = vec![];
    let hints = vec!["a hint".to_string()];

    let directive = format_scene_directive(&beats, &stakes, &hints);

    // Hints should not count toward element cap
    assert_eq!(directive.narrative_hints.len(), 1);
    assert_eq!(directive.mandatory_elements.len(), 1);
}

// =========================================================================
// Faction events — empty for story 6-1 (wired in 6-5)
// =========================================================================

#[test]
fn faction_events_empty_in_story_6_1() {
    let beats = vec![fired_beat("event", 0.5, "stakes")];
    let stakes = vec![active_stake("a stake")];
    let hints = vec!["hint".to_string()];

    let directive = format_scene_directive(&beats, &stakes, &hints);

    assert!(
        directive.faction_events.is_empty(),
        "Faction events should be empty — wired in story 6-5"
    );
}

// =========================================================================
// DirectivePriority — from_beat_urgency mapping
// =========================================================================

#[test]
fn high_urgency_beat_gets_high_priority() {
    let beats = vec![fired_beat("critical escalation", 0.9, "critical")];
    let stakes: Vec<ActiveStake> = vec![];
    let hints: Vec<String> = vec![];

    let directive = format_scene_directive(&beats, &stakes, &hints);

    assert_eq!(directive.mandatory_elements[0].priority, DirectivePriority::High);
}

#[test]
fn medium_urgency_beat_gets_medium_priority() {
    let beats = vec![fired_beat("moderate event", 0.5, "moderate")];
    let stakes: Vec<ActiveStake> = vec![];
    let hints: Vec<String> = vec![];

    let directive = format_scene_directive(&beats, &stakes, &hints);

    assert_eq!(directive.mandatory_elements[0].priority, DirectivePriority::Medium);
}

#[test]
fn low_urgency_beat_gets_low_priority() {
    let beats = vec![fired_beat("minor detail", 0.15, "minor")];
    let stakes: Vec<ActiveStake> = vec![];
    let hints: Vec<String> = vec![];

    let directive = format_scene_directive(&beats, &stakes, &hints);

    assert_eq!(directive.mandatory_elements[0].priority, DirectivePriority::Low);
}

#[test]
fn active_stakes_get_medium_priority() {
    let beats: Vec<FiredBeat> = vec![];
    let stakes = vec![active_stake("ongoing threat")];
    let hints: Vec<String> = vec![];

    let directive = format_scene_directive(&beats, &stakes, &hints);

    assert_eq!(
        directive.mandatory_elements[0].priority,
        DirectivePriority::Medium,
        "Active stakes should always have Medium priority per spec"
    );
}

// =========================================================================
// DirectivePriority ordering — PartialOrd / Ord
// =========================================================================

#[test]
fn priority_high_greater_than_medium() {
    assert!(DirectivePriority::High > DirectivePriority::Medium);
}

#[test]
fn priority_medium_greater_than_low() {
    assert!(DirectivePriority::Medium > DirectivePriority::Low);
}

#[test]
fn priority_high_greater_than_low() {
    assert!(DirectivePriority::High > DirectivePriority::Low);
}

// =========================================================================
// DirectiveSource — enum variants exist
// =========================================================================

#[test]
fn directive_source_trope_beat_variant_exists() {
    let source = DirectiveSource::TropeBeat;
    assert_eq!(source, DirectiveSource::TropeBeat);
}

#[test]
fn directive_source_active_stake_variant_exists() {
    let source = DirectiveSource::ActiveStake;
    assert_eq!(source, DirectiveSource::ActiveStake);
}

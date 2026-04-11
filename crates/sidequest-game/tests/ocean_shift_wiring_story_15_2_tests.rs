//! Story 15-2: Wire OCEAN shift proposals into game flow — events trigger
//! personality evolution.
//!
//! Tests for the full pipeline: typed event → proposal → profile mutation
//! on NpcRegistryEntry → summary regeneration.
//!
//! NOTE: Event detection is now done via structured JSON extraction from
//! the narrator (typed PersonalityEvent enum), NOT keyword matching.
//! These tests verify the proposal→application→persistence pipeline.
//!   AC-1: PersonalityEvent enum deserializes from JSON (typed, not keyword)
//!   AC-2: Proposals generated and applied to NPC OCEAN profiles
//!   AC-3: OceanShift log returned (not discarded)
//!   AC-4: Personality changes persist (mutated on NpcRegistryEntry)
//!   AC-5: End-to-end: typed event → proposal → application → summary update

use sidequest_game::{
    apply_ocean_shifts, NpcRegistryEntry, OceanDimension, OceanProfile, PersonalityEvent,
};

// ─── Helpers ───────────────────────────────────────────────

fn make_entry(name: &str, ocean: OceanProfile) -> NpcRegistryEntry {
    let summary = ocean.behavioral_summary();
    NpcRegistryEntry {
        name: name.to_string(),
        pronouns: "they/them".to_string(),
        role: "test NPC".to_string(),
        location: "tavern".to_string(),
        last_seen_turn: 0,
        age: String::new(),
        appearance: String::new(),
        ocean_summary: summary,
        ocean: Some(ocean),
        hp: 0,
        max_hp: 0,
        portrait_url: None,
    }
}

fn make_entry_no_ocean(name: &str) -> NpcRegistryEntry {
    NpcRegistryEntry {
        name: name.to_string(),
        pronouns: "they/them".to_string(),
        role: "test NPC".to_string(),
        location: "tavern".to_string(),
        last_seen_turn: 0,
        age: String::new(),
        appearance: String::new(),
        ocean_summary: String::new(),
        ocean: None,
        hp: 0,
        max_hp: 0,
        portrait_url: None,
    }
}

fn default_profile() -> OceanProfile {
    OceanProfile {
        openness: 5.0,
        conscientiousness: 5.0,
        extraversion: 5.0,
        agreeableness: 5.0,
        neuroticism: 5.0,
    }
}

// ─── AC-1: PersonalityEvent serde round-trip ──────────────

#[test]
fn personality_event_deserializes_from_snake_case() {
    let json = r#""betrayal""#;
    let event: PersonalityEvent = serde_json::from_str(json).unwrap();
    assert_eq!(event, PersonalityEvent::Betrayal);
}

#[test]
fn personality_event_deserializes_near_death() {
    let json = r#""near_death""#;
    let event: PersonalityEvent = serde_json::from_str(json).unwrap();
    assert_eq!(event, PersonalityEvent::NearDeath);
}

#[test]
fn personality_event_deserializes_all_variants() {
    for (json, expected) in [
        (r#""betrayal""#, PersonalityEvent::Betrayal),
        (r#""near_death""#, PersonalityEvent::NearDeath),
        (r#""victory""#, PersonalityEvent::Victory),
        (r#""defeat""#, PersonalityEvent::Defeat),
        (r#""social_bonding""#, PersonalityEvent::SocialBonding),
    ] {
        let event: PersonalityEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event, expected, "failed to deserialize {json}");
    }
}

#[test]
fn personality_event_rejects_unknown_variant() {
    let json = r#""showed_courage""#;
    let result = serde_json::from_str::<PersonalityEvent>(json);
    assert!(result.is_err(), "should reject unknown variant");
}

#[test]
fn personality_event_serializes_to_snake_case() {
    let json = serde_json::to_string(&PersonalityEvent::SocialBonding).unwrap();
    assert_eq!(json, r#""social_bonding""#);
}

// ─── AC-2: Proposals applied to NPC OCEAN profiles ─────────

#[test]
fn apply_shifts_modifies_npc_ocean_profile() {
    let mut registry = vec![make_entry("Griselda", default_profile())];

    let events = vec![("Griselda".to_string(), PersonalityEvent::Betrayal)];
    let (applied, _log) = apply_ocean_shifts(&mut registry, &events, 1);

    assert!(!applied.is_empty(), "should return applied proposals");

    let profile = registry[0]
        .ocean
        .as_ref()
        .expect("should still have OCEAN profile");
    assert!(
        profile.agreeableness < 5.0,
        "Betrayal should lower Agreeableness from 5.0, got {}",
        profile.agreeableness
    );
}

#[test]
fn apply_shifts_skips_npc_without_ocean_profile() {
    let mut registry = vec![make_entry_no_ocean("NoOcean")];

    let events = vec![("NoOcean".to_string(), PersonalityEvent::Victory)];
    let (applied, _log) = apply_ocean_shifts(&mut registry, &events, 1);

    assert!(
        applied.is_empty(),
        "should not apply shifts to NPC without OCEAN profile"
    );
}

#[test]
fn apply_shifts_skips_unknown_npc() {
    let mut registry = vec![make_entry("Griselda", OceanProfile::default())];

    let events = vec![("UnknownNpc".to_string(), PersonalityEvent::Victory)];
    let (applied, _log) = apply_ocean_shifts(&mut registry, &events, 1);

    assert!(
        applied.is_empty(),
        "should not apply shifts when NPC is not in registry"
    );
}

// ─── AC-3: Shift log returned ──────────────────────────────

#[test]
fn apply_shifts_returns_log_with_entries() {
    let mut registry = vec![make_entry("Viktor", default_profile())];

    let events = vec![("Viktor".to_string(), PersonalityEvent::NearDeath)];
    let (applied, log) = apply_ocean_shifts(&mut registry, &events, 3);

    assert!(!applied.is_empty(), "should apply at least one shift");
    assert!(
        !log.shifts().is_empty(),
        "shift log should contain entries, got empty log"
    );

    let profile = registry[0].ocean.as_ref().unwrap();
    assert!(
        profile.neuroticism > 5.0,
        "NearDeath should raise Neuroticism from 5.0, got {}",
        profile.neuroticism
    );
}

// ─── AC-4: Changes persist across turns ────────────────────

#[test]
fn shifts_accumulate_across_multiple_applications() {
    let mut registry = vec![make_entry("Mira", default_profile())];

    // Turn 1: Victory
    let events_1 = vec![("Mira".to_string(), PersonalityEvent::Victory)];
    apply_ocean_shifts(&mut registry, &events_1, 1);

    let after_victory = registry[0].ocean.as_ref().unwrap().conscientiousness;
    assert!(
        after_victory > 5.0,
        "Victory should raise Conscientiousness"
    );

    // Turn 2: Another Victory — should stack
    let events_2 = vec![("Mira".to_string(), PersonalityEvent::Victory)];
    apply_ocean_shifts(&mut registry, &events_2, 2);

    let after_second = registry[0].ocean.as_ref().unwrap().conscientiousness;
    assert!(
        after_second > after_victory,
        "Second Victory should stack: {} should be > {}",
        after_second,
        after_victory
    );
}

#[test]
fn profile_stays_within_bounds_after_many_shifts() {
    let ocean = OceanProfile {
        openness: 9.0,
        conscientiousness: 9.0,
        extraversion: 9.0,
        agreeableness: 1.0,
        neuroticism: 9.0,
    };
    let mut registry = vec![make_entry("Boundary", ocean)];

    for turn in 1..=10 {
        let events = vec![("Boundary".to_string(), PersonalityEvent::NearDeath)];
        apply_ocean_shifts(&mut registry, &events, turn);
    }

    let profile = registry[0].ocean.as_ref().unwrap();
    for dim in [
        OceanDimension::Openness,
        OceanDimension::Conscientiousness,
        OceanDimension::Extraversion,
        OceanDimension::Agreeableness,
        OceanDimension::Neuroticism,
    ] {
        let val = profile.get(dim);
        assert!(
            (0.0..=10.0).contains(&val),
            "{:?} = {} out of bounds after repeated shifts",
            dim,
            val
        );
    }
}

// ─── AC-5: End-to-end typed event → application → summary ──

#[test]
fn end_to_end_typed_event_to_profile_change() {
    let mut registry = vec![make_entry("Sable", default_profile())];

    // Typed event — no keyword detection, directly from narrator JSON
    let events = vec![("Sable".to_string(), PersonalityEvent::Defeat)];
    let (applied, _log) = apply_ocean_shifts(&mut registry, &events, 1);

    assert!(!applied.is_empty(), "should apply at least one shift");

    let profile = registry[0].ocean.as_ref().unwrap();
    let neuroticism_changed = profile.neuroticism != 5.0;
    let extraversion_changed = profile.extraversion != 5.0;
    assert!(
        neuroticism_changed || extraversion_changed,
        "Defeat should change Neuroticism or Extraversion. N={}, E={}",
        profile.neuroticism,
        profile.extraversion
    );
}

#[test]
fn end_to_end_with_multiple_npcs() {
    let mut registry = vec![
        make_entry("Griselda", OceanProfile::default()),
        make_entry("Kael", OceanProfile::default()),
    ];

    // Two typed events — from narrator's structured JSON block
    let events = vec![
        ("Griselda".to_string(), PersonalityEvent::Betrayal),
        ("Kael".to_string(), PersonalityEvent::NearDeath),
    ];
    let (applied, _log) = apply_ocean_shifts(&mut registry, &events, 1);

    let any_changed = registry.iter().any(|entry| {
        if let Some(profile) = &entry.ocean {
            profile.openness != 5.0
                || profile.conscientiousness != 5.0
                || profile.extraversion != 5.0
                || profile.agreeableness != 5.0
                || profile.neuroticism != 5.0
        } else {
            false
        }
    });

    assert!(
        any_changed,
        "at least one NPC profile should change. Applied: {}",
        applied.len()
    );
}

#[test]
fn ocean_summary_regenerated_after_shift() {
    let ocean = OceanProfile {
        openness: 5.0,
        conscientiousness: 5.0,
        extraversion: 5.0,
        agreeableness: 4.0,
        neuroticism: 5.0,
    };
    let mut registry = vec![make_entry("Mira", ocean)];
    let summary_before = registry[0].ocean_summary.clone();

    let events = vec![("Mira".to_string(), PersonalityEvent::Betrayal)];
    apply_ocean_shifts(&mut registry, &events, 1);

    assert_ne!(
        registry[0].ocean_summary, summary_before,
        "ocean_summary should be regenerated after shift crosses threshold"
    );
}

//! Story 15-2: Wire OCEAN shift proposals into game flow — events trigger
//! personality evolution.
//!
//! Tests for the full pipeline: narration → event detection → proposal →
//! profile mutation on NpcRegistryEntry → summary regeneration.
//!   AC-1: PersonalityEvents detected from narration (at least 2 event types)
//!   AC-2: Proposals generated and applied to NPC OCEAN profiles
//!   AC-3: OceanShift log returned (not discarded)
//!   AC-4: Personality changes persist (mutated on NpcRegistryEntry)
//!   AC-5: End-to-end: detection → proposal → application → summary update

use sidequest_game::{
    apply_ocean_shifts, detect_personality_events, NpcRegistryEntry, OceanDimension, OceanProfile,
    PersonalityEvent,
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

// ─── AC-1: Event detection from narration ──────────────────

#[test]
fn detects_betrayal_from_narration() {
    let narration = "Griselda turns on you, plunging her dagger into your back. \
                     The betrayal is complete.";
    let npc_names = vec!["Griselda"];
    let events = detect_personality_events(narration, &npc_names);

    assert!(
        events.iter().any(|(name, e)| name == "Griselda" && *e == PersonalityEvent::Betrayal),
        "should detect Betrayal for Griselda, got: {:?}",
        events
    );
}

#[test]
fn detects_near_death_from_narration() {
    let narration = "Kael collapses to the ground, barely clinging to life. \
                     Blood pools beneath him as he nearly dies from the wound.";
    let npc_names = vec!["Kael"];
    let events = detect_personality_events(narration, &npc_names);

    assert!(
        events.iter().any(|(name, e)| name == "Kael" && *e == PersonalityEvent::NearDeath),
        "should detect NearDeath for Kael, got: {:?}",
        events
    );
}

#[test]
fn detects_victory_from_narration() {
    let narration = "Mira strikes the final blow, vanquishing the beast. \
                     A triumphant cry rises from the party as victory is theirs.";
    let npc_names = vec!["Mira"];
    let events = detect_personality_events(narration, &npc_names);

    assert!(
        events.iter().any(|(name, e)| name == "Mira" && *e == PersonalityEvent::Victory),
        "should detect Victory for Mira, got: {:?}",
        events
    );
}

#[test]
fn detects_defeat_from_narration() {
    let narration = "Torval drops his weapon in shame, utterly defeated. \
                     The loss weighs heavy on his shoulders.";
    let npc_names = vec!["Torval"];
    let events = detect_personality_events(narration, &npc_names);

    assert!(
        events.iter().any(|(name, e)| name == "Torval" && *e == PersonalityEvent::Defeat),
        "should detect Defeat for Torval, got: {:?}",
        events
    );
}

#[test]
fn detects_social_bonding_from_narration() {
    let narration = "Anya shares a quiet moment by the fire, forming a deep \
                     bond of friendship with the party. Trust builds between you.";
    let npc_names = vec!["Anya"];
    let events = detect_personality_events(narration, &npc_names);

    assert!(
        events.iter().any(|(name, e)| name == "Anya" && *e == PersonalityEvent::SocialBonding),
        "should detect SocialBonding for Anya, got: {:?}",
        events
    );
}

#[test]
fn returns_empty_when_no_events_detected() {
    let narration = "The sun sets over the quiet village. Nothing of note happens.";
    let npc_names = vec!["Griselda", "Kael"];
    let events = detect_personality_events(narration, &npc_names);

    assert!(events.is_empty(), "should return no events for mundane narration, got: {:?}", events);
}

#[test]
fn only_detects_events_for_known_npcs() {
    let narration = "Zephyr betrays the group, stabbing Griselda in the back.";
    let npc_names = vec!["Griselda"];
    let events = detect_personality_events(narration, &npc_names);

    assert!(
        events.iter().all(|(name, _)| name != "Zephyr"),
        "should not detect events for unknown NPC Zephyr, got: {:?}",
        events
    );
}

#[test]
fn detects_multiple_events_in_single_narration() {
    let narration = "Griselda betrays the party while Kael nearly dies from \
                     the poison. Mira claims victory over the assassin.";
    let npc_names = vec!["Griselda", "Kael", "Mira"];
    let events = detect_personality_events(narration, &npc_names);

    let unique_npcs: std::collections::HashSet<_> =
        events.iter().map(|(name, _)| name.as_str()).collect();
    assert!(
        unique_npcs.len() >= 2,
        "should detect events for at least 2 NPCs, got: {:?}",
        events
    );
}

#[test]
fn no_false_positive_on_vagabond() {
    let narration = "The vagabond wanders into town. Griselda greets them warmly.";
    let npc_names = vec!["Griselda"];
    let events = detect_personality_events(narration, &npc_names);

    assert!(
        !events.iter().any(|(_, e)| *e == PersonalityEvent::SocialBonding),
        "vagabond should not trigger SocialBonding, got: {:?}",
        events
    );
}

// ─── AC-2: Proposals applied to NPC OCEAN profiles ─────────

#[test]
fn apply_shifts_modifies_npc_ocean_profile() {
    let mut registry = vec![make_entry("Griselda", default_profile())];

    let events = vec![("Griselda".to_string(), PersonalityEvent::Betrayal)];
    let (applied, _log) = apply_ocean_shifts(&mut registry, &events, 1);

    assert!(!applied.is_empty(), "should return applied proposals");

    let profile = registry[0].ocean.as_ref().expect("should still have OCEAN profile");
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

    assert!(applied.is_empty(), "should not apply shifts to NPC without OCEAN profile");
}

#[test]
fn apply_shifts_skips_unknown_npc() {
    let mut registry = vec![make_entry("Griselda", OceanProfile::default())];

    let events = vec![("UnknownNpc".to_string(), PersonalityEvent::Victory)];
    let (applied, _log) = apply_ocean_shifts(&mut registry, &events, 1);

    assert!(applied.is_empty(), "should not apply shifts when NPC is not in registry");
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
    assert!(after_victory > 5.0, "Victory should raise Conscientiousness");

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

// ─── AC-5: End-to-end detection → application → summary ───

#[test]
fn end_to_end_narration_to_profile_change() {
    let mut registry = vec![make_entry("Sable", default_profile())];

    let narration = "Sable suffers a crushing defeat at the hands of the warlord.";
    let npc_names: Vec<&str> = registry.iter().map(|e| e.name.as_str()).collect();
    let events = detect_personality_events(narration, &npc_names);

    assert!(!events.is_empty(), "should detect at least one event from defeat narration");

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

    let narration = "Griselda betrays the party while Kael barely survives \
                     the ambush, nearly dying from his wounds.";
    let npc_names: Vec<&str> = registry.iter().map(|e| e.name.as_str()).collect();
    let events = detect_personality_events(narration, &npc_names);

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

    assert!(any_changed, "at least one NPC profile should change. Applied: {}", applied.len());
}

#[test]
fn ocean_summary_regenerated_after_shift() {
    // Start agreeableness at 4.0 — betrayal shifts it by -1.5 to 2.5, crossing
    // the ≤3.0 threshold and producing "competitive and blunt" in the summary.
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

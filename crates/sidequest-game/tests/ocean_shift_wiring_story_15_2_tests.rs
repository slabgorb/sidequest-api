//! Story 15-2: Wire OCEAN shift proposals into game flow — events trigger
//! personality evolution.
//!
//! RED phase: tests for the *wiring* layer that 10-6 left unconnected.
//! The proposal function itself is tested in 10-6; these tests cover:
//!   AC-1: PersonalityEvents detected from narration (at least 2 event types)
//!   AC-2: Proposals generated and applied to NPC OCEAN profiles
//!   AC-3: OceanShift events logged to shift log
//!   AC-4: Personality changes persist (applied to Npc in GameSnapshot)
//!   AC-5: End-to-end: detection → proposal → application

use sidequest_game::{
    detect_personality_events, apply_ocean_shifts,
    Combatant, Disposition, GameSnapshot, Npc, OceanProfile, PersonalityEvent,
    OceanDimension,
};
use sidequest_game::creature_core::CreatureCore;
use sidequest_game::inventory::Inventory;
use sidequest_protocol::NonBlankString;

// ─── Helpers ───────────────────────────────────────────────

fn make_npc(name: &str, ocean: OceanProfile) -> Npc {
    Npc {
        core: CreatureCore {
            name: NonBlankString::new(name).unwrap(),
            description: NonBlankString::new("test NPC").unwrap(),
            personality: NonBlankString::new("test").unwrap(),
            level: 1,
            hp: 10,
            max_hp: 10,
            ac: 10,
            xp: 0,
            statuses: vec![],
            inventory: Inventory::default(),
        },
        voice_id: None,
        disposition: Disposition::new(0),
        location: None,
        pronouns: None,
        appearance: None,
        age: None,
        build: None,
        height: None,
        distinguishing_features: vec![],
        ocean: Some(ocean),
    }
}

fn make_npc_no_ocean(name: &str) -> Npc {
    Npc {
        core: CreatureCore {
            name: NonBlankString::new(name).unwrap(),
            description: NonBlankString::new("test NPC").unwrap(),
            personality: NonBlankString::new("test").unwrap(),
            level: 1,
            hp: 10,
            max_hp: 10,
            ac: 10,
            xp: 0,
            statuses: vec![],
            inventory: Inventory::default(),
        },
        voice_id: None,
        disposition: Disposition::new(0),
        location: None,
        pronouns: None,
        appearance: None,
        age: None,
        build: None,
        height: None,
        distinguishing_features: vec![],
        ocean: None,
    }
}

fn snapshot_with_npcs(npcs: Vec<Npc>) -> GameSnapshot {
    GameSnapshot {
        npcs,
        ..Default::default()
    }
}

// ─── AC-1: Event detection from narration ──────────────────

#[test]
fn detects_betrayal_from_narration() {
    let narration = "Griselda turns on you, plunging her dagger into your back. \
                     The betrayal is complete.";
    let npc_names = vec!["Griselda"];
    let events = detect_personality_events(narration, &npc_names);

    let griselda_events: Vec<_> = events.iter()
        .filter(|(name, _)| name == "Griselda")
        .collect();
    assert!(
        griselda_events.iter().any(|(_, e)| *e == PersonalityEvent::Betrayal),
        "should detect Betrayal for Griselda, got: {:?}",
        griselda_events
    );
}

#[test]
fn detects_near_death_from_narration() {
    let narration = "Kael collapses to the ground, barely clinging to life. \
                     Blood pools beneath him as he nearly dies from the wound.";
    let npc_names = vec!["Kael"];
    let events = detect_personality_events(narration, &npc_names);

    let kael_events: Vec<_> = events.iter()
        .filter(|(name, _)| name == "Kael")
        .collect();
    assert!(
        kael_events.iter().any(|(_, e)| *e == PersonalityEvent::NearDeath),
        "should detect NearDeath for Kael, got: {:?}",
        kael_events
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

    assert!(
        events.is_empty(),
        "should return no events for mundane narration, got: {:?}",
        events
    );
}

#[test]
fn only_detects_events_for_known_npcs() {
    let narration = "Zephyr betrays the group, stabbing Griselda in the back.";
    // Only Griselda is a known NPC — Zephyr is not in the list
    let npc_names = vec!["Griselda"];
    let events = detect_personality_events(narration, &npc_names);

    // Should not produce events for Zephyr since they're not a known NPC
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

    // Should find events for multiple NPCs
    let unique_npcs: std::collections::HashSet<_> = events.iter()
        .map(|(name, _): &(String, PersonalityEvent)| name.as_str())
        .collect();
    assert!(
        unique_npcs.len() >= 2,
        "should detect events for at least 2 NPCs, got: {:?}",
        events
    );
}

// ─── AC-2: Proposals applied to NPC OCEAN profiles ─────────

#[test]
fn apply_shifts_modifies_npc_ocean_profile() {
    let ocean = OceanProfile {
        openness: 5.0,
        conscientiousness: 5.0,
        extraversion: 5.0,
        agreeableness: 5.0,
        neuroticism: 5.0,
    };
    let npc = make_npc("Griselda", ocean);
    let mut snapshot = snapshot_with_npcs(vec![npc]);

    let events = vec![
        ("Griselda".to_string(), PersonalityEvent::Betrayal),
    ];

    let applied = apply_ocean_shifts(&mut snapshot, &events, 1);
    assert!(!applied.is_empty(), "should return applied proposals");

    // Griselda's agreeableness should have decreased (betrayal lowers it)
    let griselda = snapshot.npcs.iter().find(|n| n.name() == "Griselda").unwrap();
    let profile = griselda.ocean.as_ref().expect("should still have OCEAN profile");
    assert!(
        profile.agreeableness < 5.0,
        "Betrayal should lower Agreeableness from 5.0, got {}",
        profile.agreeableness
    );
}

#[test]
fn apply_shifts_skips_npc_without_ocean_profile() {
    let npc = make_npc_no_ocean("NoOcean");
    let mut snapshot = snapshot_with_npcs(vec![npc]);

    let events = vec![
        ("NoOcean".to_string(), PersonalityEvent::Victory),
    ];

    let applied = apply_ocean_shifts(&mut snapshot, &events, 1);
    assert!(
        applied.is_empty(),
        "should not apply shifts to NPC without OCEAN profile"
    );
}

#[test]
fn apply_shifts_skips_unknown_npc() {
    let ocean = OceanProfile::default();
    let npc = make_npc("Griselda", ocean);
    let mut snapshot = snapshot_with_npcs(vec![npc]);

    let events = vec![
        ("UnknownNpc".to_string(), PersonalityEvent::Victory),
    ];

    let applied = apply_ocean_shifts(&mut snapshot, &events, 1);
    assert!(
        applied.is_empty(),
        "should not apply shifts when NPC is not in snapshot"
    );
}

// ─── AC-3: Shift log records changes ───────────────────────

#[test]
fn apply_shifts_changes_neuroticism_for_near_death() {
    let ocean = OceanProfile {
        openness: 5.0,
        conscientiousness: 5.0,
        extraversion: 5.0,
        agreeableness: 5.0,
        neuroticism: 5.0,
    };
    let npc = make_npc("Viktor", ocean);
    let mut snapshot = snapshot_with_npcs(vec![npc]);

    let events = vec![
        ("Viktor".to_string(), PersonalityEvent::NearDeath),
    ];

    let applied = apply_ocean_shifts(&mut snapshot, &events, 3);
    assert!(!applied.is_empty(), "should apply at least one shift");

    // Neuroticism should have increased
    let viktor = snapshot.npcs.iter().find(|n| n.name() == "Viktor").unwrap();
    let profile = viktor.ocean.as_ref().unwrap();
    assert!(
        profile.neuroticism > 5.0,
        "NearDeath should raise Neuroticism from 5.0, got {}",
        profile.neuroticism
    );
}

// ─── AC-4: Changes persist across turns ────────────────────

#[test]
fn shifts_accumulate_across_multiple_applications() {
    let ocean = OceanProfile {
        openness: 5.0,
        conscientiousness: 5.0,
        extraversion: 5.0,
        agreeableness: 5.0,
        neuroticism: 5.0,
    };
    let npc = make_npc("Mira", ocean);
    let mut snapshot = snapshot_with_npcs(vec![npc]);

    // Turn 1: Victory
    let events_1 = vec![("Mira".to_string(), PersonalityEvent::Victory)];
    apply_ocean_shifts(&mut snapshot, &events_1, 1);

    let mira = snapshot.npcs.iter().find(|n| n.name() == "Mira").unwrap();
    let after_victory = mira.ocean.as_ref().unwrap().conscientiousness;
    assert!(after_victory > 5.0, "Victory should raise Conscientiousness");

    // Turn 2: Another Victory — should stack
    let events_2 = vec![("Mira".to_string(), PersonalityEvent::Victory)];
    apply_ocean_shifts(&mut snapshot, &events_2, 2);

    let mira = snapshot.npcs.iter().find(|n| n.name() == "Mira").unwrap();
    let after_second = mira.ocean.as_ref().unwrap().conscientiousness;
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
    let npc = make_npc("Boundary", ocean);
    let mut snapshot = snapshot_with_npcs(vec![npc]);

    // Hammer the same event 10 times to stress-test clamping
    for turn in 1..=10 {
        let events = vec![("Boundary".to_string(), PersonalityEvent::NearDeath)];
        apply_ocean_shifts(&mut snapshot, &events, turn);
    }

    let npc = snapshot.npcs.iter().find(|n| n.name() == "Boundary").unwrap();
    let profile = npc.ocean.as_ref().unwrap();
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

// ─── AC-5: End-to-end detection → application ──────────────

#[test]
fn end_to_end_narration_to_profile_change() {
    let ocean = OceanProfile {
        openness: 5.0,
        conscientiousness: 5.0,
        extraversion: 5.0,
        agreeableness: 5.0,
        neuroticism: 5.0,
    };
    let npc = make_npc("Sable", ocean);
    let mut snapshot = snapshot_with_npcs(vec![npc]);

    // Step 1: Detect events from narration
    let narration = "Sable suffers a crushing defeat at the hands of the warlord.";
    let npc_names: Vec<&str> = snapshot.npcs.iter().map(|n| n.name()).collect();
    let events = detect_personality_events(narration, &npc_names);

    assert!(
        !events.is_empty(),
        "should detect at least one event from defeat narration"
    );

    // Step 2: Apply to game state
    let applied = apply_ocean_shifts(&mut snapshot, &events, 1);
    assert!(!applied.is_empty(), "should apply at least one shift");

    // Step 3: Verify profile changed
    let sable = snapshot.npcs.iter().find(|n| n.name() == "Sable").unwrap();
    let profile = sable.ocean.as_ref().unwrap();

    // Defeat should raise Neuroticism and/or lower Extraversion
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
    let npcs = vec![
        make_npc("Griselda", OceanProfile::default()),
        make_npc("Kael", OceanProfile::default()),
    ];
    let mut snapshot = snapshot_with_npcs(npcs);

    let narration = "Griselda betrays the party while Kael barely survives \
                     the ambush, nearly dying from his wounds.";
    let npc_names: Vec<&str> = snapshot.npcs.iter().map(|n| n.name()).collect();
    let events = detect_personality_events(narration, &npc_names);

    let applied = apply_ocean_shifts(&mut snapshot, &events, 1);

    // At least one NPC's profile should have changed
    let any_changed = snapshot.npcs.iter().any(|npc| {
        if let Some(profile) = &npc.ocean {
            // Default is 5.0 for all dimensions
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

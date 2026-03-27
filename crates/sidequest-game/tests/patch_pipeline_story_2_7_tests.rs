//! Story 2-7: State Patch Pipeline — failing tests (RED phase)
//!
//! These tests cover the full state patch pipeline:
//!   - Extended WorldStatePatch (hp_changes, npc_attitudes, time_of_day, quest merge, region dedup)
//!   - Disposition::from_attitude_str() attitude string coercion
//!   - NPC upsert with identity locking (merge_patch, pronouns, appearance)
//!   - Extended CombatPatch (in_combat, hp_changes, turn_order, etc.)
//!   - Extended ChasePatch (separation, phase, event)
//!   - StateDelta → reactive GameMessage generation
//!   - deny_unknown_fields on patches

use std::collections::HashMap;

use sidequest_game::character::Character;
use sidequest_game::chase::{ChaseState, ChaseType};
use sidequest_game::combat::CombatState;
use sidequest_game::creature_core::CreatureCore;
use sidequest_game::delta::{compute_delta, snapshot};
use sidequest_game::disposition::{Attitude, Disposition};
use sidequest_game::inventory::Inventory;
use sidequest_game::narrative::NarrativeEntry;
use sidequest_game::npc::Npc;
use sidequest_game::state::{ChasePatch, CombatPatch, GameSnapshot, NpcPatch, WorldStatePatch};
use sidequest_game::turn::TurnManager;
use sidequest_protocol::NonBlankString;

// Story 2-7: reactive message generation
use sidequest_game::state::broadcast_state_changes;

// ============================================================================
// Test fixtures
// ============================================================================

fn test_character() -> Character {
    Character {
        core: CreatureCore {
            name: NonBlankString::new("Thorn Ironhide").unwrap(),
            description: NonBlankString::new("A scarred dwarf warrior").unwrap(),
            personality: NonBlankString::new("Gruff but loyal").unwrap(),
            level: 3,
            hp: 25,
            max_hp: 30,
            ac: 16,
            inventory: Inventory::default(),
            statuses: vec![],
        },
        backstory: NonBlankString::new("Raised in the iron mines").unwrap(),
        narrative_state: "Exploring the wastes".to_string(),
        hooks: vec!["nemesis: The Warden".to_string()],
        char_class: NonBlankString::new("Fighter").unwrap(),
        race: NonBlankString::new("Dwarf").unwrap(),
        stats: HashMap::from([("STR".to_string(), 16), ("DEX".to_string(), 10)]),
        abilities: vec![],
    }
}

fn test_npc() -> Npc {
    Npc {
        core: CreatureCore {
            name: NonBlankString::new("Marta the Innkeeper").unwrap(),
            description: NonBlankString::new("A stout woman").unwrap(),
            personality: NonBlankString::new("Warm and gossipy").unwrap(),
            level: 2,
            hp: 12,
            max_hp: 12,
            ac: 10,
            statuses: vec![],
            inventory: Inventory::default(),
        },
        voice_id: Some(3),
        disposition: Disposition::new(15),
        location: Some(NonBlankString::new("The Rusty Nail Inn").unwrap()),
        pronouns: Some("she/her".to_string()),
        appearance: Some("Flour-dusted apron, kind eyes".to_string()),
    }
}

fn test_npc_no_identity() -> Npc {
    Npc {
        core: CreatureCore {
            name: NonBlankString::new("Stranger").unwrap(),
            description: NonBlankString::new("A mysterious figure").unwrap(),
            personality: NonBlankString::new("Enigmatic").unwrap(),
            level: 1,
            hp: 8,
            max_hp: 8,
            ac: 10,
            statuses: vec![],
            inventory: Inventory::default(),
        },
        voice_id: None,
        disposition: Disposition::new(0),
        location: None,
        pronouns: None,
        appearance: None,
    }
}

fn test_snapshot() -> GameSnapshot {
    GameSnapshot {
        genre_slug: "mutant_wasteland".to_string(),
        world_slug: "flickering_reach".to_string(),
        characters: vec![test_character()],
        npcs: vec![test_npc()],
        location: "The Rusty Nail Inn".to_string(),
        time_of_day: "dusk".to_string(),
        quest_log: HashMap::from([(
            "main".to_string(),
            "Find the source of the flickering".to_string(),
        )]),
        notes: vec!["The innkeeper seems nervous".to_string()],
        narrative_log: vec![],
        combat: CombatState::new(),
        chase: None,
        active_tropes: vec![],
        atmosphere: "tense".to_string(),
        current_region: "flickering_reach".to_string(),
        discovered_regions: vec!["flickering_reach".to_string()],
        discovered_routes: vec![],
        turn_manager: TurnManager::new(),
        last_saved_at: None,
        active_stakes: String::new(),
        lore_established: vec![],
    }
}

// ============================================================================
// AC: World patch applies — location, time_of_day, atmosphere
// ============================================================================

#[test]
fn world_patch_applies_time_of_day() {
    let mut snap = test_snapshot();
    assert_eq!(snap.time_of_day, "dusk");

    let patch = WorldStatePatch {
        time_of_day: Some("midnight".to_string()),
        ..Default::default()
    };
    snap.apply_world_patch(&patch);
    assert_eq!(snap.time_of_day, "midnight");
}

#[test]
fn world_patch_applies_active_stakes() {
    let mut snap = test_snapshot();

    let patch = WorldStatePatch {
        active_stakes: Some("The town is under siege".to_string()),
        ..Default::default()
    };
    snap.apply_world_patch(&patch);
    assert_eq!(snap.active_stakes, "The town is under siege");
}

#[test]
fn world_patch_applies_lore_established() {
    let mut snap = test_snapshot();
    assert!(snap.lore_established.is_empty());

    let patch = WorldStatePatch {
        lore_established: Some(vec!["The Flickering predates the wasteland".to_string()]),
        ..Default::default()
    };
    snap.apply_world_patch(&patch);
    assert_eq!(snap.lore_established.len(), 1);
    assert_eq!(
        snap.lore_established[0],
        "The Flickering predates the wasteland"
    );
}

// ============================================================================
// AC: HP changes — hp_changes: {"Hero": -5} → character HP clamped
// ============================================================================

#[test]
fn world_patch_hp_changes_reduces_character_hp() {
    let mut snap = test_snapshot();
    assert_eq!(snap.characters[0].core.hp, 25);

    let patch = WorldStatePatch {
        hp_changes: Some(HashMap::from([("Thorn Ironhide".to_string(), -5)])),
        ..Default::default()
    };
    snap.apply_world_patch(&patch);
    assert_eq!(snap.characters[0].core.hp, 20);
}

#[test]
fn world_patch_hp_changes_clamps_to_zero() {
    let mut snap = test_snapshot();
    let patch = WorldStatePatch {
        hp_changes: Some(HashMap::from([("Thorn Ironhide".to_string(), -100)])),
        ..Default::default()
    };
    snap.apply_world_patch(&patch);
    assert_eq!(snap.characters[0].core.hp, 0);
}

#[test]
fn world_patch_hp_changes_clamps_to_max_hp() {
    let mut snap = test_snapshot();
    let patch = WorldStatePatch {
        hp_changes: Some(HashMap::from([("Thorn Ironhide".to_string(), 100)])),
        ..Default::default()
    };
    snap.apply_world_patch(&patch);
    assert_eq!(snap.characters[0].core.hp, 30); // max_hp
}

#[test]
fn world_patch_hp_changes_ignores_unknown_character() {
    let mut snap = test_snapshot();
    let hp_before = snap.characters[0].core.hp;

    let patch = WorldStatePatch {
        hp_changes: Some(HashMap::from([("Nonexistent Hero".to_string(), -10)])),
        ..Default::default()
    };
    snap.apply_world_patch(&patch);
    // Should not panic, and existing character HP unchanged
    assert_eq!(snap.characters[0].core.hp, hp_before);
}

#[test]
fn world_patch_hp_changes_applies_to_npcs_too() {
    let mut snap = test_snapshot();
    assert_eq!(snap.npcs[0].core.hp, 12);

    let patch = WorldStatePatch {
        hp_changes: Some(HashMap::from([("Marta the Innkeeper".to_string(), -3)])),
        ..Default::default()
    };
    snap.apply_world_patch(&patch);
    assert_eq!(snap.npcs[0].core.hp, 9);
}

// ============================================================================
// AC: NPC attitude — npc_attitudes: {"Merchant": "friendly"} → disposition
// ============================================================================

#[test]
fn disposition_from_attitude_str_friendly() {
    let disp = Disposition::from_attitude_str("friendly");
    assert!(disp.is_some(), "friendly should produce a disposition");
    assert_eq!(disp.unwrap().attitude(), Attitude::Friendly);
}

#[test]
fn disposition_from_attitude_str_hostile() {
    let disp = Disposition::from_attitude_str("hostile");
    assert!(disp.is_some());
    assert_eq!(disp.unwrap().attitude(), Attitude::Hostile);
}

#[test]
fn disposition_from_attitude_str_neutral() {
    let disp = Disposition::from_attitude_str("neutral");
    assert!(disp.is_some());
    assert_eq!(disp.unwrap().attitude(), Attitude::Neutral);
}

#[test]
fn disposition_from_attitude_str_case_insensitive() {
    let disp = Disposition::from_attitude_str("FRIENDLY");
    assert!(disp.is_some());
    assert_eq!(disp.unwrap().attitude(), Attitude::Friendly);
}

#[test]
fn disposition_from_attitude_str_deceased_returns_none() {
    // "deceased" means don't update disposition — return None
    let disp = Disposition::from_attitude_str("deceased");
    assert!(disp.is_none(), "deceased should return None (don't update)");
}

#[test]
fn disposition_from_attitude_str_dead_returns_none() {
    let disp = Disposition::from_attitude_str("dead");
    assert!(disp.is_none(), "dead should return None");
}

#[test]
fn world_patch_npc_attitudes_sets_disposition() {
    let mut snap = test_snapshot();
    // Marta starts friendly (disposition 15)
    assert_eq!(snap.npcs[0].attitude(), Attitude::Friendly);

    let patch = WorldStatePatch {
        npc_attitudes: Some(HashMap::from([(
            "Marta the Innkeeper".to_string(),
            "hostile".to_string(),
        )])),
        ..Default::default()
    };
    snap.apply_world_patch(&patch);
    assert_eq!(snap.npcs[0].attitude(), Attitude::Hostile);
}

#[test]
fn world_patch_npc_attitudes_deceased_skips_update() {
    let mut snap = test_snapshot();
    let disp_before = snap.npcs[0].disposition.value();

    let patch = WorldStatePatch {
        npc_attitudes: Some(HashMap::from([(
            "Marta the Innkeeper".to_string(),
            "deceased".to_string(),
        )])),
        ..Default::default()
    };
    snap.apply_world_patch(&patch);
    // Disposition should remain unchanged
    assert_eq!(snap.npcs[0].disposition.value(), disp_before);
}

#[test]
fn world_patch_npc_attitudes_ignores_unknown_npc() {
    let mut snap = test_snapshot();
    let disp_before = snap.npcs[0].disposition.value();

    let patch = WorldStatePatch {
        npc_attitudes: Some(HashMap::from([(
            "Ghost NPC".to_string(),
            "friendly".to_string(),
        )])),
        ..Default::default()
    };
    snap.apply_world_patch(&patch);
    // Should not panic, existing NPC unchanged
    assert_eq!(snap.npcs[0].disposition.value(), disp_before);
}

// ============================================================================
// AC: NPC upsert — new NPC added, existing NPC mutable fields merged
// ============================================================================

#[test]
fn world_patch_npcs_present_adds_new_npc() {
    let mut snap = test_snapshot();
    assert_eq!(snap.npcs.len(), 1);

    let new_npc = NpcPatch {
        name: "Razortooth".to_string(),
        description: Some("A scarred raider".to_string()),
        personality: Some("Cruel".to_string()),
        role: Some("bandit".to_string()),
        pronouns: Some("he/him".to_string()),
        appearance: Some("Missing teeth, scar across face".to_string()),
        location: Some("The Wasteland Highway".to_string()),
    };

    let patch = WorldStatePatch {
        npcs_present: Some(vec![new_npc]),
        ..Default::default()
    };
    snap.apply_world_patch(&patch);
    assert_eq!(snap.npcs.len(), 2);
    assert_eq!(snap.npcs[1].core.name.as_str(), "Razortooth");
}

#[test]
fn world_patch_npcs_present_merges_existing_npc() {
    let mut snap = test_snapshot();
    // Marta already exists with description "A stout woman"
    let update = NpcPatch {
        name: "Marta the Innkeeper".to_string(),
        description: Some("A stout woman with flour-dusted hands".to_string()),
        personality: None,
        role: Some("innkeeper".to_string()),
        pronouns: None,
        appearance: None,
        location: None,
    };

    let patch = WorldStatePatch {
        npcs_present: Some(vec![update]),
        ..Default::default()
    };
    snap.apply_world_patch(&patch);
    // Should still be 1 NPC (merged, not duplicated)
    assert_eq!(snap.npcs.len(), 1);
    assert_eq!(
        snap.npcs[0].core.description.as_str(),
        "A stout woman with flour-dusted hands"
    );
}

// ============================================================================
// AC: Identity locking — pronouns/appearance set once, never overwritten
// ============================================================================

#[test]
fn npc_merge_patch_updates_mutable_fields() {
    let mut npc = test_npc();
    let patch = NpcPatch {
        name: "Marta the Innkeeper".to_string(),
        description: Some("Updated description".to_string()),
        personality: None,
        role: Some("tavern owner".to_string()),
        pronouns: None,
        appearance: None,
        location: Some("The Kitchen".to_string()),
    };

    npc.merge_patch(&patch);
    assert_eq!(npc.core.description.as_str(), "Updated description");
}

#[test]
fn npc_merge_patch_locks_pronouns_after_first_set() {
    let mut npc = test_npc();
    // Pronouns already set to "she/her"
    assert_eq!(npc.pronouns.as_deref(), Some("she/her"));

    let patch = NpcPatch {
        name: "Marta the Innkeeper".to_string(),
        description: None,
        personality: None,
        role: None,
        pronouns: Some("they/them".to_string()), // attempt to overwrite
        appearance: None,
        location: None,
    };

    npc.merge_patch(&patch);
    // Should still be "she/her" — locked on first set
    assert_eq!(npc.pronouns.as_deref(), Some("she/her"));
}

#[test]
fn npc_merge_patch_locks_appearance_after_first_set() {
    let mut npc = test_npc();
    assert!(npc.appearance.is_some());

    let patch = NpcPatch {
        name: "Marta the Innkeeper".to_string(),
        description: None,
        personality: None,
        role: None,
        pronouns: None,
        appearance: Some("Completely different look".to_string()),
        location: None,
    };

    npc.merge_patch(&patch);
    // Should still be original appearance
    assert_eq!(
        npc.appearance.as_deref(),
        Some("Flour-dusted apron, kind eyes")
    );
}

#[test]
fn npc_merge_patch_sets_pronouns_when_empty() {
    let mut npc = test_npc_no_identity();
    assert!(npc.pronouns.is_none());

    let patch = NpcPatch {
        name: "Stranger".to_string(),
        description: None,
        personality: None,
        role: None,
        pronouns: Some("they/them".to_string()),
        appearance: None,
        location: None,
    };

    npc.merge_patch(&patch);
    assert_eq!(npc.pronouns.as_deref(), Some("they/them"));
}

#[test]
fn npc_merge_patch_sets_appearance_when_empty() {
    let mut npc = test_npc_no_identity();
    assert!(npc.appearance.is_none());

    let patch = NpcPatch {
        name: "Stranger".to_string(),
        description: None,
        personality: None,
        role: None,
        pronouns: None,
        appearance: Some("Tall, dark cloak".to_string()),
        location: None,
    };

    npc.merge_patch(&patch);
    assert_eq!(npc.appearance.as_deref(), Some("Tall, dark cloak"));
}

// ============================================================================
// AC: Combat patch — extended CombatPatch fields
// ============================================================================

#[test]
fn combat_patch_sets_in_combat() {
    let mut snap = test_snapshot();
    let patch = CombatPatch {
        in_combat: Some(true),
        ..Default::default()
    };
    snap.apply_combat_patch(&patch);
    assert!(snap.combat.in_combat());
}

#[test]
fn combat_patch_sets_turn_order() {
    let mut snap = test_snapshot();
    let patch = CombatPatch {
        turn_order: Some(vec!["Thorn Ironhide".to_string(), "Razortooth".to_string()]),
        ..Default::default()
    };
    snap.apply_combat_patch(&patch);
    assert_eq!(snap.combat.turn_order().len(), 2);
    assert_eq!(snap.combat.turn_order()[0], "Thorn Ironhide");
}

#[test]
fn combat_patch_sets_current_turn() {
    let mut snap = test_snapshot();
    let patch = CombatPatch {
        current_turn: Some("Thorn Ironhide".to_string()),
        ..Default::default()
    };
    snap.apply_combat_patch(&patch);
    assert_eq!(snap.combat.current_turn(), Some("Thorn Ironhide"));
}

#[test]
fn combat_patch_applies_hp_changes() {
    let mut snap = test_snapshot();
    assert_eq!(snap.characters[0].core.hp, 25);

    let patch = CombatPatch {
        hp_changes: Some(HashMap::from([("Thorn Ironhide".to_string(), -8)])),
        ..Default::default()
    };
    snap.apply_combat_patch(&patch);
    assert_eq!(snap.characters[0].core.hp, 17);
}

#[test]
fn combat_patch_sets_available_actions() {
    let mut snap = test_snapshot();
    let patch = CombatPatch {
        available_actions: Some(vec!["Attack".to_string(), "Defend".to_string()]),
        ..Default::default()
    };
    snap.apply_combat_patch(&patch);
    assert_eq!(snap.combat.available_actions().len(), 2);
}

#[test]
fn combat_patch_sets_drama_weight() {
    let mut snap = test_snapshot();
    let patch = CombatPatch {
        drama_weight: Some(0.85),
        ..Default::default()
    };
    snap.apply_combat_patch(&patch);
    assert!((snap.combat.drama_weight() - 0.85).abs() < f64::EPSILON);
}

// ============================================================================
// AC: Chase patch — extended ChasePatch fields
// ============================================================================

#[test]
fn chase_patch_sets_separation() {
    let mut snap = test_snapshot();
    snap.chase = Some(ChaseState::new(ChaseType::Footrace, 0.5));

    let patch = ChasePatch {
        separation: Some(3),
        ..Default::default()
    };
    snap.apply_chase_patch(&patch);
    assert_eq!(snap.chase.as_ref().unwrap().separation(), 3);
}

#[test]
fn chase_patch_sets_phase() {
    let mut snap = test_snapshot();
    snap.chase = Some(ChaseState::new(ChaseType::Stealth, 0.6));

    let patch = ChasePatch {
        phase: Some("closing_in".to_string()),
        ..Default::default()
    };
    snap.apply_chase_patch(&patch);
    assert_eq!(snap.chase.as_ref().unwrap().phase(), Some("closing_in"));
}

#[test]
fn chase_patch_sets_event() {
    let mut snap = test_snapshot();
    snap.chase = Some(ChaseState::new(ChaseType::Negotiation, 0.4));

    let patch = ChasePatch {
        event: Some("stumbled on debris".to_string()),
        ..Default::default()
    };
    snap.apply_chase_patch(&patch);
    assert_eq!(
        snap.chase.as_ref().unwrap().event(),
        Some("stumbled on debris")
    );
}

// ============================================================================
// AC: Quest merge — additive by key, not full replace
// ============================================================================

#[test]
fn world_patch_quest_updates_adds_new_quest() {
    let mut snap = test_snapshot();
    assert_eq!(snap.quest_log.len(), 1);

    let patch = WorldStatePatch {
        quest_updates: Some(HashMap::from([(
            "side".to_string(),
            "Help the innkeeper".to_string(),
        )])),
        ..Default::default()
    };
    snap.apply_world_patch(&patch);
    assert_eq!(snap.quest_log.len(), 2);
    assert_eq!(snap.quest_log.get("side").unwrap(), "Help the innkeeper");
    // Original quest still present
    assert!(snap.quest_log.contains_key("main"));
}

#[test]
fn world_patch_quest_updates_updates_existing_quest() {
    let mut snap = test_snapshot();

    let patch = WorldStatePatch {
        quest_updates: Some(HashMap::from([(
            "main".to_string(),
            "The flickering was a decoy".to_string(),
        )])),
        ..Default::default()
    };
    snap.apply_world_patch(&patch);
    assert_eq!(snap.quest_log.len(), 1);
    assert_eq!(
        snap.quest_log.get("main").unwrap(),
        "The flickering was a decoy"
    );
}

// ============================================================================
// AC: Region discovery — discover_regions appended, deduplicated
// ============================================================================

#[test]
fn world_patch_discover_regions_appends() {
    let mut snap = test_snapshot();
    assert_eq!(snap.discovered_regions.len(), 1);

    let patch = WorldStatePatch {
        discover_regions: Some(vec!["toxic_marshes".to_string()]),
        ..Default::default()
    };
    snap.apply_world_patch(&patch);
    assert_eq!(snap.discovered_regions.len(), 2);
    assert!(snap
        .discovered_regions
        .contains(&"toxic_marshes".to_string()));
    assert!(snap
        .discovered_regions
        .contains(&"flickering_reach".to_string()));
}

#[test]
fn world_patch_discover_regions_deduplicates() {
    let mut snap = test_snapshot();
    // flickering_reach already discovered
    let patch = WorldStatePatch {
        discover_regions: Some(vec![
            "flickering_reach".to_string(),
            "toxic_marshes".to_string(),
        ]),
        ..Default::default()
    };
    snap.apply_world_patch(&patch);
    // Should have 2, not 3 (flickering_reach not duplicated)
    assert_eq!(snap.discovered_regions.len(), 2);
}

#[test]
fn world_patch_discover_routes_appends() {
    let mut snap = test_snapshot();
    assert!(snap.discovered_routes.is_empty());

    let patch = WorldStatePatch {
        discover_routes: Some(vec!["Inn → Highway".to_string()]),
        ..Default::default()
    };
    snap.apply_world_patch(&patch);
    assert_eq!(snap.discovered_routes.len(), 1);
    assert_eq!(snap.discovered_routes[0], "Inn → Highway");
}

#[test]
fn world_patch_discover_routes_deduplicates() {
    let mut snap = test_snapshot();
    snap.discovered_routes.push("Inn → Highway".to_string());

    let patch = WorldStatePatch {
        discover_routes: Some(vec![
            "Inn → Highway".to_string(),
            "Highway → Marshes".to_string(),
        ]),
        ..Default::default()
    };
    snap.apply_world_patch(&patch);
    assert_eq!(snap.discovered_routes.len(), 2);
}

// ============================================================================
// AC: State delta — compute_delta returns only changed fields
// AC: No-change delta — nothing changed → is_empty()
// (These ACs are already covered by story 1-8 tests, but we add
//  coverage for new fields: lore_established, active_stakes)
// ============================================================================

#[test]
fn state_delta_detects_lore_change() {
    let mut snap = test_snapshot();
    let before = snapshot(&snap);
    snap.lore_established.push("Ancient truth".to_string());
    let after = snapshot(&snap);

    let delta = compute_delta(&before, &after);
    assert!(!delta.is_empty(), "lore change should be detected");
}

#[test]
fn state_delta_detects_time_of_day_change() {
    let before = snapshot(&test_snapshot());
    let mut snap = test_snapshot();
    snap.time_of_day = "midnight".to_string();
    let after = snapshot(&snap);

    let delta = compute_delta(&before, &after);
    assert!(!delta.is_empty(), "time_of_day change should be detected");
}

// ============================================================================
// AC: Reactive messages — delta → correct GameMessage list
// ============================================================================

#[test]
fn broadcast_party_status_always_included() {
    let mut snap = test_snapshot();
    let before = snapshot(&snap);
    snap.characters[0].core.hp = 20;
    let after = snapshot(&snap);
    let delta = compute_delta(&before, &after);

    let messages = broadcast_state_changes(&delta, &snap);
    let has_party = messages
        .iter()
        .any(|m| matches!(m, sidequest_protocol::GameMessage::PartyStatus { .. }));
    assert!(
        has_party,
        "PARTY_STATUS should always be included after a turn"
    );
}

#[test]
fn broadcast_chapter_marker_on_location_change() {
    let mut snap = test_snapshot();
    let before = snapshot(&snap);
    snap.location = "The Wasteland Highway".to_string();
    let after = snapshot(&snap);
    let delta = compute_delta(&before, &after);

    let messages = broadcast_state_changes(&delta, &snap);
    let has_chapter = messages
        .iter()
        .any(|m| matches!(m, sidequest_protocol::GameMessage::ChapterMarker { .. }));
    assert!(
        has_chapter,
        "CHAPTER_MARKER should be sent on location change"
    );
}

#[test]
fn broadcast_map_update_on_region_discovery() {
    let mut snap = test_snapshot();
    let before = snapshot(&snap);
    snap.discovered_regions.push("toxic_marshes".to_string());
    let after = snapshot(&snap);
    let delta = compute_delta(&before, &after);

    let messages = broadcast_state_changes(&delta, &snap);
    let has_map = messages
        .iter()
        .any(|m| matches!(m, sidequest_protocol::GameMessage::MapUpdate { .. }));
    assert!(has_map, "MAP_UPDATE should be sent on region discovery");
}

#[test]
fn broadcast_combat_event_on_combat_change() {
    let mut snap = test_snapshot();
    let before = snapshot(&snap);
    snap.combat.advance_round();
    let after = snapshot(&snap);
    let delta = compute_delta(&before, &after);

    let messages = broadcast_state_changes(&delta, &snap);
    let has_combat = messages
        .iter()
        .any(|m| matches!(m, sidequest_protocol::GameMessage::CombatEvent { .. }));
    assert!(
        has_combat,
        "COMBAT_EVENT should be sent when combat state changes"
    );
}

#[test]
fn broadcast_no_chapter_marker_when_location_unchanged() {
    let snap = test_snapshot();
    let before = snapshot(&snap);
    let after = snapshot(&snap);
    let delta = compute_delta(&before, &after);

    let messages = broadcast_state_changes(&delta, &snap);
    let has_chapter = messages
        .iter()
        .any(|m| matches!(m, sidequest_protocol::GameMessage::ChapterMarker { .. }));
    assert!(!has_chapter, "No CHAPTER_MARKER when location unchanged");
}

// ============================================================================
// AC: Invalid patch rejected — deny_unknown_fields at parse time
// ============================================================================

#[test]
fn world_patch_rejects_unknown_fields() {
    let json = r#"{"location":"x","bogus_field":"y"}"#;
    let result = serde_json::from_str::<WorldStatePatch>(json);
    assert!(
        result.is_err(),
        "deny_unknown_fields should reject patches with unexpected keys"
    );
}

#[test]
fn combat_patch_rejects_unknown_fields() {
    let json = r#"{"in_combat":true,"bogus_field":"y"}"#;
    let result = serde_json::from_str::<CombatPatch>(json);
    assert!(
        result.is_err(),
        "deny_unknown_fields should reject patches with unexpected keys"
    );
}

#[test]
fn chase_patch_rejects_unknown_fields() {
    let json = r#"{"separation":3,"bogus_field":"y"}"#;
    let result = serde_json::from_str::<ChasePatch>(json);
    assert!(
        result.is_err(),
        "deny_unknown_fields should reject patches with unexpected keys"
    );
}

// ============================================================================
// Rule #1: Silent error swallowing — from_attitude_str handles unknowns
// ============================================================================

#[test]
fn disposition_from_attitude_str_unknown_returns_neutral() {
    // Unknown attitude strings should map to neutral, not panic or silently skip
    let disp = Disposition::from_attitude_str("ambivalent");
    assert!(
        disp.is_some(),
        "unknown attitude should still produce a disposition"
    );
    assert_eq!(disp.unwrap().attitude(), Attitude::Neutral);
}

// ============================================================================
// Rule #5: Validated constructors — NpcPatch name cannot be blank
// ============================================================================

#[test]
fn npc_patch_deserialize_rejects_blank_name() {
    let json = r#"{"name":"","description":"x"}"#;
    let result = serde_json::from_str::<NpcPatch>(json);
    assert!(
        result.is_err(),
        "NpcPatch should reject blank name at deserialization"
    );
}

// ============================================================================
// Rule #8: Deserialize bypass — patches use deny_unknown_fields
// ============================================================================

#[test]
fn world_patch_deserializes_with_all_none() {
    // Valid: empty JSON object should produce all-None patch
    let json = "{}";
    let patch: WorldStatePatch = serde_json::from_str(json).expect("empty object is valid");
    assert!(patch.location.is_none());
    assert!(patch.hp_changes.is_none());
    assert!(patch.npc_attitudes.is_none());
}

#[test]
fn world_patch_deserializes_with_partial_fields() {
    let json = r#"{"location":"Tavern","time_of_day":"noon"}"#;
    let patch: WorldStatePatch = serde_json::from_str(json).expect("partial fields valid");
    assert_eq!(patch.location.as_deref(), Some("Tavern"));
    assert_eq!(patch.time_of_day.as_deref(), Some("noon"));
    assert!(patch.hp_changes.is_none());
}

// ============================================================================
// Rule #6: Test quality self-check
// ============================================================================
// Every test above uses assert_eq!, assert!, or pattern matching.
// No `let _ =` patterns. No `assert!(true)`.
// No `is_none()` on always-None values — all None checks verify
// meaningful state (e.g., patch fields that default to None).

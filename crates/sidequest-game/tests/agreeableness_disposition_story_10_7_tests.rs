//! Story 10-7: Connect Agreeableness to disposition.
//!
//! Acceptance criteria:
//!   AC-1: Agreeableness influences initial disposition value
//!   AC-2: High A (>7) → positive disposition offset
//!   AC-3: Low A (<3) → negative disposition offset
//!   AC-4: Neutral A (5.0) → no offset (0)
//!   AC-5: NPCs without OCEAN → default disposition (no offset)
//!   AC-6: Existing disposition mechanics unaffected (offset is additive)

use sidequest_game::disposition::Disposition;
use sidequest_game::npc::Npc;
use sidequest_game::{OceanProfile};

// ─── Helper ──────────────────────────────────────────────

/// Build a minimal NPC with the given OCEAN agreeableness and starting disposition.
fn npc_with_agreeableness(agreeableness: f64, base_disposition: i32) -> Npc {
    use sidequest_game::creature_core::CreatureCore;
    use sidequest_game::inventory::Inventory;
    use sidequest_protocol::NonBlankString;

    let ocean = OceanProfile {
        agreeableness,
        ..OceanProfile::default()
    };

    Npc {
        core: CreatureCore {
            name: NonBlankString::new("Test NPC").unwrap(),
            description: NonBlankString::new("A test character").unwrap(),
            personality: NonBlankString::new("Bland").unwrap(),
            level: 1,
            hp: 10,
            max_hp: 10,
            ac: 10,
            xp: 0,            statuses: vec![],
            inventory: Inventory::default(),
        },
        voice_id: None,
        disposition: Disposition::new(base_disposition),
        location: None,
        pronouns: None,
        appearance: None,
        age: None,
        build: None,
        height: None,
        distinguishing_features: vec![],
        ocean: Some(ocean),
        belief_state: sidequest_game::belief_state::BeliefState::default(),
    }
}

/// Build an NPC with no OCEAN profile.
fn npc_without_ocean(base_disposition: i32) -> Npc {
    use sidequest_game::creature_core::CreatureCore;
    use sidequest_game::inventory::Inventory;
    use sidequest_protocol::NonBlankString;

    Npc {
        core: CreatureCore {
            name: NonBlankString::new("No-Ocean NPC").unwrap(),
            description: NonBlankString::new("A test character").unwrap(),
            personality: NonBlankString::new("Bland").unwrap(),
            level: 1,
            hp: 10,
            max_hp: 10,
            ac: 10,
            xp: 0,            statuses: vec![],
            inventory: Inventory::default(),
        },
        voice_id: None,
        disposition: Disposition::new(base_disposition),
        location: None,
        pronouns: None,
        appearance: None,
        age: None,
        build: None,
        height: None,
        distinguishing_features: vec![],
        ocean: None,
        belief_state: sidequest_game::belief_state::BeliefState::default(),
    }
}

// ─── AC-1: Agreeableness influences initial disposition ──

#[test]
fn agreeableness_offset_is_applied_to_disposition() {
    // A=8.0 → offset = (8.0 - 5.0) * 1.0 = +3
    let npc = npc_with_agreeableness(8.0, 0);
    let offset = npc.agreeableness_disposition_offset();
    assert_eq!(offset, 3, "A=8.0 should produce offset +3");
}

// ─── AC-2: High A (>7) → positive offset ────────────────

#[test]
fn high_agreeableness_gives_positive_offset() {
    let npc = npc_with_agreeableness(9.0, 0);
    let offset = npc.agreeableness_disposition_offset();
    assert!(offset > 0, "A=9.0 should give positive offset, got {offset}");
    assert_eq!(offset, 4);
}

#[test]
fn max_agreeableness_gives_max_offset() {
    let npc = npc_with_agreeableness(10.0, 0);
    let offset = npc.agreeableness_disposition_offset();
    assert_eq!(offset, 5, "A=10.0 should give offset +5");
}

// ─── AC-3: Low A (<3) → negative offset ─────────────────

#[test]
fn low_agreeableness_gives_negative_offset() {
    let npc = npc_with_agreeableness(2.0, 0);
    let offset = npc.agreeableness_disposition_offset();
    assert!(offset < 0, "A=2.0 should give negative offset, got {offset}");
    assert_eq!(offset, -3);
}

#[test]
fn min_agreeableness_gives_min_offset() {
    let npc = npc_with_agreeableness(0.0, 0);
    let offset = npc.agreeableness_disposition_offset();
    assert_eq!(offset, -5, "A=0.0 should give offset -5");
}

// ─── AC-4: Neutral A (5.0) → no offset ──────────────────

#[test]
fn neutral_agreeableness_gives_zero_offset() {
    let npc = npc_with_agreeableness(5.0, 0);
    let offset = npc.agreeableness_disposition_offset();
    assert_eq!(offset, 0, "A=5.0 should give zero offset");
}

// ─── AC-5: NPCs without OCEAN → no offset ───────────────

#[test]
fn no_ocean_profile_gives_zero_offset() {
    let npc = npc_without_ocean(0);
    let offset = npc.agreeableness_disposition_offset();
    assert_eq!(offset, 0, "NPC without OCEAN should have zero offset");
}

// ─── AC-6: Offset is additive, doesn't replace disposition ──

#[test]
fn offset_is_additive_to_existing_disposition() {
    // NPC starts at disposition +10, A=8.0 → offset +3 → effective = 13
    let npc = npc_with_agreeableness(8.0, 10);
    let effective = npc.effective_disposition();
    assert_eq!(
        effective, 13,
        "disposition 10 + A-offset 3 should give effective 13"
    );
}

#[test]
fn offset_does_not_replace_negative_disposition() {
    // NPC starts at disposition -10, A=2.0 → offset -3 → effective = -13
    let npc = npc_with_agreeableness(2.0, -10);
    let effective = npc.effective_disposition();
    assert_eq!(
        effective, -13,
        "disposition -10 + A-offset -3 should give effective -13"
    );
}

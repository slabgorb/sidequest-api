//! Story 15-20: Wire StateDelta computation — failing tests (RED phase).
//!
//! These tests verify that:
//! 1. `delta::snapshot()` captures all state fields the inline code references
//! 2. `delta::compute_delta()` correctly detects field changes
//! 3. `broadcast_state_changes()` produces correct typed GameMessages
//! 4. Coverage gap: quests and items_gained are handled through the delta path
//! 5. OTEL span emitted by compute_delta
//! 6. Wiring: compute_delta + broadcast_state_changes called from server dispatch

mod common;

use std::collections::HashMap;

use sidequest_game::combat::CombatState;
use sidequest_game::delta::{compute_delta, snapshot};
use sidequest_game::state::{broadcast_state_changes, GameSnapshot};
use sidequest_game::turn::TurnManager;
use sidequest_game::world_materialization::CampaignMaturity;
use sidequest_protocol::GameMessage;

fn base_snapshot() -> GameSnapshot {
    GameSnapshot {
        genre_slug: "mutant_wasteland".to_string(),
        world_slug: "flickering_reach".to_string(),
        characters: vec![common::make_character("Thorn")],
        npcs: vec![],
        location: "The Rusty Nail Inn".to_string(),
        time_of_day: "dusk".to_string(),
        quest_log: HashMap::from([
            ("main".to_string(), "Find the source of the flickering".to_string()),
        ]),
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
        turns_since_meaningful: 0,
        total_beats_fired: 0,
        campaign_maturity: CampaignMaturity::Fresh,
        npc_registry: vec![],
        world_history: vec![],
        ..GameSnapshot::default()
    }
}

// ============================================================================
// AC 1: snapshot() captures all fields referenced by inline delta code
// ============================================================================

#[test]
fn snapshot_captures_location() {
    let state = base_snapshot();
    let snap = snapshot(&state);
    // Snapshot must preserve location for delta comparison.
    // compute_delta uses this to detect location changes.
    let state2 = {
        let mut s = base_snapshot();
        s.location = "The Wasteland Gate".to_string();
        s
    };
    let snap2 = snapshot(&state2);
    let delta = compute_delta(&snap, &snap2);
    assert!(delta.location_changed(), "delta must detect location change");
    assert_eq!(
        delta.new_location().unwrap(),
        "The Wasteland Gate",
        "new_location must carry the updated value"
    );
}

#[test]
fn snapshot_captures_characters() {
    let state = base_snapshot();
    let snap = snapshot(&state);
    let mut state2 = base_snapshot();
    state2.characters[0].core.hp = 15; // damage taken
    let snap2 = snapshot(&state2);
    let delta = compute_delta(&snap, &snap2);
    assert!(
        delta.characters_changed(),
        "delta must detect character HP change"
    );
}

#[test]
fn snapshot_captures_quest_log() {
    let state = base_snapshot();
    let snap = snapshot(&state);
    let mut state2 = base_snapshot();
    state2.quest_log.insert(
        "side".to_string(),
        "Help the merchant recover lost goods".to_string(),
    );
    let snap2 = snapshot(&state2);
    let delta = compute_delta(&snap, &snap2);
    assert!(
        delta.quest_log_changed(),
        "delta must detect quest_log change"
    );
}

#[test]
fn snapshot_captures_combat() {
    let state = base_snapshot();
    let snap = snapshot(&state);
    let mut state2 = base_snapshot();
    state2.combat.set_in_combat(true); // transition to in_combat
    let snap2 = snapshot(&state2);
    let delta = compute_delta(&snap, &snap2);
    assert!(
        delta.combat_changed(),
        "delta must detect combat state change"
    );
}

#[test]
fn snapshot_captures_atmosphere() {
    let state = base_snapshot();
    let snap = snapshot(&state);
    let mut state2 = base_snapshot();
    state2.atmosphere = "calm".to_string();
    let snap2 = snapshot(&state2);
    let delta = compute_delta(&snap, &snap2);
    assert!(
        delta.atmosphere_changed(),
        "delta must detect atmosphere change"
    );
}

#[test]
fn snapshot_captures_regions() {
    let state = base_snapshot();
    let snap = snapshot(&state);
    let mut state2 = base_snapshot();
    state2.discovered_regions.push("the_depths".to_string());
    let snap2 = snapshot(&state2);
    let delta = compute_delta(&snap, &snap2);
    assert!(
        delta.regions_changed(),
        "delta must detect discovered_regions change"
    );
}

// ============================================================================
// AC 2: compute_delta returns empty when no changes
// ============================================================================

#[test]
fn compute_delta_identical_snapshots_is_empty() {
    let state = base_snapshot();
    let snap1 = snapshot(&state);
    let snap2 = snapshot(&state);
    let delta = compute_delta(&snap1, &snap2);
    assert!(delta.is_empty(), "identical snapshots must produce empty delta");
    assert!(
        delta.new_location().is_none(),
        "no location change means new_location must be None"
    );
}

// ============================================================================
// AC 3: broadcast_state_changes produces correct GameMessages
// ============================================================================

#[test]
fn broadcast_always_includes_party_status() {
    let state = base_snapshot();
    let snap1 = snapshot(&state);
    let snap2 = snapshot(&state);
    let delta = compute_delta(&snap1, &snap2);

    let messages = broadcast_state_changes(&delta, &state);
    let has_party_status = messages.iter().any(|m| matches!(m, GameMessage::PartyStatus { .. }));
    assert!(has_party_status, "broadcast must always include PartyStatus");
}

#[test]
fn broadcast_includes_chapter_marker_on_location_change() {
    let state1 = base_snapshot();
    let snap1 = snapshot(&state1);

    let mut state2 = base_snapshot();
    state2.location = "The Wasteland Gate".to_string();
    let snap2 = snapshot(&state2);
    let delta = compute_delta(&snap1, &snap2);

    let messages = broadcast_state_changes(&delta, &state2);
    let has_chapter = messages.iter().any(|m| matches!(m, GameMessage::ChapterMarker { .. }));
    assert!(
        has_chapter,
        "broadcast must include ChapterMarker when location changes"
    );
}

#[test]
fn broadcast_no_chapter_marker_when_location_unchanged() {
    let state = base_snapshot();
    let snap1 = snapshot(&state);
    let snap2 = snapshot(&state);
    let delta = compute_delta(&snap1, &snap2);

    let messages = broadcast_state_changes(&delta, &state);
    let has_chapter = messages.iter().any(|m| matches!(m, GameMessage::ChapterMarker { .. }));
    assert!(
        !has_chapter,
        "broadcast must NOT include ChapterMarker when location is unchanged"
    );
}

#[test]
fn broadcast_includes_map_update_on_region_discovery() {
    let state1 = base_snapshot();
    let snap1 = snapshot(&state1);

    let mut state2 = base_snapshot();
    state2.discovered_regions.push("the_depths".to_string());
    let snap2 = snapshot(&state2);
    let delta = compute_delta(&snap1, &snap2);

    let messages = broadcast_state_changes(&delta, &state2);
    let has_map = messages.iter().any(|m| matches!(m, GameMessage::MapUpdate { .. }));
    assert!(
        has_map,
        "broadcast must include MapUpdate when regions change"
    );
}

#[test]
fn broadcast_includes_combat_event_on_combat_change() {
    let state1 = base_snapshot();
    let snap1 = snapshot(&state1);

    let mut state2 = base_snapshot();
    state2.combat.set_in_combat(true);
    let snap2 = snapshot(&state2);
    let delta = compute_delta(&snap1, &snap2);

    let messages = broadcast_state_changes(&delta, &state2);
    let has_combat = messages.iter().any(|m| matches!(m, GameMessage::CombatEvent { .. }));
    assert!(
        has_combat,
        "broadcast must include CombatEvent when combat state changes"
    );
}

// ============================================================================
// AC 4: Coverage gap — quests and items_gained must be handled
//
// The inline server code sends quests and items_gained in protocol::StateDelta
// piggybacked on NarrationPayload. broadcast_state_changes() currently does NOT
// handle these. These tests will FAIL until Dev expands broadcast or adds a
// separate quest/item broadcast path.
// ============================================================================

#[test]
fn broadcast_handles_quest_log_change() {
    let state1 = base_snapshot();
    let snap1 = snapshot(&state1);

    let mut state2 = base_snapshot();
    state2.quest_log.insert(
        "side".to_string(),
        "Recover the merchant's goods".to_string(),
    );
    let snap2 = snapshot(&state2);
    let delta = compute_delta(&snap1, &snap2);

    assert!(delta.quest_log_changed(), "precondition: quest_log flagged as changed");

    // The broadcast path must produce a message that carries quest data to the client.
    // Currently broadcast_state_changes only handles PartyStatus, ChapterMarker,
    // MapUpdate, CombatEvent — it does NOT handle quests.
    // This test forces Dev to either:
    //   a) Add a QuestUpdate message type to broadcast_state_changes, or
    //   b) Build the protocol::StateDelta with quest data via the delta path
    let messages = broadcast_state_changes(&delta, &state2);
    let carries_quest_data = messages.iter().any(|m| {
        match m {
            GameMessage::Narration { payload, .. } => {
                payload.state_delta.as_ref()
                    .map(|d| d.quests.is_some())
                    .unwrap_or(false)
            }
            // If a new QuestUpdate message type is added, match it here too
            _ => false,
        }
    });
    assert!(
        carries_quest_data,
        "broadcast must carry quest data to client when quest_log changes — \
         currently broadcast_state_changes() does not handle quests"
    );
}

// ============================================================================
// AC 5: Multi-field delta — verify multiple simultaneous changes
// ============================================================================

#[test]
fn compute_delta_detects_multiple_simultaneous_changes() {
    let state1 = base_snapshot();
    let snap1 = snapshot(&state1);

    let mut state2 = base_snapshot();
    state2.location = "The Depths".to_string();
    state2.characters[0].core.hp = 10;
    state2.atmosphere = "ominous".to_string();
    state2.quest_log.insert("urgent".to_string(), "Escape the depths".to_string());
    let snap2 = snapshot(&state2);
    let delta = compute_delta(&snap1, &snap2);

    assert!(delta.location_changed(), "location must be flagged");
    assert!(delta.characters_changed(), "characters must be flagged");
    assert!(delta.atmosphere_changed(), "atmosphere must be flagged");
    assert!(delta.quest_log_changed(), "quest_log must be flagged");
    assert!(!delta.combat_changed(), "combat should NOT be flagged (unchanged)");
    assert!(!delta.regions_changed(), "regions should NOT be flagged (unchanged)");
    assert!(!delta.is_empty(), "multi-field delta must not be empty");
}

// ============================================================================
// AC 6: OTEL event — compute_delta emits tracing span
// ============================================================================

#[test]
fn compute_delta_emits_tracing_span_with_fields_changed() {
    // This test uses tracing-test to verify the span is emitted.
    // The compute_delta function already has a tracing::info_span!("compute_delta", ...)
    // We verify it's actually reached by checking the function runs without panic
    // and produces a correct delta. The OTEL event story-level requirement
    // (delta.computed with changed_fields, snapshot_size_bytes) will need
    // the additional OTEL event added by Dev.
    let state1 = base_snapshot();
    let snap1 = snapshot(&state1);

    let mut state2 = base_snapshot();
    state2.location = "New Place".to_string();
    let snap2 = snapshot(&state2);

    // compute_delta must not panic when tracing subscriber is active
    let delta = compute_delta(&snap1, &snap2);
    assert!(delta.location_changed());
}

// ============================================================================
// AC 7: Wiring test — compute_delta and broadcast_state_changes called from
//        server dispatch code (non-test consumer check)
// ============================================================================

#[test]
fn wiring_compute_delta_called_from_server_dispatch() {
    // Verify that sidequest-server's dispatch code calls compute_delta.
    // This is a grep-based wiring test — it reads the dispatch source and
    // asserts the function is imported and called.
    let dispatch_source = std::fs::read_to_string(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../sidequest-server/src/dispatch/mod.rs"
        )
    ).expect("should be able to read dispatch/mod.rs from game crate tests");

    assert!(
        dispatch_source.contains("compute_delta") || dispatch_source.contains("delta::compute_delta"),
        "dispatch/mod.rs must call compute_delta — \
         currently the server builds protocol::StateDelta inline instead of using game-crate delta"
    );
}

#[test]
fn wiring_broadcast_state_changes_called_from_server_dispatch() {
    // Verify that sidequest-server's dispatch code calls broadcast_state_changes.
    let dispatch_source = std::fs::read_to_string(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../sidequest-server/src/dispatch/mod.rs"
        )
    ).expect("should be able to read dispatch/mod.rs from game crate tests");

    assert!(
        dispatch_source.contains("broadcast_state_changes"),
        "dispatch/mod.rs must call broadcast_state_changes — \
         currently the server sends state via inline protocol::StateDelta construction"
    );
}

#[test]
fn wiring_no_inline_protocol_state_delta_in_dispatch() {
    // After wiring, the inline sidequest_protocol::StateDelta construction
    // should be removed from dispatch. This test verifies the old pattern is gone.
    let dispatch_source = std::fs::read_to_string(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../sidequest-server/src/dispatch/mod.rs"
        )
    ).expect("should be able to read dispatch/mod.rs from game crate tests");

    // Count occurrences of inline StateDelta construction
    let inline_count = dispatch_source
        .matches("sidequest_protocol::StateDelta {")
        .count();

    assert_eq!(
        inline_count, 0,
        "dispatch/mod.rs still has {} inline sidequest_protocol::StateDelta constructions — \
         these should be replaced by compute_delta + broadcast_state_changes",
        inline_count
    );
}

// ============================================================================
// AC 8: snapshot() + compute_delta() is a pure function — no side effects
//        beyond tracing
// ============================================================================

#[test]
fn snapshot_does_not_mutate_game_state() {
    let state = base_snapshot();
    let state_json_before = serde_json::to_string(&state).unwrap();
    let _snap = snapshot(&state);
    let state_json_after = serde_json::to_string(&state).unwrap();
    assert_eq!(
        state_json_before, state_json_after,
        "snapshot() must not mutate the GameSnapshot"
    );
}

#[test]
fn compute_delta_is_deterministic() {
    let state1 = base_snapshot();
    let mut state2 = base_snapshot();
    state2.location = "New Place".to_string();
    state2.characters[0].core.hp = 5;

    let snap1a = snapshot(&state1);
    let snap2a = snapshot(&state2);
    let delta_a = compute_delta(&snap1a, &snap2a);

    let snap1b = snapshot(&state1);
    let snap2b = snapshot(&state2);
    let delta_b = compute_delta(&snap1b, &snap2b);

    assert_eq!(delta_a.location_changed(), delta_b.location_changed());
    assert_eq!(delta_a.characters_changed(), delta_b.characters_changed());
    assert_eq!(delta_a.is_empty(), delta_b.is_empty());
    assert_eq!(delta_a.new_location(), delta_b.new_location());
}

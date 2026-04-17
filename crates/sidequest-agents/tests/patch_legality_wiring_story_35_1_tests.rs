//! Story 35-1 RED: Wire patch_legality into turn validator cold path.
//!
//! These tests verify that `run_validator()` in `turn_record.rs` calls
//! `run_legality_checks()` for every TurnRecord it receives, and emits
//! proper OTEL telemetry events via `WatcherEventBuilder`.
//!
//! Acceptance criteria:
//!   1. `run_legality_checks(&record)` is called inside `run_validator()`
//!   2. ValidationResult::Violation emits WatcherEventBuilder("patch_legality", ValidationWarning)
//!   3. Summary WatcherEventBuilder("patch_legality", SubsystemExerciseSummary) emitted per turn
//!   4. Integration: TurnRecord with HP-exceeding snapshot_after triggers violation OTEL event
//!   5. entity_reference::check_entity_references() is exercised transitively
//!
//! RED state: `run_validator()` currently only logs and collects turn_ids.
//! It does not call `run_legality_checks()` or emit WatcherEvents.

use std::collections::HashMap;

use chrono::Utc;
use tokio::sync::mpsc;

use serial_test::serial;
use sidequest_agents::agents::intent_router::Intent;
use sidequest_agents::turn_record::{run_validator, PatchSummary, TurnRecord};
use sidequest_game::{
    CreatureCore, Disposition, GameSnapshot, Inventory, Npc, StateDelta, TurnManager,
};
use sidequest_protocol::NonBlankString;

// ===========================================================================
// Test infrastructure: mock builders (reused from story 3-3 pattern)
// ===========================================================================

fn mock_game_snapshot() -> GameSnapshot {
    GameSnapshot {
        genre_slug: "mutant_wasteland".to_string(),
        world_slug: "flickering_reach".to_string(),
        characters: vec![],
        npcs: vec![],
        location: "The Rusty Valve".to_string(),
        time_of_day: "dusk".to_string(),
        quest_log: HashMap::new(),
        notes: vec![],
        narrative_log: vec![],
        encounter: None,
        active_tropes: vec![],
        atmosphere: "tense and electric".to_string(),
        current_region: "flickering_reach".to_string(),
        discovered_regions: vec!["flickering_reach".to_string()],
        discovered_routes: vec![],
        turn_manager: TurnManager::new(),
        last_saved_at: None,
        active_stakes: String::new(),
        lore_established: vec![],
        turns_since_meaningful: 0,
        ..GameSnapshot::default()
    }
}

fn mock_state_delta() -> StateDelta {
    serde_json::from_value(serde_json::json!({
        "characters": false,
        "npcs": false,
        "location": false,
        "time_of_day": false,
        "quest_log": false,
        "notes": false,
        "combat": false,
        "chase": false,
        "tropes": false,
        "atmosphere": false,
        "regions": false,
        "routes": false,
        "active_stakes": false,
        "lore": false,
        "new_location": null
    }))
    .expect("mock StateDelta should deserialize")
}

fn make_npc(name: &str, hp: i32, max_hp: i32, statuses: Vec<String>) -> Npc {
    Npc {
        core: CreatureCore {
            name: NonBlankString::new(name).unwrap(),
            description: NonBlankString::new("A test NPC").unwrap(),
            personality: NonBlankString::new("Stoic").unwrap(),
            level: 3,
            hp,
            max_hp,
            ac: 12,
            xp: 0,
            inventory: Inventory::default(),
            statuses,
        },
        voice_id: None,
        disposition: Disposition::new(0),
        pronouns: None,
        appearance: None,
        age: None,
        build: None,
        height: None,
        distinguishing_features: vec![],
        location: Some(NonBlankString::new("The Rusty Valve").unwrap()),
        ocean: None,
        belief_state: sidequest_game::belief_state::BeliefState::default(),
        resolution_tier: sidequest_game::npc::ResolutionTier::default(),
        non_transactional_interactions: 0,
        jungian_id: None,
        rpg_role_id: None,
        npc_role_id: None,
        resolved_archetype: None,
    }
}

fn make_mock_record(turn_id: u64) -> TurnRecord {
    TurnRecord {
        turn_id,
        timestamp: Utc::now(),
        player_input: "test action".to_string(),
        classified_intent: Intent::Exploration,
        agent_name: "narrator".to_string(),
        narration: "Test narration.".to_string(),
        patches_applied: vec![PatchSummary {
            patch_type: "world".to_string(),
            fields_changed: vec!["notes".to_string()],
        }],
        snapshot_before: mock_game_snapshot(),
        snapshot_after: mock_game_snapshot(),
        delta: mock_state_delta(),
        beats_fired: vec![],
        token_count_in: 500,
        token_count_out: 100,
        agent_duration_ms: 1200,
        is_degraded: false,
        spans: vec![],
        prompt_text: None,
        raw_response_text: None,
    }
}

// ===========================================================================
// Placeholder tests (will be replaced once run_validator wires legality checks)
// ===========================================================================

#[tokio::test]
#[serial]
async fn test_run_validator_receives_turn_records() {
    // Placeholder: Just verify that we can call run_validator without panic.
    // Real test will verify legality checks and OTEL events.

    let (_tx, rx) = mpsc::channel(100);

    // This should not panic even if no legality checks are wired yet.
    let result = run_validator(rx).await;
    assert_eq!(result.len(), 0); // No records sent, so empty result
}

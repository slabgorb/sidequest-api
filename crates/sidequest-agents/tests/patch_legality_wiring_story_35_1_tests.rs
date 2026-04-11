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
use sidequest_telemetry::{init_global_channel, subscribe_global, WatcherEvent, WatcherEventType};

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

/// Helper: drain all available WatcherEvents from a broadcast receiver.
fn drain_watcher_events(
    rx: &mut tokio::sync::broadcast::Receiver<WatcherEvent>,
) -> Vec<WatcherEvent> {
    let mut events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }
    events
}

// ===========================================================================
// AC1: run_legality_checks(&record) is called inside run_validator()
// ===========================================================================

/// A clean TurnRecord (no violations) should still be processed by the validator.
/// This confirms run_validator calls run_legality_checks — if it does, the summary
/// OTEL event will be emitted even for clean records.
///
/// RED: run_validator does not call run_legality_checks, so no summary event.
#[tokio::test]
#[serial]
async fn ac1_clean_record_emits_legality_summary() {
    let _tx = init_global_channel();
    let mut watcher_rx = subscribe_global().expect("channel initialized");

    let (tx, rx) = mpsc::channel(8);
    let record = make_mock_record(1);
    tx.send(record).await.unwrap();
    drop(tx);

    let processed = run_validator(rx).await;
    assert_eq!(processed, vec![1], "Turn 1 should be processed");

    let events = drain_watcher_events(&mut watcher_rx);
    let summaries: Vec<_> = events
        .iter()
        .filter(|e| {
            e.component == "patch_legality"
                && matches!(e.event_type, WatcherEventType::SubsystemExerciseSummary)
        })
        .collect();

    assert!(
        !summaries.is_empty(),
        "run_validator must emit a SubsystemExerciseSummary for patch_legality even on clean records, got events: {:?}",
        events.iter().map(|e| (&e.component, &e.event_type)).collect::<Vec<_>>()
    );
}

/// Multiple TurnRecords should each get their own legality check run.
///
/// RED: run_validator does not call run_legality_checks.
#[tokio::test]
#[serial]
async fn ac1_multiple_records_each_checked() {
    let _tx = init_global_channel();
    let mut watcher_rx = subscribe_global().expect("channel initialized");

    let (tx, rx) = mpsc::channel(8);
    tx.send(make_mock_record(1)).await.unwrap();
    tx.send(make_mock_record(2)).await.unwrap();
    tx.send(make_mock_record(3)).await.unwrap();
    drop(tx);

    let processed = run_validator(rx).await;
    assert_eq!(processed, vec![1, 2, 3]);

    let events = drain_watcher_events(&mut watcher_rx);
    let summary_count = events
        .iter()
        .filter(|e| {
            e.component == "patch_legality"
                && matches!(e.event_type, WatcherEventType::SubsystemExerciseSummary)
        })
        .count();

    assert_eq!(
        summary_count, 3,
        "Each of 3 TurnRecords must produce its own SubsystemExerciseSummary, got {}",
        summary_count
    );
}

// ===========================================================================
// AC2: ValidationResult::Violation emits WatcherEventBuilder("patch_legality", ValidationWarning)
// ===========================================================================

/// When a TurnRecord has an HP violation, run_validator must emit a
/// WatcherEvent with component="patch_legality" and type=ValidationWarning.
///
/// RED: run_validator does not call run_legality_checks or emit WatcherEvents.
#[tokio::test]
#[serial]
async fn ac2_violation_emits_validation_warning_event() {
    let _tx = init_global_channel();
    let mut watcher_rx = subscribe_global().expect("channel initialized");

    let (tx, rx) = mpsc::channel(8);
    let mut record = make_mock_record(1);
    // NPC with hp=25 > max_hp=20 — will trigger check_hp_bounds violation
    record.snapshot_after.npcs = vec![make_npc("Overhealbot", 25, 20, vec![])];
    tx.send(record).await.unwrap();
    drop(tx);

    run_validator(rx).await;

    let events = drain_watcher_events(&mut watcher_rx);
    let warnings: Vec<_> = events
        .iter()
        .filter(|e| {
            e.component == "patch_legality"
                && matches!(e.event_type, WatcherEventType::ValidationWarning)
        })
        .collect();

    assert!(
        !warnings.is_empty(),
        "HP violation must emit WatcherEvent(patch_legality, ValidationWarning), got events: {:?}",
        events
            .iter()
            .map(|e| (&e.component, &e.event_type))
            .collect::<Vec<_>>()
    );
}

/// The ValidationWarning event should contain the check name and violation text.
///
/// RED: no WatcherEvents emitted at all.
#[tokio::test]
#[serial]
async fn ac2_validation_warning_contains_check_name_and_text() {
    let _tx = init_global_channel();
    let mut watcher_rx = subscribe_global().expect("channel initialized");

    let (tx, rx) = mpsc::channel(8);
    let mut record = make_mock_record(1);
    record.snapshot_after.npcs = vec![make_npc("Overhealbot", 25, 20, vec![])];
    tx.send(record).await.unwrap();
    drop(tx);

    run_validator(rx).await;

    let events = drain_watcher_events(&mut watcher_rx);
    let warnings: Vec<_> = events
        .iter()
        .filter(|e| {
            e.component == "patch_legality"
                && matches!(e.event_type, WatcherEventType::ValidationWarning)
        })
        .collect();

    assert!(
        !warnings.is_empty(),
        "Must have at least one ValidationWarning"
    );

    let warning = &warnings[0];

    // Must contain "check" field identifying which legality check fired
    let check_field = warning
        .fields
        .get("check")
        .expect("ValidationWarning must have 'check' field");
    assert!(
        check_field.as_str().is_some(),
        "check field must be a string, got {:?}",
        check_field
    );

    // Must contain violation text
    let violation_field = warning
        .fields
        .get("violation")
        .expect("ValidationWarning must have 'violation' field with violation text");
    let text = violation_field
        .as_str()
        .expect("violation field must be a string");
    assert!(
        text.contains("HP") && text.contains("exceeds"),
        "Violation text should describe the HP bounds issue, got: {}",
        text
    );
}

// ===========================================================================
// AC3: Summary WatcherEventBuilder("patch_legality", SubsystemExerciseSummary)
// ===========================================================================

/// The summary event must include total checks run, warnings count, and violations count.
///
/// RED: no summary event emitted.
#[tokio::test]
#[serial]
async fn ac3_summary_contains_check_counts() {
    let _tx = init_global_channel();
    let mut watcher_rx = subscribe_global().expect("channel initialized");

    let (tx, rx) = mpsc::channel(8);
    let mut record = make_mock_record(1);
    // One violation (HP bounds) to verify the count appears in summary
    record.snapshot_after.npcs = vec![make_npc("Overhealbot", 25, 20, vec![])];
    tx.send(record).await.unwrap();
    drop(tx);

    run_validator(rx).await;

    let events = drain_watcher_events(&mut watcher_rx);
    let summaries: Vec<_> = events
        .iter()
        .filter(|e| {
            e.component == "patch_legality"
                && matches!(e.event_type, WatcherEventType::SubsystemExerciseSummary)
        })
        .collect();

    assert!(!summaries.is_empty(), "Must emit SubsystemExerciseSummary");

    let summary = &summaries[0];

    // Must report total number of checks run
    assert!(
        summary.fields.contains_key("total_checks"),
        "Summary must have 'total_checks' field, got fields: {:?}",
        summary.fields.keys().collect::<Vec<_>>()
    );

    // Must report violation count
    assert!(
        summary.fields.contains_key("violations"),
        "Summary must have 'violations' field, got fields: {:?}",
        summary.fields.keys().collect::<Vec<_>>()
    );

    // Must report warning count
    assert!(
        summary.fields.contains_key("warnings"),
        "Summary must have 'warnings' field, got fields: {:?}",
        summary.fields.keys().collect::<Vec<_>>()
    );

    // Violations count should be >= 1 (the HP bounds violation)
    let violations = summary.fields["violations"]
        .as_u64()
        .expect("violations must be a number");
    assert!(
        violations >= 1,
        "Should have at least 1 violation from HP bounds, got {}",
        violations
    );
}

/// Summary must include the turn_id it corresponds to.
///
/// RED: no summary event emitted.
#[tokio::test]
#[serial]
async fn ac3_summary_includes_turn_id() {
    let _tx = init_global_channel();
    let mut watcher_rx = subscribe_global().expect("channel initialized");

    let (tx, rx) = mpsc::channel(8);
    tx.send(make_mock_record(42)).await.unwrap();
    drop(tx);

    run_validator(rx).await;

    let events = drain_watcher_events(&mut watcher_rx);
    let summaries: Vec<_> = events
        .iter()
        .filter(|e| {
            e.component == "patch_legality"
                && matches!(e.event_type, WatcherEventType::SubsystemExerciseSummary)
        })
        .collect();

    assert!(!summaries.is_empty(), "Must emit SubsystemExerciseSummary");

    let turn_id = summaries[0]
        .fields
        .get("turn_id")
        .expect("Summary must include turn_id field");
    assert_eq!(
        turn_id.as_u64(),
        Some(42),
        "Summary turn_id must match the TurnRecord's turn_id"
    );
}

// ===========================================================================
// AC4: Integration — HP-exceeding snapshot_after triggers violation OTEL event
// ===========================================================================

/// End-to-end: send a TurnRecord with an HP violation through the full
/// run_validator pipeline and verify the violation WatcherEvent arrives.
///
/// This is the integration test — it exercises the complete path:
/// mpsc channel → run_validator → run_legality_checks → check_hp_bounds
/// → WatcherEventBuilder → global telemetry channel.
///
/// RED: run_validator does not call any legality checks.
#[tokio::test]
#[serial]
async fn ac4_integration_hp_violation_through_full_pipeline() {
    let _tx = init_global_channel();
    let mut watcher_rx = subscribe_global().expect("channel initialized");

    let (tx, rx) = mpsc::channel(8);

    // Build a record with clear HP violation
    let mut record = make_mock_record(1);
    record.snapshot_after.npcs = vec![
        make_npc("OverhealedGoblin", 30, 15, vec![]), // HP 30 > max 15
        make_npc("HealthyGuard", 10, 20, vec![]),     // Clean — no violation
    ];
    tx.send(record).await.unwrap();
    drop(tx);

    let processed = run_validator(rx).await;
    assert_eq!(processed, vec![1], "Turn must be processed");

    let events = drain_watcher_events(&mut watcher_rx);

    // Must have at least one ValidationWarning for the HP violation
    let violation_events: Vec<_> = events
        .iter()
        .filter(|e| {
            e.component == "patch_legality"
                && matches!(e.event_type, WatcherEventType::ValidationWarning)
        })
        .collect();

    assert!(
        !violation_events.is_empty(),
        "HP violation (30 > max 15) must produce a ValidationWarning WatcherEvent through the full pipeline"
    );

    // Must also have the summary
    let has_summary = events.iter().any(|e| {
        e.component == "patch_legality"
            && matches!(e.event_type, WatcherEventType::SubsystemExerciseSummary)
    });
    assert!(
        has_summary,
        "Full pipeline must also emit SubsystemExerciseSummary"
    );
}

/// Integration: dead NPC gaining HP should also be detected through run_validator.
///
/// RED: run_validator does not call run_legality_checks.
#[tokio::test]
#[serial]
async fn ac4_integration_dead_npc_revival_through_pipeline() {
    let _tx = init_global_channel();
    let mut watcher_rx = subscribe_global().expect("channel initialized");

    let (tx, rx) = mpsc::channel(8);

    let mut record = make_mock_record(1);
    // Dead NPC in snapshot_before (hp=0)
    record.snapshot_before.npcs = vec![make_npc("DeadBandit", 0, 20, vec!["dead".to_string()])];
    // Same NPC gained HP in snapshot_after — illegal revival
    record.snapshot_after.npcs = vec![make_npc("DeadBandit", 5, 20, vec![])];
    tx.send(record).await.unwrap();
    drop(tx);

    run_validator(rx).await;

    let events = drain_watcher_events(&mut watcher_rx);
    let violation_events: Vec<_> = events
        .iter()
        .filter(|e| {
            e.component == "patch_legality"
                && matches!(e.event_type, WatcherEventType::ValidationWarning)
        })
        .collect();

    assert!(
        !violation_events.is_empty(),
        "Dead NPC revival (hp 0 -> 5) must produce a ValidationWarning through run_validator"
    );
}

// ===========================================================================
// AC5: entity_reference::check_entity_references() exercised transitively
// ===========================================================================

/// Entity reference check is inside run_legality_checks (line 131 of patch_legality.rs).
/// If run_validator calls run_legality_checks, entity references are checked transitively.
/// A narration referencing a non-existent entity should produce a Warning-level event.
///
/// RED: run_validator does not call run_legality_checks.
#[tokio::test]
#[serial]
async fn ac5_entity_reference_check_exercised_through_validator() {
    let _tx = init_global_channel();
    let mut watcher_rx = subscribe_global().expect("channel initialized");

    let (tx, rx) = mpsc::channel(8);

    let mut record = make_mock_record(1);
    // Narration references "Grimjaw" who doesn't exist in game state.
    // The entity reference checker scans for capitalized words mid-sentence.
    record.narration = "The shadows shift as Grimjaw emerges from the ruins.".to_string();
    // No NPCs in snapshot_after — Grimjaw is unresolved
    record.snapshot_after.npcs = vec![];
    record.snapshot_after.characters = vec![];
    tx.send(record).await.unwrap();
    drop(tx);

    run_validator(rx).await;

    let events = drain_watcher_events(&mut watcher_rx);

    // The entity reference check produces ValidationResult::Warning, which should
    // be emitted as a WatcherEvent through run_validator's legality check wiring.
    // We check for either a ValidationWarning event or a field indicating entity_reference.
    let entity_ref_events: Vec<_> = events
        .iter()
        .filter(|e| {
            e.component == "patch_legality"
                && (matches!(e.event_type, WatcherEventType::ValidationWarning)
                    || matches!(e.event_type, WatcherEventType::SubsystemExerciseSummary))
        })
        .collect();

    assert!(
        !entity_ref_events.is_empty(),
        "Entity reference check must be exercised transitively through run_validator — \
         narration mentioning unknown 'Grimjaw' should produce telemetry events"
    );

    // The summary should show at least 1 warning (from entity_reference)
    let summaries: Vec<_> = events
        .iter()
        .filter(|e| {
            e.component == "patch_legality"
                && matches!(e.event_type, WatcherEventType::SubsystemExerciseSummary)
        })
        .collect();

    if !summaries.is_empty() {
        let warnings = summaries[0]
            .fields
            .get("warnings")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        assert!(
            warnings >= 1,
            "Summary should report at least 1 warning from entity_reference check, got {}",
            warnings
        );
    }
}

// ===========================================================================
// Wiring test: run_validator is the production consumer of run_legality_checks
// ===========================================================================

/// Wiring verification: run_validator must be the non-test consumer of
/// run_legality_checks. This test confirms the integration exists by
/// checking that the validator processes records AND produces telemetry —
/// something only possible if run_legality_checks is wired in.
///
/// RED: run_validator just logs, no legality check call.
#[tokio::test]
#[serial]
async fn wiring_run_validator_is_production_consumer_of_legality_checks() {
    let _tx = init_global_channel();
    let mut watcher_rx = subscribe_global().expect("channel initialized");

    let (tx, rx) = mpsc::channel(8);

    // Send two records: one clean, one with violation
    let clean_record = make_mock_record(1);
    let mut violation_record = make_mock_record(2);
    violation_record.snapshot_after.npcs = vec![make_npc("BrokenBot", 99, 10, vec![])];

    tx.send(clean_record).await.unwrap();
    tx.send(violation_record).await.unwrap();
    drop(tx);

    let processed = run_validator(rx).await;
    assert_eq!(processed, vec![1, 2]);

    let events = drain_watcher_events(&mut watcher_rx);

    // Must have exactly 2 summaries (one per turn)
    let summary_count = events
        .iter()
        .filter(|e| {
            e.component == "patch_legality"
                && matches!(e.event_type, WatcherEventType::SubsystemExerciseSummary)
        })
        .count();
    assert_eq!(
        summary_count, 2,
        "Must emit one SubsystemExerciseSummary per TurnRecord"
    );

    // Must have at least one ValidationWarning (from the violation record)
    let warning_count = events
        .iter()
        .filter(|e| {
            e.component == "patch_legality"
                && matches!(e.event_type, WatcherEventType::ValidationWarning)
        })
        .count();
    assert!(
        warning_count >= 1,
        "Violation record must produce at least one ValidationWarning, got {}",
        warning_count
    );
}

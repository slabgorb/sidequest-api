//! Story 3-3 RED: Patch legality checks — deterministic validation tests.
//!
//! Tests that the cold-path validator correctly detects:
//!   1. HP exceeding max_hp
//!   2. Dead NPCs acting or speaking
//!   3. Location transitions to undiscovered regions
//!   4. Combat patches without active combat
//!   5. Chase patches without active chase
//!
//! Each violation must emit a `tracing::warn!` with `component="watcher"`
//! and `check="patch_legality"`.
//!
//! RED state: All stubs return empty Vecs, so every assertion expecting
//! violations will fail. The Dev agent implements GREEN.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use tracing::Subscriber;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::Registry;

use sidequest_agents::agents::intent_router::Intent;
use sidequest_agents::patch_legality::{
    check_chase_coherence, check_combat_coherence, check_dead_entity_actions, check_hp_bounds,
    check_location_validity, run_legality_checks, ValidationResult,
};
use sidequest_agents::turn_record::{PatchSummary, TurnRecord};
use sidequest_game::{
    ChaseState, ChaseType, CombatState, CreatureCore, Disposition, GameSnapshot, Inventory, Npc,
    StateDelta, TurnManager,
};
use sidequest_protocol::NonBlankString;

// ===========================================================================
// Test infrastructure: mock builders
// ===========================================================================

/// Build a minimal GameSnapshot for testing.
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
        combat: CombatState::new(),
        chase: None,
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
    }
}

/// Build a mock StateDelta (all fields private, must go through serde).
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

/// Build an NPC with specified HP values.
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
            inventory: Inventory::default(),
            statuses,
        },
        voice_id: None,
        disposition: Disposition::new(0),
        pronouns: None,
        appearance: None,
        location: Some(NonBlankString::new("The Rusty Valve").unwrap()),
        ocean: None,
    }
}

/// Build a mock TurnRecord with customizable snapshots.
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
        extraction_tier: 1,
        token_count_in: 500,
        token_count_out: 100,
        agent_duration_ms: 1200,
        is_degraded: false,
    }
}

// ===========================================================================
// Tracing capture infrastructure
// ===========================================================================

/// A captured tracing event with field name-value pairs.
#[derive(Debug, Clone)]
struct CapturedEvent {
    fields: Vec<(String, String)>,
    level: tracing::Level,
}

impl CapturedEvent {
    fn field_value(&self, name: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

/// Layer that captures tracing events for assertion.
struct EventCaptureLayer {
    captured: Arc<Mutex<Vec<CapturedEvent>>>,
}

impl EventCaptureLayer {
    fn new() -> (Self, Arc<Mutex<Vec<CapturedEvent>>>) {
        let captured = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                captured: captured.clone(),
            },
            captured,
        )
    }
}

impl<S: Subscriber> tracing_subscriber::Layer<S> for EventCaptureLayer {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut fields = Vec::new();
        let mut visitor = EventFieldVisitor(&mut fields);
        event.record(&mut visitor);

        self.captured.lock().unwrap().push(CapturedEvent {
            fields,
            level: *event.metadata().level(),
        });
    }
}

/// Visitor that collects event field name-value pairs.
struct EventFieldVisitor<'a>(&'a mut Vec<(String, String)>);

impl<'a> tracing::field::Visit for EventFieldVisitor<'a> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.0
            .push((field.name().to_string(), format!("{:?}", value)));
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.0.push((field.name().to_string(), value.to_string()));
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.0.push((field.name().to_string(), value.to_string()));
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.0.push((field.name().to_string(), value.to_string()));
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.0.push((field.name().to_string(), value.to_string()));
    }
}

/// Helper: find captured WARN events with both component="watcher" and check="patch_legality".
fn legality_warnings(events: &[CapturedEvent]) -> Vec<&CapturedEvent> {
    events
        .iter()
        .filter(|e| {
            e.level == tracing::Level::WARN
                && e.field_value("component") == Some("watcher")
                && e.field_value("check") == Some("patch_legality")
        })
        .collect()
}

// ===========================================================================
// AC1: HP cannot exceed max_hp
// ===========================================================================

/// An NPC whose HP exceeds max_hp in snapshot_after must produce a Violation.
///
/// RED: check_hp_bounds stub returns empty Vec.
#[test]
fn hp_exceeding_max_hp_is_violation() {
    let mut record = make_mock_record(1);
    // NPC with hp=25 but max_hp=20 — illegal overheal
    record.snapshot_after.npcs = vec![make_npc("Overhealbot", 25, 20, vec![])];

    let results = check_hp_bounds(&record);

    let violations: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Violation(_)))
        .collect();

    assert!(
        !violations.is_empty(),
        "HP 25 > max_hp 20 should produce a Violation, got {:?}",
        results
    );
}

/// Multiple NPCs with HP violations should each produce their own Violation.
///
/// RED: stub returns empty.
#[test]
fn multiple_hp_violations_reported_individually() {
    let mut record = make_mock_record(1);
    record.snapshot_after.npcs = vec![
        make_npc("Overhealbot", 25, 20, vec![]),
        make_npc("Also Overhealed", 50, 30, vec![]),
    ];

    let results = check_hp_bounds(&record);

    let violations: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Violation(_)))
        .collect();

    assert!(
        violations.len() >= 2,
        "Two NPCs with HP > max_hp should produce at least 2 violations, got {}",
        violations.len()
    );
}

/// HP exactly equal to max_hp is legal — boundary case, no violation.
#[test]
fn hp_at_max_hp_is_clean() {
    let mut record = make_mock_record(1);
    record.snapshot_after.npcs = vec![make_npc("Full Health", 20, 20, vec![])];

    let results = check_hp_bounds(&record);

    let violations: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Violation(_)))
        .collect();

    assert!(
        violations.is_empty(),
        "HP == max_hp should be clean, got violations: {:?}",
        violations
    );
}

/// HP below max_hp is legal — no violation.
#[test]
fn hp_below_max_hp_is_clean() {
    let mut record = make_mock_record(1);
    record.snapshot_after.npcs = vec![make_npc("Wounded", 10, 20, vec![])];

    let results = check_hp_bounds(&record);

    let violations: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Violation(_)))
        .collect();

    assert!(violations.is_empty(), "HP 10 < max_hp 20 should be clean");
}

/// The violation message should identify the offending creature by name.
///
/// RED: stub returns empty.
#[test]
fn hp_violation_identifies_creature_name() {
    let mut record = make_mock_record(1);
    record.snapshot_after.npcs = vec![make_npc("Razortooth", 30, 20, vec![])];

    let results = check_hp_bounds(&record);

    let violation_msgs: Vec<_> = results
        .iter()
        .filter_map(|r| match r {
            ValidationResult::Violation(msg) => Some(msg.as_str()),
            _ => None,
        })
        .collect();

    assert!(
        !violation_msgs.is_empty(),
        "Should have a violation for Razortooth"
    );
    assert!(
        violation_msgs.iter().any(|msg| msg.contains("Razortooth")),
        "Violation message should name the creature, got: {:?}",
        violation_msgs
    );
}

// ===========================================================================
// AC2: Dead NPCs cannot act or speak
// ===========================================================================

/// A dead NPC (hp == 0) whose HP increases in snapshot_after is a violation.
/// Dead things don't spontaneously heal.
///
/// RED: stub returns empty.
#[test]
fn dead_npc_gaining_hp_is_violation() {
    let mut record = make_mock_record(1);
    // snapshot_before: dead NPC (hp=0)
    record.snapshot_before.npcs = vec![make_npc("Corpse Guy", 0, 20, vec!["dead".to_string()])];
    // snapshot_after: same NPC somehow gained HP
    record.snapshot_after.npcs = vec![make_npc("Corpse Guy", 5, 20, vec![])];

    let results = check_dead_entity_actions(&record);

    let violations: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Violation(_)))
        .collect();

    assert!(
        !violations.is_empty(),
        "Dead NPC gaining HP should be a violation, got {:?}",
        results
    );
}

/// A dead NPC (has "dead" status) that appears in patches_applied is a violation.
///
/// RED: stub returns empty.
#[test]
fn dead_npc_with_dead_status_acting_is_violation() {
    let mut record = make_mock_record(1);
    record
        .snapshot_before
        .npcs
        .push(make_npc("Ghost Actor", 0, 15, vec!["dead".to_string()]));
    // NPC still in after snapshot with same dead status but different location
    let mut after_npc = make_npc("Ghost Actor", 0, 15, vec!["dead".to_string()]);
    after_npc.location = Some(NonBlankString::new("The Void").unwrap());
    record.snapshot_after.npcs.push(after_npc);

    let results = check_dead_entity_actions(&record);

    let violations: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Violation(_)))
        .collect();

    assert!(
        !violations.is_empty(),
        "Dead NPC changing location should be a violation"
    );
}

/// A living NPC taking actions is clean — no violation.
#[test]
fn living_npc_acting_is_clean() {
    let mut record = make_mock_record(1);
    record.snapshot_before.npcs = vec![make_npc("Alive Guy", 15, 20, vec![])];
    record.snapshot_after.npcs = vec![make_npc("Alive Guy", 10, 20, vec![])];

    let results = check_dead_entity_actions(&record);

    let violations: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Violation(_)))
        .collect();

    assert!(
        violations.is_empty(),
        "Living NPC acting should be clean, got violations: {:?}",
        violations
    );
}

/// Dead NPC violation message should identify the NPC by name.
///
/// RED: stub returns empty.
#[test]
fn dead_npc_violation_identifies_creature() {
    let mut record = make_mock_record(1);
    record.snapshot_before.npcs = vec![make_npc("Skeleton Bob", 0, 20, vec!["dead".to_string()])];
    record.snapshot_after.npcs = vec![make_npc("Skeleton Bob", 10, 20, vec![])];

    let results = check_dead_entity_actions(&record);

    let violation_msgs: Vec<_> = results
        .iter()
        .filter_map(|r| match r {
            ValidationResult::Violation(msg) => Some(msg.as_str()),
            _ => None,
        })
        .collect();

    assert!(
        !violation_msgs.is_empty(),
        "Should have a violation for Skeleton Bob"
    );
    assert!(
        violation_msgs
            .iter()
            .any(|msg| msg.contains("Skeleton Bob")),
        "Violation should name the dead NPC, got: {:?}",
        violation_msgs
    );
}

// ===========================================================================
// AC3: Location transitions must be to discovered regions
// ===========================================================================

/// Moving to a region not in discovered_regions is a violation.
///
/// RED: stub returns empty.
#[test]
fn location_to_undiscovered_region_is_violation() {
    let mut record = make_mock_record(1);
    record.snapshot_before.current_region = "flickering_reach".to_string();
    record.snapshot_before.discovered_regions = vec!["flickering_reach".to_string()];

    // After: moved to undiscovered region
    record.snapshot_after.current_region = "bone_wastes".to_string();
    record.snapshot_after.discovered_regions = vec!["flickering_reach".to_string()];

    let results = check_location_validity(&record);

    let violations: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Violation(_)))
        .collect();

    assert!(
        !violations.is_empty(),
        "Moving to undiscovered region 'bone_wastes' should be a violation, got {:?}",
        results
    );
}

/// Moving to a discovered region is clean.
#[test]
fn location_to_discovered_region_is_clean() {
    let mut record = make_mock_record(1);
    record.snapshot_before.current_region = "flickering_reach".to_string();
    record.snapshot_before.discovered_regions =
        vec!["flickering_reach".to_string(), "bone_wastes".to_string()];

    record.snapshot_after.current_region = "bone_wastes".to_string();
    record.snapshot_after.discovered_regions =
        vec!["flickering_reach".to_string(), "bone_wastes".to_string()];

    let results = check_location_validity(&record);

    let violations: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Violation(_)))
        .collect();

    assert!(
        violations.is_empty(),
        "Moving to discovered region should be clean"
    );
}

/// Staying in the same region is clean — no violation.
#[test]
fn staying_in_same_region_is_clean() {
    let mut record = make_mock_record(1);
    record.snapshot_before.current_region = "flickering_reach".to_string();
    record.snapshot_after.current_region = "flickering_reach".to_string();

    let results = check_location_validity(&record);

    let violations: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Violation(_)))
        .collect();

    assert!(
        violations.is_empty(),
        "Staying in the same region should be clean"
    );
}

/// Violation message should name the undiscovered region.
///
/// RED: stub returns empty.
#[test]
fn location_violation_names_undiscovered_region() {
    let mut record = make_mock_record(1);
    record.snapshot_after.current_region = "shadow_fen".to_string();
    record.snapshot_after.discovered_regions = vec!["flickering_reach".to_string()];

    let results = check_location_validity(&record);

    let violation_msgs: Vec<_> = results
        .iter()
        .filter_map(|r| match r {
            ValidationResult::Violation(msg) => Some(msg.as_str()),
            _ => None,
        })
        .collect();

    assert!(
        !violation_msgs.is_empty(),
        "Should have a violation for shadow_fen"
    );
    assert!(
        violation_msgs.iter().any(|msg| msg.contains("shadow_fen")),
        "Violation should name the undiscovered region, got: {:?}",
        violation_msgs
    );
}

// ===========================================================================
// AC4: Combat patches require active combat state
// ===========================================================================

/// A combat patch applied when combat is not active (round 1, no damage log)
/// should produce a violation.
///
/// RED: stub returns empty.
#[test]
fn combat_patch_without_active_combat_is_violation() {
    let mut record = make_mock_record(1);
    // Combat is at default state (round 1, empty damage_log) — not active
    record.snapshot_before.combat = CombatState::new();

    // But a combat-type patch was applied
    record.patches_applied = vec![PatchSummary {
        patch_type: "combat".to_string(),
        fields_changed: vec!["round".to_string()],
    }];

    let results = check_combat_coherence(&record);

    let violations: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Violation(_)))
        .collect();

    assert!(
        !violations.is_empty(),
        "Combat patch without active combat should be a violation, got {:?}",
        results
    );
}

/// A combat patch applied when combat IS active (round > 1) is clean.
#[test]
fn combat_patch_with_active_combat_is_clean() {
    let mut record = make_mock_record(1);
    let mut combat = CombatState::new();
    combat.advance_round(); // now round 2 — combat is active
    record.snapshot_before.combat = combat;

    record.patches_applied = vec![PatchSummary {
        patch_type: "combat".to_string(),
        fields_changed: vec!["round".to_string()],
    }];

    let results = check_combat_coherence(&record);

    let violations: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Violation(_)))
        .collect();

    assert!(
        violations.is_empty(),
        "Combat patch with active combat (round 2) should be clean"
    );
}

/// A world patch applied when combat is not active is fine — not a combat patch.
#[test]
fn non_combat_patch_without_active_combat_is_clean() {
    let mut record = make_mock_record(1);
    record.snapshot_before.combat = CombatState::new();

    record.patches_applied = vec![PatchSummary {
        patch_type: "world".to_string(),
        fields_changed: vec!["atmosphere".to_string()],
    }];

    let results = check_combat_coherence(&record);

    let violations: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Violation(_)))
        .collect();

    assert!(
        violations.is_empty(),
        "World patch without combat should be clean"
    );
}

/// A combat patch with damage already logged (active combat) is clean.
#[test]
fn combat_patch_with_damage_log_is_clean() {
    let mut record = make_mock_record(1);
    let mut combat = CombatState::new();
    combat.log_damage(sidequest_game::DamageEvent {
        attacker: "Player".to_string(),
        target: "Goblin".to_string(),
        damage: 5,
        round: 1,
    });
    record.snapshot_before.combat = combat;

    record.patches_applied = vec![PatchSummary {
        patch_type: "combat".to_string(),
        fields_changed: vec!["damage_log".to_string()],
    }];

    let results = check_combat_coherence(&record);

    let violations: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Violation(_)))
        .collect();

    assert!(
        violations.is_empty(),
        "Combat patch with active damage log should be clean"
    );
}

// ===========================================================================
// AC5: Chase patches require active chase state
// ===========================================================================

/// A chase patch applied when no chase is active (chase == None) is a violation.
///
/// RED: stub returns empty.
#[test]
fn chase_patch_without_active_chase_is_violation() {
    let mut record = make_mock_record(1);
    record.snapshot_before.chase = None; // no active chase

    record.patches_applied = vec![PatchSummary {
        patch_type: "chase".to_string(),
        fields_changed: vec!["escape_roll".to_string()],
    }];

    let results = check_chase_coherence(&record);

    let violations: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Violation(_)))
        .collect();

    assert!(
        !violations.is_empty(),
        "Chase patch without active chase should be a violation, got {:?}",
        results
    );
}

/// A chase patch applied when a chase IS active is clean.
#[test]
fn chase_patch_with_active_chase_is_clean() {
    let mut record = make_mock_record(1);
    record.snapshot_before.chase = Some(ChaseState::new(ChaseType::Footrace, 0.5));

    record.patches_applied = vec![PatchSummary {
        patch_type: "chase".to_string(),
        fields_changed: vec!["escape_roll".to_string()],
    }];

    let results = check_chase_coherence(&record);

    let violations: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Violation(_)))
        .collect();

    assert!(
        violations.is_empty(),
        "Chase patch with active chase should be clean"
    );
}

/// A world patch when chase is None is fine — not a chase patch.
#[test]
fn non_chase_patch_without_chase_is_clean() {
    let mut record = make_mock_record(1);
    record.snapshot_before.chase = None;

    record.patches_applied = vec![PatchSummary {
        patch_type: "world".to_string(),
        fields_changed: vec!["location".to_string()],
    }];

    let results = check_chase_coherence(&record);

    let violations: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Violation(_)))
        .collect();

    assert!(
        violations.is_empty(),
        "World patch without chase should be clean"
    );
}

// ===========================================================================
// Runner: run_legality_checks aggregates all checks
// ===========================================================================

/// run_legality_checks must detect HP violations from check_hp_bounds.
///
/// RED: stub returns empty.
#[test]
fn runner_detects_hp_violations() {
    let mut record = make_mock_record(1);
    record.snapshot_after.npcs = vec![make_npc("Overhealbot", 25, 20, vec![])];

    let results = run_legality_checks(&record);

    let violations: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Violation(_)))
        .collect();

    assert!(
        !violations.is_empty(),
        "run_legality_checks should aggregate HP violations, got {:?}",
        results
    );
}

/// run_legality_checks must detect chase coherence violations.
///
/// RED: stub returns empty.
#[test]
fn runner_detects_chase_violations() {
    let mut record = make_mock_record(1);
    record.snapshot_before.chase = None;
    record.patches_applied = vec![PatchSummary {
        patch_type: "chase".to_string(),
        fields_changed: vec!["roll".to_string()],
    }];

    let results = run_legality_checks(&record);

    let violations: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Violation(_)))
        .collect();

    assert!(
        !violations.is_empty(),
        "run_legality_checks should aggregate chase violations"
    );
}

/// A clean record (no violations) should produce only Ok results.
#[test]
fn runner_clean_record_produces_no_violations() {
    let record = make_mock_record(1);

    let results = run_legality_checks(&record);

    let violations: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Violation(_)))
        .collect();

    assert!(
        violations.is_empty(),
        "Clean record should produce no violations, got {:?}",
        violations
    );
}

// ===========================================================================
// Tracing: violations emit warn! with component="watcher", check="patch_legality"
// ===========================================================================

/// HP violations must emit tracing::warn! with component="watcher" and
/// check="patch_legality".
///
/// RED: stub doesn't emit tracing events.
#[test]
fn hp_violation_emits_tracing_warn() {
    let (layer, captured) = EventCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    let mut record = make_mock_record(1);
    record.snapshot_after.npcs = vec![make_npc("Overhealbot", 25, 20, vec![])];

    tracing::subscriber::with_default(subscriber, || {
        run_legality_checks(&record);
    });

    let events = captured.lock().unwrap();
    let warnings = legality_warnings(&events);

    assert!(
        !warnings.is_empty(),
        "HP violation should emit tracing::warn! with component=watcher and \
         check=patch_legality, got {} total events",
        events.len()
    );
}

/// Chase coherence violation must emit tracing::warn! with the correct tags.
///
/// RED: stub doesn't emit tracing events.
#[test]
fn chase_violation_emits_tracing_warn() {
    let (layer, captured) = EventCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    let mut record = make_mock_record(1);
    record.snapshot_before.chase = None;
    record.patches_applied = vec![PatchSummary {
        patch_type: "chase".to_string(),
        fields_changed: vec!["roll".to_string()],
    }];

    tracing::subscriber::with_default(subscriber, || {
        run_legality_checks(&record);
    });

    let events = captured.lock().unwrap();
    let warnings = legality_warnings(&events);

    assert!(
        !warnings.is_empty(),
        "Chase violation should emit tracing::warn! with component=watcher, \
         check=patch_legality"
    );
}

/// Combat coherence violation must emit tracing::warn! with the correct tags.
///
/// RED: stub doesn't emit tracing events.
#[test]
fn combat_violation_emits_tracing_warn() {
    let (layer, captured) = EventCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    let mut record = make_mock_record(1);
    record.snapshot_before.combat = CombatState::new();
    record.patches_applied = vec![PatchSummary {
        patch_type: "combat".to_string(),
        fields_changed: vec!["round".to_string()],
    }];

    tracing::subscriber::with_default(subscriber, || {
        run_legality_checks(&record);
    });

    let events = captured.lock().unwrap();
    let warnings = legality_warnings(&events);

    assert!(
        !warnings.is_empty(),
        "Combat violation should emit tracing::warn! with component=watcher, \
         check=patch_legality"
    );
}

/// Dead NPC violation must emit tracing::warn! with the correct tags.
///
/// RED: stub doesn't emit tracing events.
#[test]
fn dead_npc_violation_emits_tracing_warn() {
    let (layer, captured) = EventCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    let mut record = make_mock_record(1);
    record.snapshot_before.npcs = vec![make_npc("Corpse", 0, 20, vec!["dead".to_string()])];
    record.snapshot_after.npcs = vec![make_npc("Corpse", 10, 20, vec![])];

    tracing::subscriber::with_default(subscriber, || {
        run_legality_checks(&record);
    });

    let events = captured.lock().unwrap();
    let warnings = legality_warnings(&events);

    assert!(
        !warnings.is_empty(),
        "Dead NPC violation should emit tracing::warn! with component=watcher, \
         check=patch_legality"
    );
}

/// Location violation must emit tracing::warn! with the correct tags.
///
/// RED: stub returns empty.
#[test]
fn location_violation_emits_tracing_warn() {
    let (layer, captured) = EventCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    let mut record = make_mock_record(1);
    record.snapshot_after.current_region = "bone_wastes".to_string();
    record.snapshot_after.discovered_regions = vec!["flickering_reach".to_string()];

    tracing::subscriber::with_default(subscriber, || {
        run_legality_checks(&record);
    });

    let events = captured.lock().unwrap();
    let warnings = legality_warnings(&events);

    assert!(
        !warnings.is_empty(),
        "Location violation should emit tracing::warn! with component=watcher, \
         check=patch_legality"
    );
}

/// A clean record should produce NO tracing::warn! events.
#[test]
fn clean_record_emits_no_warnings() {
    let (layer, captured) = EventCaptureLayer::new();
    let subscriber = Registry::default().with(layer);

    let record = make_mock_record(1);

    tracing::subscriber::with_default(subscriber, || {
        run_legality_checks(&record);
    });

    let events = captured.lock().unwrap();
    let warnings = legality_warnings(&events);

    assert!(
        warnings.is_empty(),
        "Clean record should produce no legality warnings, got {}",
        warnings.len()
    );
}

// ===========================================================================
// Edge cases
// ===========================================================================

/// A record with no NPCs at all should produce no violations from any check.
#[test]
fn empty_npc_list_is_clean() {
    let mut record = make_mock_record(1);
    record.snapshot_before.npcs = vec![];
    record.snapshot_after.npcs = vec![];

    let hp_results = check_hp_bounds(&record);
    let dead_results = check_dead_entity_actions(&record);

    let hp_violations: Vec<_> = hp_results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Violation(_)))
        .collect();
    let dead_violations: Vec<_> = dead_results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Violation(_)))
        .collect();

    assert!(hp_violations.is_empty(), "No NPCs means no HP violations");
    assert!(
        dead_violations.is_empty(),
        "No NPCs means no dead-entity violations"
    );
}

/// No patches applied should produce no combat or chase violations.
#[test]
fn no_patches_is_clean_for_combat_and_chase() {
    let mut record = make_mock_record(1);
    record.patches_applied = vec![];
    record.snapshot_before.combat = CombatState::new();
    record.snapshot_before.chase = None;

    let combat_results = check_combat_coherence(&record);
    let chase_results = check_chase_coherence(&record);

    let combat_violations: Vec<_> = combat_results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Violation(_)))
        .collect();
    let chase_violations: Vec<_> = chase_results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Violation(_)))
        .collect();

    assert!(
        combat_violations.is_empty(),
        "No patches means no combat violations"
    );
    assert!(
        chase_violations.is_empty(),
        "No patches means no chase violations"
    );
}

/// Multiple violation types in one record should all be captured by the runner.
///
/// RED: stub returns empty.
#[test]
fn multiple_violation_types_aggregated() {
    let mut record = make_mock_record(1);
    // HP violation
    record.snapshot_after.npcs = vec![make_npc("Overhealbot", 25, 20, vec![])];
    // Chase violation
    record.snapshot_before.chase = None;
    record.patches_applied = vec![PatchSummary {
        patch_type: "chase".to_string(),
        fields_changed: vec!["roll".to_string()],
    }];
    // Location violation
    record.snapshot_after.current_region = "bone_wastes".to_string();
    record.snapshot_after.discovered_regions = vec!["flickering_reach".to_string()];

    let results = run_legality_checks(&record);

    let violations: Vec<_> = results
        .iter()
        .filter(|r| matches!(r, ValidationResult::Violation(_)))
        .collect();

    assert!(
        violations.len() >= 3,
        "Record with HP + chase + location violations should produce at least 3 violations, \
         got {}",
        violations.len()
    );
}

// ===========================================================================
// ValidationResult type contract
// ===========================================================================

/// ValidationResult must implement Debug, Clone, PartialEq, Eq.
#[test]
fn validation_result_derives() {
    let ok = ValidationResult::Ok;
    let warning = ValidationResult::Warning("test warning".to_string());
    let violation = ValidationResult::Violation("test violation".to_string());

    // Clone
    let ok_clone = ok.clone();
    let warning_clone = warning.clone();

    // PartialEq
    assert_eq!(ok, ok_clone);
    assert_eq!(warning, warning_clone);
    assert_ne!(ok, violation);

    // Debug
    let debug_str = format!("{:?}", violation);
    assert!(
        debug_str.contains("Violation"),
        "Debug output should contain variant name"
    );
}

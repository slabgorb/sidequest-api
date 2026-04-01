//! Story 16-5: Migrate chase as confrontation
//!
//! RED phase — ChaseState becomes a confrontation type preset.
//! All existing chase mechanics (separation, escape threshold, chase beats,
//! RigStats, fuel, terrain modifiers, multi-actor roles) are expressed
//! as a StructuredEncounter with type="chase".
//!
//! Key mappings:
//!   - separation_distance → metric.current (name="separation", Ascending)
//!   - goal → metric.threshold_high
//!   - rig → SecondaryStats via from_rig_stats()
//!   - actors → EncounterActor with string roles
//!   - beat → beat (direct)
//!   - structured_phase → EncounterPhase mapping
//!   - outcome → outcome as string
//!   - !is_resolved() → !resolved
//!
//! ACs:
//!   AC-1: All chase tests pass via StructuredEncounter
//!   AC-2: All chase_depth tests pass
//!   AC-3: Rig stats expressible through SecondaryStats
//!   AC-4: Cinematography — camera modes, sentence ranges, format_chase_context
//!   AC-5: Terrain — terrain modifiers, danger escalation
//!   AC-6: Beat system — advancement, phase transitions, outcome checking
//!   AC-7: Server wiring — shared_session.rs chase references compile

use serde_json;

use sidequest_game::chase::{ChaseState, ChaseType};
use sidequest_game::chase_depth::{
    ChaseActor, ChaseBeat, ChaseOutcome, ChasePhase, ChaseRole, BeatDecision,
    RigStats, RigType, RigDamageTier,
    check_outcome, phase_for_beat, danger_for_beat, terrain_modifiers,
    apply_terrain_to_rig, camera_for_phase, cinematography_for_phase,
    format_chase_context, CameraMode,
};
use sidequest_game::encounter::{
    EncounterPhase, MetricDirection, SecondaryStats, StructuredEncounter,
};

// ==========================================================================
// AC-1: StructuredEncounter::chase() constructor and basic chase semantics
// ==========================================================================

/// chase() constructor should create a chase-type encounter with
/// separation as an ascending metric toward the escape goal.
#[test]
fn chase_encounter_constructor_creates_chase_type() {
    let encounter = StructuredEncounter::chase(0.5, None, 10);

    assert_eq!(encounter.encounter_type, "chase");
    assert_eq!(encounter.metric.name, "separation");
    assert_eq!(encounter.metric.direction, MetricDirection::Ascending);
    assert_eq!(
        encounter.metric.threshold_high,
        Some(10),
        "chase resolves when separation reaches goal"
    );
    assert!(
        encounter.metric.threshold_low.is_none(),
        "chase has no lower threshold on metric"
    );
    assert_eq!(encounter.metric.current, 0, "separation starts at 0");
    assert_eq!(encounter.metric.starting, 0);
    assert!(!encounter.resolved);
}

/// chase() with a rig type should populate secondary_stats.
#[test]
fn chase_encounter_constructor_with_rig() {
    let encounter = StructuredEncounter::chase(0.5, Some(RigType::Interceptor), 15);

    assert!(
        encounter.secondary_stats.is_some(),
        "vehicle chase must have secondary stats"
    );
    let stats = encounter.secondary_stats.as_ref().unwrap();
    let hp = stats.stats.get("hp").expect("rig stats must have hp");
    assert_eq!(hp.current, 15, "Interceptor has 15 HP");
    assert_eq!(hp.max, 15);

    let speed = stats.stats.get("speed").expect("rig stats must have speed");
    assert_eq!(speed.current, 5, "Interceptor has 5 speed");

    let fuel = stats.stats.get("fuel").expect("rig stats must have fuel");
    assert_eq!(fuel.current, 8, "Interceptor has 8 fuel");
    assert_eq!(fuel.max, 8);
}

/// chase() without rig should have no secondary stats.
#[test]
fn chase_encounter_constructor_no_rig() {
    let encounter = StructuredEncounter::chase(0.5, None, 10);
    assert!(
        encounter.secondary_stats.is_none(),
        "foot chase should have no secondary stats"
    );
}

/// chase() should start at beat 0 in Setup phase.
#[test]
fn chase_encounter_constructor_starts_at_setup() {
    let encounter = StructuredEncounter::chase(0.5, None, 10);

    assert_eq!(encounter.beat, 0);
    assert_eq!(
        encounter.structured_phase,
        Some(EncounterPhase::Setup),
        "chase should begin in Setup phase"
    );
}

// ==========================================================================
// AC-2: from_chase_state() migration preserves all fields
// ==========================================================================

/// from_chase_state() should map separation_distance to metric.current.
#[test]
fn from_chase_state_preserves_separation_as_metric() {
    let mut chase = ChaseState::new(ChaseType::Footrace, 0.5);
    chase.set_separation(7);

    let encounter = StructuredEncounter::from_chase_state(&chase);

    assert_eq!(encounter.encounter_type, "chase");
    assert_eq!(encounter.metric.name, "separation");
    assert_eq!(encounter.metric.current, 7, "separation must map to metric.current");
    assert_eq!(encounter.metric.direction, MetricDirection::Ascending);
}

/// from_chase_state() should map goal to metric.threshold_high.
#[test]
fn from_chase_state_preserves_goal_as_threshold() {
    let chase = ChaseState::new(ChaseType::Footrace, 0.5);
    // Default goal is 10

    let encounter = StructuredEncounter::from_chase_state(&chase);
    assert_eq!(
        encounter.metric.threshold_high,
        Some(10),
        "goal must map to threshold_high"
    );
}

/// from_chase_state() should map beat number directly.
#[test]
fn from_chase_state_preserves_beat() {
    let mut chase = ChaseState::new_vehicle_chase(
        ChaseType::Footrace,
        0.5,
        RigType::Frankenstein,
        10,
    );
    chase.advance_beat();
    chase.advance_beat(); // beat is now 2

    let encounter = StructuredEncounter::from_chase_state(&chase);
    assert_eq!(encounter.beat, 2, "beat must transfer directly");
}

/// from_chase_state() should map ChasePhase to EncounterPhase.
#[test]
fn from_chase_state_maps_phase() {
    let mut chase = ChaseState::new_vehicle_chase(
        ChaseType::Footrace,
        0.5,
        RigType::Frankenstein,
        10,
    );
    // After new_vehicle_chase, structured_phase is Setup
    let encounter = StructuredEncounter::from_chase_state(&chase);
    assert_eq!(encounter.structured_phase, Some(EncounterPhase::Setup));

    // Advance to get Opening phase
    chase.set_separation(3); // Prevent Caught outcome
    chase.advance_beat(); // beat 1 → Opening

    let encounter = StructuredEncounter::from_chase_state(&chase);
    assert_eq!(encounter.structured_phase, Some(EncounterPhase::Opening));
}

/// from_chase_state() should map actors with string-keyed roles.
#[test]
fn from_chase_state_maps_actors() {
    let mut chase = ChaseState::new(ChaseType::Footrace, 0.5);
    chase.set_actors(vec![
        ChaseActor {
            name: "Max".to_string(),
            role: ChaseRole::Driver,
        },
        ChaseActor {
            name: "Furiosa".to_string(),
            role: ChaseRole::Gunner,
        },
    ]);

    let encounter = StructuredEncounter::from_chase_state(&chase);

    assert_eq!(encounter.actors.len(), 2);
    assert_eq!(encounter.actors[0].name, "Max");
    assert_eq!(encounter.actors[0].role, "Driver");
    assert_eq!(encounter.actors[1].name, "Furiosa");
    assert_eq!(encounter.actors[1].role, "Gunner");
}

/// from_chase_state() should map resolved flag.
#[test]
fn from_chase_state_active_chase_is_not_resolved() {
    let chase = ChaseState::new(ChaseType::Footrace, 0.5);

    let encounter = StructuredEncounter::from_chase_state(&chase);
    assert!(!encounter.resolved, "active chase should not be resolved");
}

/// from_chase_state() should map outcome.
#[test]
fn from_chase_state_abandoned_chase_is_resolved() {
    let mut chase = ChaseState::new(ChaseType::Footrace, 0.5);
    chase.abandon();

    let encounter = StructuredEncounter::from_chase_state(&chase);
    assert!(encounter.resolved, "abandoned chase should be resolved");
    assert!(
        encounter.outcome.is_some(),
        "abandoned chase must have outcome"
    );
    let outcome_str = encounter.outcome.as_ref().unwrap();
    assert!(
        outcome_str.contains("Abandoned") || outcome_str.contains("abandoned"),
        "outcome should indicate abandonment, got: {}",
        outcome_str
    );
}

// ==========================================================================
// AC-3: Rig stats through SecondaryStats
// ==========================================================================

/// from_chase_state() should convert RigStats to SecondaryStats.
#[test]
fn from_chase_state_preserves_rig_stats() {
    let chase = ChaseState::new_vehicle_chase(
        ChaseType::Footrace,
        0.5,
        RigType::WarRig,
        15,
    );

    let encounter = StructuredEncounter::from_chase_state(&chase);

    let stats = encounter
        .secondary_stats
        .as_ref()
        .expect("vehicle chase must have secondary stats");

    let hp = stats.stats.get("hp").expect("must have hp");
    assert_eq!(hp.current, 30, "WarRig has 30 HP");
    assert_eq!(hp.max, 30);

    let armor = stats.stats.get("armor").expect("must have armor");
    assert_eq!(armor.current, 5, "WarRig has 5 armor");

    let fuel = stats.stats.get("fuel").expect("must have fuel");
    assert_eq!(fuel.current, 12, "WarRig has 12 fuel");
    assert_eq!(fuel.max, 12);
}

/// Rig damage tier should be preserved in SecondaryStats.
#[test]
fn from_chase_state_preserves_damage_tier() {
    let mut chase = ChaseState::new_vehicle_chase(
        ChaseType::Footrace,
        0.5,
        RigType::Frankenstein,
        10,
    );
    // Frankenstein: 18 HP, 2 armor. Take 7 raw = 5 effective → 13 HP (72%) → Cosmetic
    chase.rig_mut().unwrap().apply_damage(7);

    let encounter = StructuredEncounter::from_chase_state(&chase);
    let stats = encounter.secondary_stats.as_ref().unwrap();

    assert_eq!(
        stats.damage_tier.as_deref(),
        Some("COSMETIC"),
        "damage tier should be preserved as string"
    );
}

/// All 5 rig archetypes should convert to SecondaryStats correctly.
#[test]
fn all_rig_archetypes_convert_to_secondary_stats() {
    let specs: Vec<(RigType, i32, i32, i32, i32, i32)> = vec![
        (RigType::Interceptor, 15, 5, 1, 3, 8),
        (RigType::WarRig, 30, 2, 5, 1, 12),
        (RigType::Bike, 8, 4, 0, 5, 5),
        (RigType::Hauler, 25, 2, 3, 1, 20),
        (RigType::Frankenstein, 18, 3, 2, 3, 10),
    ];

    for (rig_type, hp, speed, armor, maneuver, fuel) in specs {
        let stats = SecondaryStats::rig(rig_type);

        let s_hp = stats.stats.get("hp").unwrap();
        assert_eq!(s_hp.current, hp, "{:?} HP", rig_type);
        assert_eq!(s_hp.max, hp, "{:?} max HP", rig_type);

        let s_speed = stats.stats.get("speed").unwrap();
        assert_eq!(s_speed.current, speed, "{:?} speed", rig_type);

        let s_armor = stats.stats.get("armor").unwrap();
        assert_eq!(s_armor.current, armor, "{:?} armor", rig_type);

        let s_maneuver = stats.stats.get("maneuver").unwrap();
        assert_eq!(s_maneuver.current, maneuver, "{:?} maneuver", rig_type);

        let s_fuel = stats.stats.get("fuel").unwrap();
        assert_eq!(s_fuel.current, fuel, "{:?} fuel", rig_type);
        assert_eq!(s_fuel.max, fuel, "{:?} max fuel", rig_type);
    }
}

/// from_chase_state() with no rig should have no secondary stats.
#[test]
fn from_chase_state_foot_chase_no_secondary_stats() {
    let chase = ChaseState::new(ChaseType::Stealth, 0.6);

    let encounter = StructuredEncounter::from_chase_state(&chase);
    assert!(
        encounter.secondary_stats.is_none(),
        "foot chase should have no secondary stats"
    );
}

// ==========================================================================
// AC-4: Serde roundtrip for chase encounters
// ==========================================================================

/// A chase encounter must survive JSON serialization/deserialization.
#[test]
fn chase_encounter_serde_roundtrip() {
    let encounter = StructuredEncounter::chase(0.5, Some(RigType::Interceptor), 10);

    let json = serde_json::to_string(&encounter).expect("serialize chase encounter");
    let deserialized: StructuredEncounter =
        serde_json::from_str(&json).expect("deserialize chase encounter");

    assert_eq!(deserialized.encounter_type, "chase");
    assert_eq!(deserialized.metric.name, "separation");
    assert_eq!(deserialized.metric.direction, MetricDirection::Ascending);
    assert_eq!(deserialized.metric.threshold_high, Some(10));
    assert!(!deserialized.resolved);

    let stats = deserialized.secondary_stats.as_ref().expect("stats survive roundtrip");
    assert!(stats.stats.contains_key("hp"));
    assert!(stats.stats.contains_key("fuel"));
}

/// A migrated chase encounter must survive serde roundtrip.
#[test]
fn migrated_chase_encounter_serde_roundtrip() {
    let mut chase = ChaseState::new_vehicle_chase(
        ChaseType::Footrace,
        0.5,
        RigType::WarRig,
        15,
    );
    chase.set_separation(5);
    chase.set_actors(vec![
        ChaseActor {
            name: "Max".to_string(),
            role: ChaseRole::Driver,
        },
    ]);
    chase.advance_beat();

    let encounter = StructuredEncounter::from_chase_state(&chase);
    let json = serde_json::to_string(&encounter).expect("serialize migrated");
    let deserialized: StructuredEncounter =
        serde_json::from_str(&json).expect("deserialize migrated");

    assert_eq!(deserialized.encounter_type, "chase");
    assert_eq!(deserialized.metric.current, chase.separation());
    assert_eq!(deserialized.beat, chase.beat());
    assert_eq!(deserialized.actors.len(), 1);
    assert_eq!(deserialized.actors[0].name, "Max");
}

// ==========================================================================
// AC-5: ChaseState behavioral regression — existing API still works
// ==========================================================================

/// ChaseState::new() still works.
#[test]
fn chase_state_new_still_works() {
    let chase = ChaseState::new(ChaseType::Footrace, 0.5);
    assert_eq!(chase.chase_type(), ChaseType::Footrace);
    assert_eq!(chase.escape_threshold(), 0.5);
    assert_eq!(chase.round(), 1);
    assert!(!chase.is_resolved());
    assert_eq!(chase.separation(), 0);
    assert_eq!(chase.beat(), 0);
    assert_eq!(chase.goal(), 10);
}

/// ChaseState::new_vehicle_chase() still works.
#[test]
fn chase_state_vehicle_chase_still_works() {
    let chase = ChaseState::new_vehicle_chase(
        ChaseType::Footrace,
        0.5,
        RigType::Interceptor,
        15,
    );
    assert!(chase.rig().is_some());
    assert_eq!(chase.rig().unwrap().rig_hp, 15);
    assert_eq!(chase.goal(), 15);
    assert_eq!(chase.structured_phase(), Some(ChasePhase::Setup));
}

/// ChaseState::record_roll() still tracks rounds and resolves.
#[test]
fn chase_state_record_roll_still_works() {
    let mut chase = ChaseState::new(ChaseType::Stealth, 0.6);

    chase.record_roll(0.3); // Below threshold, no escape
    assert!(!chase.is_resolved());
    assert_eq!(chase.rounds().len(), 1);
    assert!(!chase.rounds()[0].escaped);

    chase.record_roll(0.7); // Above threshold, escape!
    assert!(chase.is_resolved());
    assert_eq!(chase.rounds().len(), 2);
    assert!(chase.rounds()[1].escaped);
}

/// ChaseState::record_roll() is no-op after resolution.
#[test]
fn chase_state_record_roll_noop_after_resolved() {
    let mut chase = ChaseState::new(ChaseType::Footrace, 0.5);
    chase.record_roll(0.9); // Escape
    assert!(chase.is_resolved());

    chase.record_roll(0.1); // Should be ignored
    assert_eq!(chase.rounds().len(), 1, "no new round after resolution");
}

/// ChaseState::advance_beat() still drives phase transitions.
#[test]
fn chase_state_advance_beat_still_works() {
    let mut chase = ChaseState::new_vehicle_chase(
        ChaseType::Footrace,
        0.5,
        RigType::Frankenstein,
        10,
    );
    chase.set_separation(5); // Mid-chase, prevent Caught

    let (phase, outcome) = chase.advance_beat(); // beat 1 → Opening
    assert_eq!(phase, ChasePhase::Opening);
    assert!(outcome.is_none());

    let (phase, _) = chase.advance_beat(); // beat 2 → Escalation
    assert_eq!(phase, ChasePhase::Escalation);
}

/// ChaseState::abandon() still works.
#[test]
fn chase_state_abandon_still_works() {
    let mut chase = ChaseState::new(ChaseType::Negotiation, 0.5);
    chase.abandon();
    assert!(chase.is_resolved());
    assert_eq!(chase.outcome(), Some(ChaseOutcome::Abandoned));
}

/// ChaseState serde roundtrip still works.
#[test]
fn chase_state_serde_roundtrip_still_works() {
    let mut chase = ChaseState::new_vehicle_chase(
        ChaseType::Footrace,
        0.5,
        RigType::WarRig,
        15,
    );
    chase.set_separation(5);
    chase.set_phase("In hot pursuit".to_string());
    chase.set_event("Tire blowout".to_string());
    chase.set_actors(vec![ChaseActor {
        name: "Max".to_string(),
        role: ChaseRole::Driver,
    }]);

    let json = serde_json::to_string(&chase).expect("serialize ChaseState");
    let deserialized: ChaseState =
        serde_json::from_str(&json).expect("deserialize ChaseState");

    assert_eq!(deserialized.chase_type(), ChaseType::Footrace);
    assert_eq!(deserialized.separation(), 5);
    assert_eq!(deserialized.phase(), Some("In hot pursuit"));
    assert_eq!(deserialized.event(), Some("Tire blowout"));
    assert_eq!(deserialized.actors().len(), 1);
    assert!(deserialized.rig().is_some());
}

// ==========================================================================
// AC-5 (continued): Terrain modifiers still work
// ==========================================================================

/// Terrain modifiers are computed correctly.
#[test]
fn terrain_modifiers_still_work() {
    let m0 = terrain_modifiers(0);
    assert_eq!(m0.rig_damage_per_beat, 0);
    assert!(!m0.terrain_decision);

    let m3 = terrain_modifiers(3);
    assert_eq!(m3.rig_damage_per_beat, 1);
    assert!(m3.terrain_decision);

    let m5 = terrain_modifiers(5);
    assert_eq!(m5.rig_damage_per_beat, 3);
    assert!(m5.terrain_decision);
}

/// Terrain applied to rig stats still works.
#[test]
fn terrain_applied_to_rig_still_works() {
    let rig = RigStats::from_type(RigType::Interceptor);
    let mods = terrain_modifiers(5);
    let (speed, maneuver) = apply_terrain_to_rig(&rig, &mods);
    assert_eq!(speed, 3);
    assert_eq!(maneuver, 2);
}

// ==========================================================================
// AC-4 (continued): Cinematography still works
// ==========================================================================

/// Camera modes map correctly to phases.
#[test]
fn camera_modes_still_work() {
    assert_eq!(camera_for_phase(ChasePhase::Setup), CameraMode::WideEstablishing);
    assert_eq!(camera_for_phase(ChasePhase::Climax), CameraMode::CloseUpSlowMotion);
    assert_eq!(camera_for_phase(ChasePhase::Resolution), CameraMode::WidePullBack);
}

/// Cinematography for climax phase produces intense output.
#[test]
fn cinematography_climax_still_intense() {
    let cine = cinematography_for_phase(ChasePhase::Climax);
    assert_eq!(cine.camera, CameraMode::CloseUpSlowMotion);
    assert_eq!(cine.sentence_range, (4, 6));
    assert_eq!(cine.pace, "Peak intensity");
}

/// format_chase_context still produces all sections.
#[test]
fn format_chase_context_still_produces_all_sections() {
    let beat = ChaseBeat {
        beat_number: 3,
        phase: ChasePhase::Escalation,
        decisions: vec![
            BeatDecision {
                description: "Floor it through the gap".to_string(),
                separation_delta: 2,
                risk: "high damage".to_string(),
            },
        ],
        terrain_danger: 3,
    };
    let rig = RigStats::from_type(RigType::Interceptor);
    let actors = vec![
        ChaseActor {
            name: "Max".to_string(),
            role: ChaseRole::Driver,
        },
    ];

    let ctx = format_chase_context(&beat, &rig, &actors, 5, 10);

    assert!(ctx.contains("[CHASE SEQUENCE]"));
    assert!(ctx.contains("ESCALATION"));
    assert!(ctx.contains("Beat: 3"));
    assert!(ctx.contains("Separation: 5/10"));
    assert!(ctx.contains("Interceptor"));
    assert!(ctx.contains("Max (Driver)"));
    assert!(ctx.contains("Floor it through the gap"));
    assert!(ctx.contains("Tight tracking"));
}

// ==========================================================================
// AC-6: Beat system — phase transitions and outcome checking
// ==========================================================================

/// Phase transitions by beat number still correct.
#[test]
fn phase_transitions_still_correct() {
    assert_eq!(phase_for_beat(0, false), ChasePhase::Setup);
    assert_eq!(phase_for_beat(1, false), ChasePhase::Opening);
    assert_eq!(phase_for_beat(2, false), ChasePhase::Escalation);
    assert_eq!(phase_for_beat(4, false), ChasePhase::Escalation);
    assert_eq!(phase_for_beat(5, false), ChasePhase::Climax);
}

/// Resolved outcome forces Resolution phase.
#[test]
fn resolved_forces_resolution_phase() {
    assert_eq!(phase_for_beat(2, true), ChasePhase::Resolution);
    assert_eq!(phase_for_beat(5, true), ChasePhase::Resolution);
}

/// Outcome checking: escape, caught, crashed priority.
#[test]
fn outcome_checking_still_works() {
    assert_eq!(check_outcome(10, 10, 5), Some(ChaseOutcome::Escape));
    assert_eq!(check_outcome(0, 10, 5), Some(ChaseOutcome::Caught));
    assert_eq!(check_outcome(0, 10, 0), Some(ChaseOutcome::Crashed)); // crashed priority
    assert_eq!(check_outcome(5, 10, 10), None); // in progress
}

/// Danger escalation by beat still works.
#[test]
fn danger_escalation_still_works() {
    assert_eq!(danger_for_beat(0, ChasePhase::Setup), 0);
    assert_eq!(danger_for_beat(1, ChasePhase::Opening), 1);
    assert_eq!(danger_for_beat(5, ChasePhase::Climax), 5);
}

// ==========================================================================
// AC-3 (continued): Rig damage and fuel still work through encounter
// ==========================================================================

/// Rig damage tracked correctly after apply_damage via ChaseState,
/// then migration preserves the damaged state.
#[test]
fn damaged_rig_migrates_correctly() {
    let mut chase = ChaseState::new_vehicle_chase(
        ChaseType::Footrace,
        0.5,
        RigType::Bike,
        10,
    );
    // Bike: 8 HP, 0 armor. Take 5 damage → 3 HP (37.5%) → Failing
    chase.rig_mut().unwrap().apply_damage(5);
    assert_eq!(chase.rig().unwrap().rig_hp, 3);
    assert_eq!(chase.rig().unwrap().damage_tier(), RigDamageTier::Failing);

    let encounter = StructuredEncounter::from_chase_state(&chase);
    let stats = encounter.secondary_stats.as_ref().unwrap();

    let hp = stats.stats.get("hp").unwrap();
    assert_eq!(hp.current, 3, "damaged HP must migrate");
    assert_eq!(hp.max, 8, "max HP must migrate");

    assert_eq!(
        stats.damage_tier.as_deref(),
        Some("FAILING"),
        "damage tier must reflect current damage"
    );
}

/// Fuel consumption tracked correctly through migration.
#[test]
fn fuel_consumption_migrates_correctly() {
    let mut chase = ChaseState::new_vehicle_chase(
        ChaseType::Footrace,
        0.5,
        RigType::Hauler, // 20 fuel
        10,
    );
    chase.rig_mut().unwrap().consume_fuel(15);
    assert_eq!(chase.rig().unwrap().fuel, 5);

    let encounter = StructuredEncounter::from_chase_state(&chase);
    let stats = encounter.secondary_stats.as_ref().unwrap();

    let fuel = stats.stats.get("fuel").unwrap();
    assert_eq!(fuel.current, 5, "consumed fuel must migrate");
    assert_eq!(fuel.max, 20, "max fuel must migrate");
}

// ==========================================================================
// AC-7: Wiring test — GameSnapshot accepts chase encounter
// ==========================================================================

/// GameSnapshot should accept a chase-type StructuredEncounter.
#[test]
fn game_snapshot_accepts_chase_encounter() {
    use sidequest_game::state::GameSnapshot;

    let mut snapshot = GameSnapshot::default();
    let encounter = StructuredEncounter::chase(0.5, Some(RigType::Interceptor), 10);

    snapshot.encounter = Some(encounter);

    let enc = snapshot.encounter.as_ref().expect("encounter set");
    assert_eq!(enc.encounter_type, "chase");
    assert_eq!(enc.metric.name, "separation");
    assert!(enc.secondary_stats.is_some());
}

/// GameSnapshot with a chase encounter survives serde roundtrip.
#[test]
fn game_snapshot_chase_encounter_serde_roundtrip() {
    use sidequest_game::state::GameSnapshot;

    let mut snapshot = GameSnapshot::default();
    snapshot.encounter = Some(StructuredEncounter::chase(
        0.5,
        Some(RigType::WarRig),
        15,
    ));

    let json = serde_json::to_string(&snapshot).expect("serialize snapshot");
    let de: GameSnapshot = serde_json::from_str(&json).expect("deserialize snapshot");

    let enc = de.encounter.as_ref().expect("encounter survived roundtrip");
    assert_eq!(enc.encounter_type, "chase");
    assert_eq!(enc.metric.threshold_high, Some(15));

    let stats = enc.secondary_stats.as_ref().expect("stats survived");
    assert!(stats.stats.contains_key("hp"));
}

/// GameSnapshot with BOTH old chase field AND new encounter field
/// should preserve both during serde roundtrip (backward compat).
#[test]
fn game_snapshot_chase_and_encounter_coexist() {
    use sidequest_game::state::GameSnapshot;

    let mut snapshot = GameSnapshot::default();
    snapshot.chase = Some(ChaseState::new(ChaseType::Footrace, 0.5));
    snapshot.encounter = Some(StructuredEncounter::chase(0.5, None, 10));

    let json = serde_json::to_string(&snapshot).expect("serialize");
    let de: GameSnapshot = serde_json::from_str(&json).expect("deserialize");

    assert!(de.chase.is_some(), "old chase field survives");
    assert!(de.encounter.is_some(), "new encounter field survives");
}

// ==========================================================================
// EncounterPhase drama_weight matches ChasePhase
// ==========================================================================

/// EncounterPhase drama weights must match ChasePhase weights exactly.
/// This ensures the cinematography system produces identical output
/// regardless of whether the code reads from ChasePhase or EncounterPhase.
#[test]
fn encounter_phase_drama_weights_match_chase_phase() {
    let mappings = [
        (ChasePhase::Setup, EncounterPhase::Setup),
        (ChasePhase::Opening, EncounterPhase::Opening),
        (ChasePhase::Escalation, EncounterPhase::Escalation),
        (ChasePhase::Climax, EncounterPhase::Climax),
        (ChasePhase::Resolution, EncounterPhase::Resolution),
    ];

    for (chase_phase, encounter_phase) in &mappings {
        assert_eq!(
            chase_phase.drama_weight(),
            encounter_phase.drama_weight(),
            "drama_weight mismatch for {:?} vs {:?}",
            chase_phase,
            encounter_phase,
        );
    }
}

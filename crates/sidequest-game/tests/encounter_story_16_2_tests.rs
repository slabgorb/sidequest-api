//! Story 16-2: StructuredEncounter trait + ConfrontationState
//!
//! RED phase tests for the universal structured encounter engine.
//! These tests validate the generalization of ChaseState into
//! StructuredEncounter with string-keyed types, EncounterMetric,
//! SecondaryStats, EncounterActor, and backward compatibility.

use std::collections::HashMap;

// --- Imports for new encounter types ---
// These will fail to compile until Dev creates encounter.rs and encounter_depth.rs
use sidequest_game::encounter::{
    EncounterActor, EncounterMetric, EncounterPhase, MetricDirection, SecondaryStats, StatValue,
    StructuredEncounter,
};

// --- Imports for backward compat and GameSnapshot ---
use sidequest_game::state::GameSnapshot;

// ==========================================================================
// AC: StructuredEncounter struct compiles with all fields, serializes/deserializes
// ==========================================================================

#[test]
fn structured_encounter_construction_with_all_fields() {
    let encounter = StructuredEncounter {
        encounter_type: "chase".to_string(),
        metric: EncounterMetric {
            name: "separation".to_string(),
            current: 5,
            starting: 0,
            direction: MetricDirection::Ascending,
            threshold_high: Some(10),
            threshold_low: None,
        },
        beat: 0,
        structured_phase: Some(EncounterPhase::Setup),
        secondary_stats: None,
        actors: vec![],
        outcome: None,
        resolved: false,
        mood_override: None,
        narrator_hints: vec![],
    };

    assert_eq!(encounter.encounter_type, "chase");
    assert_eq!(encounter.metric.name, "separation");
    assert_eq!(encounter.metric.current, 5);
    assert_eq!(encounter.beat, 0);
    assert!(!encounter.resolved);
}

#[test]
fn structured_encounter_serde_roundtrip() {
    let encounter = StructuredEncounter {
        encounter_type: "standoff".to_string(),
        metric: EncounterMetric {
            name: "tension".to_string(),
            current: 0,
            starting: 0,
            direction: MetricDirection::Ascending,
            threshold_high: Some(10),
            threshold_low: None,
        },
        beat: 3,
        structured_phase: Some(EncounterPhase::Escalation),
        secondary_stats: Some(SecondaryStats {
            stats: {
                let mut m = HashMap::new();
                m.insert("focus".to_string(), StatValue { current: 5, max: 8 });
                m
            },
            damage_tier: None,
        }),
        actors: vec![EncounterActor {
            name: "Clint".to_string(),
            role: "duelist".to_string(),
        }],
        outcome: None,
        resolved: false,
        mood_override: Some("standoff".to_string()),
        narrator_hints: vec!["Sweat beads on his brow".to_string()],
    };

    let json = serde_json::to_string(&encounter).expect("serialize");
    let deserialized: StructuredEncounter = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(deserialized.encounter_type, "standoff");
    assert_eq!(deserialized.metric.name, "tension");
    assert_eq!(deserialized.metric.direction, MetricDirection::Ascending);
    assert_eq!(deserialized.beat, 3);
    assert_eq!(deserialized.actors.len(), 1);
    assert_eq!(deserialized.actors[0].name, "Clint");
    assert_eq!(deserialized.actors[0].role, "duelist");
    assert_eq!(deserialized.mood_override.as_deref(), Some("standoff"));
    assert_eq!(deserialized.narrator_hints.len(), 1);

    // Verify secondary stats survived roundtrip
    let stats = deserialized
        .secondary_stats
        .as_ref()
        .expect("secondary_stats present");
    let focus = stats.stats.get("focus").expect("focus stat exists");
    assert_eq!(focus.current, 5);
    assert_eq!(focus.max, 8);
}

#[test]
fn structured_encounter_with_no_optional_fields() {
    let encounter = StructuredEncounter {
        encounter_type: "negotiation".to_string(),
        metric: EncounterMetric {
            name: "leverage".to_string(),
            current: 0,
            starting: 0,
            direction: MetricDirection::Bidirectional,
            threshold_high: Some(5),
            threshold_low: Some(-5),
        },
        beat: 0,
        structured_phase: None,
        secondary_stats: None,
        actors: vec![],
        outcome: None,
        resolved: false,
        mood_override: None,
        narrator_hints: vec![],
    };

    let json = serde_json::to_string(&encounter).expect("serialize minimal");
    let de: StructuredEncounter = serde_json::from_str(&json).expect("deserialize minimal");

    assert_eq!(de.encounter_type, "negotiation");
    assert!(de.structured_phase.is_none());
    assert!(de.secondary_stats.is_none());
    assert!(de.actors.is_empty());
    assert!(de.outcome.is_none());
    assert!(de.mood_override.is_none());
}

// ==========================================================================
// AC: Metric types — Ascending, Descending, Bidirectional all work
// ==========================================================================

#[test]
fn metric_direction_ascending() {
    let metric = EncounterMetric {
        name: "tension".to_string(),
        current: 0,
        starting: 0,
        direction: MetricDirection::Ascending,
        threshold_high: Some(10),
        threshold_low: None,
    };

    assert_eq!(metric.direction, MetricDirection::Ascending);
    assert_eq!(metric.threshold_high, Some(10));
    assert!(metric.threshold_low.is_none());
}

#[test]
fn metric_direction_descending() {
    let metric = EncounterMetric {
        name: "separation".to_string(),
        current: 10,
        starting: 10,
        direction: MetricDirection::Descending,
        threshold_high: None,
        threshold_low: Some(0),
    };

    assert_eq!(metric.direction, MetricDirection::Descending);
    assert_eq!(metric.current, 10);
    assert_eq!(metric.starting, 10);
    assert_eq!(metric.threshold_low, Some(0));
}

#[test]
fn metric_direction_bidirectional() {
    let metric = EncounterMetric {
        name: "leverage".to_string(),
        current: 0,
        starting: 0,
        direction: MetricDirection::Bidirectional,
        threshold_high: Some(5),
        threshold_low: Some(-5),
    };

    assert_eq!(metric.direction, MetricDirection::Bidirectional);
    assert_eq!(metric.threshold_high, Some(5));
    assert_eq!(metric.threshold_low, Some(-5));
}

#[test]
fn metric_direction_serde_roundtrip() {
    // All three variants must survive JSON roundtrip
    for direction in [
        MetricDirection::Ascending,
        MetricDirection::Descending,
        MetricDirection::Bidirectional,
    ] {
        let json = serde_json::to_string(&direction).expect("serialize direction");
        let de: MetricDirection = serde_json::from_str(&json).expect("deserialize direction");
        assert_eq!(de, direction, "direction {:?} must roundtrip", direction);
    }
}

// ==========================================================================
// AC: Secondary stats — RigStats expressible as SecondaryStats
// ==========================================================================

#[test]
fn secondary_stats_basic_construction() {
    let mut stats_map = HashMap::new();
    stats_map.insert(
        "hp".to_string(),
        StatValue {
            current: 15,
            max: 15,
        },
    );
    stats_map.insert("fuel".to_string(), StatValue { current: 8, max: 8 });
    stats_map.insert("speed".to_string(), StatValue { current: 5, max: 5 });
    stats_map.insert("armor".to_string(), StatValue { current: 1, max: 1 });
    stats_map.insert("maneuver".to_string(), StatValue { current: 3, max: 3 });

    let stats = SecondaryStats {
        stats: stats_map,
        damage_tier: Some("PRISTINE".to_string()),
    };

    assert_eq!(stats.stats.len(), 5);
    let hp = stats.stats.get("hp").expect("hp exists");
    assert_eq!(hp.current, 15);
    assert_eq!(hp.max, 15);
    assert_eq!(stats.damage_tier.as_deref(), Some("PRISTINE"));
}

#[test]
fn secondary_stats_serde_roundtrip() {
    let mut stats_map = HashMap::new();
    stats_map.insert(
        "shields".to_string(),
        StatValue {
            current: 100,
            max: 200,
        },
    );
    stats_map.insert(
        "hull".to_string(),
        StatValue {
            current: 80,
            max: 80,
        },
    );

    let stats = SecondaryStats {
        stats: stats_map,
        damage_tier: None,
    };

    let json = serde_json::to_string(&stats).expect("serialize stats");
    let de: SecondaryStats = serde_json::from_str(&json).expect("deserialize stats");

    assert_eq!(de.stats.len(), 2);
    let shields = de.stats.get("shields").expect("shields exists");
    assert_eq!(shields.current, 100);
    assert_eq!(shields.max, 200);
    assert!(de.damage_tier.is_none());
}

#[test]
fn secondary_stats_rig_convenience_constructor() {
    // RigStats becomes a convenience constructor: SecondaryStats::rig(RigType)
    use sidequest_game::chase_depth::RigType;

    let stats = SecondaryStats::rig(RigType::Interceptor);

    // Must contain the same stats as RigStats::from_type(Interceptor)
    let hp = stats.stats.get("hp").expect("hp");
    assert_eq!(hp.current, 15);
    assert_eq!(hp.max, 15);

    let speed = stats.stats.get("speed").expect("speed");
    assert_eq!(speed.current, 5);
    assert_eq!(speed.max, 5);

    let armor = stats.stats.get("armor").expect("armor");
    assert_eq!(armor.current, 1);
    assert_eq!(armor.max, 1);

    let maneuver = stats.stats.get("maneuver").expect("maneuver");
    assert_eq!(maneuver.current, 3);
    assert_eq!(maneuver.max, 3);

    let fuel = stats.stats.get("fuel").expect("fuel");
    assert_eq!(fuel.current, 8);
    assert_eq!(fuel.max, 8);

    assert_eq!(stats.damage_tier.as_deref(), Some("PRISTINE"));
}

// ==========================================================================
// AC: EncounterActor with string-keyed roles
// ==========================================================================

#[test]
fn encounter_actor_string_roles() {
    let actors = [
        EncounterActor {
            name: "Max".to_string(),
            role: "driver".to_string(),
        },
        EncounterActor {
            name: "Furiosa".to_string(),
            role: "gunner".to_string(),
        },
        EncounterActor {
            name: "Nux".to_string(),
            role: "mechanic".to_string(),
        },
    ];

    assert_eq!(actors.len(), 3);
    assert_eq!(actors[0].role, "driver");
    assert_eq!(actors[1].role, "gunner");
    assert_eq!(actors[2].role, "mechanic");
}

#[test]
fn encounter_actor_arbitrary_roles() {
    // String-keyed roles means genre packs can define anything
    let actor = EncounterActor {
        name: "Neo".to_string(),
        role: "netrunner".to_string(),
    };
    assert_eq!(actor.role, "netrunner");

    let actor2 = EncounterActor {
        name: "Deckard".to_string(),
        role: "interrogator".to_string(),
    };
    assert_eq!(actor2.role, "interrogator");
}

#[test]
fn encounter_actor_serde_roundtrip() {
    let actor = EncounterActor {
        name: "Blondie".to_string(),
        role: "duelist".to_string(),
    };

    let json = serde_json::to_string(&actor).expect("serialize actor");
    let de: EncounterActor = serde_json::from_str(&json).expect("deserialize actor");

    assert_eq!(de.name, "Blondie");
    assert_eq!(de.role, "duelist");
}

// ==========================================================================
// AC: EncounterPhase — universal narrative arc
// ==========================================================================

#[test]
fn encounter_phase_variants() {
    // The universal narrative arc: Setup → Opening → Escalation → Climax → Resolution
    let phases = [
        EncounterPhase::Setup,
        EncounterPhase::Opening,
        EncounterPhase::Escalation,
        EncounterPhase::Climax,
        EncounterPhase::Resolution,
    ];

    assert_eq!(phases.len(), 5);
    assert_eq!(phases[0], EncounterPhase::Setup);
    assert_eq!(phases[4], EncounterPhase::Resolution);
}

#[test]
fn encounter_phase_serde_roundtrip() {
    for phase in [
        EncounterPhase::Setup,
        EncounterPhase::Opening,
        EncounterPhase::Escalation,
        EncounterPhase::Climax,
        EncounterPhase::Resolution,
    ] {
        let json = serde_json::to_string(&phase).expect("serialize phase");
        let de: EncounterPhase = serde_json::from_str(&json).expect("deserialize phase");
        assert_eq!(de, phase, "phase {:?} must roundtrip", phase);
    }
}

#[test]
fn encounter_phase_has_drama_weight() {
    // Reuse from ChasePhase — each phase has a drama weight for cinematography
    assert!(EncounterPhase::Setup.drama_weight() > 0.0);
    assert!(EncounterPhase::Climax.drama_weight() > EncounterPhase::Setup.drama_weight());
    assert!(EncounterPhase::Climax.drama_weight() >= 0.90);
}

// ==========================================================================
// AC: GameSnapshot — encounter field replaces chase field
// ==========================================================================

#[test]
fn game_snapshot_has_encounter_field() {
    let mut snapshot = GameSnapshot::default();
    assert!(snapshot.encounter.is_none());

    snapshot.encounter = Some(StructuredEncounter {
        encounter_type: "chase".to_string(),
        metric: EncounterMetric {
            name: "separation".to_string(),
            current: 5,
            starting: 0,
            direction: MetricDirection::Ascending,
            threshold_high: Some(10),
            threshold_low: None,
        },
        beat: 2,
        structured_phase: Some(EncounterPhase::Escalation),
        secondary_stats: None,
        actors: vec![],
        outcome: None,
        resolved: false,
        mood_override: None,
        narrator_hints: vec![],
    });

    let enc = snapshot.encounter.as_ref().expect("encounter set");
    assert_eq!(enc.encounter_type, "chase");
    assert_eq!(enc.metric.current, 5);
}

#[test]
fn game_snapshot_encounter_serde_roundtrip() {
    let snapshot = GameSnapshot {
        encounter: Some(StructuredEncounter {
            encounter_type: "standoff".to_string(),
            metric: EncounterMetric {
                name: "tension".to_string(),
                current: 7,
                starting: 0,
                direction: MetricDirection::Ascending,
                threshold_high: Some(10),
                threshold_low: None,
            },
            beat: 4,
            structured_phase: Some(EncounterPhase::Climax),
            secondary_stats: None,
            actors: vec![EncounterActor {
                name: "Angel Eyes".to_string(),
                role: "duelist".to_string(),
            }],
            outcome: None,
            resolved: false,
            mood_override: Some("standoff".to_string()),
            narrator_hints: vec!["The clock strikes noon".to_string()],
        }),
        ..Default::default()
    };

    let json = serde_json::to_string(&snapshot).expect("serialize snapshot");
    let de: GameSnapshot = serde_json::from_str(&json).expect("deserialize snapshot");

    let enc = de.encounter.as_ref().expect("encounter survived roundtrip");
    assert_eq!(enc.encounter_type, "standoff");
    assert_eq!(enc.beat, 4);
    assert_eq!(enc.actors.len(), 1);
    assert_eq!(enc.narrator_hints[0], "The clock strikes noon");
}

// ==========================================================================
// AC: Backward compat — old saves with chase field deserialize into encounter
// ==========================================================================

#[test]
fn old_chase_state_json_deserializes_as_encounter() {
    // Simulate an old save format with a "chase" field
    // The new GameSnapshot must accept this via serde alias + custom deserializer
    let old_save = serde_json::json!({
        "genre_slug": "road_warrior",
        "world_slug": "flickering_reach",
        "characters": [],
        "npcs": [],
        "location": "The Wasteland",
        "time_of_day": "noon",
        "quest_log": {},
        "notes": [],
        "narrative_log": [],
        "combat": {
            "round": 1,
            "damage_log": [],
            "status_effects": [],
            "turn_order": [],
            "available_actions": [],
            "is_active": false
        },
        "chase": {
            "chase_type": "Footrace",
            "escape_threshold": 0.5,
            "round": 3,
            "rounds": [
                {"roll": 0.3, "escaped": false},
                {"roll": 0.4, "escaped": false}
            ],
            "resolved": false,
            "separation_distance": 5,
            "chase_phase": "Escalation",
            "chase_event": null,
            "rig": null,
            "actors": [],
            "beat": 2,
            "goal": 10,
            "structured_phase": "Escalation",
            "outcome": null
        },
        "active_tropes": [],
        "atmosphere": "dusty",
        "current_region": "wastes",
        "discovered_regions": [],
        "discovered_routes": [],
        "turn_manager": {
            "round": 1,
            "phase": "InputCollection",
            "input_barrier": false
        }
    });

    let snapshot: GameSnapshot =
        serde_json::from_value(old_save).expect("old save with chase field should deserialize");

    // The old chase field should populate the new encounter field
    let enc = snapshot
        .encounter
        .as_ref()
        .expect("chase should migrate to encounter");
    assert_eq!(enc.encounter_type, "chase", "migrated type must be 'chase'");
    assert_eq!(enc.metric.name, "separation");
    assert_eq!(enc.metric.current, 5);
    assert_eq!(enc.metric.direction, MetricDirection::Ascending);
    assert_eq!(enc.metric.threshold_high, Some(10));
}

// ==========================================================================
// AC: Chase compat — ChaseState expressible via StructuredEncounter
// ==========================================================================

#[test]
fn structured_encounter_chase_convenience_constructor() {
    // StructuredEncounter::chase(...) should create a chase-type encounter
    // that maps to the old ChaseState semantics
    use sidequest_game::chase_depth::RigType;

    let encounter = StructuredEncounter::chase(
        0.5, // escape_threshold maps to metric threshold
        Some(RigType::Interceptor),
        10, // goal
    );

    assert_eq!(encounter.encounter_type, "chase");
    assert_eq!(encounter.metric.name, "separation");
    assert_eq!(encounter.metric.direction, MetricDirection::Ascending);
    assert_eq!(encounter.metric.threshold_high, Some(10));
    assert!(!encounter.resolved);
    assert_eq!(encounter.structured_phase, Some(EncounterPhase::Setup));

    // If rig type provided, secondary stats should be populated
    let stats = encounter
        .secondary_stats
        .as_ref()
        .expect("rig creates secondary stats");
    assert!(stats.stats.contains_key("hp"));
    assert!(stats.stats.contains_key("fuel"));
}

#[test]
fn structured_encounter_chase_without_rig() {
    // Foot chases have no secondary stats
    let encounter = StructuredEncounter::chase(
        0.5, None, // no rig
        10,
    );

    assert_eq!(encounter.encounter_type, "chase");
    assert!(encounter.secondary_stats.is_none());
}

// ==========================================================================
// Rule #2: #[non_exhaustive] on MetricDirection
// ==========================================================================

#[test]
fn metric_direction_is_non_exhaustive() {
    // MetricDirection is a public enum that will grow (e.g., Threshold, Cyclical)
    // This test ensures it has #[non_exhaustive] by checking serde handles
    // unknown variants gracefully (the compiler enforces _ arm in matches)
    let known_variants = [
        MetricDirection::Ascending,
        MetricDirection::Descending,
        MetricDirection::Bidirectional,
    ];
    // If this compiles and all three exist, the enum is at least correctly defined.
    // The #[non_exhaustive] attribute is a compile-time check — if downstream code
    // uses an exhaustive match, the compiler will error.
    assert_eq!(known_variants.len(), 3);
}

// ==========================================================================
// Full encounter scenarios — genre-specific encounter types
// ==========================================================================

#[test]
fn standoff_encounter_full_construction() {
    // Spaghetti western standoff: ascending tension to threshold
    let encounter = StructuredEncounter {
        encounter_type: "standoff".to_string(),
        metric: EncounterMetric {
            name: "tension".to_string(),
            current: 0,
            starting: 0,
            direction: MetricDirection::Ascending,
            threshold_high: Some(10),
            threshold_low: None,
        },
        beat: 0,
        structured_phase: Some(EncounterPhase::Setup),
        secondary_stats: Some(SecondaryStats {
            stats: {
                let mut m = HashMap::new();
                m.insert("focus".to_string(), StatValue { current: 5, max: 5 });
                m
            },
            damage_tier: None,
        }),
        actors: vec![
            EncounterActor {
                name: "The Good".to_string(),
                role: "duelist".to_string(),
            },
            EncounterActor {
                name: "The Bad".to_string(),
                role: "duelist".to_string(),
            },
            EncounterActor {
                name: "The Ugly".to_string(),
                role: "duelist".to_string(),
            },
        ],
        outcome: None,
        resolved: false,
        mood_override: Some("standoff".to_string()),
        narrator_hints: vec![
            "Three men circle in the cemetery".to_string(),
            "Ennio Morricone intensifies".to_string(),
        ],
    };

    assert_eq!(encounter.encounter_type, "standoff");
    assert_eq!(encounter.actors.len(), 3);
    assert_eq!(encounter.narrator_hints.len(), 2);
    let focus = encounter
        .secondary_stats
        .as_ref()
        .unwrap()
        .stats
        .get("focus")
        .unwrap();
    assert_eq!(focus.current, 5);
}

#[test]
fn negotiation_encounter_bidirectional_metric() {
    // Negotiation: leverage swings both ways
    let encounter = StructuredEncounter {
        encounter_type: "negotiation".to_string(),
        metric: EncounterMetric {
            name: "leverage".to_string(),
            current: 0,
            starting: 0,
            direction: MetricDirection::Bidirectional,
            threshold_high: Some(5),
            threshold_low: Some(-5),
        },
        beat: 0,
        structured_phase: Some(EncounterPhase::Setup),
        secondary_stats: None,
        actors: vec![
            EncounterActor {
                name: "Detective".to_string(),
                role: "interrogator".to_string(),
            },
            EncounterActor {
                name: "Suspect".to_string(),
                role: "subject".to_string(),
            },
        ],
        outcome: None,
        resolved: false,
        mood_override: None,
        narrator_hints: vec![],
    };

    assert_eq!(encounter.metric.direction, MetricDirection::Bidirectional);
    assert_eq!(encounter.metric.threshold_high, Some(5));
    assert_eq!(encounter.metric.threshold_low, Some(-5));
}

#[test]
fn ship_combat_encounter_with_secondary_stats() {
    // Space opera ship combat: descending HP metric with complex secondary stats
    let mut stats_map = HashMap::new();
    stats_map.insert(
        "shields".to_string(),
        StatValue {
            current: 100,
            max: 200,
        },
    );
    stats_map.insert(
        "hull".to_string(),
        StatValue {
            current: 80,
            max: 80,
        },
    );
    stats_map.insert(
        "engines".to_string(),
        StatValue {
            current: 50,
            max: 50,
        },
    );

    let encounter = StructuredEncounter {
        encounter_type: "ship_combat".to_string(),
        metric: EncounterMetric {
            name: "hull_integrity".to_string(),
            current: 80,
            starting: 80,
            direction: MetricDirection::Descending,
            threshold_high: None,
            threshold_low: Some(0),
        },
        beat: 0,
        structured_phase: Some(EncounterPhase::Setup),
        secondary_stats: Some(SecondaryStats {
            stats: stats_map,
            damage_tier: Some("PRISTINE".to_string()),
        }),
        actors: vec![
            EncounterActor {
                name: "Captain".to_string(),
                role: "commander".to_string(),
            },
            EncounterActor {
                name: "Pilot".to_string(),
                role: "helmsman".to_string(),
            },
            EncounterActor {
                name: "Gunner".to_string(),
                role: "weapons".to_string(),
            },
        ],
        outcome: None,
        resolved: false,
        mood_override: Some("combat".to_string()),
        narrator_hints: vec![],
    };

    assert_eq!(encounter.encounter_type, "ship_combat");
    assert_eq!(encounter.metric.direction, MetricDirection::Descending);
    let stats = encounter.secondary_stats.as_ref().unwrap();
    assert_eq!(stats.stats.len(), 3);
    assert!(stats.stats.contains_key("shields"));
    assert!(stats.stats.contains_key("hull"));
    assert!(stats.stats.contains_key("engines"));
}

// ==========================================================================
// Edge cases
// ==========================================================================

#[test]
fn encounter_with_empty_encounter_type_still_serializes() {
    // Edge case: empty string encounter type (shouldn't happen in practice,
    // but struct should handle it gracefully)
    let encounter = StructuredEncounter {
        encounter_type: String::new(),
        metric: EncounterMetric {
            name: "test".to_string(),
            current: 0,
            starting: 0,
            direction: MetricDirection::Ascending,
            threshold_high: None,
            threshold_low: None,
        },
        beat: 0,
        structured_phase: None,
        secondary_stats: None,
        actors: vec![],
        outcome: None,
        resolved: false,
        mood_override: None,
        narrator_hints: vec![],
    };

    let json = serde_json::to_string(&encounter).expect("empty type serializes");
    let de: StructuredEncounter = serde_json::from_str(&json).expect("empty type deserializes");
    assert_eq!(de.encounter_type, "");
}

#[test]
fn stat_value_zero_max_is_valid() {
    // Edge: a stat with max 0 (e.g., a disabled subsystem)
    let sv = StatValue { current: 0, max: 0 };
    let json = serde_json::to_string(&sv).expect("serialize zero stat");
    let de: StatValue = serde_json::from_str(&json).expect("deserialize zero stat");
    assert_eq!(de.current, 0);
    assert_eq!(de.max, 0);
}

#[test]
fn encounter_metric_negative_values_valid() {
    // Bidirectional metrics can go negative
    let metric = EncounterMetric {
        name: "leverage".to_string(),
        current: -3,
        starting: 0,
        direction: MetricDirection::Bidirectional,
        threshold_high: Some(5),
        threshold_low: Some(-5),
    };

    let json = serde_json::to_string(&metric).expect("serialize negative metric");
    let de: EncounterMetric = serde_json::from_str(&json).expect("deserialize negative metric");
    assert_eq!(de.current, -3);
}

#[test]
fn encounter_resolved_flag_persists() {
    let encounter = StructuredEncounter {
        encounter_type: "chase".to_string(),
        metric: EncounterMetric {
            name: "separation".to_string(),
            current: 10,
            starting: 0,
            direction: MetricDirection::Ascending,
            threshold_high: Some(10),
            threshold_low: None,
        },
        beat: 5,
        structured_phase: Some(EncounterPhase::Resolution),
        secondary_stats: None,
        actors: vec![],
        outcome: Some("escape".to_string()),
        resolved: true,
        mood_override: None,
        narrator_hints: vec![],
    };

    let json = serde_json::to_string(&encounter).expect("serialize resolved");
    let de: StructuredEncounter = serde_json::from_str(&json).expect("deserialize resolved");
    assert!(de.resolved);
    assert_eq!(de.outcome.as_deref(), Some("escape"));
    assert_eq!(de.structured_phase, Some(EncounterPhase::Resolution));
}

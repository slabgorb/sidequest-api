//! Failing tests for Story 29-5: TACTICAL_STATE protocol message
//!
//! Tests verify TACTICAL_STATE and TACTICAL_ACTION message variants,
//! payload serialization, and wiring into the protocol crate.

use super::*;

// ==========================================================================
// AC-1: TacticalStatePayload serializes/deserializes correctly
// ==========================================================================

mod tactical_state_payload_tests {
    use super::*;

    #[test]
    fn tactical_state_payload_round_trip() {
        let payload = TacticalStatePayload {
            room_id: "mawdeep_entrance".to_string(),
            grid: TacticalGridPayload {
                width: 5,
                height: 3,
                cells: vec![
                    vec![
                        "wall".into(),
                        "wall".into(),
                        "door_closed".into(),
                        "wall".into(),
                        "wall".into(),
                    ],
                    vec![
                        "wall".into(),
                        "floor".into(),
                        "floor".into(),
                        "floor".into(),
                        "wall".into(),
                    ],
                    vec![
                        "wall".into(),
                        "wall".into(),
                        "wall".into(),
                        "wall".into(),
                        "wall".into(),
                    ],
                ],
                features: vec![TacticalFeaturePayload {
                    glyph: 'A',
                    feature_type: "cover".to_string(),
                    label: "Stalagmite".to_string(),
                    positions: vec![[1, 1]],
                }],
            },
            entities: vec![TacticalEntityPayload {
                id: "player-1".to_string(),
                name: "Tormund".to_string(),
                x: 2,
                y: 1,
                size: 1,
                faction: "player".to_string(),
            }],
            zones: vec![],
        };

        let json = serde_json::to_string(&payload).expect("serialize");
        let deserialized: TacticalStatePayload = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.room_id, "mawdeep_entrance");
        assert_eq!(deserialized.grid.width, 5);
        assert_eq!(deserialized.grid.height, 3);
        assert_eq!(deserialized.grid.cells.len(), 3);
        assert_eq!(deserialized.grid.features.len(), 1);
        assert_eq!(deserialized.grid.features[0].glyph, 'A');
        assert_eq!(deserialized.entities.len(), 1);
        assert_eq!(deserialized.entities[0].name, "Tormund");
        assert_eq!(deserialized.entities[0].faction, "player");
        assert!(deserialized.zones.is_empty());
    }

    #[test]
    fn tactical_state_payload_empty_grid() {
        let payload = TacticalStatePayload {
            room_id: "empty_room".to_string(),
            grid: TacticalGridPayload {
                width: 0,
                height: 0,
                cells: vec![],
                features: vec![],
            },
            entities: vec![],
            zones: vec![],
        };

        let json = serde_json::to_string(&payload).expect("serialize");
        let deserialized: TacticalStatePayload = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.grid.width, 0);
        assert!(deserialized.grid.cells.is_empty());
    }

    #[test]
    fn tactical_grid_payload_preserves_cell_types() {
        let grid = TacticalGridPayload {
            width: 3,
            height: 1,
            cells: vec![vec![
                "floor".into(),
                "water".into(),
                "difficult_terrain".into(),
            ]],
            features: vec![],
        };

        let json = serde_json::to_string(&grid).expect("serialize");
        let deserialized: TacticalGridPayload = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.cells[0][0], "floor");
        assert_eq!(deserialized.cells[0][1], "water");
        assert_eq!(deserialized.cells[0][2], "difficult_terrain");
    }

    #[test]
    fn tactical_feature_payload_positions_preserved() {
        let feature = TacticalFeaturePayload {
            glyph: 'B',
            feature_type: "hazard".to_string(),
            label: "Spike Trap".to_string(),
            positions: vec![[3, 4], [3, 5]],
        };

        let json = serde_json::to_string(&feature).expect("serialize");
        let deserialized: TacticalFeaturePayload =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.positions.len(), 2);
        assert_eq!(deserialized.positions[0], [3, 4]);
        assert_eq!(deserialized.positions[1], [3, 5]);
    }

    #[test]
    fn tactical_entity_payload_round_trip() {
        let entity = TacticalEntityPayload {
            id: "goblin-1".to_string(),
            name: "Gruk".to_string(),
            x: 4,
            y: 2,
            size: 1,
            faction: "hostile".to_string(),
        };

        let json = serde_json::to_string(&entity).expect("serialize");
        let deserialized: TacticalEntityPayload = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.id, "goblin-1");
        assert_eq!(deserialized.x, 4);
        assert_eq!(deserialized.y, 2);
        assert_eq!(deserialized.faction, "hostile");
    }
}

// ==========================================================================
// AC-2: TacticalActionPayload serializes/deserializes correctly
// ==========================================================================

mod tactical_action_payload_tests {
    use super::*;

    #[test]
    fn tactical_action_move_round_trip() {
        let payload = TacticalActionPayload {
            action_type: "move".to_string(),
            entity_id: Some("player-1".to_string()),
            target: Some([3, 2]),
            ability: None,
        };

        let json = serde_json::to_string(&payload).expect("serialize");
        let deserialized: TacticalActionPayload = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.action_type, "move");
        assert_eq!(deserialized.entity_id.as_deref(), Some("player-1"));
        assert_eq!(deserialized.target, Some([3, 2]));
        assert!(deserialized.ability.is_none());
    }

    #[test]
    fn tactical_action_inspect_round_trip() {
        let payload = TacticalActionPayload {
            action_type: "inspect".to_string(),
            entity_id: None,
            target: Some([1, 1]),
            ability: None,
        };

        let json = serde_json::to_string(&payload).expect("serialize");
        let deserialized: TacticalActionPayload = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.action_type, "inspect");
        assert!(deserialized.entity_id.is_none());
        assert_eq!(deserialized.target, Some([1, 1]));
    }

    #[test]
    fn tactical_action_target_with_ability() {
        let payload = TacticalActionPayload {
            action_type: "target".to_string(),
            entity_id: Some("goblin-1".to_string()),
            target: Some([4, 2]),
            ability: Some("fireball".to_string()),
        };

        let json = serde_json::to_string(&payload).expect("serialize");
        let deserialized: TacticalActionPayload = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.action_type, "target");
        assert_eq!(deserialized.ability.as_deref(), Some("fireball"));
    }
}

// ==========================================================================
// AC-3 & AC-4: TACTICAL_STATE and TACTICAL_ACTION variants in GameMessage
// ==========================================================================

mod game_message_variant_tests {
    use super::*;

    #[test]
    fn tactical_state_message_serializes_with_correct_type_tag() {
        let msg = GameMessage::TacticalState {
            payload: TacticalStatePayload {
                room_id: "entrance".to_string(),
                grid: TacticalGridPayload {
                    width: 3,
                    height: 3,
                    cells: vec![
                        vec!["wall".into(); 3],
                        vec!["wall".into(), "floor".into(), "wall".into()],
                        vec!["wall".into(); 3],
                    ],
                    features: vec![],
                },
                entities: vec![],
                zones: vec![],
            },
            player_id: "p1".to_string(),
        };

        let json = serde_json::to_string(&msg).expect("serialize");
        let v: serde_json::Value = serde_json::from_str(&json).expect("parse");
        assert_eq!(v["type"], "TACTICAL_STATE");
        assert_eq!(v["payload"]["room_id"], "entrance");
    }

    #[test]
    fn tactical_action_message_serializes_with_correct_type_tag() {
        let msg = GameMessage::TacticalAction {
            payload: TacticalActionPayload {
                action_type: "move".to_string(),
                entity_id: Some("player-1".to_string()),
                target: Some([2, 3]),
                ability: None,
            },
            player_id: "p1".to_string(),
        };

        let json = serde_json::to_string(&msg).expect("serialize");
        let v: serde_json::Value = serde_json::from_str(&json).expect("parse");
        assert_eq!(v["type"], "TACTICAL_ACTION");
        assert_eq!(v["payload"]["action_type"], "move");
    }

    #[test]
    fn tactical_state_message_deserializes_from_json() {
        let json = r#"{
            "type": "TACTICAL_STATE",
            "payload": {
                "room_id": "test_room",
                "grid": {
                    "width": 1,
                    "height": 1,
                    "cells": [["floor"]],
                    "features": []
                },
                "entities": [],
                "zones": []
            },
            "player_id": "p1"
        }"#;

        let msg: GameMessage = serde_json::from_str(json).expect("deserialize");
        match msg {
            GameMessage::TacticalState { payload, player_id } => {
                assert_eq!(payload.room_id, "test_room");
                assert_eq!(payload.grid.width, 1);
                assert_eq!(player_id, "p1");
            }
            other => panic!("Expected TacticalState, got {:?}", other),
        }
    }

    #[test]
    fn tactical_action_message_deserializes_from_json() {
        let json = r#"{
            "type": "TACTICAL_ACTION",
            "payload": {
                "action_type": "inspect",
                "entity_id": null,
                "target": [5, 5],
                "ability": null
            },
            "player_id": "p2"
        }"#;

        let msg: GameMessage = serde_json::from_str(json).expect("deserialize");
        match msg {
            GameMessage::TacticalAction { payload, player_id } => {
                assert_eq!(payload.action_type, "inspect");
                assert_eq!(payload.target, Some([5, 5]));
                assert_eq!(player_id, "p2");
            }
            other => panic!("Expected TacticalAction, got {:?}", other),
        }
    }
}

// ==========================================================================
// AC-5 & AC-6: Dispatch emits TACTICAL_STATE on room entry (when grid present)
// These tests verify the tactical dispatch module exists and is wired.
// ==========================================================================

mod dispatch_wiring_tests {
    use super::*;

    /// AC-8 wiring test: TacticalStatePayload can be constructed and embedded
    /// in a GameMessage. This verifies the protocol types are wired into the
    /// message enum and are usable by dispatch code.
    /// (The actual dispatch integration test lives in sidequest-server.)
    #[test]
    fn tactical_state_embeds_in_game_message() {
        let payload = TacticalStatePayload {
            room_id: "dispatch_test_room".to_string(),
            grid: TacticalGridPayload {
                width: 2,
                height: 2,
                cells: vec![
                    vec!["wall".into(), "wall".into()],
                    vec!["wall".into(), "floor".into()],
                ],
                features: vec![],
            },
            entities: vec![],
            zones: vec![],
        };
        let msg = GameMessage::TacticalState {
            payload,
            player_id: "server".to_string(),
        };
        // Verify the message can be serialized (as dispatch would do before sending)
        let json = serde_json::to_string(&msg).expect("dispatch must be able to serialize");
        assert!(json.contains("TACTICAL_STATE"));
        assert!(json.contains("dispatch_test_room"));
    }
}

// ==========================================================================
// Effect zone payload (for completeness — zones are story 29-13 but the
// payload struct is defined here for the protocol message)
// ==========================================================================

mod effect_zone_payload_tests {
    use super::*;

    #[test]
    fn effect_zone_payload_circle_round_trip() {
        let zone = EffectZonePayload {
            id: "zone-1".to_string(),
            zone_type: "circle".to_string(),
            params: serde_json::json!({ "center": [3, 3], "radius": 2 }),
            label: "Fireball".to_string(),
            color: Some("#FF4400".to_string()),
        };

        let json = serde_json::to_string(&zone).expect("serialize");
        let deserialized: EffectZonePayload = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.zone_type, "circle");
        assert_eq!(deserialized.label, "Fireball");
        assert_eq!(deserialized.color.as_deref(), Some("#FF4400"));
    }

    #[test]
    fn effect_zone_payload_no_color() {
        let zone = EffectZonePayload {
            id: "zone-2".to_string(),
            zone_type: "rect".to_string(),
            params: serde_json::json!({ "x": 0, "y": 0, "w": 4, "h": 2 }),
            label: "Darkness".to_string(),
            color: None,
        };

        let json = serde_json::to_string(&zone).expect("serialize");
        let deserialized: EffectZonePayload = serde_json::from_str(&json).expect("deserialize");
        assert!(deserialized.color.is_none());
        assert_eq!(deserialized.zone_type, "rect");
    }
}

// ==========================================================================
// Rust lang-review rule enforcement
// ==========================================================================

mod rule_enforcement_tests {
    use super::*;

    /// Rule #2: Payload structs should derive Serialize + Deserialize.
    /// Verifying by attempting serde operations (covered by round-trip tests above).
    /// This test specifically checks that the structs implement the traits.
    #[test]
    fn payload_structs_are_serde() {
        fn assert_serde<T: serde::Serialize + for<'de> serde::Deserialize<'de>>() {}
        assert_serde::<TacticalStatePayload>();
        assert_serde::<TacticalActionPayload>();
        assert_serde::<TacticalGridPayload>();
        assert_serde::<TacticalFeaturePayload>();
        assert_serde::<TacticalEntityPayload>();
        assert_serde::<EffectZonePayload>();
    }

    /// Rule #9: Payload structs should derive PartialEq for testability.
    #[test]
    fn payload_structs_derive_partial_eq() {
        let p1 = TacticalEntityPayload {
            id: "a".into(),
            name: "b".into(),
            x: 1,
            y: 2,
            size: 1,
            faction: "player".into(),
        };
        let p2 = p1.clone();
        assert_eq!(p1, p2);
    }

    /// Rule #9: Payload structs should derive Clone.
    #[test]
    fn payload_structs_derive_clone() {
        let payload = TacticalStatePayload {
            room_id: "r1".to_string(),
            grid: TacticalGridPayload {
                width: 1,
                height: 1,
                cells: vec![vec!["floor".into()]],
                features: vec![],
            },
            entities: vec![],
            zones: vec![],
        };
        let cloned = payload.clone();
        assert_eq!(cloned.room_id, "r1");
    }
}

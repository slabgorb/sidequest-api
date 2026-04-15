//! RED tests for Story 13-3: ActionReveal protocol message
//!
//! Defines the wire contract for ACTION_REVEAL — broadcast of all submitted
//! player actions when a sealed-letter turn resolves. Tests cover:
//! - ActionRevealPayload and PlayerActionEntry type construction
//! - Serde round-trip (serialize → deserialize → equality)
//! - Wire format compatibility (JSON field names match client expectations)
//! - deny_unknown_fields enforcement
//!
//! All tests fail until Dev adds the ActionReveal variant to GameMessage.

use super::*;

/// Story 33-18 sweep: PlayerActionEntry fields are now `NonBlankString`.
/// Local macro keeps literal construction terse while routing through the
/// validating constructor.
macro_rules! nbs {
    ($s:expr) => {
        crate::types::NonBlankString::new($s).expect("test literal must be non-blank")
    };
}

// ===========================================================================
// AC-5: Protocol defines ActionReveal message shape (serde)
// ===========================================================================

mod action_reveal_type_tests {
    use super::*;

    #[test]
    fn action_reveal_payload_constructs() {
        // ActionRevealPayload should carry a list of player actions and turn metadata
        let payload = ActionRevealPayload {
            actions: vec![
                PlayerActionEntry {
                    character_name: nbs!("Thorn"),
                    player_id: nbs!("player-1"),
                    action: nbs!("I search the room for traps"),
                },
                PlayerActionEntry {
                    character_name: nbs!("Elara"),
                    player_id: nbs!("player-2"),
                    action: nbs!("I guard the door"),
                },
            ],
            turn_number: 3,
            auto_resolved: vec![],
        };
        assert_eq!(payload.actions.len(), 2);
        assert_eq!(payload.turn_number, 3);
    }

    #[test]
    fn player_action_entry_has_required_fields() {
        let entry = PlayerActionEntry {
            character_name: nbs!("Kael"),
            player_id: nbs!("p1"),
            action: nbs!("I cast fireball"),
        };
        assert_eq!(entry.character_name.as_str(), "Kael");
        assert_eq!(entry.player_id.as_str(), "p1");
        assert_eq!(entry.action.as_str(), "I cast fireball");
    }

    #[test]
    fn action_reveal_with_auto_resolved_players() {
        // When a player times out, their name appears in auto_resolved
        let payload = ActionRevealPayload {
            actions: vec![PlayerActionEntry {
                character_name: nbs!("Thorn"),
                player_id: nbs!("player-1"),
                action: nbs!("I search the room"),
            }],
            turn_number: 5,
            auto_resolved: vec!["Elara".into()],
        };
        assert_eq!(payload.auto_resolved.len(), 1);
        assert_eq!(payload.auto_resolved[0], "Elara");
    }

    #[test]
    fn action_reveal_empty_auto_resolved() {
        let payload = ActionRevealPayload {
            actions: vec![PlayerActionEntry {
                character_name: nbs!("Thorn"),
                player_id: nbs!("player-1"),
                action: nbs!("I attack"),
            }],
            turn_number: 1,
            auto_resolved: vec![],
        };
        assert!(payload.auto_resolved.is_empty());
    }
}

// ===========================================================================
// Serde round-trip
// ===========================================================================

mod action_reveal_serde_tests {
    use super::*;

    #[test]
    fn action_reveal_round_trip() {
        let msg = GameMessage::ActionReveal {
            payload: ActionRevealPayload {
                actions: vec![
                    PlayerActionEntry {
                        character_name: nbs!("Thorn"),
                        player_id: nbs!("player-1"),
                        action: nbs!("I search the room"),
                    },
                    PlayerActionEntry {
                        character_name: nbs!("Elara"),
                        player_id: nbs!("player-2"),
                        action: nbs!("I guard the door"),
                    },
                ],
                turn_number: 3,
                auto_resolved: vec![],
            },
            player_id: "server".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: GameMessage = serde_json::from_str(&json).unwrap();

        assert!(json.contains(r#""type":"ACTION_REVEAL""#));
        assert_eq!(msg, decoded);
    }

    #[test]
    fn action_reveal_with_auto_resolved_round_trip() {
        let msg = GameMessage::ActionReveal {
            payload: ActionRevealPayload {
                actions: vec![PlayerActionEntry {
                    character_name: nbs!("Thorn"),
                    player_id: nbs!("player-1"),
                    action: nbs!("I search the room"),
                }],
                turn_number: 5,
                auto_resolved: vec!["Elara".into()],
            },
            player_id: "server".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: GameMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn action_reveal_json_contains_character_names() {
        let msg = GameMessage::ActionReveal {
            payload: ActionRevealPayload {
                actions: vec![PlayerActionEntry {
                    character_name: nbs!("Lyra Dawnforge"),
                    player_id: nbs!("p2"),
                    action: nbs!("I heal the wounded"),
                }],
                turn_number: 1,
                auto_resolved: vec![],
            },
            player_id: "server".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("Lyra Dawnforge"));
        assert!(json.contains("I heal the wounded"));
    }
}

// ===========================================================================
// Wire compatibility — JSON matches expected client format
// ===========================================================================

mod action_reveal_wire_tests {
    use super::*;

    #[test]
    fn action_reveal_wire_format() {
        let json = r#"{
            "type": "ACTION_REVEAL",
            "payload": {
                "actions": [
                    {
                        "character_name": "Thorn",
                        "player_id": "player-1",
                        "action": "I search the room"
                    },
                    {
                        "character_name": "Elara",
                        "player_id": "player-2",
                        "action": "I guard the door"
                    }
                ],
                "turn_number": 3,
                "auto_resolved": []
            },
            "player_id": "server"
        }"#;
        let msg: GameMessage = serde_json::from_str(json).unwrap();
        match &msg {
            GameMessage::ActionReveal { payload, player_id } => {
                assert_eq!(payload.actions.len(), 2);
                assert_eq!(payload.actions[0].character_name.as_str(), "Thorn");
                assert_eq!(payload.actions[0].action.as_str(), "I search the room");
                assert_eq!(payload.actions[1].character_name.as_str(), "Elara");
                assert_eq!(payload.turn_number, 3);
                assert!(payload.auto_resolved.is_empty());
                assert_eq!(player_id, "server");
            }
            other => panic!("expected ActionReveal, got {:?}", other),
        }
    }

    #[test]
    fn action_reveal_with_auto_resolved_wire_format() {
        let json = r#"{
            "type": "ACTION_REVEAL",
            "payload": {
                "actions": [
                    {
                        "character_name": "Thorn",
                        "player_id": "player-1",
                        "action": "I search the room"
                    }
                ],
                "turn_number": 5,
                "auto_resolved": ["Elara"]
            },
            "player_id": "server"
        }"#;
        let msg: GameMessage = serde_json::from_str(json).unwrap();
        match &msg {
            GameMessage::ActionReveal { payload, .. } => {
                assert_eq!(payload.actions.len(), 1);
                assert_eq!(payload.auto_resolved, vec!["Elara".to_string()]);
            }
            other => panic!("expected ActionReveal, got {:?}", other),
        }
    }
}

// ===========================================================================
// deny_unknown_fields enforcement
// ===========================================================================

mod action_reveal_deny_unknown_tests {
    use super::*;

    #[test]
    fn action_reveal_rejects_extra_payload_fields() {
        let json = r#"{
            "type": "ACTION_REVEAL",
            "payload": {
                "actions": [],
                "turn_number": 1,
                "auto_resolved": [],
                "hacker_field": "gotcha"
            },
            "player_id": ""
        }"#;
        let result: Result<GameMessage, _> = serde_json::from_str(json);
        assert!(
            result.is_err(),
            "extra fields in ActionRevealPayload must be rejected"
        );
    }

    #[test]
    fn player_action_entry_rejects_extra_fields() {
        let json = r#"{
            "type": "ACTION_REVEAL",
            "payload": {
                "actions": [{
                    "character_name": "Thorn",
                    "player_id": "p1",
                    "action": "attack",
                    "secret": "leaked"
                }],
                "turn_number": 1,
                "auto_resolved": []
            },
            "player_id": ""
        }"#;
        let result: Result<GameMessage, _> = serde_json::from_str(json);
        assert!(
            result.is_err(),
            "extra fields in PlayerActionEntry must be rejected"
        );
    }
}

// ===========================================================================
// Edge cases
// ===========================================================================

mod action_reveal_edge_tests {
    use super::*;

    #[test]
    fn action_reveal_single_player_action() {
        let msg = GameMessage::ActionReveal {
            payload: ActionRevealPayload {
                actions: vec![PlayerActionEntry {
                    character_name: nbs!("Solo"),
                    player_id: nbs!("player-1"),
                    action: nbs!("I open the chest"),
                }],
                turn_number: 1,
                auto_resolved: vec![],
            },
            player_id: "server".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: GameMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn action_reveal_empty_actions_list() {
        // Edge case: all players auto-resolved, no explicit actions
        let msg = GameMessage::ActionReveal {
            payload: ActionRevealPayload {
                actions: vec![],
                turn_number: 2,
                auto_resolved: vec!["Thorn".into(), "Elara".into()],
            },
            player_id: "server".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: GameMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }

    #[test]
    fn action_reveal_preserves_action_text_with_special_characters() {
        let msg = GameMessage::ActionReveal {
            payload: ActionRevealPayload {
                actions: vec![PlayerActionEntry {
                    character_name: nbs!("Thorn"),
                    player_id: nbs!("player-1"),
                    action: nbs!(r#"I shout "For glory!" and charge the dragon"#),
                }],
                turn_number: 1,
                auto_resolved: vec![],
            },
            player_id: "server".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: GameMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, decoded);
    }
}

//! Story 19-6: GameMessage::ResourceMinReached protocol variant
//!
//! Tests that RESOURCE_MIN_REACHED GameMessage variant exists with correct fields,
//! serializes/deserializes properly.
//!
//! AC coverage:
//! - AC3: GameMessage when resource hits min

use sidequest_protocol::{GameMessage, NonBlankString};

/// ResourceMinReached variant exists and can be constructed.
#[test]
fn resource_min_reached_constructible() {
    let msg = GameMessage::ResourceMinReached {
        payload: sidequest_protocol::ResourceMinReachedPayload {
            resource_name: NonBlankString::new("heat").expect("heat is non-blank"),
            min_value: 0.0,
        },
        player_id: "server".to_string(),
    };

    match &msg {
        GameMessage::ResourceMinReached { payload, player_id } => {
            assert_eq!(payload.resource_name.as_str(), "heat");
            assert!((payload.min_value - 0.0).abs() < 1e-9);
            assert_eq!(player_id, "server");
        }
        _ => panic!("Expected ResourceMinReached variant"),
    }
}

/// ResourceMinReached serializes with type = "RESOURCE_MIN_REACHED".
#[test]
fn resource_min_reached_serializes_with_correct_type_tag() {
    let msg = GameMessage::ResourceMinReached {
        payload: sidequest_protocol::ResourceMinReachedPayload {
            resource_name: NonBlankString::new("luck").expect("luck is non-blank"),
            min_value: 0.0,
        },
        player_id: "server".to_string(),
    };

    let json = serde_json::to_string(&msg).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(value["type"].as_str().unwrap(), "RESOURCE_MIN_REACHED");
    assert_eq!(value["payload"]["resource_name"].as_str().unwrap(), "luck");
    assert!((value["payload"]["min_value"].as_f64().unwrap() - 0.0).abs() < 1e-9);
}

/// ResourceMinReached round-trips through JSON.
#[test]
fn resource_min_reached_json_roundtrip() {
    let msg = GameMessage::ResourceMinReached {
        payload: sidequest_protocol::ResourceMinReachedPayload {
            resource_name: NonBlankString::new("humanity").expect("humanity is non-blank"),
            min_value: -5.0,
        },
        player_id: "server".to_string(),
    };

    let json = serde_json::to_string(&msg).unwrap();
    let back: GameMessage = serde_json::from_str(&json).unwrap();

    assert_eq!(
        msg, back,
        "ResourceMinReached should survive JSON round-trip"
    );
}

/// Deserialize from raw JSON.
#[test]
fn resource_min_reached_deserializes_from_raw_json() {
    let json = r#"{
        "type": "RESOURCE_MIN_REACHED",
        "payload": {
            "resource_name": "heat",
            "min_value": 0.0
        },
        "player_id": "server"
    }"#;

    let msg: GameMessage = serde_json::from_str(json).unwrap();
    match msg {
        GameMessage::ResourceMinReached { payload, .. } => {
            assert_eq!(payload.resource_name.as_str(), "heat");
            assert!((payload.min_value - 0.0).abs() < 1e-9);
        }
        _ => panic!("Expected ResourceMinReached"),
    }
}

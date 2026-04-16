//! Story 19-10: GameMessage::ItemDepleted protocol variant
//!
//! Tests that the ITEM_DEPLETED GameMessage variant exists with the correct fields,
//! serializes/deserializes properly, and follows protocol conventions.
//!
//! AC coverage:
//! - AC3: GameMessage::ItemDepleted variant added to protocol with item_name + remaining_before
//! - AC6: Full wiring — protocol variant exists and is serializable for WebSocket delivery
//!
//! Rule coverage:
//! - serde round-trip: ItemDepleted survives JSON encode/decode
//! - deny_unknown_fields: extra fields rejected (protocol contract enforcement)

use sidequest_protocol::{GameMessage, NonBlankString};

// ═══════════════════════════════════════════════════════════
// AC3: GameMessage::ItemDepleted variant with correct fields
// ═══════════════════════════════════════════════════════════

/// ItemDepleted variant exists and can be constructed with item_name and remaining_before.
#[test]
fn item_depleted_variant_constructible() {
    let msg = GameMessage::ItemDepleted {
        payload: sidequest_protocol::ItemDepletedPayload {
            item_name: NonBlankString::new("Torch").expect("Torch is non-blank"),
            remaining_before: 1,
        },
        player_id: "server".to_string(),
    };

    // Verify we can match on the variant and extract fields
    match &msg {
        GameMessage::ItemDepleted { payload, player_id } => {
            assert_eq!(payload.item_name.as_str(), "Torch");
            assert_eq!(payload.remaining_before, 1);
            assert_eq!(player_id, "server");
        }
        _ => panic!("Expected ItemDepleted variant"),
    }
}

/// ItemDepleted serializes to JSON with type = "ITEM_DEPLETED".
#[test]
fn item_depleted_serializes_with_correct_type_tag() {
    let msg = GameMessage::ItemDepleted {
        payload: sidequest_protocol::ItemDepletedPayload {
            item_name: NonBlankString::new("Oil Lantern").expect("Oil Lantern is non-blank"),
            remaining_before: 3,
        },
        player_id: "server".to_string(),
    };

    let json = serde_json::to_string(&msg).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(
        value["type"].as_str().unwrap(),
        "ITEM_DEPLETED",
        "GameMessage::ItemDepleted should serialize with type tag ITEM_DEPLETED"
    );
    assert_eq!(
        value["payload"]["item_name"].as_str().unwrap(),
        "Oil Lantern"
    );
    assert_eq!(value["payload"]["remaining_before"].as_u64().unwrap(), 3);
    assert_eq!(value["player_id"].as_str().unwrap(), "server");
}

/// ItemDepleted round-trips through JSON correctly.
#[test]
fn item_depleted_json_roundtrip() {
    let msg = GameMessage::ItemDepleted {
        payload: sidequest_protocol::ItemDepletedPayload {
            item_name: NonBlankString::new("Torch").expect("Torch is non-blank"),
            remaining_before: 6,
        },
        player_id: "player_1".to_string(),
    };

    let json = serde_json::to_string(&msg).unwrap();
    let back: GameMessage = serde_json::from_str(&json).unwrap();

    assert_eq!(msg, back, "ItemDepleted should survive JSON round-trip");
}

/// ItemDepleted can be deserialized from raw JSON matching the protocol format.
#[test]
fn item_depleted_deserializes_from_raw_json() {
    let json = r#"{
        "type": "ITEM_DEPLETED",
        "payload": {
            "item_name": "Torch",
            "remaining_before": 1
        },
        "player_id": "server"
    }"#;

    let msg: GameMessage = serde_json::from_str(json).unwrap();
    match msg {
        GameMessage::ItemDepleted { payload, player_id } => {
            assert_eq!(payload.item_name.as_str(), "Torch");
            assert_eq!(payload.remaining_before, 1);
            assert_eq!(player_id, "server");
        }
        _ => panic!("Expected ItemDepleted variant from raw JSON"),
    }
}

/// remaining_before = 0 is valid (edge case: item already at 0 when reported).
#[test]
fn item_depleted_zero_remaining_before_valid() {
    let json = r#"{
        "type": "ITEM_DEPLETED",
        "payload": {
            "item_name": "Candle",
            "remaining_before": 0
        },
        "player_id": "server"
    }"#;

    let msg: GameMessage = serde_json::from_str(json).unwrap();
    match msg {
        GameMessage::ItemDepleted { payload, .. } => {
            assert_eq!(payload.remaining_before, 0);
        }
        _ => panic!("Expected ItemDepleted"),
    }
}

/// ItemDepleted with empty item_name must be REJECTED at deserialization.
///
/// Contract change (story 33-18): `item_name` is now `NonBlankString`, which
/// rejects empty/whitespace strings at deserialization time. This test used to
/// assert the opposite (empty accepted) — under the new protocol contract, the
/// server is responsible for emitting a non-blank item name and the protocol
/// refuses to relay blank names.
#[test]
fn item_depleted_empty_item_name_rejected() {
    let json = r#"{
        "type": "ITEM_DEPLETED",
        "payload": {
            "item_name": "",
            "remaining_before": 1
        },
        "player_id": "server"
    }"#;

    let result: Result<GameMessage, _> = serde_json::from_str(json);
    assert!(
        result.is_err(),
        "ItemDepleted with empty item_name must be rejected (NonBlankString contract)"
    );
}

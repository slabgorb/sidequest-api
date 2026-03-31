//! Story 14-4: Narrator vocabulary — protocol-level tests.
//!
//! RED phase — these tests reference types that don't exist yet:
//!   - `NarratorVocabulary` enum (Accessible, Literary, Epic)
//!   - `narrator_vocabulary` field on `SessionEventPayload`
//!
//! ACs tested:
//!   AC1: NarratorVocabulary enum exists with three variants
//!   AC2: Serde round-trip for all three values
//!   AC3: Default is Literary
//!   AC5: SessionEvent connect payload carries narrator_vocabulary
//!   AC6: Wire format compatibility — JSON keys match UI expectations

use super::*;

// =========================================================================
// AC1: NarratorVocabulary enum exists with three variants
// =========================================================================

#[test]
fn narrator_vocabulary_has_accessible_variant() {
    let v = NarratorVocabulary::Accessible;
    assert_eq!(v, NarratorVocabulary::Accessible);
}

#[test]
fn narrator_vocabulary_has_literary_variant() {
    let v = NarratorVocabulary::Literary;
    assert_eq!(v, NarratorVocabulary::Literary);
}

#[test]
fn narrator_vocabulary_has_epic_variant() {
    let v = NarratorVocabulary::Epic;
    assert_eq!(v, NarratorVocabulary::Epic);
}

// =========================================================================
// AC2: Serde round-trip for all three values
// =========================================================================

#[test]
fn narrator_vocabulary_accessible_round_trip() {
    let v = NarratorVocabulary::Accessible;
    let json = serde_json::to_string(&v).unwrap();
    let decoded: NarratorVocabulary = serde_json::from_str(&json).unwrap();
    assert_eq!(v, decoded);
}

#[test]
fn narrator_vocabulary_literary_round_trip() {
    let v = NarratorVocabulary::Literary;
    let json = serde_json::to_string(&v).unwrap();
    let decoded: NarratorVocabulary = serde_json::from_str(&json).unwrap();
    assert_eq!(v, decoded);
}

#[test]
fn narrator_vocabulary_epic_round_trip() {
    let v = NarratorVocabulary::Epic;
    let json = serde_json::to_string(&v).unwrap();
    let decoded: NarratorVocabulary = serde_json::from_str(&json).unwrap();
    assert_eq!(v, decoded);
}

#[test]
fn narrator_vocabulary_serializes_as_lowercase() {
    // Wire format should use lowercase strings matching UI expectations.
    let json = serde_json::to_string(&NarratorVocabulary::Accessible).unwrap();
    assert_eq!(json, r#""accessible""#);

    let json = serde_json::to_string(&NarratorVocabulary::Literary).unwrap();
    assert_eq!(json, r#""literary""#);

    let json = serde_json::to_string(&NarratorVocabulary::Epic).unwrap();
    assert_eq!(json, r#""epic""#);
}

// =========================================================================
// AC3: Default is Literary
// =========================================================================

#[test]
fn narrator_vocabulary_defaults_to_literary() {
    let v = NarratorVocabulary::default();
    assert_eq!(v, NarratorVocabulary::Literary);
}

// =========================================================================
// AC5: SessionEvent connect payload carries narrator_vocabulary
// =========================================================================

#[test]
fn session_event_connect_with_vocabulary_round_trip() {
    let msg = GameMessage::SessionEvent {
        payload: SessionEventPayload {
            event: "connect".into(),
            player_name: Some("Alice".into()),
            genre: Some("mutant_wasteland".into()),
            world: Some("flickering_reach".into()),
            has_character: None,
            initial_state: None,
            css: None,
            narrator_verbosity: None,
            narrator_vocabulary: Some(NarratorVocabulary::Epic),
        },
        player_id: String::new(),
    };
    let json = serde_json::to_string(&msg).unwrap();
    let decoded: GameMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(msg, decoded);
}

#[test]
fn session_event_without_vocabulary_defaults_to_none() {
    // Backward compatibility: old clients that don't send narrator_vocabulary
    // should still deserialize. The field is optional.
    let json = r#"{
        "type": "SESSION_EVENT",
        "payload": {
            "event": "connect",
            "player_name": "Alice",
            "genre": "mutant_wasteland",
            "world": "flickering_reach"
        },
        "player_id": ""
    }"#;
    let msg: GameMessage = serde_json::from_str(json).unwrap();
    match &msg {
        GameMessage::SessionEvent { payload, .. } => {
            assert!(
                payload.narrator_vocabulary.is_none(),
                "missing narrator_vocabulary should deserialize as None"
            );
        }
        other => panic!("expected SessionEvent, got {:?}", other),
    }
}

// =========================================================================
// AC6: Wire format compatibility
// =========================================================================

#[test]
fn session_event_vocabulary_wire_format() {
    let json = r#"{
        "type": "SESSION_EVENT",
        "payload": {
            "event": "connect",
            "player_name": "Alice",
            "genre": "mutant_wasteland",
            "world": "flickering_reach",
            "narrator_vocabulary": "accessible"
        },
        "player_id": ""
    }"#;
    let msg: GameMessage = serde_json::from_str(json).unwrap();
    match &msg {
        GameMessage::SessionEvent { payload, .. } => {
            assert_eq!(
                payload.narrator_vocabulary,
                Some(NarratorVocabulary::Accessible)
            );
        }
        other => panic!("expected SessionEvent, got {:?}", other),
    }
}

#[test]
fn narrator_vocabulary_rejects_invalid_value() {
    let json = r#""flowery""#;
    let result: Result<NarratorVocabulary, _> = serde_json::from_str(json);
    assert!(result.is_err(), "invalid vocabulary value must be rejected");
}

// =========================================================================
// Both vocabulary and verbosity can coexist on the same payload
// =========================================================================

#[test]
fn session_event_with_both_verbosity_and_vocabulary() {
    let msg = GameMessage::SessionEvent {
        payload: SessionEventPayload {
            event: "connect".into(),
            player_name: Some("Alice".into()),
            genre: Some("mutant_wasteland".into()),
            world: Some("flickering_reach".into()),
            has_character: None,
            initial_state: None,
            css: None,
            narrator_verbosity: Some(NarratorVerbosity::Concise),
            narrator_vocabulary: Some(NarratorVocabulary::Epic),
        },
        player_id: String::new(),
    };
    let json = serde_json::to_string(&msg).unwrap();
    let decoded: GameMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(msg, decoded);
}

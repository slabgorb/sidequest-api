//! Story 14-3: Narrator verbosity — protocol-level tests.
//!
//! RED phase — these tests reference types that don't exist yet:
//!   - `NarratorVerbosity` enum (Concise, Standard, Verbose)
//!   - `narrator_verbosity` field on `SessionEventPayload`
//!
//! ACs tested:
//!   AC1: NarratorVerbosity enum exists with three variants
//!   AC2: Serde round-trip for all three values
//!   AC3: Default is Standard
//!   AC5: SessionEvent connect payload carries narrator_verbosity
//!   AC6: Wire format compatibility — JSON keys match UI expectations

use super::*;

// =========================================================================
// AC1: NarratorVerbosity enum exists with three variants
// =========================================================================

#[test]
fn narrator_verbosity_has_concise_variant() {
    let v = NarratorVerbosity::Concise;
    assert_eq!(v, NarratorVerbosity::Concise);
}

#[test]
fn narrator_verbosity_has_standard_variant() {
    let v = NarratorVerbosity::Standard;
    assert_eq!(v, NarratorVerbosity::Standard);
}

#[test]
fn narrator_verbosity_has_verbose_variant() {
    let v = NarratorVerbosity::Verbose;
    assert_eq!(v, NarratorVerbosity::Verbose);
}

// =========================================================================
// AC2: Serde round-trip for all three values
// =========================================================================

#[test]
fn narrator_verbosity_concise_round_trip() {
    let v = NarratorVerbosity::Concise;
    let json = serde_json::to_string(&v).unwrap();
    let decoded: NarratorVerbosity = serde_json::from_str(&json).unwrap();
    assert_eq!(v, decoded);
}

#[test]
fn narrator_verbosity_standard_round_trip() {
    let v = NarratorVerbosity::Standard;
    let json = serde_json::to_string(&v).unwrap();
    let decoded: NarratorVerbosity = serde_json::from_str(&json).unwrap();
    assert_eq!(v, decoded);
}

#[test]
fn narrator_verbosity_verbose_round_trip() {
    let v = NarratorVerbosity::Verbose;
    let json = serde_json::to_string(&v).unwrap();
    let decoded: NarratorVerbosity = serde_json::from_str(&json).unwrap();
    assert_eq!(v, decoded);
}

#[test]
fn narrator_verbosity_serializes_as_lowercase() {
    // Wire format should use lowercase strings matching UI expectations.
    let json = serde_json::to_string(&NarratorVerbosity::Concise).unwrap();
    assert_eq!(json, r#""concise""#);

    let json = serde_json::to_string(&NarratorVerbosity::Standard).unwrap();
    assert_eq!(json, r#""standard""#);

    let json = serde_json::to_string(&NarratorVerbosity::Verbose).unwrap();
    assert_eq!(json, r#""verbose""#);
}

// =========================================================================
// AC3: Default is Standard
// =========================================================================

#[test]
fn narrator_verbosity_defaults_to_standard() {
    let v = NarratorVerbosity::default();
    assert_eq!(v, NarratorVerbosity::Standard);
}

// =========================================================================
// AC5: SessionEvent connect payload carries narrator_verbosity
// =========================================================================

#[test]
fn session_event_connect_with_verbosity_round_trip() {
    let msg = GameMessage::SessionEvent {
        payload: SessionEventPayload {
            event: "connect".into(),
            player_name: Some("Alice".into()),
            genre: Some("mutant_wasteland".into()),
            world: Some("flickering_reach".into()),
            has_character: None,
            initial_state: None,
            css: None,
            narrator_verbosity: Some(NarratorVerbosity::Verbose),
            narrator_vocabulary: None,
            image_cooldown_seconds: None,
        },
        player_id: String::new(),
    };
    let json = serde_json::to_string(&msg).unwrap();
    let decoded: GameMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(msg, decoded);
}

#[test]
fn session_event_without_verbosity_defaults_to_none() {
    // Backward compatibility: old clients that don't send narrator_verbosity
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
                payload.narrator_verbosity.is_none(),
                "missing narrator_verbosity should deserialize as None"
            );
        }
        other => panic!("expected SessionEvent, got {:?}", other),
    }
}

// =========================================================================
// AC6: Wire format compatibility
// =========================================================================

#[test]
fn session_event_verbosity_wire_format() {
    let json = r#"{
        "type": "SESSION_EVENT",
        "payload": {
            "event": "connect",
            "player_name": "Alice",
            "genre": "mutant_wasteland",
            "world": "flickering_reach",
            "narrator_verbosity": "concise"
        },
        "player_id": ""
    }"#;
    let msg: GameMessage = serde_json::from_str(json).unwrap();
    match &msg {
        GameMessage::SessionEvent { payload, .. } => {
            assert_eq!(payload.narrator_verbosity, Some(NarratorVerbosity::Concise));
        }
        other => panic!("expected SessionEvent, got {:?}", other),
    }
}

#[test]
fn narrator_verbosity_rejects_invalid_value() {
    let json = r#""extra_verbose""#;
    let result: Result<NarratorVerbosity, _> = serde_json::from_str(json);
    assert!(result.is_err(), "invalid verbosity value must be rejected");
}

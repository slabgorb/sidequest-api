//! Story 18-6: PromptAssembled WatcherEventType variant tests.
//!
//! RED phase — the `PromptAssembled` variant doesn't exist yet in WatcherEventType.

use std::collections::HashMap;
use sidequest_server::{WatcherEvent, WatcherEventType, Severity};

// ============================================================================
// AC-1: PromptAssembled variant exists and serializes correctly
// ============================================================================

#[test]
fn prompt_assembled_variant_exists() {
    // RED: This variant doesn't exist yet
    let event_type = WatcherEventType::PromptAssembled;
    // Must be a valid WatcherEventType
    let json = serde_json::to_string(&event_type).unwrap();
    assert_eq!(
        json, "\"prompt_assembled\"",
        "PromptAssembled must serialize as snake_case per serde rename_all"
    );
}

#[test]
fn prompt_assembled_deserializes_from_snake_case() {
    let event_type: WatcherEventType =
        serde_json::from_str("\"prompt_assembled\"").unwrap();
    assert!(
        matches!(event_type, WatcherEventType::PromptAssembled),
        "Must deserialize 'prompt_assembled' to PromptAssembled variant"
    );
}

// ============================================================================
// AC-1: Full WatcherEvent with PromptAssembled type roundtrips
// ============================================================================

#[test]
fn prompt_assembled_event_roundtrips_through_json() {
    let event = WatcherEvent {
        timestamp: chrono::Utc::now(),
        component: "prompt".to_string(),
        event_type: WatcherEventType::PromptAssembled,
        severity: Severity::Info,
        fields: {
            let mut f = HashMap::new();
            f.insert("agent".to_string(), serde_json::json!("narrator"));
            f.insert("total_tokens".to_string(), serde_json::json!(4200));
            f.insert("section_count".to_string(), serde_json::json!(7));
            f.insert(
                "zones".to_string(),
                serde_json::json!([
                    {"zone": "primacy", "total_tokens": 45, "section_count": 1},
                    {"zone": "early", "total_tokens": 120, "section_count": 2},
                    {"zone": "valley", "total_tokens": 2800, "section_count": 2},
                    {"zone": "late", "total_tokens": 85, "section_count": 1},
                    {"zone": "recency", "total_tokens": 50, "section_count": 1},
                ]),
            );
            f
        },
    };

    let json = serde_json::to_string(&event).unwrap();
    let roundtripped: WatcherEvent = serde_json::from_str(&json).unwrap();

    assert!(
        matches!(roundtripped.event_type, WatcherEventType::PromptAssembled),
        "Event type must survive JSON roundtrip"
    );
    assert_eq!(roundtripped.component, "prompt");
    assert_eq!(
        roundtripped.fields["total_tokens"].as_i64().unwrap(),
        4200,
        "total_tokens field must survive roundtrip"
    );
    assert_eq!(
        roundtripped.fields["zones"].as_array().unwrap().len(),
        5,
        "zones array must have 5 entries"
    );
}

#[test]
fn prompt_assembled_event_has_required_fields() {
    // Verify the contract: PromptAssembled events must have specific fields
    let event = WatcherEvent {
        timestamp: chrono::Utc::now(),
        component: "prompt".to_string(),
        event_type: WatcherEventType::PromptAssembled,
        severity: Severity::Info,
        fields: {
            let mut f = HashMap::new();
            f.insert("agent".to_string(), serde_json::json!("narrator"));
            f.insert("total_tokens".to_string(), serde_json::json!(3500));
            f.insert("section_count".to_string(), serde_json::json!(6));
            f.insert("zones".to_string(), serde_json::json!([]));
            f
        },
    };

    // Required fields for PromptAssembled events
    assert!(event.fields.contains_key("agent"), "Must have 'agent' field");
    assert!(event.fields.contains_key("total_tokens"), "Must have 'total_tokens' field");
    assert!(event.fields.contains_key("section_count"), "Must have 'section_count' field");
    assert!(event.fields.contains_key("zones"), "Must have 'zones' field");
}

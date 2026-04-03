//! Story 18-4: LoreRetrieval watcher event type tests.
//!
//! Tests that the LoreRetrieval variant exists on WatcherEventType,
//! serializes/deserializes correctly, and can carry the required fields.

use sidequest_server::{Severity, WatcherEvent, WatcherEventType};
use std::collections::HashMap;

// ============================================================
// AC-1: LoreRetrieval variant exists on WatcherEventType
// ============================================================

#[test]
fn lore_retrieval_variant_exists() {
    let _evt = WatcherEventType::LoreRetrieval;
}

#[test]
fn lore_retrieval_serializes_to_snake_case() {
    let evt = WatcherEventType::LoreRetrieval;
    let json = serde_json::to_string(&evt).unwrap();
    assert_eq!(json, "\"lore_retrieval\"");
}

#[test]
fn lore_retrieval_deserializes_from_snake_case() {
    let evt: WatcherEventType = serde_json::from_str("\"lore_retrieval\"").unwrap();
    assert!(matches!(evt, WatcherEventType::LoreRetrieval));
}

#[test]
fn lore_retrieval_event_roundtrips_through_json() {
    let event = WatcherEvent {
        timestamp: chrono::Utc::now(),
        component: "lore".to_string(),
        event_type: WatcherEventType::LoreRetrieval,
        severity: Severity::Info,
        fields: {
            let mut f = HashMap::new();
            f.insert("budget".to_string(), serde_json::json!(500));
            f.insert("selected_count".to_string(), serde_json::json!(3));
            f.insert("rejected_count".to_string(), serde_json::json!(5));
            f.insert("tokens_used".to_string(), serde_json::json!(420));
            f.insert(
                "selected".to_string(),
                serde_json::json!([
                    {"id": "hist-001", "category": "history", "tokens": 150},
                    {"id": "geo-002", "category": "geography", "tokens": 120},
                    {"id": "fac-003", "category": "faction", "tokens": 150}
                ]),
            );
            f.insert(
                "rejected".to_string(),
                serde_json::json!([
                    {"id": "char-004", "category": "character", "tokens": 200},
                    {"id": "item-005", "category": "item", "tokens": 180}
                ]),
            );
            f
        },
    };
    let json = serde_json::to_string(&event).unwrap();
    let parsed: WatcherEvent = serde_json::from_str(&json).unwrap();
    assert!(matches!(
        parsed.event_type,
        WatcherEventType::LoreRetrieval
    ));
    assert_eq!(parsed.component, "lore");
    assert_eq!(parsed.fields["budget"], serde_json::json!(500));
    assert_eq!(parsed.fields["selected_count"], serde_json::json!(3));
    assert_eq!(parsed.fields["rejected_count"], serde_json::json!(5));
    assert_eq!(parsed.fields["tokens_used"], serde_json::json!(420));

    let selected = parsed.fields["selected"].as_array().unwrap();
    assert_eq!(selected.len(), 3);
    assert_eq!(selected[0]["id"], "hist-001");

    let rejected = parsed.fields["rejected"].as_array().unwrap();
    assert_eq!(rejected.len(), 2);
}

// ============================================================
// AC-2: Event contract name matches snake_case convention
// ============================================================

#[test]
fn lore_retrieval_event_has_required_fields() {
    // Verify the event can carry all fields needed by the dashboard:
    // budget, selected (with id, category, tokens), rejected (with id, category, tokens),
    // tokens_used, context_hint
    let event = WatcherEvent {
        timestamp: chrono::Utc::now(),
        component: "lore".to_string(),
        event_type: WatcherEventType::LoreRetrieval,
        severity: Severity::Info,
        fields: {
            let mut f = HashMap::new();
            f.insert("budget".to_string(), serde_json::json!(500));
            f.insert("tokens_used".to_string(), serde_json::json!(0));
            f.insert("selected_count".to_string(), serde_json::json!(0));
            f.insert("rejected_count".to_string(), serde_json::json!(0));
            f.insert("selected".to_string(), serde_json::json!([]));
            f.insert("rejected".to_string(), serde_json::json!([]));
            f.insert(
                "context_hint".to_string(),
                serde_json::json!("flickering_reach"),
            );
            f.insert("total_fragments".to_string(), serde_json::json!(12));
            f
        },
    };
    let json = serde_json::to_string(&event).unwrap();
    let parsed: WatcherEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.fields["context_hint"], "flickering_reach");
    assert_eq!(parsed.fields["total_fragments"], 12);
}

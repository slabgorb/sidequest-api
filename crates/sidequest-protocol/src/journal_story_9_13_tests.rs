//! Story 9-13: Journal browse view — protocol types for JOURNAL_REQUEST / JOURNAL_RESPONSE
//!
//! RED phase — these tests reference types that don't exist yet.
//! They will fail to compile until Dev implements:
//!   - GameMessage::JournalRequest variant
//!   - GameMessage::JournalResponse variant
//!   - JournalRequestPayload { category, sort_by }
//!   - JournalResponsePayload { entries }
//!   - JournalEntry struct { fact_id, content, category, source, confidence, learned_turn }
//!   - JournalSortOrder enum (Time, Category)
//!
//! ACs tested: AC1 (protocol types), AC5 (serde round-trip for pipeline)

use super::*;

// ============================================================================
// AC1: JournalRequest / JournalResponse GameMessage variants exist
// ============================================================================

#[test]
fn journal_request_variant_exists() {
    let msg = GameMessage::JournalRequest {
        payload: JournalRequestPayload {
            category: None,
            sort_by: JournalSortOrder::Time,
        },
        player_id: "p1".to_string(),
    };
    // Verify it's the right variant via pattern match
    assert!(matches!(msg, GameMessage::JournalRequest { .. }));
}

#[test]
fn journal_response_variant_exists() {
    let msg = GameMessage::JournalResponse {
        payload: JournalResponsePayload { entries: vec![] },
        player_id: "p1".to_string(),
    };
    assert!(matches!(msg, GameMessage::JournalResponse { .. }));
}

// ============================================================================
// AC1: JournalRequestPayload — category filter + sort order
// ============================================================================

#[test]
fn journal_request_with_category_filter() {
    let payload = JournalRequestPayload {
        category: Some(FactCategory::Lore),
        sort_by: JournalSortOrder::Time,
    };
    assert_eq!(payload.category, Some(FactCategory::Lore));
}

#[test]
fn journal_request_without_category_filter() {
    let payload = JournalRequestPayload {
        category: None,
        sort_by: JournalSortOrder::Time,
    };
    assert!(payload.category.is_none());
}

#[test]
fn journal_request_sort_by_category() {
    let payload = JournalRequestPayload {
        category: None,
        sort_by: JournalSortOrder::Category,
    };
    assert!(matches!(payload.sort_by, JournalSortOrder::Category));
}

// ============================================================================
// AC1: JournalEntry struct — all fields present
// ============================================================================

#[test]
fn journal_entry_has_all_fields() {
    let entry = JournalEntry {
        fact_id: "f1".to_string(),
        content: "The grove's oldest tree radiates corruption".to_string(),
        category: FactCategory::Place,
        source: "Observation".to_string(),
        confidence: "Certain".to_string(),
        learned_turn: 3,
    };
    assert_eq!(entry.fact_id, "f1");
    assert_eq!(entry.content, "The grove's oldest tree radiates corruption");
    assert_eq!(entry.category, FactCategory::Place);
    assert_eq!(entry.source, "Observation");
    assert_eq!(entry.confidence, "Certain");
    assert_eq!(entry.learned_turn, 3);
}

// ============================================================================
// AC1: JournalResponsePayload — carries entries
// ============================================================================

#[test]
fn journal_response_carries_entries() {
    let payload = JournalResponsePayload {
        entries: vec![
            JournalEntry {
                fact_id: "f1".to_string(),
                content: "Corruption in the grove".to_string(),
                category: FactCategory::Place,
                source: "Observation".to_string(),
                confidence: "Certain".to_string(),
                learned_turn: 3,
            },
            JournalEntry {
                fact_id: "f2".to_string(),
                content: "Elder Mirova guards a secret".to_string(),
                category: FactCategory::Person,
                source: "Dialogue".to_string(),
                confidence: "Suspected".to_string(),
                learned_turn: 5,
            },
        ],
    };
    assert_eq!(payload.entries.len(), 2);
    assert_eq!(payload.entries[0].category, FactCategory::Place);
    assert_eq!(payload.entries[1].category, FactCategory::Person);
}

#[test]
fn journal_response_empty_entries() {
    let payload = JournalResponsePayload { entries: vec![] };
    assert!(payload.entries.is_empty());
}

// ============================================================================
// AC1 + AC5: Serde round-trip — wire compatibility
// ============================================================================

#[test]
fn journal_request_serde_round_trip() {
    let msg = GameMessage::JournalRequest {
        payload: JournalRequestPayload {
            category: Some(FactCategory::Quest),
            sort_by: JournalSortOrder::Time,
        },
        player_id: "p1".to_string(),
    };
    let json = serde_json::to_string(&msg).expect("serialize JournalRequest");
    let restored: GameMessage = serde_json::from_str(&json).expect("deserialize JournalRequest");
    match restored {
        GameMessage::JournalRequest { payload, player_id } => {
            assert_eq!(player_id, "p1");
            assert_eq!(payload.category, Some(FactCategory::Quest));
            assert!(matches!(payload.sort_by, JournalSortOrder::Time));
        }
        _ => panic!("expected JournalRequest variant"),
    }
}

#[test]
fn journal_request_serializes_with_type_tag() {
    let msg = GameMessage::JournalRequest {
        payload: JournalRequestPayload {
            category: None,
            sort_by: JournalSortOrder::Time,
        },
        player_id: "p1".to_string(),
    };
    let json = serde_json::to_string(&msg).expect("serialize");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        v["type"], "JOURNAL_REQUEST",
        "must use SCREAMING_CASE type tag"
    );
}

#[test]
fn journal_response_serializes_with_type_tag() {
    let msg = GameMessage::JournalResponse {
        payload: JournalResponsePayload { entries: vec![] },
        player_id: "".to_string(),
    };
    let json = serde_json::to_string(&msg).expect("serialize");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        v["type"], "JOURNAL_RESPONSE",
        "must use SCREAMING_CASE type tag"
    );
}

#[test]
fn journal_response_serde_round_trip_with_entries() {
    let msg = GameMessage::JournalResponse {
        payload: JournalResponsePayload {
            entries: vec![JournalEntry {
                fact_id: "f1".to_string(),
                content: "The runes pulse with ward magic".to_string(),
                category: FactCategory::Lore,
                source: "Discovery".to_string(),
                confidence: "Certain".to_string(),
                learned_turn: 7,
            }],
        },
        player_id: "server".to_string(),
    };
    let json = serde_json::to_string(&msg).expect("serialize JournalResponse");
    let restored: GameMessage = serde_json::from_str(&json).expect("deserialize JournalResponse");
    match restored {
        GameMessage::JournalResponse { payload, player_id } => {
            assert_eq!(player_id, "server");
            assert_eq!(payload.entries.len(), 1);
            assert_eq!(payload.entries[0].fact_id, "f1");
            assert_eq!(payload.entries[0].learned_turn, 7);
            assert_eq!(payload.entries[0].category, FactCategory::Lore);
        }
        _ => panic!("expected JournalResponse variant"),
    }
}

#[test]
fn journal_request_category_none_serializes_without_field() {
    let payload = JournalRequestPayload {
        category: None,
        sort_by: JournalSortOrder::Time,
    };
    let json = serde_json::to_string(&payload).expect("serialize");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    // category: None should be absent from the wire format (skip_serializing_if)
    assert!(
        v.get("category").is_none() || v["category"].is_null(),
        "None category should not appear in JSON or be null"
    );
}

// ============================================================================
// Rule coverage: #2 non_exhaustive on JournalSortOrder
// ============================================================================

#[test]
fn journal_sort_order_has_time_variant() {
    let sort = JournalSortOrder::Time;
    assert!(matches!(sort, JournalSortOrder::Time));
}

#[test]
fn journal_sort_order_has_category_variant() {
    let sort = JournalSortOrder::Category;
    assert!(matches!(sort, JournalSortOrder::Category));
}

// ============================================================================
// Rule #6: All FactCategory variants usable in journal entries
// ============================================================================

#[test]
fn journal_entry_accepts_all_fact_categories() {
    for category in [
        FactCategory::Lore,
        FactCategory::Place,
        FactCategory::Person,
        FactCategory::Quest,
        FactCategory::Ability,
    ] {
        let entry = JournalEntry {
            fact_id: "test".to_string(),
            content: "test content".to_string(),
            category,
            source: "Observation".to_string(),
            confidence: "Certain".to_string(),
            learned_turn: 1,
        };
        // Not vacuous — verifies each category survives construction + serde
        let json = serde_json::to_string(&entry).expect("serialize");
        let restored: JournalEntry = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(
            std::mem::discriminant(&entry.category),
            std::mem::discriminant(&restored.category),
            "FactCategory variant must survive round-trip",
        );
    }
}

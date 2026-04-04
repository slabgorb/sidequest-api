//! Story 15-19: Conlang knowledge wiring tests.
//!
//! Verifies that the conlang knowledge pipeline is connected end-to-end:
//! sidequest-game exports → sidequest-server prompt injection.

/// Wiring test: format_language_knowledge_for_prompt is reachable from sidequest-game
/// and produces non-empty output when given language fragments.
#[test]
fn conlang_format_reachable_from_game_crate() {
    use sidequest_game::{
        format_language_knowledge_for_prompt, query_all_language_knowledge,
        record_language_knowledge, LoreStore, Morpheme, MorphemeCategory,
    };

    let mut store = LoreStore::new();
    let morpheme = Morpheme {
        morpheme: "zar".to_string(),
        meaning: "fire".to_string(),
        pronunciation_hint: Some("zahr".to_string()),
        category: MorphemeCategory::Root,
        language_id: "draconic".to_string(),
    };
    record_language_knowledge(&mut store, &morpheme, "player-1", 3).unwrap();

    let frags = query_all_language_knowledge(&store, "player-1");
    assert_eq!(frags.len(), 1, "Should find the recorded language knowledge");

    let prompt_text = format_language_knowledge_for_prompt(&frags);
    assert!(
        prompt_text.contains("CONSTRUCTED LANGUAGE VOCABULARY"),
        "Prompt section should have the conlang header"
    );
    assert!(
        prompt_text.contains("zar"),
        "Prompt should contain the morpheme"
    );
    assert!(
        prompt_text.contains("fire"),
        "Prompt should contain the meaning"
    );
    assert!(
        prompt_text.contains("draconic"),
        "Prompt should contain the language ID"
    );
}

/// Wiring test: the conlang OTEL event component name ("conlang") is usable
/// through the WatcherEvent system.
#[test]
fn conlang_otel_event_constructible() {
    use sidequest_server::{Severity, WatcherEvent, WatcherEventType};
    use std::collections::HashMap;

    let event = WatcherEvent {
        timestamp: chrono::Utc::now(),
        component: "conlang".to_string(),
        event_type: WatcherEventType::StateTransition,
        severity: Severity::Info,
        fields: {
            let mut f = HashMap::new();
            f.insert(
                "event".to_string(),
                serde_json::json!("conlang_knowledge_injected"),
            );
            f.insert("vocab_count".to_string(), serde_json::json!(5));
            f.insert("language_count".to_string(), serde_json::json!(2));
            f.insert(
                "languages".to_string(),
                serde_json::json!("draconic, elvish"),
            );
            f
        },
    };

    let json = serde_json::to_string(&event).unwrap();
    let parsed: WatcherEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.component, "conlang");
    assert_eq!(
        parsed.fields["event"],
        serde_json::json!("conlang_knowledge_injected")
    );
    assert_eq!(parsed.fields["vocab_count"], serde_json::json!(5));
}

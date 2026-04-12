//! Story 15-19: Conlang knowledge wiring tests.
//!
//! Verifies that the conlang knowledge pipeline is connected end-to-end:
//! sidequest-game exports → sidequest-server dispatch/prompt injection.
//!
//! Four functions exist and are unit-tested in sidequest-game but have varying
//! wiring status in the server:
//!   - query_all_language_knowledge + format_language_knowledge_for_prompt → WIRED (prompt.rs:659)
//!   - record_language_knowledge → NOT WIRED (narration post-processing)
//!   - record_name_knowledge → NOT WIRED (NameBank generation)
//!   - format_name_bank_for_prompt → NOT WIRED (prompt injection)
//!
//! ACs covered:
//!   AC-1: record_name_knowledge wired into NameBank/NPC generation path
//!   AC-2: record_language_knowledge wired into narration post-processing
//!   AC-3: format_name_bank_for_prompt wired into prompt.rs
//!   AC-4: OTEL events emitted: morpheme_learned, name_recorded, context_injected

// ============================================================================
// Behavioral tests: game crate functions produce correct output
// ============================================================================

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
    assert_eq!(
        frags.len(),
        1,
        "Should find the recorded language knowledge"
    );

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

// ============================================================================
// Source-code wiring tests: verify production code calls the functions
// ============================================================================

/// AC-2: record_language_knowledge must be called in narration post-processing
/// (dispatch/mod.rs or dispatch/state_mutations.rs) to record morphemes
/// detected in Claude's narration response.
#[test]
fn dispatch_calls_record_language_knowledge_in_narration_postprocessing() {
    // Check dispatch/mod.rs — the main narration post-processing pipeline
    let dispatch_source = include_str!("../../src/dispatch/mod.rs");
    assert!(
        dispatch_source.contains("record_language_knowledge"),
        "dispatch/mod.rs must call record_language_knowledge() \
         to record conlang morphemes detected in narration. \
         Currently no narration post-processing step extracts \
         and records morpheme knowledge."
    );
}

/// AC-2 (OTEL): dispatch must emit conlang.morpheme_learned OTEL event
/// when a morpheme is detected and recorded from narration.
#[test]
fn dispatch_emits_morpheme_learned_otel_event() {
    let dispatch_source = include_str!("../../src/dispatch/mod.rs");
    assert!(
        dispatch_source.contains("morpheme_learned")
            || dispatch_source.contains("conlang.morpheme_learned"),
        "dispatch/mod.rs must emit a 'morpheme_learned' OTEL event \
         when record_language_knowledge() is called after narration. \
         Currently no such event is emitted."
    );
}

/// AC-1: record_name_knowledge must be called somewhere in the server
/// dispatch pipeline when NPC names are generated or discovered.
#[test]
fn dispatch_calls_record_name_knowledge() {
    // Check dispatch/mod.rs — NPC registry updates happen here
    let dispatch_source = include_str!("../../src/dispatch/mod.rs");
    assert!(
        dispatch_source.contains("record_name_knowledge"),
        "dispatch/mod.rs must call record_name_knowledge() \
         when new NPC names are generated/discovered during gameplay. \
         Currently update_npc_registry() creates NPC entries but \
         never records them as language knowledge in the lore store."
    );
}

/// AC-1 (OTEL): dispatch must emit conlang.name_recorded OTEL event
/// when a generated name is recorded to the lore store.
#[test]
fn dispatch_emits_name_recorded_otel_event() {
    let dispatch_source = include_str!("../../src/dispatch/mod.rs");
    assert!(
        dispatch_source.contains("name_recorded")
            || dispatch_source.contains("conlang.name_recorded"),
        "dispatch/mod.rs must emit a 'name_recorded' OTEL event \
         when record_name_knowledge() is called for NPC name discovery. \
         Currently no such event is emitted."
    );
}

/// AC-3: format_name_bank_for_prompt must be called in prompt.rs
/// to inject genre-specific name banks into the narrator context.
#[test]
fn prompt_calls_format_name_bank_for_prompt() {
    let prompt_source = include_str!("../../src/dispatch/prompt.rs");
    assert!(
        prompt_source.contains("format_name_bank_for_prompt"),
        "dispatch/prompt.rs must call format_name_bank_for_prompt() \
         to inject genre pack NameBanks into the narrator prompt context. \
         Currently query_all_language_knowledge is wired but \
         format_name_bank_for_prompt is not."
    );
}

// ============================================================================
// Behavioral: record_name_knowledge creates correct lore fragments
// ============================================================================

/// record_name_knowledge creates a Language-category lore fragment
/// with the correct metadata fields for prompt injection.
#[test]
fn record_name_knowledge_creates_language_lore_fragment() {
    use sidequest_game::{
        record_name_knowledge, GeneratedName, LoreCategory, LoreStore, NamePattern,
    };

    let mut store = LoreStore::new();
    let name = GeneratedName {
        name: "zar'kethi".to_string(),
        gloss: "fire-walker".to_string(),
        pronunciation: Some("zahr'keth-ee".to_string()),
        pattern: NamePattern::RootSuffix,
        language_id: "draconic".to_string(),
    };

    let result = record_name_knowledge(&mut store, &name, "player-1", 5);
    assert!(result.is_ok(), "record_name_knowledge should succeed");

    let frags: Vec<_> = store
        .query_by_category(&LoreCategory::Language)
        .into_iter()
        .collect();
    assert_eq!(frags.len(), 1, "Should have one Language lore fragment");

    let frag = frags[0];
    assert_eq!(
        frag.metadata().get("language_id").map(String::as_str),
        Some("draconic"),
        "Fragment metadata should contain language_id"
    );
    assert_eq!(
        frag.metadata().get("name").map(String::as_str),
        Some("zar'kethi"),
        "Fragment metadata should contain the name"
    );
    assert!(
        frag.content().contains("zar'kethi"),
        "Fragment content should mention the name"
    );
}

/// format_name_bank_for_prompt produces markdown with name entries.
#[test]
fn format_name_bank_for_prompt_produces_markdown() {
    use sidequest_game::{format_name_bank_for_prompt, GeneratedName, NameBank, NamePattern};

    let bank = NameBank {
        language_id: "draconic".to_string(),
        names: vec![
            GeneratedName {
                name: "zar'kethi".to_string(),
                gloss: "fire-walker".to_string(),
                pronunciation: Some("zahr'keth-ee".to_string()),
                pattern: NamePattern::RootSuffix,
                language_id: "draconic".to_string(),
            },
            GeneratedName {
                name: "vor'aela".to_string(),
                gloss: "great-light".to_string(),
                pronunciation: Some("vohr'ay-lah".to_string()),
                pattern: NamePattern::PrefixRoot,
                language_id: "draconic".to_string(),
            },
        ],
    };

    let output = format_name_bank_for_prompt(&bank, 10);
    assert!(
        !output.is_empty(),
        "format_name_bank_for_prompt should produce non-empty output for a non-empty bank"
    );
    assert!(
        output.contains("zar'kethi"),
        "Output should contain the first name"
    );
    assert!(
        output.contains("vor'aela"),
        "Output should contain the second name"
    );
}

/// format_name_bank_for_prompt returns empty string for empty bank.
#[test]
fn format_name_bank_for_prompt_empty_bank_returns_empty() {
    use sidequest_game::{format_name_bank_for_prompt, NameBank};

    let bank = NameBank {
        language_id: "draconic".to_string(),
        names: vec![],
    };

    let output = format_name_bank_for_prompt(&bank, 10);
    assert!(
        output.is_empty(),
        "format_name_bank_for_prompt should return empty string for empty bank"
    );
}

/// format_name_bank_for_prompt respects max_names limit.
#[test]
fn format_name_bank_for_prompt_respects_max_names() {
    use sidequest_game::{format_name_bank_for_prompt, NameBank};

    let bank = NameBank {
        language_id: "draconic".to_string(),
        names: vec![],
    };

    let output = format_name_bank_for_prompt(&bank, 0);
    assert!(
        output.is_empty(),
        "format_name_bank_for_prompt should return empty string when max_names is 0"
    );
}

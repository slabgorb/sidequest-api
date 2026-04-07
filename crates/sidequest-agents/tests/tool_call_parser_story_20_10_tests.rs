//! Story 20-10: Wire tool call parsing — connect Claude tool output to ToolCallResults
//!
//! RED phase — tests for the tool call parser that reads sidecar JSONL files
//! produced by script tools during Claude CLI execution and maps them to
//! `ToolCallResults` for `assemble_turn`.
//!
//! ACs tested:
//!   1. Tool scripts write structured results to a sidecar file during CLI execution
//!   2. `parse_tool_results()` reads sidecar JSONL and produces `ToolCallResults`
//!   3. Orchestrator passes real `ToolCallResults` (not default) to `assemble_turn`
//!   4. OTEL spans emitted for each parsed tool result
//!   5. Wiring: tool result flows script → sidecar → parser → orchestrator → assemble_turn
//!   6. All existing tests pass unchanged
//!   7. Sidecar file is cleaned up after parsing

use std::collections::HashMap;
use std::io::Write;

use sidequest_agents::tools::assemble_turn::{assemble_turn, ToolCallResults};
use sidequest_agents::tools::tool_call_parser::{
    parse_tool_results, sidecar_path, ToolCallRecord, SIDECAR_DIR,
};
use sidequest_agents::orchestrator::{
    ActionFlags, ActionRewrite, NarratorExtraction,
};

// ============================================================================
// Helpers
// ============================================================================

fn default_rewrite() -> ActionRewrite {
    ActionRewrite {
        you: "You look around".to_string(),
        named: "Kael looks around".to_string(),
        intent: "look around".to_string(),
    }
}

fn default_flags() -> ActionFlags {
    ActionFlags {
        is_power_grab: false,
        references_inventory: false,
        references_npc: false,
        references_ability: false,
        references_location: false,
    }
}

fn extraction_with_known_values() -> NarratorExtraction {
    NarratorExtraction {
        prose: "The air crackles with tension.".to_string(),
        footnotes: vec![],
        items_gained: vec![],
        npcs_present: vec![],
        quest_updates: HashMap::new(),
        visual_scene: None,
        scene_mood: Some("narrator_fallback_mood".to_string()),
        personality_events: vec![],
        scene_intent: Some("narrator_fallback_intent".to_string()),
        resource_deltas: HashMap::new(),
        lore_established: None,
        merchant_transactions: vec![],
        sfx_triggers: vec![],
        action_rewrite: None,
        action_flags: None,
    beat_selections: vec![],
    }
}

/// Write JSONL lines to the sidecar path for a given session ID.
fn write_sidecar(session_id: &str, lines: &[&str]) {
    let path = sidecar_path(session_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("failed to create sidecar dir");
    }
    let mut file = std::fs::File::create(&path).expect("failed to create sidecar file");
    for line in lines {
        writeln!(file, "{}", line).expect("failed to write line");
    }
}

/// Generate a unique session ID for test isolation.
fn test_session_id(test_name: &str) -> String {
    format!("test-20-10-{}-{}", test_name, std::process::id())
}

/// Clean up sidecar file after test.
fn cleanup_sidecar(session_id: &str) {
    let path = sidecar_path(session_id);
    let _ = std::fs::remove_file(path);
}

// ============================================================================
// AC-2: parse_tool_results reads sidecar JSONL and produces ToolCallResults
// ============================================================================

#[test]
fn parse_tool_results_extracts_scene_mood() {
    let sid = test_session_id("mood");
    write_sidecar(&sid, &[
        r#"{"tool":"set_mood","result":{"mood":"tension"}}"#,
    ]);

    let results = parse_tool_results(&sid);
    assert_eq!(
        results.scene_mood.as_deref(),
        Some("tension"),
        "scene_mood should be populated from set_mood tool result"
    );

    cleanup_sidecar(&sid);
}

#[test]
fn parse_tool_results_extracts_scene_intent() {
    let sid = test_session_id("intent");
    write_sidecar(&sid, &[
        r#"{"tool":"set_intent","result":{"intent":"exploration"}}"#,
    ]);

    let results = parse_tool_results(&sid);
    assert_eq!(
        results.scene_intent.as_deref(),
        Some("exploration"),
        "scene_intent should be populated from set_intent tool result"
    );

    cleanup_sidecar(&sid);
}

#[test]
fn parse_tool_results_extracts_both_mood_and_intent() {
    let sid = test_session_id("both");
    write_sidecar(&sid, &[
        r#"{"tool":"set_mood","result":{"mood":"wonder"}}"#,
        r#"{"tool":"set_intent","result":{"intent":"dialogue"}}"#,
    ]);

    let results = parse_tool_results(&sid);
    assert_eq!(results.scene_mood.as_deref(), Some("wonder"));
    assert_eq!(results.scene_intent.as_deref(), Some("dialogue"));

    cleanup_sidecar(&sid);
}

#[test]
fn parse_tool_results_returns_default_when_no_sidecar_file() {
    // Non-existent session ID — no sidecar file
    let sid = test_session_id("missing-file-xyzzy");
    let results = parse_tool_results(&sid);

    assert_eq!(results.scene_mood, None, "no sidecar → scene_mood should be None");
    assert_eq!(results.scene_intent, None, "no sidecar → scene_intent should be None");
}

#[test]
fn parse_tool_results_returns_default_for_empty_sidecar() {
    let sid = test_session_id("empty");
    write_sidecar(&sid, &[]);

    let results = parse_tool_results(&sid);
    assert_eq!(results.scene_mood, None);
    assert_eq!(results.scene_intent, None);

    cleanup_sidecar(&sid);
}

// ============================================================================
// Malformed input handling
// ============================================================================

#[test]
fn parse_tool_results_skips_malformed_json_lines() {
    let sid = test_session_id("malformed");
    write_sidecar(&sid, &[
        r#"{"tool":"set_mood","result":{"mood":"tension"}}"#,
        r#"this is not json"#,
        r#"{"tool":"set_intent","result":{"intent":"stealth"}}"#,
    ]);

    let results = parse_tool_results(&sid);
    assert_eq!(
        results.scene_mood.as_deref(),
        Some("tension"),
        "valid mood line should still be parsed"
    );
    assert_eq!(
        results.scene_intent.as_deref(),
        Some("stealth"),
        "valid intent line after malformed line should still be parsed"
    );

    cleanup_sidecar(&sid);
}

#[test]
fn parse_tool_results_skips_unknown_tool_names() {
    let sid = test_session_id("unknown-tool");
    write_sidecar(&sid, &[
        r#"{"tool":"unknown_future_tool","result":{"foo":"bar"}}"#,
        r#"{"tool":"set_mood","result":{"mood":"calm"}}"#,
    ]);

    let results = parse_tool_results(&sid);
    assert_eq!(
        results.scene_mood.as_deref(),
        Some("calm"),
        "known tool should still be parsed after unknown tool"
    );

    cleanup_sidecar(&sid);
}

#[test]
fn parse_tool_results_handles_missing_result_field() {
    let sid = test_session_id("no-result");
    write_sidecar(&sid, &[
        r#"{"tool":"set_mood"}"#,
    ]);

    // Should not panic — missing "result" key is treated as malformed
    let results = parse_tool_results(&sid);
    assert_eq!(results.scene_mood, None, "missing result field → None");

    cleanup_sidecar(&sid);
}

#[test]
fn parse_tool_results_last_call_wins_on_duplicate_tool() {
    let sid = test_session_id("duplicate");
    write_sidecar(&sid, &[
        r#"{"tool":"set_mood","result":{"mood":"tension"}}"#,
        r#"{"tool":"set_mood","result":{"mood":"triumph"}}"#,
    ]);

    let results = parse_tool_results(&sid);
    assert_eq!(
        results.scene_mood.as_deref(),
        Some("triumph"),
        "last set_mood call should win"
    );

    cleanup_sidecar(&sid);
}

// ============================================================================
// AC-7: Sidecar file is cleaned up after parsing
// ============================================================================

#[test]
fn parse_tool_results_cleans_up_sidecar_file() {
    let sid = test_session_id("cleanup");
    write_sidecar(&sid, &[
        r#"{"tool":"set_mood","result":{"mood":"calm"}}"#,
    ]);

    let path = sidecar_path(&sid);
    assert!(path.exists(), "sidecar file should exist before parsing");

    let _results = parse_tool_results(&sid);
    assert!(!path.exists(), "sidecar file should be deleted after parsing");
}

// ============================================================================
// AC-1: ToolCallRecord serialization (sidecar protocol)
// ============================================================================

#[test]
fn tool_call_record_serializes_to_expected_jsonl() {
    let record = ToolCallRecord {
        tool: "set_mood".to_string(),
        result: serde_json::json!({"mood": "tension"}),
    };

    let json = serde_json::to_string(&record).expect("should serialize");
    assert!(json.contains(r#""tool":"set_mood""#), "should contain tool name");
    assert!(json.contains(r#""mood":"tension""#), "should contain result");
}

#[test]
fn tool_call_record_deserializes_from_jsonl() {
    let line = r#"{"tool":"set_intent","result":{"intent":"exploration"}}"#;
    let record: ToolCallRecord = serde_json::from_str(line).expect("should deserialize");
    assert_eq!(record.tool, "set_intent");
    assert_eq!(record.result["intent"], "exploration");
}

// ============================================================================
// Sidecar path convention
// ============================================================================

#[test]
fn sidecar_path_uses_session_id() {
    let path = sidecar_path("my-session-123");
    let path_str = path.to_string_lossy();
    assert!(
        path_str.contains("my-session-123"),
        "sidecar path should contain session ID: {path_str}"
    );
    assert!(
        path_str.ends_with(".jsonl"),
        "sidecar path should have .jsonl extension: {path_str}"
    );
}

#[test]
fn sidecar_dir_constant_is_defined() {
    // SIDECAR_DIR should be a well-known path that tools can discover
    assert!(
        !SIDECAR_DIR.is_empty(),
        "SIDECAR_DIR must be a non-empty path"
    );
}

// ============================================================================
// AC-5: Wiring — parsed tool results flow through to assemble_turn
// ============================================================================

#[test]
fn parsed_tool_results_override_narrator_extraction_in_assemble_turn() {
    // Simulate the full flow: sidecar → parse → assemble_turn
    let sid = test_session_id("wiring-e2e");
    write_sidecar(&sid, &[
        r#"{"tool":"set_mood","result":{"mood":"foreboding"}}"#,
        r#"{"tool":"set_intent","result":{"intent":"investigation"}}"#,
    ]);

    // Step 1: Parse sidecar (what orchestrator will do)
    let tool_results = parse_tool_results(&sid);
    assert_eq!(tool_results.scene_mood.as_deref(), Some("foreboding"));
    assert_eq!(tool_results.scene_intent.as_deref(), Some("investigation"));

    // Step 2: Build extraction with different values (narrator fallback)
    let extraction = extraction_with_known_values();
    let rewrite = default_rewrite();
    let flags = default_flags();

    // Step 3: Assemble — tool results should override narrator values
    let result = assemble_turn(extraction, rewrite, flags, tool_results);

    assert_eq!(
        result.scene_mood.as_deref(),
        Some("foreboding"),
        "tool result should override narrator extraction for scene_mood"
    );
    assert_eq!(
        result.scene_intent.as_deref(),
        Some("investigation"),
        "tool result should override narrator extraction for scene_intent"
    );
}

#[test]
fn missing_sidecar_falls_back_to_narrator_extraction() {
    // No sidecar file → default ToolCallResults → narrator extraction wins
    let sid = test_session_id("fallback-e2e");

    let tool_results = parse_tool_results(&sid);
    let extraction = extraction_with_known_values();
    let rewrite = default_rewrite();
    let flags = default_flags();

    let result = assemble_turn(extraction, rewrite, flags, tool_results);

    assert_eq!(
        result.scene_mood.as_deref(),
        Some("narrator_fallback_mood"),
        "with no sidecar, narrator extraction mood should pass through"
    );
    assert_eq!(
        result.scene_intent.as_deref(),
        Some("narrator_fallback_intent"),
        "with no sidecar, narrator extraction intent should pass through"
    );
}

// ============================================================================
// AC-3: Orchestrator wiring test — source code verification
// ============================================================================

#[test]
fn orchestrator_imports_parse_tool_results() {
    // Verify that orchestrator.rs imports and uses parse_tool_results
    // (not ToolCallResults::default)
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let orchestrator_src =
        std::fs::read_to_string(format!("{manifest_dir}/src/orchestrator.rs"))
            .expect("should be able to read orchestrator.rs");

    assert!(
        orchestrator_src.contains("parse_tool_results"),
        "orchestrator.rs must import/use parse_tool_results — \
         currently uses ToolCallResults::default() which discards tool call output"
    );
}

#[test]
fn orchestrator_does_not_use_default_tool_call_results() {
    // After 20-10, the orchestrator should NOT be calling ToolCallResults::default()
    // in process_action(). It should call parse_tool_results() instead.
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let orchestrator_src =
        std::fs::read_to_string(format!("{manifest_dir}/src/orchestrator.rs"))
            .expect("should be able to read orchestrator.rs");

    // The default() call from story 20-9 should be replaced
    assert!(
        !orchestrator_src.contains("ToolCallResults::default()"),
        "orchestrator.rs must NOT use ToolCallResults::default() — \
         story 20-10 replaces this with parse_tool_results()"
    );
}

// ============================================================================
// AC-3 wiring: tool_call_parser module is public and exported
// ============================================================================

#[test]
fn tool_call_parser_module_is_exported() {
    // This test compiles only if tool_call_parser is a public module
    // accessible from integration tests. The import at the top of this
    // file covers this check — if the module doesn't exist, compilation fails.
    let _fn_ptr: fn(&str) -> ToolCallResults = parse_tool_results;
}

//! Story 20-13: lore_mark sidecar tool — narrator calls tool to emit footnotes,
//! sidecar parser collects into lore_established
//!
//! RED phase — tests for the lore_mark tool wired into the sidecar tool call
//! pipeline. Follows the pattern established by item_acquire (20-11) and
//! merchant_transact (20-12). This completes Epic 20 — all mechanical
//! extraction flows through sidecar tools, zero narrator JSON.
//!
//! ACs tested:
//!   1. lore_mark tool call is fully wired in the sidecar tool call pipeline
//!   2. Parser validates lore facts (text, category, confidence)
//!   3. assemble_turn feeds lore_mark results into lore_established
//!   4. Tests verify full pipeline (unit + integration + wiring)
//!   5. No regressions in other tool pipelines

use std::collections::HashMap;
use std::io::Write;

use sidequest_agents::tools::assemble_turn::{assemble_turn, ToolCallResults};
use sidequest_agents::tools::lore_mark::{validate_lore_mark, LoreMarkResult};
use sidequest_agents::tools::tool_call_parser::{
    parse_tool_results, sidecar_path,
};
use sidequest_agents::orchestrator::{
    ActionFlags, ActionRewrite, NarratorExtraction,
};

// ============================================================================
// Helpers
// ============================================================================

fn default_rewrite() -> ActionRewrite {
    ActionRewrite {
        you: "You examine the ancient text".to_string(),
        named: "Kael examines the ancient text".to_string(),
        intent: "examine ancient text".to_string(),
    }
}

fn default_flags() -> ActionFlags {
    ActionFlags {
        is_power_grab: false,
        references_inventory: false,
        references_npc: false,
        references_ability: false,
        references_location: true,
    }
}

fn empty_extraction() -> NarratorExtraction {
    NarratorExtraction {
        prose: "The runes tell of an ancient war between the factions.".to_string(),
        footnotes: vec![],
        items_gained: vec![],
        npcs_present: vec![],
        quest_updates: HashMap::new(),
        visual_scene: None,
        scene_mood: None,
        personality_events: vec![],
        scene_intent: None,
        resource_deltas: HashMap::new(),
        lore_established: None,
        merchant_transactions: vec![],
        sfx_triggers: vec![],
        action_rewrite: None,
        action_flags: None,
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
    format!("test-20-13-{}-{}", test_name, std::process::id())
}

/// Clean up sidecar file after test.
fn cleanup_sidecar(session_id: &str) {
    let path = sidecar_path(session_id);
    let _ = std::fs::remove_file(path);
}

// ============================================================================
// AC-2: validate_lore_mark — validates lore facts and confidence levels
// ============================================================================

#[test]
fn validate_lore_mark_accepts_valid_world_lore() {
    let result = validate_lore_mark(
        "The Obsidian Tower was built during the First Age",
        "world",
        "high",
    );
    assert!(result.is_ok(), "valid world lore should be accepted");

    let validated = result.unwrap();
    assert_eq!(validated.text(), "The Obsidian Tower was built during the First Age");
    assert_eq!(validated.category(), "world");
    assert_eq!(validated.confidence(), "high");
}

#[test]
fn validate_lore_mark_accepts_all_valid_categories() {
    for category in &["world", "npc", "faction", "location", "quest", "custom"] {
        let result = validate_lore_mark("Some lore fact", category, "high");
        assert!(
            result.is_ok(),
            "category '{}' should be accepted",
            category
        );
        assert_eq!(result.unwrap().category(), *category);
    }
}

#[test]
fn validate_lore_mark_accepts_all_valid_confidence_levels() {
    for confidence in &["high", "medium", "low"] {
        let result = validate_lore_mark("Some lore fact", "world", confidence);
        assert!(
            result.is_ok(),
            "confidence '{}' should be accepted",
            confidence
        );
        assert_eq!(result.unwrap().confidence(), *confidence);
    }
}

#[test]
fn validate_lore_mark_rejects_empty_text() {
    let result = validate_lore_mark("", "world", "high");
    assert!(result.is_err(), "empty text should be rejected");
}

#[test]
fn validate_lore_mark_rejects_whitespace_only_text() {
    let result = validate_lore_mark("   ", "world", "high");
    assert!(result.is_err(), "whitespace-only text should be rejected");
}

#[test]
fn validate_lore_mark_rejects_invalid_category() {
    let result = validate_lore_mark("Some fact", "geography", "high");
    assert!(
        result.is_err(),
        "category 'geography' is not in the valid set"
    );
}

#[test]
fn validate_lore_mark_rejects_empty_category() {
    let result = validate_lore_mark("Some fact", "", "high");
    assert!(result.is_err(), "empty category should be rejected");
}

#[test]
fn validate_lore_mark_rejects_invalid_confidence() {
    let result = validate_lore_mark("Some fact", "world", "certain");
    assert!(
        result.is_err(),
        "confidence 'certain' is not in the valid set"
    );
}

#[test]
fn validate_lore_mark_rejects_empty_confidence() {
    let result = validate_lore_mark("Some fact", "world", "");
    assert!(result.is_err(), "empty confidence should be rejected");
}

#[test]
fn validate_lore_mark_trims_whitespace() {
    let result = validate_lore_mark(
        "  The tower stands tall  ",
        "  world  ",
        "  high  ",
    );
    assert!(result.is_ok(), "should accept after trimming whitespace");

    let validated = result.unwrap();
    assert_eq!(validated.text(), "The tower stands tall");
    assert_eq!(validated.category(), "world");
    assert_eq!(validated.confidence(), "high");
}

#[test]
fn validate_lore_mark_case_insensitive_category() {
    let result = validate_lore_mark("Some fact", "WORLD", "high");
    assert!(result.is_ok(), "category should be case-insensitive");
    assert_eq!(
        result.unwrap().category(),
        "world",
        "category should be normalized to lowercase"
    );
}

#[test]
fn validate_lore_mark_case_insensitive_confidence() {
    let result = validate_lore_mark("Some fact", "world", "HIGH");
    assert!(result.is_ok(), "confidence should be case-insensitive");
    assert_eq!(
        result.unwrap().confidence(),
        "high",
        "confidence should be normalized to lowercase"
    );
}

#[test]
fn validate_lore_mark_to_lore_text_returns_text() {
    let result = validate_lore_mark(
        "The faction controls the eastern mines",
        "faction",
        "medium",
    )
    .unwrap();
    let lore_text = result.to_lore_text();
    assert_eq!(lore_text, "The faction controls the eastern mines");
}

// ============================================================================
// AC-1: Parser handles lore_mark tool call records
// ============================================================================

#[test]
fn parse_tool_results_extracts_lore_mark() {
    let sid = test_session_id("basic");
    write_sidecar(&sid, &[
        r#"{"tool":"lore_mark","result":{"text":"The Obsidian Tower was built in the First Age","category":"world","confidence":"high"}}"#,
    ]);

    let results = parse_tool_results(&sid);
    let lore = results
        .lore_established
        .expect("lore_established should be Some after lore_mark tool call");
    assert_eq!(lore.len(), 1);
    assert_eq!(lore[0], "The Obsidian Tower was built in the First Age");

    cleanup_sidecar(&sid);
}

#[test]
fn parse_tool_results_accumulates_multiple_lore_marks() {
    let sid = test_session_id("multi");
    write_sidecar(&sid, &[
        r#"{"tool":"lore_mark","result":{"text":"The tower was built in the First Age","category":"world","confidence":"high"}}"#,
        r#"{"tool":"lore_mark","result":{"text":"Gareth was once a knight","category":"npc","confidence":"medium"}}"#,
        r#"{"tool":"lore_mark","result":{"text":"The Iron Fist controls the docks","category":"faction","confidence":"low"}}"#,
    ]);

    let results = parse_tool_results(&sid);
    let lore = results
        .lore_established
        .expect("should have lore_established");
    assert_eq!(lore.len(), 3, "all three lore marks should be collected");
    assert_eq!(lore[0], "The tower was built in the First Age");
    assert_eq!(lore[1], "Gareth was once a knight");
    assert_eq!(lore[2], "The Iron Fist controls the docks");

    cleanup_sidecar(&sid);
}

#[test]
fn parse_tool_results_skips_invalid_lore_mark() {
    let sid = test_session_id("invalid-lore");
    write_sidecar(&sid, &[
        // Valid
        r#"{"tool":"lore_mark","result":{"text":"Valid lore fact","category":"world","confidence":"high"}}"#,
        // Invalid — missing text field
        r#"{"tool":"lore_mark","result":{"category":"world","confidence":"high"}}"#,
        // Invalid — bad category
        r#"{"tool":"lore_mark","result":{"text":"Another fact","category":"invalid_cat","confidence":"high"}}"#,
    ]);

    let results = parse_tool_results(&sid);
    let lore = results
        .lore_established
        .expect("should have lore_established from valid call");
    assert_eq!(lore.len(), 1, "only the valid lore mark should be collected");
    assert_eq!(lore[0], "Valid lore fact");

    cleanup_sidecar(&sid);
}

#[test]
fn parse_tool_results_lore_established_none_when_no_lore_tools_fired() {
    let sid = test_session_id("no-lore");
    write_sidecar(&sid, &[
        r#"{"tool":"set_mood","result":{"mood":"mystery"}}"#,
    ]);

    let results = parse_tool_results(&sid);
    assert!(
        results.lore_established.is_none(),
        "lore_established should be None when no lore_mark tools fired"
    );

    cleanup_sidecar(&sid);
}

// ============================================================================
// AC-1: lore_mark coexists with other tool calls
// ============================================================================

#[test]
fn parse_tool_results_lore_mark_with_other_tools() {
    let sid = test_session_id("coexist");
    write_sidecar(&sid, &[
        r#"{"tool":"set_mood","result":{"mood":"mystery"}}"#,
        r#"{"tool":"lore_mark","result":{"text":"Ancient runes speak of war","category":"world","confidence":"high"}}"#,
        r#"{"tool":"item_acquire","result":{"item_ref":"scroll","name":"Ancient Scroll","category":"quest"}}"#,
    ]);

    let results = parse_tool_results(&sid);

    assert_eq!(results.scene_mood.as_deref(), Some("mystery"));

    let lore = results.lore_established.expect("should have lore_established");
    assert_eq!(lore.len(), 1);
    assert_eq!(lore[0], "Ancient runes speak of war");

    let items = results.items_acquired.expect("should have items_acquired");
    assert_eq!(items.len(), 1);

    cleanup_sidecar(&sid);
}

// ============================================================================
// AC-3: assemble_turn feeds lore_mark results into lore_established
// ============================================================================

#[test]
fn assemble_turn_uses_tool_lore_established_over_extraction() {
    let extraction = empty_extraction();
    let rewrite = default_rewrite();
    let flags = default_flags();

    let mut tool_results = ToolCallResults::default();
    tool_results.lore_established = Some(vec![
        "The tower was built in the First Age".to_string(),
    ]);

    let result = assemble_turn(extraction, rewrite, flags, tool_results);

    let lore = result.lore_established.expect("should have lore_established from tool results");
    assert_eq!(lore.len(), 1);
    assert_eq!(lore[0], "The tower was built in the First Age");
}

#[test]
fn assemble_turn_falls_back_to_extraction_when_no_lore_tool() {
    let mut extraction = empty_extraction();
    extraction.lore_established = Some(vec![
        "Extraction lore fact".to_string(),
    ]);
    let rewrite = default_rewrite();
    let flags = default_flags();
    let tool_results = ToolCallResults::default();

    let result = assemble_turn(extraction, rewrite, flags, tool_results);

    let lore = result.lore_established.expect("should fall back to extraction lore");
    assert_eq!(lore.len(), 1);
    assert_eq!(lore[0], "Extraction lore fact");
}

#[test]
fn assemble_turn_tool_results_override_extraction_lore_established() {
    let mut extraction = empty_extraction();
    extraction.lore_established = Some(vec!["Extraction fact".to_string()]);
    let rewrite = default_rewrite();
    let flags = default_flags();

    let mut tool_results = ToolCallResults::default();
    tool_results.lore_established = Some(vec!["Tool fact".to_string()]);

    let result = assemble_turn(extraction, rewrite, flags, tool_results);

    let lore = result.lore_established.expect("should have lore from tools");
    assert_eq!(lore.len(), 1);
    assert_eq!(lore[0], "Tool fact", "tool results should override extraction lore_established");
}

// ============================================================================
// AC-4: Integration — sidecar → parse → assemble_turn → ActionResult
// ============================================================================

#[test]
fn lore_mark_full_pipeline_sidecar_to_action_result() {
    let sid = test_session_id("pipeline");
    write_sidecar(&sid, &[
        r#"{"tool":"lore_mark","result":{"text":"The Obsidian Tower guards the rift","category":"location","confidence":"high"}}"#,
        r#"{"tool":"lore_mark","result":{"text":"Gareth betrayed his order","category":"npc","confidence":"medium"}}"#,
    ]);

    let tool_results = parse_tool_results(&sid);
    let lore_ref = tool_results
        .lore_established
        .as_ref()
        .expect("should have parsed lore_established");
    assert_eq!(lore_ref.len(), 2);

    let extraction = empty_extraction();
    let rewrite = default_rewrite();
    let flags = default_flags();

    let result = assemble_turn(extraction, rewrite, flags, tool_results);

    let lore = result.lore_established.expect("ActionResult should have lore_established");
    assert_eq!(lore.len(), 2);
    assert_eq!(lore[0], "The Obsidian Tower guards the rift");
    assert_eq!(lore[1], "Gareth betrayed his order");

    cleanup_sidecar(&sid);
}

// ============================================================================
// AC-4: Wiring — source code verification
// ============================================================================

#[test]
fn tool_call_parser_handles_lore_mark() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let parser_src =
        std::fs::read_to_string(format!("{manifest_dir}/src/tools/tool_call_parser.rs"))
            .expect("should be able to read tool_call_parser.rs");

    assert!(
        parser_src.contains("\"lore_mark\""),
        "tool_call_parser.rs must handle 'lore_mark' tool records"
    );
}

#[test]
fn tool_call_results_has_lore_established_field() {
    let results = ToolCallResults::default();
    let _field: &Option<Vec<String>> = &results.lore_established;
}

#[test]
fn assemble_turn_reads_lore_established_from_tool_results() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let assemble_src =
        std::fs::read_to_string(format!("{manifest_dir}/src/tools/assemble_turn.rs"))
            .expect("should be able to read assemble_turn.rs");

    assert!(
        assemble_src.contains("tool_results.lore_established")
            || assemble_src.contains("tools.lore_established"),
        "assemble_turn must use lore_established from tool_results, not just extraction"
    );
}

#[test]
fn lore_mark_module_is_exported() {
    let _fn_ptr: fn(&str, &str, &str) -> Result<LoreMarkResult, _> = validate_lore_mark;
}

// ============================================================================
// AC-5: No regression — other tools unaffected by lore_mark addition
// ============================================================================

#[test]
fn merchant_transact_unaffected_by_lore_mark_addition() {
    let sid = test_session_id("no-regress-merchant");
    write_sidecar(&sid, &[
        r#"{"tool":"merchant_transact","result":{"transaction_type":"buy","item_id":"potion","merchant":"Zara"}}"#,
    ]);

    let results = parse_tool_results(&sid);

    let txns = results.merchant_transactions.expect("merchant_transactions should still work");
    assert_eq!(txns.len(), 1);
    assert_eq!(txns[0].item_id, "potion");

    assert!(results.lore_established.is_none(), "lore should be None when only merchant fired");

    cleanup_sidecar(&sid);
}

#[test]
fn item_acquire_unaffected_by_lore_mark_addition() {
    let sid = test_session_id("no-regress-item");
    write_sidecar(&sid, &[
        r#"{"tool":"item_acquire","result":{"item_ref":"sword","name":"Iron Sword","category":"weapon"}}"#,
    ]);

    let results = parse_tool_results(&sid);

    let items = results.items_acquired.expect("items should still work");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].name, "Iron Sword");

    assert!(results.lore_established.is_none(), "lore should be None when only item_acquire fired");

    cleanup_sidecar(&sid);
}

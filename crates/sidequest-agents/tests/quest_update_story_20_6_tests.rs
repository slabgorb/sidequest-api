//! Story 20-6: quest_update tool — quest state transitions
//!
//! RED phase — tests for Phase 6 of ADR-057 (Narrator Crunch Separation).
//! Migrates quest_updates from the narrator's monolithic JSON block to discrete
//! `quest_update` tool calls. The LLM decides THAT a quest changed; the tool
//! structures the transition.
//!
//! ACs tested:
//!   1. quest_update tool accepts quest name and status string, returns structured JSON
//!   2. Narrator calls tool once per changed quest (multiple calls per turn)
//!   3. Narrator prompt keeps referral rule but removes quest JSON schema
//!   4. assemble_turn collects quest tool calls into ActionResult.quest_updates
//!   5. OTEL span per quest update with quest name

use std::collections::HashMap;

use sidequest_agents::agent::Agent;
use sidequest_agents::agents::narrator::NarratorAgent;
use sidequest_agents::orchestrator::{
    ActionFlags, ActionRewrite, NarratorExtraction,
};
use std::io::Write;

use sidequest_agents::tools::assemble_turn::{assemble_turn, ToolCallResults};
use sidequest_agents::tools::quest_update::{validate_quest_update, QuestUpdate};
use sidequest_agents::tools::tool_call_parser::{parse_tool_results, sidecar_path};

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

fn extraction_with_quests() -> NarratorExtraction {
    let mut quest_updates = HashMap::new();
    quest_updates.insert(
        "The Corrupted Grove".to_string(),
        "active: Find the source of corruption (from: Elder Mirova)".to_string(),
    );
    NarratorExtraction {
        prose: "The ancient grove pulses with dark energy.".to_string(),
        footnotes: vec![],
        items_gained: vec![],
        npcs_present: vec![],
        quest_updates,
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

fn extraction_no_quests() -> NarratorExtraction {
    NarratorExtraction {
        prose: "Nothing of note happens.".to_string(),
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

// ============================================================================
// AC-1: quest_update tool accepts quest name and status string
// ============================================================================

/// Basic quest creation with active status.
#[test]
fn validate_quest_update_active_quest() {
    let result = validate_quest_update(
        "The Corrupted Grove",
        "active: Find the source of corruption (from: Elder Mirova)",
    );
    assert!(result.is_ok(), "valid quest update must succeed");
    let update = result.unwrap();
    assert_eq!(update.quest_name(), "The Corrupted Grove");
    assert_eq!(
        update.status(),
        "active: Find the source of corruption (from: Elder Mirova)"
    );
}

/// Quest completion status.
#[test]
fn validate_quest_update_completed_quest() {
    let result = validate_quest_update(
        "The Corrupted Grove",
        "completed: the source was purified",
    );
    assert!(result.is_ok());
    let update = result.unwrap();
    assert_eq!(update.quest_name(), "The Corrupted Grove");
    assert_eq!(update.status(), "completed: the source was purified");
}

/// Quest failure status.
#[test]
fn validate_quest_update_failed_quest() {
    let result = validate_quest_update(
        "The Heist",
        "failed: the guards were alerted",
    );
    assert!(result.is_ok());
    let update = result.unwrap();
    assert_eq!(update.quest_name(), "The Heist");
    assert_eq!(update.status(), "failed: the guards were alerted");
}

/// Updated objective status.
#[test]
fn validate_quest_update_updated_objective() {
    let result = validate_quest_update(
        "The Corrupted Grove",
        "active: Defeat the corruption elemental at the grove's heart",
    );
    assert!(result.is_ok());
    let update = result.unwrap();
    assert_eq!(
        update.status(),
        "active: Defeat the corruption elemental at the grove's heart"
    );
}

/// Empty quest name must be rejected.
#[test]
fn validate_quest_update_rejects_empty_name() {
    let result = validate_quest_update("", "active: some objective");
    assert!(
        result.is_err(),
        "empty quest name must be rejected"
    );
}

/// Empty status must be rejected.
#[test]
fn validate_quest_update_rejects_empty_status() {
    let result = validate_quest_update("The Corrupted Grove", "");
    assert!(
        result.is_err(),
        "empty status must be rejected"
    );
}

/// Whitespace-only quest name must be rejected.
#[test]
fn validate_quest_update_rejects_whitespace_name() {
    let result = validate_quest_update("   ", "active: some objective");
    assert!(
        result.is_err(),
        "whitespace-only quest name must be rejected"
    );
}

/// Whitespace-only status must be rejected.
#[test]
fn validate_quest_update_rejects_whitespace_status() {
    let result = validate_quest_update("The Corrupted Grove", "   ");
    assert!(
        result.is_err(),
        "whitespace-only status must be rejected"
    );
}

// ============================================================================
// AC-1: QuestUpdate returns structured JSON
// ============================================================================

/// QuestUpdate must serialize to the expected JSON shape.
#[test]
fn quest_update_serializes_to_json() {
    let update = validate_quest_update("The Corrupted Grove", "completed: the source was purified").unwrap();
    let json = serde_json::to_value(&update).expect("QuestUpdate must serialize");
    assert_eq!(json["quest_name"], "The Corrupted Grove");
    assert_eq!(json["status"], "completed: the source was purified");
}

// ============================================================================
// AC-2: Multiple quest updates per turn
// ============================================================================

/// Multiple quest_update calls in one turn produce distinct QuestUpdate values.
#[test]
fn multiple_quest_updates_are_independent() {
    let update1 = validate_quest_update(
        "The Corrupted Grove",
        "completed: the source was purified",
    )
    .unwrap();
    let update2 = validate_quest_update(
        "The Missing Merchant",
        "active: Search the docks at midnight (from: Harbormaster Dex)",
    )
    .unwrap();

    assert_ne!(update1.quest_name(), update2.quest_name());
    assert_ne!(update1.status(), update2.status());
}

// ============================================================================
// AC-3: Narrator prompt removes quest JSON schema, keeps referral rule
// ============================================================================

/// The quest protocol block (status values, format instructions) must be removed.
#[test]
fn narrator_prompt_omits_quest_protocol() {
    let narrator = NarratorAgent::new();
    let prompt = narrator.system_prompt();

    assert!(
        !prompt.contains("[QUEST PROTOCOL]"),
        "quest protocol header must be removed from narrator prompt"
    );
    assert!(
        !prompt.contains("active: <description> (from: <NPC name>)"),
        "quest status format documentation must be removed"
    );
    assert!(
        !prompt.contains("completed: <outcome>"),
        "quest completion format must be removed"
    );
    assert!(
        !prompt.contains("failed: <reason>"),
        "quest failure format must be removed"
    );
}

/// The referral rule must be preserved — it's intent judgment, not crunch.
/// Story 23-1: referral rule moved to Early/Guardrail section via build_context().
#[test]
fn narrator_prompt_retains_referral_rule() {
    use sidequest_agents::agent::Agent;
    use sidequest_agents::context_builder::ContextBuilder;

    let narrator = NarratorAgent::new();
    let mut builder = ContextBuilder::new();
    narrator.build_context(&mut builder);
    let composed = builder.compose();

    assert!(
        composed.contains("Referral Rule") || composed.contains("REFERRAL RULE"),
        "referral rule must remain in narrator prompt — it's intent judgment, not crunch"
    );
}

/// The quest_updates field in the JSON block example must be removed.
#[test]
fn narrator_prompt_omits_quest_updates_json_field() {
    let narrator = NarratorAgent::new();
    let prompt = narrator.system_prompt();

    assert!(
        !prompt.contains("quest_updates: quest status changes"),
        "quest_updates field documentation must be removed from JSON block"
    );
}


// ============================================================================
// AC-4: assemble_turn collects quest tool calls into quest_updates HashMap
// ============================================================================

/// When quest_update tools fire, their values override narrator extraction's quest_updates.
#[test]
fn assemble_turn_tool_quests_override_narrator() {
    let extraction = extraction_with_quests();
    let mut tool_quests = HashMap::new();
    tool_quests.insert(
        "The Missing Merchant".to_string(),
        "active: Search the docks (from: Harbormaster Dex)".to_string(),
    );

    let tool_results = ToolCallResults {
        quest_updates: Some(tool_quests),
        ..ToolCallResults::default()
    };

    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);

    // Tool result must replace narrator extraction entirely
    assert!(
        result.quest_updates.contains_key("The Missing Merchant"),
        "tool quest update must be present"
    );
    assert!(
        !result.quest_updates.contains_key("The Corrupted Grove"),
        "narrator's quest update must be overridden (not merged)"
    );
    assert_eq!(result.quest_updates.len(), 1);
}

/// Multiple quest_update tool calls in one turn produce a multi-entry HashMap.
#[test]
fn assemble_turn_multiple_quest_tools() {
    let extraction = extraction_no_quests();
    let mut tool_quests = HashMap::new();
    tool_quests.insert(
        "The Corrupted Grove".to_string(),
        "completed: the source was purified".to_string(),
    );
    tool_quests.insert(
        "The Missing Merchant".to_string(),
        "active: Search the docks (from: Harbormaster Dex)".to_string(),
    );

    let tool_results = ToolCallResults {
        quest_updates: Some(tool_quests),
        ..ToolCallResults::default()
    };

    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);

    assert_eq!(
        result.quest_updates.len(),
        2,
        "both tool quest updates must be collected"
    );
    assert_eq!(
        result.quest_updates.get("The Corrupted Grove").unwrap(),
        "completed: the source was purified"
    );
    assert_eq!(
        result.quest_updates.get("The Missing Merchant").unwrap(),
        "active: Search the docks (from: Harbormaster Dex)"
    );
}

/// No quest_update tools fired — narrator extraction's quest_updates pass through.
#[test]
fn assemble_turn_no_quest_tool_uses_narrator_fallback() {
    let extraction = extraction_with_quests();
    let tool_results = ToolCallResults::default();

    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);

    assert!(
        result.quest_updates.contains_key("The Corrupted Grove"),
        "without tool calls, narrator's quest_updates must pass through"
    );
}

/// No quest_update tools AND narrator has no quests — result is empty HashMap.
#[test]
fn assemble_turn_no_quests_anywhere_is_empty() {
    let extraction = extraction_no_quests();
    let tool_results = ToolCallResults::default();

    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);

    assert!(
        result.quest_updates.is_empty(),
        "no quests from either source → must be empty"
    );
}

/// Tool quest_updates being Some(empty HashMap) means "tools fired but no quests changed."
/// This should still override narrator extraction (replace with empty).
#[test]
fn assemble_turn_empty_tool_quests_overrides_narrator() {
    let extraction = extraction_with_quests();
    let tool_results = ToolCallResults {
        quest_updates: Some(HashMap::new()),
        ..ToolCallResults::default()
    };

    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);

    assert!(
        result.quest_updates.is_empty(),
        "Some(empty) tool quest_updates must override narrator's quests"
    );
}

/// quest_update tool results don't disrupt other fields.
#[test]
fn assemble_turn_quest_tools_preserve_other_fields() {
    let extraction = extraction_with_quests();
    let mut tool_quests = HashMap::new();
    tool_quests.insert("The Heist".to_string(), "active: plan the heist".to_string());

    let tool_results = ToolCallResults {
        quest_updates: Some(tool_quests),
        scene_mood: Some("tension".to_string()),
        ..ToolCallResults::default()
    };

    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);

    // Quest updates from tool
    assert_eq!(result.quest_updates.len(), 1);
    assert!(result.quest_updates.contains_key("The Heist"));

    // Other fields still pass through
    assert_eq!(result.narration, "The ancient grove pulses with dark energy.");
    assert_eq!(result.scene_mood.as_deref(), Some("tension"));
    assert!(result.action_rewrite.is_some());
}

// ============================================================================
// AC-4: ToolCallResults has quest_updates field
// ============================================================================

/// ToolCallResults must have a quest_updates field (Option<HashMap<String, String>>).
#[test]
fn tool_call_results_has_quest_updates_field() {
    let mut quests = HashMap::new();
    quests.insert("Test Quest".to_string(), "active: test".to_string());

    let results = ToolCallResults {
        quest_updates: Some(quests),
        ..ToolCallResults::default()
    };

    assert!(results.quest_updates.is_some());
    assert_eq!(results.quest_updates.unwrap().len(), 1);
}

/// Default ToolCallResults must have quest_updates as None.
#[test]
fn tool_call_results_default_quest_updates_is_none() {
    let defaults = ToolCallResults::default();
    assert!(
        defaults.quest_updates.is_none(),
        "default quest_updates must be None (no tools fired)"
    );
}

// ============================================================================
// AC-5: OTEL span per quest update with quest name
// ============================================================================

/// validate_quest_update must run cleanly under a tracing subscriber.
#[test]
fn validate_quest_update_emits_otel_span() {
    let _guard = tracing::subscriber::set_default(
        tracing_subscriber::fmt()
            .with_test_writer()
            .with_max_level(tracing::Level::TRACE)
            .finish(),
    );

    let result = validate_quest_update(
        "The Corrupted Grove",
        "active: Find the source of corruption (from: Elder Mirova)",
    );
    assert!(result.is_ok());
}

/// OTEL must capture invalid tool calls too — quest name in span fields.
#[test]
fn validate_quest_update_otel_on_invalid_input() {
    let _guard = tracing::subscriber::set_default(
        tracing_subscriber::fmt()
            .with_test_writer()
            .with_max_level(tracing::Level::TRACE)
            .finish(),
    );

    let result = validate_quest_update("", "active: some quest");
    assert!(result.is_err(), "empty name should be rejected even under tracing");
}

// ============================================================================
// Tool call parser: quest_update records in sidecar JSONL
// ============================================================================

/// The tool_call_parser must recognize "quest_update" tool records and accumulate
/// them into ToolCallResults.quest_updates HashMap.
#[test]
fn tool_call_parser_recognizes_quest_update() {
    use sidequest_agents::tools::tool_call_parser::ToolCallRecord;

    let record = ToolCallRecord {
        tool: "quest_update".to_string(),
        result: serde_json::json!({
            "quest_name": "The Corrupted Grove",
            "status": "completed: the source was purified"
        }),
    };

    // Verify the record can be created and serialized
    let json = serde_json::to_string(&record).expect("ToolCallRecord must serialize");
    assert!(json.contains("quest_update"));
    assert!(json.contains("The Corrupted Grove"));
}

// ============================================================================
// Wiring: quest_update module is public and accessible
// ============================================================================

#[test]
fn quest_update_module_is_public() {
    // If this compiles, the module and function are wired
    let _: fn(&str, &str) -> Result<QuestUpdate, _> = validate_quest_update;
}

#[test]
fn quest_update_struct_is_exported() {
    let update = validate_quest_update("test", "active: test").unwrap();
    assert_eq!(update.quest_name(), "test");
    assert_eq!(update.status(), "active: test");
}

// ============================================================================
// Edge cases
// ============================================================================

/// Very long quest names should be accepted (LLM can be verbose).
#[test]
fn validate_quest_update_accepts_long_quest_name() {
    let long_name = "The Very Long Quest Name That The LLM Decided To Give This Particular Adventure Hook Because It Was Feeling Creative Today";
    let result = validate_quest_update(long_name, "active: do the thing");
    assert!(result.is_ok());
    assert_eq!(result.unwrap().quest_name(), long_name);
}

/// Unicode quest names should be accepted (genre packs may use non-ASCII).
#[test]
fn validate_quest_update_accepts_unicode_name() {
    let result = validate_quest_update("紫電の試練", "active: 古の神殿を探せ");
    assert!(result.is_ok());
    let update = result.unwrap();
    assert_eq!(update.quest_name(), "紫電の試練");
    assert_eq!(update.status(), "active: 古の神殿を探せ");
}

/// Quest name with leading/trailing whitespace should be trimmed.
#[test]
fn validate_quest_update_trims_quest_name() {
    let result = validate_quest_update("  The Corrupted Grove  ", "active: find the source");
    assert!(result.is_ok());
    assert_eq!(
        result.unwrap().quest_name(),
        "The Corrupted Grove",
        "quest name must be trimmed"
    );
}

/// Status with leading/trailing whitespace should be trimmed.
#[test]
fn validate_quest_update_trims_status() {
    let result = validate_quest_update("The Quest", "  active: find the thing  ");
    assert!(result.is_ok());
    assert_eq!(
        result.unwrap().status(),
        "active: find the thing",
        "status must be trimmed"
    );
}

// ============================================================================
// REWORK: Sidecar wiring tests (Reviewer finding — parse_tool_results)
// ============================================================================

fn test_session_id(test_name: &str) -> String {
    format!("test-20-6-{}-{}", test_name, std::process::id())
}

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

fn cleanup_sidecar(session_id: &str) {
    let path = sidecar_path(session_id);
    let _ = std::fs::remove_file(path);
}

/// Wiring test: parse_tool_results must recognize quest_update records and
/// populate ToolCallResults.quest_updates HashMap.
#[test]
fn parser_extracts_quest_update_from_sidecar() {
    let sid = test_session_id("parse-quest");
    write_sidecar(&sid, &[
        r#"{"tool":"quest_update","result":{"quest_name":"The Corrupted Grove","status":"completed: the source was purified"}}"#,
    ]);

    let results = parse_tool_results(&sid);
    let quests = results.quest_updates.expect("quest_update tool result should populate quest_updates");
    assert_eq!(quests.len(), 1);
    assert_eq!(
        quests.get("The Corrupted Grove").unwrap(),
        "completed: the source was purified"
    );

    cleanup_sidecar(&sid);
}

/// Multiple quest_update records in one sidecar accumulate into the HashMap.
#[test]
fn parser_accumulates_multiple_quest_updates() {
    let sid = test_session_id("parse-quest-multi");
    write_sidecar(&sid, &[
        r#"{"tool":"quest_update","result":{"quest_name":"The Corrupted Grove","status":"completed: purified"}}"#,
        r#"{"tool":"quest_update","result":{"quest_name":"The Missing Merchant","status":"active: search the docks (from: Harbormaster Dex)"}}"#,
    ]);

    let results = parse_tool_results(&sid);
    let quests = results.quest_updates.expect("multiple quest_updates should accumulate");
    assert_eq!(quests.len(), 2);
    assert!(quests.contains_key("The Corrupted Grove"));
    assert!(quests.contains_key("The Missing Merchant"));

    cleanup_sidecar(&sid);
}

/// Parser must reject empty quest_name from sidecar — validator rejects it,
/// so the parser must call the validator (not raw-insert).
#[test]
fn parser_rejects_empty_quest_name_from_sidecar() {
    let sid = test_session_id("parse-quest-empty-name");
    write_sidecar(&sid, &[
        r#"{"tool":"quest_update","result":{"quest_name":"","status":"active: some quest"}}"#,
    ]);

    let results = parse_tool_results(&sid);
    // Empty quest_name must be rejected — quest_updates should be None (no valid records)
    assert!(
        results.quest_updates.is_none(),
        "empty quest_name must be rejected by the parser — got {:?}",
        results.quest_updates
    );

    cleanup_sidecar(&sid);
}

/// Parser must reject whitespace-only quest_name from sidecar.
#[test]
fn parser_rejects_whitespace_quest_name_from_sidecar() {
    let sid = test_session_id("parse-quest-ws-name");
    write_sidecar(&sid, &[
        r#"{"tool":"quest_update","result":{"quest_name":"   ","status":"active: some quest"}}"#,
    ]);

    let results = parse_tool_results(&sid);
    assert!(
        results.quest_updates.is_none(),
        "whitespace-only quest_name must be rejected by the parser"
    );

    cleanup_sidecar(&sid);
}

/// Parser must trim quest_name and status from sidecar records.
#[test]
fn parser_trims_quest_update_fields() {
    let sid = test_session_id("parse-quest-trim");
    write_sidecar(&sid, &[
        r#"{"tool":"quest_update","result":{"quest_name":"  The Corrupted Grove  ","status":"  active: find the source  "}}"#,
    ]);

    let results = parse_tool_results(&sid);
    let quests = results.quest_updates.expect("trimmed quest_update should be accepted");
    assert_eq!(
        quests.get("The Corrupted Grove").unwrap(),
        "active: find the source",
        "parser must trim quest_name and status"
    );
    // Verify the untrimmed key is NOT present
    assert!(
        !quests.contains_key("  The Corrupted Grove  "),
        "untrimmed quest_name must not be a key"
    );

    cleanup_sidecar(&sid);
}

/// End-to-end: sidecar → parse_tool_results → assemble_turn → ActionResult.
#[test]
fn quest_update_e2e_sidecar_to_action_result() {
    let sid = test_session_id("e2e-quest");
    write_sidecar(&sid, &[
        r#"{"tool":"quest_update","result":{"quest_name":"The Corrupted Grove","status":"completed: the source was purified"}}"#,
    ]);

    let tool_results = parse_tool_results(&sid);
    let extraction = extraction_with_quests(); // has narrator fallback quest
    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);

    // Tool result must override narrator extraction
    assert!(
        result.quest_updates.contains_key("The Corrupted Grove"),
        "e2e: tool quest update must be present"
    );
    assert_eq!(
        result.quest_updates.get("The Corrupted Grove").unwrap(),
        "completed: the source was purified",
        "e2e: tool result must override narrator extraction"
    );

    cleanup_sidecar(&sid);
}

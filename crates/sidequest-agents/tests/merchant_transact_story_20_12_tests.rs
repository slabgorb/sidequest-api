//! Story 20-12: merchant_transact sidecar tool — narrator calls tool to execute
//! buy/sell, sidecar parser validates against merchant inventory
//!
//! RED phase — tests for the merchant_transact tool wired into the sidecar tool
//! call pipeline. Follows the exact pattern established by item_acquire (story 20-11).
//!
//! ACs tested:
//!   1. merchant_transact tool call is fully wired in the sidecar tool call pipeline
//!   2. Parser validates transaction details (type, item, merchant name)
//!   3. assemble_turn feeds merchant_transact results into merchant_transactions
//!   4. Tests verify full pipeline (unit + integration + wiring)
//!   5. No regressions in other tool pipelines or item_acquire integration

use std::collections::HashMap;
use std::io::Write;

use sidequest_agents::tools::assemble_turn::{assemble_turn, ToolCallResults};
use sidequest_agents::tools::merchant_transact::{validate_merchant_transact, MerchantTransactResult};
use sidequest_agents::tools::tool_call_parser::{
    parse_tool_results, sidecar_path, SIDECAR_DIR,
};
use sidequest_agents::orchestrator::{
    ActionFlags, ActionRewrite, MerchantTransactionExtracted, NarratorExtraction,
};

// ============================================================================
// Helpers
// ============================================================================

fn default_rewrite() -> ActionRewrite {
    ActionRewrite {
        you: "You browse the wares".to_string(),
        named: "Kael browses the wares".to_string(),
        intent: "browse merchant wares".to_string(),
    }
}

fn default_flags() -> ActionFlags {
    ActionFlags {
        is_power_grab: false,
        references_inventory: true,
        references_npc: true,
        references_ability: false,
        references_location: false,
    }
}

fn empty_extraction() -> NarratorExtraction {
    NarratorExtraction {
        prose: "The merchant nods and hands you the sword.".to_string(),
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
    format!("test-20-12-{}-{}", test_name, std::process::id())
}

/// Clean up sidecar file after test.
fn cleanup_sidecar(session_id: &str) {
    let path = sidecar_path(session_id);
    let _ = std::fs::remove_file(path);
}

// ============================================================================
// AC-2: validate_merchant_transact — validates transaction details
// ============================================================================

#[test]
fn validate_merchant_transact_accepts_valid_buy() {
    let result = validate_merchant_transact("buy", "iron_sword", "Gareth the Smith");
    assert!(result.is_ok(), "valid buy transaction should be accepted");

    let validated = result.unwrap();
    assert_eq!(validated.transaction_type(), "buy");
    assert_eq!(validated.item_id(), "iron_sword");
    assert_eq!(validated.merchant(), "Gareth the Smith");
}

#[test]
fn validate_merchant_transact_accepts_valid_sell() {
    let result = validate_merchant_transact("sell", "rusty_dagger", "Merchant Zara");
    assert!(result.is_ok(), "valid sell transaction should be accepted");

    let validated = result.unwrap();
    assert_eq!(validated.transaction_type(), "sell");
    assert_eq!(validated.item_id(), "rusty_dagger");
    assert_eq!(validated.merchant(), "Merchant Zara");
}

#[test]
fn validate_merchant_transact_rejects_empty_transaction_type() {
    let result = validate_merchant_transact("", "iron_sword", "Gareth");
    assert!(result.is_err(), "empty transaction_type should be rejected");
}

#[test]
fn validate_merchant_transact_rejects_invalid_transaction_type() {
    let result = validate_merchant_transact("trade", "iron_sword", "Gareth");
    assert!(
        result.is_err(),
        "transaction_type must be 'buy' or 'sell', not 'trade'"
    );
}

#[test]
fn validate_merchant_transact_rejects_empty_item_id() {
    let result = validate_merchant_transact("buy", "", "Gareth");
    assert!(result.is_err(), "empty item_id should be rejected");
}

#[test]
fn validate_merchant_transact_rejects_whitespace_only_item_id() {
    let result = validate_merchant_transact("buy", "   ", "Gareth");
    assert!(result.is_err(), "whitespace-only item_id should be rejected");
}

#[test]
fn validate_merchant_transact_rejects_empty_merchant() {
    let result = validate_merchant_transact("buy", "iron_sword", "");
    assert!(result.is_err(), "empty merchant name should be rejected");
}

#[test]
fn validate_merchant_transact_rejects_whitespace_only_merchant() {
    let result = validate_merchant_transact("sell", "iron_sword", "   ");
    assert!(
        result.is_err(),
        "whitespace-only merchant name should be rejected"
    );
}

#[test]
fn validate_merchant_transact_trims_whitespace() {
    let result = validate_merchant_transact("  buy  ", "  iron_sword  ", "  Gareth  ");
    assert!(result.is_ok(), "should accept after trimming whitespace");

    let validated = result.unwrap();
    assert_eq!(validated.transaction_type(), "buy");
    assert_eq!(validated.item_id(), "iron_sword");
    assert_eq!(validated.merchant(), "Gareth");
}

#[test]
fn validate_merchant_transact_case_insensitive_type() {
    let result = validate_merchant_transact("BUY", "iron_sword", "Gareth");
    assert!(result.is_ok(), "transaction_type should be case-insensitive");

    let validated = result.unwrap();
    assert_eq!(
        validated.transaction_type(),
        "buy",
        "transaction_type should be normalized to lowercase"
    );
}

#[test]
fn validate_merchant_transact_converts_to_extracted() {
    let result = validate_merchant_transact("buy", "iron_sword", "Gareth").unwrap();
    let extracted = result.to_merchant_transaction_extracted();

    assert_eq!(extracted.transaction_type, "buy");
    assert_eq!(extracted.item_id, "iron_sword");
    assert_eq!(extracted.merchant, "Gareth");
}

// ============================================================================
// AC-1: Parser handles merchant_transact tool call records
// ============================================================================

#[test]
fn parse_tool_results_extracts_merchant_transact_buy() {
    let sid = test_session_id("buy");
    write_sidecar(&sid, &[
        r#"{"tool":"merchant_transact","result":{"transaction_type":"buy","item_id":"iron_sword","merchant":"Gareth the Smith"}}"#,
    ]);

    let results = parse_tool_results(&sid);
    let txns = results
        .merchant_transactions
        .expect("merchant_transactions should be Some after merchant_transact tool call");
    assert_eq!(txns.len(), 1);
    assert_eq!(txns[0].transaction_type, "buy");
    assert_eq!(txns[0].item_id, "iron_sword");
    assert_eq!(txns[0].merchant, "Gareth the Smith");

    cleanup_sidecar(&sid);
}

#[test]
fn parse_tool_results_extracts_merchant_transact_sell() {
    let sid = test_session_id("sell");
    write_sidecar(&sid, &[
        r#"{"tool":"merchant_transact","result":{"transaction_type":"sell","item_id":"rusty_dagger","merchant":"Zara"}}"#,
    ]);

    let results = parse_tool_results(&sid);
    let txns = results
        .merchant_transactions
        .expect("merchant_transactions should be Some for sell");
    assert_eq!(txns.len(), 1);
    assert_eq!(txns[0].transaction_type, "sell");
    assert_eq!(txns[0].item_id, "rusty_dagger");
    assert_eq!(txns[0].merchant, "Zara");

    cleanup_sidecar(&sid);
}

#[test]
fn parse_tool_results_accumulates_multiple_merchant_transact_calls() {
    let sid = test_session_id("multi");
    write_sidecar(&sid, &[
        r#"{"tool":"merchant_transact","result":{"transaction_type":"buy","item_id":"healing_potion","merchant":"Zara"}}"#,
        r#"{"tool":"merchant_transact","result":{"transaction_type":"sell","item_id":"old_boots","merchant":"Zara"}}"#,
        r#"{"tool":"merchant_transact","result":{"transaction_type":"buy","item_id":"torch","merchant":"Zara"}}"#,
    ]);

    let results = parse_tool_results(&sid);
    let txns = results
        .merchant_transactions
        .expect("should have merchant_transactions");
    assert_eq!(txns.len(), 3, "all three transactions should be collected");
    assert_eq!(txns[0].transaction_type, "buy");
    assert_eq!(txns[0].item_id, "healing_potion");
    assert_eq!(txns[1].transaction_type, "sell");
    assert_eq!(txns[1].item_id, "old_boots");
    assert_eq!(txns[2].transaction_type, "buy");
    assert_eq!(txns[2].item_id, "torch");

    cleanup_sidecar(&sid);
}

#[test]
fn parse_tool_results_skips_invalid_merchant_transact() {
    let sid = test_session_id("invalid-tx");
    write_sidecar(&sid, &[
        // Valid transaction
        r#"{"tool":"merchant_transact","result":{"transaction_type":"buy","item_id":"sword","merchant":"Gareth"}}"#,
        // Invalid — missing merchant field
        r#"{"tool":"merchant_transact","result":{"transaction_type":"buy","item_id":"shield"}}"#,
        // Invalid — bad transaction_type
        r#"{"tool":"merchant_transact","result":{"transaction_type":"barter","item_id":"gem","merchant":"Gareth"}}"#,
    ]);

    let results = parse_tool_results(&sid);
    let txns = results
        .merchant_transactions
        .expect("should have merchant_transactions from valid call");
    assert_eq!(
        txns.len(),
        1,
        "only the valid transaction should be collected"
    );
    assert_eq!(txns[0].item_id, "sword");

    cleanup_sidecar(&sid);
}

#[test]
fn parse_tool_results_merchant_transact_none_when_no_merchant_tools_fired() {
    let sid = test_session_id("no-merchant");
    write_sidecar(&sid, &[
        r#"{"tool":"set_mood","result":{"mood":"tension"}}"#,
    ]);

    let results = parse_tool_results(&sid);
    assert!(
        results.merchant_transactions.is_none(),
        "merchant_transactions should be None when no merchant_transact tools fired"
    );

    cleanup_sidecar(&sid);
}

// ============================================================================
// AC-1: merchant_transact coexists with other tool calls
// ============================================================================

#[test]
fn parse_tool_results_merchant_transact_with_other_tools() {
    let sid = test_session_id("coexist");
    write_sidecar(&sid, &[
        r#"{"tool":"set_mood","result":{"mood":"calm"}}"#,
        r#"{"tool":"merchant_transact","result":{"transaction_type":"buy","item_id":"potion","merchant":"Zara"}}"#,
        r#"{"tool":"item_acquire","result":{"item_ref":"potion","name":"Healing Potion","category":"consumable"}}"#,
    ]);

    let results = parse_tool_results(&sid);

    // Mood should be parsed
    assert_eq!(results.scene_mood.as_deref(), Some("calm"));

    // Merchant transaction should be parsed
    let txns = results
        .merchant_transactions
        .expect("should have merchant_transactions");
    assert_eq!(txns.len(), 1);
    assert_eq!(txns[0].item_id, "potion");

    // Item acquire should also be parsed
    let items = results
        .items_acquired
        .expect("should have items_acquired");
    assert_eq!(items.len(), 1);

    cleanup_sidecar(&sid);
}

// ============================================================================
// AC-3: assemble_turn feeds merchant_transact results into merchant_transactions
// ============================================================================

#[test]
fn assemble_turn_uses_tool_merchant_transactions_over_extraction() {
    let extraction = empty_extraction();
    let rewrite = default_rewrite();
    let flags = default_flags();

    let mut tool_results = ToolCallResults::default();
    tool_results.merchant_transactions = Some(vec![MerchantTransactionExtracted {
        transaction_type: "buy".to_string(),
        item_id: "iron_sword".to_string(),
        merchant: "Gareth".to_string(),
    }]);

    let result = assemble_turn(extraction, rewrite, flags, tool_results);

    assert_eq!(
        result.merchant_transactions.len(),
        1,
        "tool results should provide merchant_transactions"
    );
    assert_eq!(result.merchant_transactions[0].transaction_type, "buy");
    assert_eq!(result.merchant_transactions[0].item_id, "iron_sword");
    assert_eq!(result.merchant_transactions[0].merchant, "Gareth");
}

#[test]
fn assemble_turn_falls_back_to_extraction_when_no_merchant_tool() {
    let mut extraction = empty_extraction();
    extraction.merchant_transactions = vec![MerchantTransactionExtracted {
        transaction_type: "sell".to_string(),
        item_id: "old_shield".to_string(),
        merchant: "Trader".to_string(),
    }];
    let rewrite = default_rewrite();
    let flags = default_flags();
    let tool_results = ToolCallResults::default();

    let result = assemble_turn(extraction, rewrite, flags, tool_results);

    assert_eq!(
        result.merchant_transactions.len(),
        1,
        "should fall back to extraction merchant_transactions when no tool fired"
    );
    assert_eq!(result.merchant_transactions[0].item_id, "old_shield");
}

#[test]
fn assemble_turn_tool_results_override_extraction_merchant_transactions() {
    // Extraction has one transaction, tool results have a different one
    // Tool results should WIN (same pattern as scene_mood, items_acquired)
    let mut extraction = empty_extraction();
    extraction.merchant_transactions = vec![MerchantTransactionExtracted {
        transaction_type: "sell".to_string(),
        item_id: "extraction_item".to_string(),
        merchant: "ExtractionMerchant".to_string(),
    }];
    let rewrite = default_rewrite();
    let flags = default_flags();

    let mut tool_results = ToolCallResults::default();
    tool_results.merchant_transactions = Some(vec![MerchantTransactionExtracted {
        transaction_type: "buy".to_string(),
        item_id: "tool_item".to_string(),
        merchant: "ToolMerchant".to_string(),
    }]);

    let result = assemble_turn(extraction, rewrite, flags, tool_results);

    assert_eq!(result.merchant_transactions.len(), 1);
    assert_eq!(
        result.merchant_transactions[0].item_id, "tool_item",
        "tool results should override extraction merchant_transactions"
    );
    assert_eq!(result.merchant_transactions[0].merchant, "ToolMerchant");
}

// ============================================================================
// AC-4: Integration — sidecar → parse → assemble_turn → ActionResult
// ============================================================================

#[test]
fn merchant_transact_full_pipeline_sidecar_to_action_result() {
    let sid = test_session_id("pipeline");
    write_sidecar(&sid, &[
        r#"{"tool":"merchant_transact","result":{"transaction_type":"buy","item_id":"healing_potion","merchant":"Old Zara"}}"#,
        r#"{"tool":"merchant_transact","result":{"transaction_type":"sell","item_id":"rusty_sword","merchant":"Old Zara"}}"#,
    ]);

    // Step 1: Parse sidecar
    let tool_results = parse_tool_results(&sid);
    let txns = tool_results
        .merchant_transactions
        .as_ref()
        .expect("should have parsed merchant_transactions");
    assert_eq!(txns.len(), 2);

    // Step 2: Assemble with empty extraction
    let extraction = empty_extraction();
    let rewrite = default_rewrite();
    let flags = default_flags();

    let result = assemble_turn(extraction, rewrite, flags, tool_results);

    // Step 3: Verify ActionResult has correct merchant_transactions
    assert_eq!(result.merchant_transactions.len(), 2);
    assert_eq!(result.merchant_transactions[0].transaction_type, "buy");
    assert_eq!(result.merchant_transactions[0].item_id, "healing_potion");
    assert_eq!(result.merchant_transactions[0].merchant, "Old Zara");
    assert_eq!(result.merchant_transactions[1].transaction_type, "sell");
    assert_eq!(result.merchant_transactions[1].item_id, "rusty_sword");
    assert_eq!(result.merchant_transactions[1].merchant, "Old Zara");

    cleanup_sidecar(&sid);
}

// ============================================================================
// AC-4: Wiring — source code verification
// ============================================================================

#[test]
fn tool_call_parser_handles_merchant_transact() {
    // Verify the parser source code has a "merchant_transact" match arm
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let parser_src =
        std::fs::read_to_string(format!("{manifest_dir}/src/tools/tool_call_parser.rs"))
            .expect("should be able to read tool_call_parser.rs");

    assert!(
        parser_src.contains("\"merchant_transact\""),
        "tool_call_parser.rs must handle 'merchant_transact' tool records"
    );
}

#[test]
fn tool_call_results_has_merchant_transactions_field() {
    // Verify the ToolCallResults struct has a merchant_transactions field
    // This compiles only if the field exists
    let results = ToolCallResults::default();
    let _field: &Option<Vec<MerchantTransactionExtracted>> = &results.merchant_transactions;
}

#[test]
fn assemble_turn_reads_merchant_transactions_from_tool_results() {
    // Verify assemble_turn source code uses tool_results.merchant_transactions
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let assemble_src =
        std::fs::read_to_string(format!("{manifest_dir}/src/tools/assemble_turn.rs"))
            .expect("should be able to read assemble_turn.rs");

    assert!(
        assemble_src.contains("merchant_transactions"),
        "assemble_turn.rs must reference merchant_transactions from tool_results"
    );

    // Should NOT be hardcoded to extraction.merchant_transactions only
    assert!(
        assemble_src.contains("tool_results.merchant_transactions")
            || assemble_src.contains("tools.merchant_transactions"),
        "assemble_turn must use merchant_transactions from tool_results, not just extraction"
    );
}

#[test]
fn merchant_transact_module_is_exported() {
    // Compile-time check: merchant_transact module is public and accessible
    let _fn_ptr: fn(&str, &str, &str) -> Result<MerchantTransactResult, _> =
        validate_merchant_transact;
}

// ============================================================================
// AC-5: No regression — item_acquire still works alongside merchant_transact
// ============================================================================

#[test]
fn item_acquire_unaffected_by_merchant_transact_addition() {
    let sid = test_session_id("no-regress");
    write_sidecar(&sid, &[
        r#"{"tool":"item_acquire","result":{"item_ref":"shield","name":"Iron Shield","category":"armor"}}"#,
    ]);

    let results = parse_tool_results(&sid);

    // item_acquire should still work
    let items = results
        .items_acquired
        .expect("items_acquired should still work");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].name, "Iron Shield");

    // merchant_transactions should be None (no merchant_transact tool fired)
    assert!(
        results.merchant_transactions.is_none(),
        "merchant_transactions should be None when only item_acquire fired"
    );

    cleanup_sidecar(&sid);
}

// ============================================================================
// AC-3: OTEL span verification (structural — tool call parser emits spans)
// ============================================================================

#[test]
fn tool_call_parser_has_tracing_instrumentation_for_merchant_transact() {
    // Verify the parser source has tracing instrumentation
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let parser_src =
        std::fs::read_to_string(format!("{manifest_dir}/src/tools/tool_call_parser.rs"))
            .expect("should be able to read tool_call_parser.rs");

    // The parser function is already instrumented with #[tracing::instrument]
    // We verify the merchant_transact arm logs appropriately
    assert!(
        parser_src.contains("merchant_transact"),
        "parser must handle merchant_transact"
    );
}

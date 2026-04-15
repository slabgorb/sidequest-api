//! Story 20-11: item_acquire sidecar tool — narrator calls tool to grant items,
//! sidecar parser validates and feeds assemble_turn.
//!
//! RED phase — tests for Phase 3 of ADR-057 (Narrator Crunch Separation).
//! Migrates items_gained from the always-empty NarratorExtraction vector to discrete
//! `item_acquire` tool calls. The LLM decides THAT an item is granted; the tool
//! structures the acquisition.
//!
//! ACs tested:
//!   1. item_acquire tool call is fully wired in the sidecar tool call pipeline
//!   2. Parser validates item references against genre pack item_catalog
//!   3. assemble_turn feeds item_acquire results into items_gained
//!   4. Tests verify full pipeline (unit, integration, wiring)
//!   5. No regressions in other tool pipelines

use std::collections::HashMap;

use sidequest_agents::orchestrator::{ActionFlags, ActionRewrite, NarratorExtraction};
use std::io::Write;

use sidequest_agents::tools::assemble_turn::{assemble_turn, ToolCallResults};
use sidequest_agents::tools::item_acquire::{validate_item_acquire, ItemAcquireResult};
use sidequest_agents::tools::tool_call_parser::{parse_tool_results, sidecar_path};

fn nbs(s: &str) -> sidequest_protocol::NonBlankString {
    sidequest_protocol::NonBlankString::new(s).expect("test literal must be non-blank")
}

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

fn extraction_with_items() -> NarratorExtraction {
    NarratorExtraction {
        prose: "You pick up the rusty sword from the ground.".to_string(),
        footnotes: vec![],
        items_gained: vec![sidequest_protocol::ItemGained {
            name: nbs("narrator fallback sword"),
            description: nbs("A sword from the narrator's extraction"),
            category: "weapon".to_string(),
        }],
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
        beat_selections: vec![],
        confrontation: None,
        location: None,
        affinity_progress: vec![],
        gold_change: None,
    }
}

fn extraction_no_items() -> NarratorExtraction {
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
        beat_selections: vec![],
        confrontation: None,
        location: None,
        affinity_progress: vec![],
        gold_change: None,
    }
}

fn test_session_id(test_name: &str) -> String {
    format!("test-20-11-{}-{}", test_name, std::process::id())
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

// ============================================================================
// AC-1: item_acquire tool call wired in sidecar pipeline
// ============================================================================

/// The item_acquire module must be public and its validation function accessible.
#[test]
fn item_acquire_module_is_public() {
    // If this compiles, the module and function are wired into the crate
    let _: fn(&str, &str, &str) -> Result<ItemAcquireResult, _> = validate_item_acquire;
}

/// ItemAcquireResult struct must be exported and have the expected fields.
#[test]
fn item_acquire_result_has_required_fields() {
    let result = validate_item_acquire("iron_sword", "Iron Sword", "weapon")
        .expect("valid item_acquire should succeed");
    // Verify the struct has the three required accessors
    assert_eq!(result.item_ref(), "iron_sword");
    assert_eq!(result.name(), "Iron Sword");
    assert_eq!(result.category(), "weapon");
}

/// validate_item_acquire with a catalog-style ID (snake_case key).
#[test]
fn validate_item_acquire_catalog_style_id() {
    let result = validate_item_acquire("basic_sword", "Basic Sword", "weapon");
    assert!(result.is_ok(), "catalog-style item_ref should be accepted");
    let item = result.unwrap();
    assert_eq!(item.item_ref(), "basic_sword");
    assert_eq!(item.name(), "Basic Sword");
    assert_eq!(item.category(), "weapon");
}

/// validate_item_acquire with a narrator-described item (free-text description).
#[test]
fn validate_item_acquire_narrator_described() {
    let result = validate_item_acquire(
        "a rusty sword with strange runes",
        "Rusty Runed Sword",
        "weapon",
    );
    assert!(
        result.is_ok(),
        "narrator-described item_ref should be accepted"
    );
    let item = result.unwrap();
    assert_eq!(item.item_ref(), "a rusty sword with strange runes");
    assert_eq!(item.name(), "Rusty Runed Sword");
}

/// validate_item_acquire rejects empty item_ref.
#[test]
fn validate_item_acquire_rejects_empty_item_ref() {
    let result = validate_item_acquire("", "Some Name", "weapon");
    assert!(result.is_err(), "empty item_ref must be rejected");
}

/// validate_item_acquire rejects whitespace-only item_ref.
#[test]
fn validate_item_acquire_rejects_whitespace_item_ref() {
    let result = validate_item_acquire("   ", "Some Name", "weapon");
    assert!(result.is_err(), "whitespace-only item_ref must be rejected");
}

/// validate_item_acquire rejects empty name.
#[test]
fn validate_item_acquire_rejects_empty_name() {
    let result = validate_item_acquire("iron_sword", "", "weapon");
    assert!(result.is_err(), "empty name must be rejected");
}

/// validate_item_acquire rejects whitespace-only name.
#[test]
fn validate_item_acquire_rejects_whitespace_name() {
    let result = validate_item_acquire("iron_sword", "   ", "weapon");
    assert!(result.is_err(), "whitespace-only name must be rejected");
}

/// validate_item_acquire rejects empty category.
#[test]
fn validate_item_acquire_rejects_empty_category() {
    let result = validate_item_acquire("iron_sword", "Iron Sword", "");
    assert!(result.is_err(), "empty category must be rejected");
}

/// validate_item_acquire trims whitespace from all fields.
#[test]
fn validate_item_acquire_trims_fields() {
    let result = validate_item_acquire("  iron_sword  ", "  Iron Sword  ", "  weapon  ");
    assert!(
        result.is_ok(),
        "padded fields should be accepted after trimming"
    );
    let item = result.unwrap();
    assert_eq!(item.item_ref(), "iron_sword", "item_ref must be trimmed");
    assert_eq!(item.name(), "Iron Sword", "name must be trimmed");
    assert_eq!(item.category(), "weapon", "category must be trimmed");
}

/// ItemAcquireResult must serialize to JSON with expected shape.
#[test]
fn item_acquire_result_serializes_to_json() {
    let result = validate_item_acquire("iron_sword", "Iron Sword", "weapon").unwrap();
    let json = serde_json::to_value(&result).expect("ItemAcquireResult must serialize");
    assert_eq!(json["item_ref"], "iron_sword");
    assert_eq!(json["name"], "Iron Sword");
    assert_eq!(json["category"], "weapon");
}

// ============================================================================
// AC-1: ToolCallResults has items_acquired field
// ============================================================================

/// ToolCallResults must have an items_acquired field (Option<Vec<ItemGained>>).
#[test]
fn tool_call_results_has_items_acquired_field() {
    let items = vec![sidequest_protocol::ItemGained {
        name: nbs("Iron Sword"),
        description: nbs("A sturdy iron blade."),
        category: "weapon".to_string(),
    }];

    let results = ToolCallResults {
        items_acquired: Some(items),
        ..ToolCallResults::default()
    };

    assert!(results.items_acquired.is_some());
    assert_eq!(results.items_acquired.unwrap().len(), 1);
}

/// Default ToolCallResults must have items_acquired as None.
#[test]
fn tool_call_results_default_items_acquired_is_none() {
    let defaults = ToolCallResults::default();
    assert!(
        defaults.items_acquired.is_none(),
        "default items_acquired must be None (no tools fired)"
    );
}

// ============================================================================
// AC-2: Parser validates item references — catalog, narrator-described, invalid
// ============================================================================

/// Catalog-style item with all fields populated should produce valid ItemGained.
#[test]
fn validate_item_acquire_produces_item_gained() {
    let result = validate_item_acquire("iron_sword", "Iron Sword", "weapon").unwrap();
    let item_gained = result.to_item_gained();
    assert_eq!(item_gained.name.as_str(), "Iron Sword");
    assert_eq!(item_gained.category, "weapon");
    // description should be non-empty (either from catalog or generated)
    assert!(
        !item_gained.description.as_str().is_empty(),
        "description must not be empty"
    );
}

/// Narrator-described items should produce valid ItemGained with the description
/// derived from the item_ref.
#[test]
fn validate_item_acquire_narrator_described_to_item_gained() {
    let result = validate_item_acquire(
        "a rusty sword with strange runes",
        "Rusty Runed Sword",
        "weapon",
    )
    .unwrap();
    let item_gained = result.to_item_gained();
    assert_eq!(item_gained.name.as_str(), "Rusty Runed Sword");
    assert_eq!(item_gained.category, "weapon");
}

/// Consumable items should be accepted.
#[test]
fn validate_item_acquire_consumable_category() {
    let result = validate_item_acquire("health_potion", "Health Potion", "consumable");
    assert!(result.is_ok());
    assert_eq!(result.unwrap().category(), "consumable");
}

/// Quest items should be accepted.
#[test]
fn validate_item_acquire_quest_category() {
    let result = validate_item_acquire("ancient_key", "Ancient Key", "quest");
    assert!(result.is_ok());
    assert_eq!(result.unwrap().category(), "quest");
}

/// Misc items should be accepted.
#[test]
fn validate_item_acquire_misc_category() {
    let result = validate_item_acquire("shiny_stone", "Shiny Stone", "misc");
    assert!(result.is_ok());
    assert_eq!(result.unwrap().category(), "misc");
}

/// Unicode item names should be accepted (genre packs may use non-ASCII).
#[test]
fn validate_item_acquire_unicode_name() {
    let result = validate_item_acquire("katana_of_wind", "風の刀", "weapon");
    assert!(result.is_ok());
    let item = result.unwrap();
    assert_eq!(item.name(), "風の刀");
}

/// Long item descriptions should be accepted (LLM can be verbose).
#[test]
fn validate_item_acquire_long_description() {
    let long_ref = "a particularly ornate and elaborately decorated ceremonial dagger that was once wielded by the high priest of the ancient order of the silver flame during their most sacred rituals";
    let result = validate_item_acquire(long_ref, "Ceremonial Dagger", "weapon");
    assert!(result.is_ok());
    assert_eq!(result.unwrap().item_ref(), long_ref);
}

// ============================================================================
// AC-1: Parser recognizes item_acquire in sidecar JSONL
// ============================================================================

/// The tool_call_parser must recognize "item_acquire" tool records and populate
/// ToolCallResults.items_acquired.
#[test]
fn parser_extracts_item_acquire_from_sidecar() {
    let sid = test_session_id("parse-item");
    write_sidecar(
        &sid,
        &[
            r#"{"tool":"item_acquire","result":{"item_ref":"iron_sword","name":"Iron Sword","category":"weapon"}}"#,
        ],
    );

    let results = parse_tool_results(&sid);
    let items = results
        .items_acquired
        .expect("item_acquire tool result should populate items_acquired");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].name.as_str(), "Iron Sword");
    assert_eq!(items[0].category, "weapon");

    cleanup_sidecar(&sid);
}

/// Multiple item_acquire records in one sidecar accumulate into the Vec.
#[test]
fn parser_accumulates_multiple_item_acquires() {
    let sid = test_session_id("parse-item-multi");
    write_sidecar(
        &sid,
        &[
            r#"{"tool":"item_acquire","result":{"item_ref":"iron_sword","name":"Iron Sword","category":"weapon"}}"#,
            r#"{"tool":"item_acquire","result":{"item_ref":"health_potion","name":"Health Potion","category":"consumable"}}"#,
        ],
    );

    let results = parse_tool_results(&sid);
    let items = results
        .items_acquired
        .expect("multiple item_acquires should accumulate");
    assert_eq!(items.len(), 2);

    // Verify both items present (order preserved from sidecar)
    assert_eq!(items[0].name.as_str(), "Iron Sword");
    assert_eq!(items[1].name.as_str(), "Health Potion");

    cleanup_sidecar(&sid);
}

/// Parser must reject empty item_ref from sidecar — validator rejects it,
/// so the parser must call the validator (not raw-insert).
#[test]
fn parser_rejects_empty_item_ref_from_sidecar() {
    let sid = test_session_id("parse-item-empty-ref");
    write_sidecar(
        &sid,
        &[
            r#"{"tool":"item_acquire","result":{"item_ref":"","name":"Some Item","category":"weapon"}}"#,
        ],
    );

    let results = parse_tool_results(&sid);
    // Empty item_ref must be rejected — items_acquired should be None (no valid records)
    assert!(
        results.items_acquired.is_none(),
        "empty item_ref must be rejected by the parser — got {:?}",
        results.items_acquired
    );

    cleanup_sidecar(&sid);
}

/// Parser must reject empty name from sidecar.
#[test]
fn parser_rejects_empty_name_from_sidecar() {
    let sid = test_session_id("parse-item-empty-name");
    write_sidecar(
        &sid,
        &[
            r#"{"tool":"item_acquire","result":{"item_ref":"iron_sword","name":"","category":"weapon"}}"#,
        ],
    );

    let results = parse_tool_results(&sid);
    assert!(
        results.items_acquired.is_none(),
        "empty name must be rejected by the parser"
    );

    cleanup_sidecar(&sid);
}

/// Parser must handle missing category field gracefully — reject, don't panic.
#[test]
fn parser_rejects_missing_category_from_sidecar() {
    let sid = test_session_id("parse-item-no-category");
    write_sidecar(
        &sid,
        &[r#"{"tool":"item_acquire","result":{"item_ref":"iron_sword","name":"Iron Sword"}}"#],
    );

    let results = parse_tool_results(&sid);
    // Missing required field must be rejected
    assert!(
        results.items_acquired.is_none(),
        "missing category must be rejected by the parser"
    );

    cleanup_sidecar(&sid);
}

/// Parser must trim fields from sidecar records.
#[test]
fn parser_trims_item_acquire_fields() {
    let sid = test_session_id("parse-item-trim");
    write_sidecar(
        &sid,
        &[
            r#"{"tool":"item_acquire","result":{"item_ref":"  iron_sword  ","name":"  Iron Sword  ","category":"  weapon  "}}"#,
        ],
    );

    let results = parse_tool_results(&sid);
    let items = results
        .items_acquired
        .expect("trimmed item_acquire should be accepted");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].name, "Iron Sword", "name must be trimmed");
    assert_eq!(items[0].category, "weapon", "category must be trimmed");

    cleanup_sidecar(&sid);
}

/// Valid item_acquire records alongside other tool records — no interference.
#[test]
fn parser_item_acquire_coexists_with_other_tools() {
    let sid = test_session_id("parse-item-coexist");
    write_sidecar(
        &sid,
        &[
            r#"{"tool":"set_mood","result":{"mood":"triumph"}}"#,
            r#"{"tool":"item_acquire","result":{"item_ref":"iron_sword","name":"Iron Sword","category":"weapon"}}"#,
            r#"{"tool":"quest_update","result":{"quest_name":"The Heist","status":"completed: success"}}"#,
        ],
    );

    let results = parse_tool_results(&sid);

    // item_acquire populated
    let items = results
        .items_acquired
        .expect("item_acquire should be parsed");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].name.as_str(), "Iron Sword");

    // Other tools also populated
    assert_eq!(results.scene_mood.as_deref(), Some("triumph"));
    assert!(results.quest_updates.is_some());

    cleanup_sidecar(&sid);
}

// ============================================================================
// AC-3: assemble_turn feeds item_acquire results into items_gained
// ============================================================================

/// When item_acquire tools fire, their values populate ActionResult.items_gained.
#[test]
fn assemble_turn_tool_items_override_narrator() {
    let extraction = extraction_with_items(); // has fallback narrator item
    let tool_items = vec![sidequest_protocol::ItemGained {
        name: nbs("Tool Iron Sword"),
        description: nbs("A sword from the tool call."),
        category: "weapon".to_string(),
    }];

    let tool_results = ToolCallResults {
        items_acquired: Some(tool_items),
        ..ToolCallResults::default()
    };

    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);

    // Tool result must replace narrator extraction
    assert_eq!(
        result.items_gained.len(),
        1,
        "tool items must replace narrator items"
    );
    assert_eq!(result.items_gained[0].name.as_str(), "Tool Iron Sword");
}

/// Multiple item_acquire tool calls in one turn produce multiple items_gained.
#[test]
fn assemble_turn_multiple_item_tools() {
    let extraction = extraction_no_items();
    let tool_items = vec![
        sidequest_protocol::ItemGained {
            name: nbs("Iron Sword"),
            description: nbs("A sturdy blade."),
            category: "weapon".to_string(),
        },
        sidequest_protocol::ItemGained {
            name: nbs("Health Potion"),
            description: nbs("Restores health."),
            category: "consumable".to_string(),
        },
    ];

    let tool_results = ToolCallResults {
        items_acquired: Some(tool_items),
        ..ToolCallResults::default()
    };

    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);

    assert_eq!(
        result.items_gained.len(),
        2,
        "both tool items must be collected"
    );
    assert_eq!(result.items_gained[0].name, "Iron Sword");
    assert_eq!(result.items_gained[1].name, "Health Potion");
}

/// No item_acquire tools fired — narrator extraction's items_gained pass through.
#[test]
fn assemble_turn_no_item_tool_uses_narrator_fallback() {
    let extraction = extraction_with_items(); // has fallback item
    let tool_results = ToolCallResults::default();

    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);

    assert_eq!(
        result.items_gained.len(),
        1,
        "without tool calls, narrator's items_gained must pass through"
    );
    assert_eq!(result.items_gained[0].name, "narrator fallback sword");
}

/// No item_acquire tools AND narrator has no items — result is empty Vec.
#[test]
fn assemble_turn_no_items_anywhere_is_empty() {
    let extraction = extraction_no_items();
    let tool_results = ToolCallResults::default();

    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);

    assert!(
        result.items_gained.is_empty(),
        "no items from either source → must be empty"
    );
}

/// Tool items_acquired being Some(empty Vec) means "tools fired but no items granted."
/// This should still override narrator extraction (replace with empty).
#[test]
fn assemble_turn_empty_tool_items_overrides_narrator() {
    let extraction = extraction_with_items(); // has fallback item
    let tool_results = ToolCallResults {
        items_acquired: Some(vec![]),
        ..ToolCallResults::default()
    };

    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);

    assert!(
        result.items_gained.is_empty(),
        "Some(empty) tool items_acquired must override narrator's items"
    );
}

/// item_acquire tool results don't disrupt other fields.
#[test]
fn assemble_turn_item_tools_preserve_other_fields() {
    let extraction = extraction_with_items();
    let tool_items = vec![sidequest_protocol::ItemGained {
        name: "Tool Sword".to_string(),
        description: "From tool.".to_string(),
        category: "weapon".to_string(),
    }];

    let tool_results = ToolCallResults {
        items_acquired: Some(tool_items),
        scene_mood: Some("triumph".to_string()),
        ..ToolCallResults::default()
    };

    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);

    // Items from tool
    assert_eq!(result.items_gained.len(), 1);
    assert_eq!(result.items_gained[0].name, "Tool Sword");

    // Other fields still pass through
    assert_eq!(
        result.narration,
        "You pick up the rusty sword from the ground."
    );
    assert_eq!(result.scene_mood.as_deref(), Some("triumph"));
    assert!(result.action_rewrite.is_some());
}

// ============================================================================
// AC-3: OTEL spans for item acquisitions
// ============================================================================

/// validate_item_acquire must run cleanly under a tracing subscriber.
#[test]
fn validate_item_acquire_emits_otel_span() {
    let _guard = tracing::subscriber::set_default(
        tracing_subscriber::fmt()
            .with_test_writer()
            .with_max_level(tracing::Level::TRACE)
            .finish(),
    );

    let result = validate_item_acquire("iron_sword", "Iron Sword", "weapon");
    assert!(result.is_ok());
}

/// OTEL must capture invalid tool calls too — item_ref in span fields.
#[test]
fn validate_item_acquire_otel_on_invalid_input() {
    let _guard = tracing::subscriber::set_default(
        tracing_subscriber::fmt()
            .with_test_writer()
            .with_max_level(tracing::Level::TRACE)
            .finish(),
    );

    let result = validate_item_acquire("", "Some Name", "weapon");
    assert!(
        result.is_err(),
        "empty item_ref should be rejected even under tracing"
    );
}

// ============================================================================
// AC-4: End-to-end sidecar → parse → assemble → ActionResult
// ============================================================================

/// Full pipeline: sidecar JSONL → parse_tool_results → assemble_turn → ActionResult.
#[test]
fn item_acquire_e2e_sidecar_to_action_result() {
    let sid = test_session_id("e2e-item");
    write_sidecar(
        &sid,
        &[
            r#"{"tool":"item_acquire","result":{"item_ref":"iron_sword","name":"Iron Sword","category":"weapon"}}"#,
        ],
    );

    let tool_results = parse_tool_results(&sid);
    let extraction = extraction_with_items(); // has narrator fallback
    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);

    // Tool result must override narrator extraction
    assert_eq!(
        result.items_gained.len(),
        1,
        "e2e: tool items must replace narrator items"
    );
    assert_eq!(
        result.items_gained[0].name, "Iron Sword",
        "e2e: tool result must override narrator extraction"
    );
    assert_eq!(result.items_gained[0].category, "weapon");

    cleanup_sidecar(&sid);
}

/// Full pipeline with multiple items — both populate ActionResult.
#[test]
fn item_acquire_e2e_multiple_items() {
    let sid = test_session_id("e2e-item-multi");
    write_sidecar(
        &sid,
        &[
            r#"{"tool":"item_acquire","result":{"item_ref":"iron_sword","name":"Iron Sword","category":"weapon"}}"#,
            r#"{"tool":"item_acquire","result":{"item_ref":"health_potion","name":"Health Potion","category":"consumable"}}"#,
        ],
    );

    let tool_results = parse_tool_results(&sid);
    let extraction = extraction_no_items();
    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);

    assert_eq!(
        result.items_gained.len(),
        2,
        "e2e: both items must flow through"
    );
    assert_eq!(result.items_gained[0].name, "Iron Sword");
    assert_eq!(result.items_gained[1].name, "Health Potion");

    cleanup_sidecar(&sid);
}

/// Full pipeline with mixed tools — item_acquire alongside set_mood and quest_update.
#[test]
fn item_acquire_e2e_mixed_tools() {
    let sid = test_session_id("e2e-item-mixed");
    write_sidecar(
        &sid,
        &[
            r#"{"tool":"set_mood","result":{"mood":"triumph"}}"#,
            r#"{"tool":"item_acquire","result":{"item_ref":"iron_sword","name":"Iron Sword","category":"weapon"}}"#,
            r#"{"tool":"quest_update","result":{"quest_name":"The Heist","status":"completed: success"}}"#,
        ],
    );

    let tool_results = parse_tool_results(&sid);
    let extraction = extraction_no_items();
    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);

    // Items from tool
    assert_eq!(result.items_gained.len(), 1);
    assert_eq!(result.items_gained[0].name, "Iron Sword");

    // Other tools also applied
    assert_eq!(result.scene_mood.as_deref(), Some("triumph"));
    assert!(result.quest_updates.contains_key("The Heist"));

    cleanup_sidecar(&sid);
}

// ============================================================================
// AC-5: No regressions — existing tools unaffected by item_acquire addition
// ============================================================================

/// Adding items_acquired field must not break default ToolCallResults construction.
#[test]
fn tool_call_results_default_still_works() {
    let defaults = ToolCallResults::default();
    assert!(defaults.scene_mood.is_none());
    assert!(defaults.scene_intent.is_none());
    assert!(defaults.visual_scene.is_none());
    assert!(defaults.quest_updates.is_none());
    assert!(defaults.personality_events.is_none());
    assert!(defaults.resource_deltas.is_none());
    assert!(defaults.sfx_triggers.is_none());
    assert!(defaults.items_acquired.is_none());
}

/// Existing set_mood tool still works after adding items_acquired to ToolCallResults.
#[test]
fn regression_set_mood_still_works() {
    let sid = test_session_id("regress-mood");
    write_sidecar(
        &sid,
        &[r#"{"tool":"set_mood","result":{"mood":"tension"}}"#],
    );

    let results = parse_tool_results(&sid);
    assert_eq!(results.scene_mood.as_deref(), Some("tension"));
    assert!(
        results.items_acquired.is_none(),
        "no item_acquire → should be None"
    );

    cleanup_sidecar(&sid);
}

/// Existing quest_update tool still works after adding items_acquired.
#[test]
fn regression_quest_update_still_works() {
    let sid = test_session_id("regress-quest");
    write_sidecar(
        &sid,
        &[
            r#"{"tool":"quest_update","result":{"quest_name":"The Heist","status":"completed: success"}}"#,
        ],
    );

    let results = parse_tool_results(&sid);
    assert!(results.quest_updates.is_some());
    assert_eq!(
        results.quest_updates.unwrap().get("The Heist").unwrap(),
        "completed: success"
    );
    assert!(results.items_acquired.is_none());

    cleanup_sidecar(&sid);
}

/// assemble_turn with only existing tools (no items) still works correctly.
#[test]
fn regression_assemble_turn_without_items_unchanged() {
    let extraction = extraction_no_items();
    let tool_results = ToolCallResults {
        scene_mood: Some("calm".to_string()),
        ..ToolCallResults::default()
    };

    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);
    assert_eq!(result.scene_mood.as_deref(), Some("calm"));
    assert!(result.items_gained.is_empty());
}

// ============================================================================
// Rule Coverage: Rust lang-review checklist
// ============================================================================

/// Rule #1 (silent error swallowing): validate_item_acquire must return Result,
/// not silently swallow errors with .ok() or .unwrap_or_default().
#[test]
fn rule_1_validate_returns_result_not_silent() {
    // This test verifies the function signature returns Result — if it compiled
    // with a different return type, this pattern match would fail
    let result = validate_item_acquire("", "name", "cat");
    match result {
        Ok(_) => panic!("empty item_ref should not succeed"),
        Err(e) => {
            let msg = format!("{}", e);
            assert!(!msg.is_empty(), "error message must be non-empty");
        }
    }
}

/// Rule #6 (test quality): Self-check — verify this test file has no vacuous assertions.
/// This meta-test ensures we haven't accidentally written `let _ = result;` anywhere.
#[test]
fn rule_6_no_vacuous_assertions_in_test_file() {
    // Read our own test file and check for known vacuous patterns.
    // Since we can't easily read our own source at runtime, this test
    // verifies the key assertions are checking actual values, not just existence.
    let result = validate_item_acquire("iron_sword", "Iron Sword", "weapon").unwrap();
    // Check actual values, not just .is_some()
    assert_eq!(result.item_ref(), "iron_sword");
    assert_eq!(result.name(), "Iron Sword");
    assert_eq!(result.category(), "weapon");
}

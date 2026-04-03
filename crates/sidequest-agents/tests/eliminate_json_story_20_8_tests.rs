//! Story 20-8: Eliminate narrator JSON block — delete extractor.rs
//!
//! RED phase — tests for the capstone deletion of the 3-tier extraction pipeline.
//!
//! This story removes dead code after stories 20-1 through 20-7 migrated all
//! JSON fields to tool calls. `assemble_turn` is now the sole ActionResult producer;
//! `extractor.rs` and the narrator's JSON schema documentation are no longer needed.
//!
//! Targets:
//! - `narrator.rs` system prompt — no JSON schema documentation
//! - `extractor.rs` — deleted from crate
//! - `orchestrator.rs` — no JsonExtractor references
//! - `ActionResult.extraction_tier` — removed or permanently None
//! - `assemble_turn` — sole producer of ActionResult (no extraction_tier)
//!
//! ACs tested:
//!   1. Narrator system prompt contains no JSON schema documentation
//!   2. extractor.rs deleted from the crate
//!   3. JsonExtractor no longer in crate public API
//!   4. orchestrator.rs does not call any JSON extraction function
//!   5. extraction_tier removed or permanently None on ActionResult
//!   6. All existing tests pass (verified by test runner, not tested here)

use std::collections::HashMap;

use sidequest_agents::agent::Agent;
use sidequest_agents::agents::narrator::NarratorAgent;
use sidequest_agents::orchestrator::{
    ActionFlags, ActionRewrite, NarratorExtraction,
};
use sidequest_agents::tools::assemble_turn::{assemble_turn, ToolCallResults};
use sidequest_protocol::FactCategory;

// ============================================================================
// Helper: build a minimal NarratorExtraction
// ============================================================================

fn minimal_extraction() -> NarratorExtraction {
    NarratorExtraction {
        prose: "**The Collapsed Overpass**\n\nYou step into the ruins.".to_string(),
        footnotes: vec![],
        items_gained: vec![],
        npcs_present: vec![],
        quest_updates: HashMap::new(),
        visual_scene: None,
        scene_mood: Some("exploration".to_string()),
        personality_events: vec![],
        scene_intent: Some("Exploration".to_string()),
        resource_deltas: HashMap::new(),
        lore_established: None,
        merchant_transactions: vec![],
        sfx_triggers: vec![],
        action_rewrite: None,
        action_flags: None,
    }
}

// ============================================================================
// AC-1: Narrator system prompt contains no JSON schema documentation
// ============================================================================

#[test]
fn narrator_prompt_has_no_json_block_section() {
    let narrator = NarratorAgent::new();
    let prompt = narrator.system_prompt();

    assert!(
        !prompt.contains("[JSON BLOCK]"),
        "Narrator system prompt still contains [JSON BLOCK] section — \
         this must be removed now that tools produce all structured data"
    );
}

#[test]
fn narrator_prompt_has_no_footnote_protocol() {
    let narrator = NarratorAgent::new();
    let prompt = narrator.system_prompt();

    assert!(
        !prompt.contains("[FOOTNOTE PROTOCOL]"),
        "Narrator system prompt still contains [FOOTNOTE PROTOCOL] — \
         footnotes are now produced by tool calls, not JSON extraction"
    );
}

#[test]
fn narrator_prompt_has_no_item_protocol() {
    let narrator = NarratorAgent::new();
    let prompt = narrator.system_prompt();

    assert!(
        !prompt.contains("[ITEM PROTOCOL]"),
        "Narrator system prompt still contains [ITEM PROTOCOL] — \
         item acquisition is now handled by item_acquire tool calls"
    );
}

#[test]
fn narrator_prompt_has_no_npc_protocol() {
    let narrator = NarratorAgent::new();
    let prompt = narrator.system_prompt();

    assert!(
        !prompt.contains("[NPC PROTOCOL]"),
        "Narrator system prompt still contains [NPC PROTOCOL] — \
         NPC mentions are now handled by tool calls"
    );
}

#[test]
fn narrator_prompt_has_no_fenced_json_example() {
    let narrator = NarratorAgent::new();
    let prompt = narrator.system_prompt();

    // The example JSON block with footnotes/items_gained/npcs_present
    assert!(
        !prompt.contains("```json"),
        "Narrator system prompt still contains a ```json example block — \
         the narrator should produce pure prose, no JSON output expected"
    );
}

#[test]
fn narrator_prompt_retains_core_identity() {
    // Sanity check: deletion should NOT remove the narrator's core identity,
    // pacing rules, agency rules, or constraint handling.
    let narrator = NarratorAgent::new();
    let prompt = narrator.system_prompt();

    assert!(
        prompt.contains("NARRATOR"),
        "Narrator prompt must still identify the agent as NARRATOR"
    );
    assert!(
        prompt.contains("PACING"),
        "Narrator prompt must still contain pacing guidance"
    );
    assert!(
        prompt.contains("Agency"),
        "Narrator prompt must still contain agency rules"
    );
    assert!(
        prompt.contains("CONSTRAINT HANDLING"),
        "Narrator prompt must still contain constraint handling rules"
    );
}

// ============================================================================
// AC-2 & AC-3: extractor.rs deleted, JsonExtractor not in public API
// ============================================================================

#[test]
fn lib_rs_does_not_export_extractor_module() {
    // Read the crate's lib.rs at compile time to verify the extractor module
    // is no longer exported. This is a source-level assertion because once
    // the module is deleted, `use sidequest_agents::extractor` won't compile.
    let lib_source = include_str!("../src/lib.rs");

    assert!(
        !lib_source.contains("pub mod extractor"),
        "lib.rs still exports `pub mod extractor` — \
         the extractor module must be deleted (story 20-8, AC-2)"
    );
}

#[test]
fn extractor_source_file_does_not_exist() {
    // Verify at compile time that the extractor.rs file no longer exists.
    // include_str! would be a compile error if the file is gone, so we
    // check for the file's sentinel content instead.
    let extractor_exists = std::path::Path::new(
        concat!(env!("CARGO_MANIFEST_DIR"), "/src/extractor.rs")
    ).exists();

    assert!(
        !extractor_exists,
        "src/extractor.rs still exists — it must be deleted (story 20-8, AC-2)"
    );
}

// ============================================================================
// AC-4: orchestrator.rs does not call any JSON extraction function
// ============================================================================

#[test]
fn orchestrator_has_no_json_extractor_references() {
    let orchestrator_source = include_str!("../src/orchestrator.rs");

    assert!(
        !orchestrator_source.contains("JsonExtractor"),
        "orchestrator.rs still references JsonExtractor — \
         combat/chase patch extraction via JsonExtractor must be removed"
    );
}

#[test]
fn orchestrator_has_no_extractor_import() {
    let orchestrator_source = include_str!("../src/orchestrator.rs");

    assert!(
        !orchestrator_source.contains("crate::extractor"),
        "orchestrator.rs still imports from crate::extractor — \
         all extractor references must be removed"
    );
}

// ============================================================================
// AC-5: extraction_tier removed or permanently None
// ============================================================================

#[test]
fn action_result_has_no_extraction_tier_field_in_source() {
    // After 20-8, ActionResult should not have an extraction_tier field at all.
    let orchestrator_source = include_str!("../src/orchestrator.rs");
    let has_extraction_tier = orchestrator_source
        .lines()
        .any(|line| {
            let trimmed = line.trim();
            trimmed.starts_with("pub extraction_tier:") || trimmed.starts_with("extraction_tier:")
        });
    assert!(
        !has_extraction_tier,
        "ActionResult still has an `extraction_tier` field — \
         this field should be removed now that the 3-tier extraction pipeline is deleted"
    );
}

#[test]
fn narrator_extraction_has_no_tier_field_in_source() {
    // The `tier` field on NarratorExtraction only existed to report which
    // extraction tier succeeded. With extractor.rs gone, it should be removed.
    let orchestrator_source = include_str!("../src/orchestrator.rs");

    // Check that NarratorExtraction struct doesn't have a `tier: u8` field.
    // VisualScene has a `tier: String` which is unrelated — match the u8 type specifically.
    let has_tier_field = orchestrator_source
        .lines()
        .any(|line| {
            let trimmed = line.trim();
            trimmed.starts_with("pub tier: u8") || trimmed.starts_with("tier: u8")
        });

    assert!(
        !has_tier_field,
        "NarratorExtraction still has a `tier` field — \
         this field only existed for the 3-tier extraction pipeline \
         and should be removed"
    );
}

// ============================================================================
// AC-5 (extended): assemble_turn is the sole ActionResult producer
// ============================================================================

#[test]
fn assemble_turn_produces_complete_action_result() {
    // Verify assemble_turn still produces a complete ActionResult after
    // extraction_tier removal. This is a regression guard — the assembler
    // must continue to merge extraction + preprocessor + tools correctly.
    let extraction = NarratorExtraction {
        prose: "**The Market Square**\n\nVendors call out their wares.".to_string(),
        footnotes: vec![sidequest_protocol::Footnote {
            marker: Some(1),
            fact_id: None,
            summary: "The market is always busy at noon.".to_string(),
            category: FactCategory::Place,
            is_new: true,
        }],
        items_gained: vec![sidequest_protocol::ItemGained {
            name: "copper coin".to_string(),
            description: "A worn copper coin.".to_string(),
            category: "treasure".to_string(),
        }],
        npcs_present: vec![],
        quest_updates: HashMap::new(),
        visual_scene: None,
        scene_mood: Some("commerce".to_string()),
        personality_events: vec![],
        scene_intent: Some("Exploration".to_string()),
        resource_deltas: HashMap::new(),
        lore_established: Some(vec!["Market square is busy at noon".to_string()]),
        merchant_transactions: vec![],
        sfx_triggers: vec!["coins_clink".to_string()],
        action_rewrite: None,
        action_flags: None,
    };

    let rewrite = ActionRewrite {
        you: "You browse the market".to_string(),
        named: "Kael browses the market".to_string(),
        intent: "browse market".to_string(),
    };
    let flags = ActionFlags::default();
    let tool_mood = ToolCallResults {
        scene_mood: Some("bustling".to_string()),
        ..Default::default()
    };

    let result = assemble_turn(extraction, rewrite, flags, tool_mood);

    // Core fields from extraction
    assert_eq!(result.narration, "**The Market Square**\n\nVendors call out their wares.");
    assert_eq!(result.footnotes.len(), 1);
    assert_eq!(result.items_gained.len(), 1);
    assert_eq!(result.sfx_triggers, vec!["coins_clink"]);
    assert_eq!(result.lore_established, Some(vec!["Market square is busy at noon".to_string()]));

    // Tool call overrides narrator extraction
    assert_eq!(result.scene_mood, Some("bustling".to_string()),
        "Tool call scene_mood should override narrator extraction");

    // Preprocessor values present
    assert!(result.action_rewrite.is_some());
    assert_eq!(result.action_rewrite.unwrap().intent, "browse market");
}

// ============================================================================
// Structural: no JSON extraction pipeline remnants
// ============================================================================

#[test]
fn no_extraction_tier_in_assemble_turn_source() {
    // assemble_turn.rs should not reference extraction_tier after 20-8.
    let assemble_source = include_str!("../src/tools/assemble_turn.rs");

    assert!(
        !assemble_source.contains("extraction_tier"),
        "assemble_turn.rs still references extraction_tier — \
         this field should be removed from the assembler"
    );
}

#[test]
fn orchestrator_does_not_have_extract_structured_json_strategies() {
    // extract_structured_from_response currently has multiple "Strategy" comments
    // for parsing JSON from narrator output. After 20-8, the function should be
    // simplified — no more JSON parsing strategies needed since tools produce data.
    let orchestrator_source = include_str!("../src/orchestrator.rs");

    // Count JSON-parsing strategy comments
    let strategy_count = orchestrator_source
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            trimmed.starts_with("// Strategy") && trimmed.contains("JSON")
        })
        .count();

    assert_eq!(
        strategy_count, 0,
        "orchestrator.rs still has {strategy_count} JSON parsing strategy comments — \
         extract_structured_from_response should no longer parse JSON from narrator output"
    );
}

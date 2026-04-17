//! Story 37-11: action_rewrite and action_flags wiring
//!
//! RED phase — tests for re-adding narrator-emitted action_rewrite/action_flags
//! to the game_patch schema and wiring them through dispatch.
//!
//! Context: Story 20-1 moved action_rewrite/action_flags to mechanical preprocessors
//! and stripped them from the narrator prompt. But the preprocessors use keyword matching
//! (the Zork Problem — ADR-010/067), while the narrator has full semantic understanding.
//! This story re-adds them to the narrator prompt so the narrator emits LLM-quality
//! classifications alongside the mechanical fallbacks.
//!
//! ACs tested:
//!   1. Narrator prompt includes action_rewrite/action_flags in game_patch schema
//!   2. extract_structured_from_response correctly parses these fields from narrator JSON
//!   3. Narrator-extracted values flow into ActionResult (not discarded via _preprocessed)

use std::collections::HashMap;

use sidequest_agents::agent::Agent;
use sidequest_agents::agents::narrator::NarratorAgent;
use sidequest_agents::orchestrator::{
    ActionFlags, ActionRewrite, NarratorExtraction,
};
use sidequest_agents::tools::assemble_turn::{assemble_turn, ToolCallResults};

// ============================================================================
// Helper: build a minimal NarratorExtraction for testing
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
        beat_selections: vec![],
        confrontation: None,
        location: None,
        affinity_progress: vec![],
        gold_change: None,
    }
}

// ============================================================================
// AC-1: Narrator prompt MUST include action_rewrite/action_flags schema
// ============================================================================

/// The narrator's output format must document action_rewrite as a valid game_patch field.
/// This is the inverse of the 20-1 test — 37-11 re-adds what 20-1 removed.
#[test]
fn narrator_prompt_includes_action_rewrite_schema() {
    let narrator = NarratorAgent::new();
    let prompt = narrator.system_prompt();
    assert!(
        prompt.contains("action_rewrite"),
        "Narrator system prompt must contain 'action_rewrite' in the game_patch schema — \
         story 37-11 re-adds narrator-emitted action classification"
    );
}

/// The narrator's output format must document action_flags as a valid game_patch field.
#[test]
fn narrator_prompt_includes_action_flags_schema() {
    let narrator = NarratorAgent::new();
    let prompt = narrator.system_prompt();
    assert!(
        prompt.contains("action_flags"),
        "Narrator system prompt must contain 'action_flags' in the game_patch schema — \
         story 37-11 re-adds narrator-emitted action classification"
    );
}

/// The valid fields list in NARRATOR_OUTPUT_ONLY must include action_rewrite and action_flags.
#[test]
fn narrator_output_format_lists_action_rewrite_and_flags() {
    let format_text = sidequest_agents::agents::narrator::narrator_output_format_text();
    assert!(
        format_text.contains("action_rewrite"),
        "NARRATOR_OUTPUT_ONLY must list action_rewrite as a valid game_patch field"
    );
    assert!(
        format_text.contains("action_flags"),
        "NARRATOR_OUTPUT_ONLY must list action_flags as a valid game_patch field"
    );
}

/// The narrator prompt must describe the three action_rewrite sub-fields (you, named, intent)
/// so the LLM knows what to produce.
#[test]
fn narrator_prompt_describes_action_rewrite_subfields() {
    let format_text = sidequest_agents::agents::narrator::narrator_output_format_text();

    // Must describe the three forms
    assert!(
        format_text.contains("\"you\""),
        "action_rewrite schema must describe the 'you' field (second-person rewrite)"
    );
    assert!(
        format_text.contains("\"named\""),
        "action_rewrite schema must describe the 'named' field (third-person with name)"
    );
    assert!(
        format_text.contains("\"intent\""),
        "action_rewrite schema must describe the 'intent' field (neutral distilled intent)"
    );
}

/// The narrator prompt must describe the action_flags boolean sub-fields so the LLM
/// knows what to classify.
#[test]
fn narrator_prompt_describes_action_flags_subfields() {
    let format_text = sidequest_agents::agents::narrator::narrator_output_format_text();

    assert!(
        format_text.contains("is_power_grab"),
        "action_flags schema must describe is_power_grab"
    );
    assert!(
        format_text.contains("references_inventory"),
        "action_flags schema must describe references_inventory"
    );
    assert!(
        format_text.contains("references_npc"),
        "action_flags schema must describe references_npc"
    );
    assert!(
        format_text.contains("references_ability"),
        "action_flags schema must describe references_ability"
    );
    assert!(
        format_text.contains("references_location"),
        "action_flags schema must describe references_location"
    );
}

// ============================================================================
// AC-2: Extraction correctly parses action_rewrite/action_flags from game_patch
// ============================================================================

/// When the narrator emits action_rewrite in its game_patch JSON, the extraction
/// must parse it into the ActionRewrite struct. The extraction infrastructure
/// already handles this via serde — this test verifies the contract.
#[test]
fn extraction_parses_action_rewrite_from_game_patch() {
    // Use the public extraction function
    let raw = "\
**The Market Square**

You haggle with the fishmonger over the price of dried cod.

```game_patch
{
  \"location\": \"Market Square\",
  \"action_rewrite\": {
    \"you\": \"You haggle with the fishmonger\",
    \"named\": \"Kael haggles with the fishmonger\",
    \"intent\": \"haggle with fishmonger\"
  }
}
```";
    let extraction = sidequest_agents::orchestrator::extract_structured_from_response(raw);
    let rewrite = extraction
        .action_rewrite
        .expect("action_rewrite must be parsed from game_patch");
    assert_eq!(rewrite.you, "You haggle with the fishmonger");
    assert_eq!(rewrite.named, "Kael haggles with the fishmonger");
    assert_eq!(rewrite.intent, "haggle with fishmonger");
}

/// When the narrator emits action_flags in its game_patch JSON, the extraction
/// must parse it into the ActionFlags struct.
#[test]
fn extraction_parses_action_flags_from_game_patch() {
    let raw = "\
**The Armory**

You inspect the weapon rack, looking for something better than your current blade.

```game_patch
{
  \"action_flags\": {
    \"is_power_grab\": false,
    \"references_inventory\": true,
    \"references_npc\": false,
    \"references_ability\": false,
    \"references_location\": true
  }
}
```";
    let extraction = sidequest_agents::orchestrator::extract_structured_from_response(raw);
    let flags = extraction
        .action_flags
        .expect("action_flags must be parsed from game_patch");
    assert!(!flags.is_power_grab);
    assert!(flags.references_inventory);
    assert!(!flags.references_npc);
    assert!(!flags.references_ability);
    assert!(flags.references_location);
}

/// Both action_rewrite and action_flags should parse together with other fields.
#[test]
fn extraction_parses_both_fields_alongside_other_game_patch_fields() {
    let raw = "\
**The Tavern**

You ask the bartender about the strange noises from the cellar.

```game_patch
{
  \"location\": \"The Rusty Nail Tavern\",
  \"mood\": \"tense\",
  \"npcs_met\": [\"Bartender Grim\"],
  \"action_rewrite\": {
    \"you\": \"You ask the bartender about strange noises\",
    \"named\": \"Kael asks the bartender about strange noises\",
    \"intent\": \"ask bartender about cellar noises\"
  },
  \"action_flags\": {
    \"is_power_grab\": false,
    \"references_inventory\": false,
    \"references_npc\": true,
    \"references_ability\": false,
    \"references_location\": true
  },
  \"footnotes\": [
    {\"summary\": \"Strange noises have been coming from the tavern cellar\", \"category\": \"Lore\", \"is_new\": true}
  ]
}
```";
    let extraction = sidequest_agents::orchestrator::extract_structured_from_response(raw);

    // action_rewrite parsed
    let rewrite = extraction.action_rewrite.expect("action_rewrite must parse");
    assert_eq!(rewrite.intent, "ask bartender about cellar noises");

    // action_flags parsed
    let flags = extraction.action_flags.expect("action_flags must parse");
    assert!(flags.references_npc);
    assert!(flags.references_location);
    assert!(!flags.references_inventory);

    // Other fields still work
    assert_eq!(extraction.scene_mood.as_deref(), Some("tense"));
    assert_eq!(extraction.npcs_present.len(), 1);
    assert_eq!(extraction.footnotes.len(), 1);
}

/// When action_rewrite/action_flags are absent from game_patch, extraction must
/// return None (not panic or default to wrong values).
#[test]
fn extraction_returns_none_when_fields_absent() {
    let raw = "\
**The Road**

You walk down the dusty road.

```game_patch
{
  \"mood\": \"calm\"
}
```";
    let extraction = sidequest_agents::orchestrator::extract_structured_from_response(raw);
    assert!(
        extraction.action_rewrite.is_none(),
        "action_rewrite must be None when not in game_patch"
    );
    assert!(
        extraction.action_flags.is_none(),
        "action_flags must be None when not in game_patch"
    );
}

// ============================================================================
// AC-3: Narrator values flow into ActionResult via assemble_turn
// ============================================================================

/// When the narrator provides action_rewrite in NarratorExtraction, it must flow
/// through to the ActionResult. The mechanical preprocessor provides a fallback,
/// but the narrator's LLM-quality rewrite should be available downstream.
#[test]
fn narrator_action_rewrite_flows_into_action_result() {
    let mut extraction = minimal_extraction();
    extraction.action_rewrite = Some(ActionRewrite {
        you: "You carefully inspect the ancient runes".to_string(),
        named: "Kael carefully inspects the ancient runes".to_string(),
        intent: "inspect ancient runes".to_string(),
    });

    // Preprocessor provides mechanical fallback
    let preprocessor_rewrite = ActionRewrite {
        you: "You inspect the ancient runes".to_string(),
        named: "Kael inspect the ancient runes".to_string(),
        intent: "inspect the ancient runes".to_string(),
    };
    let flags = ActionFlags::default();

    let result = assemble_turn(extraction, preprocessor_rewrite, flags, ToolCallResults::default());

    let rewrite = result.action_rewrite.expect("action_rewrite must be present");
    // The narrator's richer rewrite should be used
    assert_eq!(
        rewrite.you,
        "You carefully inspect the ancient runes",
        "Narrator's action_rewrite.you should flow into ActionResult"
    );
    assert_eq!(
        rewrite.named,
        "Kael carefully inspects the ancient runes",
        "Narrator's action_rewrite.named should flow into ActionResult"
    );
}

/// When the narrator provides action_flags in NarratorExtraction, its semantic
/// classification must flow through to the ActionResult, overriding the
/// mechanical keyword-based flags.
#[test]
fn narrator_action_flags_flows_into_action_result() {
    let mut extraction = minimal_extraction();
    extraction.action_flags = Some(ActionFlags {
        is_power_grab: false,
        references_inventory: false,
        references_npc: true,  // narrator correctly detects NPC reference
        references_ability: false,
        references_location: false,
    });

    let preprocessor_rewrite = ActionRewrite {
        you: "You talk".to_string(),
        named: "Kael talks".to_string(),
        intent: "talk".to_string(),
    };
    // Mechanical preprocessor might get flags wrong (keyword matching)
    let preprocessor_flags = ActionFlags {
        is_power_grab: false,
        references_inventory: false,
        references_npc: false,  // keyword matcher missed it
        references_ability: false,
        references_location: false,
    };

    let result = assemble_turn(
        extraction,
        preprocessor_rewrite,
        preprocessor_flags,
        ToolCallResults::default(),
    );

    let flags = result.action_flags.expect("action_flags must be present");
    assert!(
        flags.references_npc,
        "Narrator's semantic classification (references_npc=true) should override \
         mechanical keyword matcher (references_npc=false)"
    );
}

/// When the narrator does NOT emit action_rewrite/action_flags, the preprocessor's
/// mechanical values must still be used as fallback.
#[test]
fn preprocessor_fallback_when_narrator_omits_fields() {
    let extraction = minimal_extraction(); // action_rewrite=None, action_flags=None

    let preprocessor_rewrite = ActionRewrite {
        you: "You look around".to_string(),
        named: "Kael looks around".to_string(),
        intent: "look around".to_string(),
    };
    let preprocessor_flags = ActionFlags {
        is_power_grab: false,
        references_inventory: false,
        references_npc: false,
        references_ability: false,
        references_location: false,
    };

    let result = assemble_turn(
        extraction,
        preprocessor_rewrite,
        preprocessor_flags,
        ToolCallResults::default(),
    );

    let rewrite = result.action_rewrite.expect("action_rewrite must be present");
    assert_eq!(
        rewrite.you, "You look around",
        "Preprocessor fallback must be used when narrator omits action_rewrite"
    );

    let flags = result.action_flags.expect("action_flags must be present");
    assert!(
        !flags.references_inventory,
        "Preprocessor flags must be used when narrator omits action_flags"
    );
}

// ============================================================================
// Wiring test: extract_structured_from_response is public
// ============================================================================

/// The extraction function must be accessible from integration tests.
/// If this compiles, the function is wired into the public API.
#[test]
fn extract_structured_from_response_is_public() {
    let _: fn(&str) -> NarratorExtraction =
        sidequest_agents::orchestrator::extract_structured_from_response;
}

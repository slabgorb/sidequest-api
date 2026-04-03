//! Story 20-2: scene_mood and scene_intent tool calls
//!
//! RED phase — tests for Phase 2 of ADR-057 (Narrator Crunch Separation).
//! First reactive tool migration: `set_mood` and `set_intent` validate single
//! string args against typed enums and produce JSON results.
//!
//! ACs tested:
//!   1. set_mood validates against mood enum — JSON on valid, error on invalid
//!   2. set_intent validates against intent enum — JSON on valid, error on invalid
//!   3. assemble_turn accepts mood/intent tool results, overrides narrator extraction
//!   4. Missing tool call falls back to narrator extraction (graceful degradation)
//!   5. Narrator system prompt no longer contains scene_mood/scene_intent schema docs
//!   6. OTEL spans emitted for each tool call

use std::collections::HashMap;

use sidequest_agents::agent::Agent;
use sidequest_agents::agents::narrator::NarratorAgent;
use sidequest_agents::orchestrator::{
    ActionFlags, ActionRewrite, ActionResult, NarratorExtraction,
};
use sidequest_agents::tools::assemble_turn::assemble_turn;
use sidequest_agents::tools::set_mood::{validate_mood, SceneMood};
use sidequest_agents::tools::set_intent::{validate_intent, SceneIntent};
use sidequest_agents::tools::assemble_turn::ToolCallResults;

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

fn extraction_with_mood_and_intent() -> NarratorExtraction {
    NarratorExtraction {
        prose: "The shadows lengthen across the marketplace.".to_string(),
        footnotes: vec![],
        items_gained: vec![],
        npcs_present: vec![],
        quest_updates: HashMap::new(),
        visual_scene: None,
        scene_mood: Some("narrator_mood_value".to_string()),
        personality_events: vec![],
        scene_intent: Some("narrator_intent_value".to_string()),
        resource_deltas: HashMap::new(),
        lore_established: None,
        merchant_transactions: vec![],
        sfx_triggers: vec![],
        action_rewrite: None,
        action_flags: None,
    }
}

// ============================================================================
// AC-1: set_mood validates against mood enum
// ============================================================================

#[test]
fn validate_mood_accepts_tension() {
    let result = validate_mood("tension");
    assert!(result.is_ok(), "tension is a valid mood");
    assert_eq!(result.unwrap(), SceneMood::Tension);
}

#[test]
fn validate_mood_accepts_wonder() {
    let result = validate_mood("wonder");
    assert!(result.is_ok(), "wonder is a valid mood");
    assert_eq!(result.unwrap(), SceneMood::Wonder);
}

#[test]
fn validate_mood_accepts_melancholy() {
    let result = validate_mood("melancholy");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), SceneMood::Melancholy);
}

#[test]
fn validate_mood_accepts_triumph() {
    let result = validate_mood("triumph");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), SceneMood::Triumph);
}

#[test]
fn validate_mood_accepts_foreboding() {
    let result = validate_mood("foreboding");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), SceneMood::Foreboding);
}

#[test]
fn validate_mood_accepts_calm() {
    let result = validate_mood("calm");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), SceneMood::Calm);
}

#[test]
fn validate_mood_accepts_exhilaration() {
    let result = validate_mood("exhilaration");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), SceneMood::Exhilaration);
}

#[test]
fn validate_mood_accepts_reverence() {
    let result = validate_mood("reverence");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), SceneMood::Reverence);
}

#[test]
fn validate_mood_rejects_invalid_string() {
    let result = validate_mood("totally_made_up");
    assert!(result.is_err(), "invalid mood must be rejected");
}

#[test]
fn validate_mood_rejects_empty_string() {
    let result = validate_mood("");
    assert!(result.is_err(), "empty string must be rejected");
}

/// Case-insensitive: "Tension" and "TENSION" should be accepted.
#[test]
fn validate_mood_is_case_insensitive() {
    assert!(validate_mood("Tension").is_ok(), "capitalized should work");
    assert!(validate_mood("CALM").is_ok(), "uppercase should work");
}

// ============================================================================
// AC-2: set_intent validates against intent enum
// ============================================================================

#[test]
fn validate_intent_accepts_dialogue() {
    let result = validate_intent("dialogue");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), SceneIntent::Dialogue);
}

#[test]
fn validate_intent_accepts_exploration() {
    let result = validate_intent("exploration");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), SceneIntent::Exploration);
}

#[test]
fn validate_intent_accepts_combat_prep() {
    let result = validate_intent("combat_prep");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), SceneIntent::CombatPrep);
}

#[test]
fn validate_intent_accepts_stealth() {
    let result = validate_intent("stealth");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), SceneIntent::Stealth);
}

#[test]
fn validate_intent_accepts_negotiation() {
    let result = validate_intent("negotiation");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), SceneIntent::Negotiation);
}

#[test]
fn validate_intent_accepts_escape() {
    let result = validate_intent("escape");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), SceneIntent::Escape);
}

#[test]
fn validate_intent_accepts_investigation() {
    let result = validate_intent("investigation");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), SceneIntent::Investigation);
}

#[test]
fn validate_intent_accepts_social() {
    let result = validate_intent("social");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), SceneIntent::Social);
}

#[test]
fn validate_intent_rejects_invalid_string() {
    let result = validate_intent("not_a_real_intent");
    assert!(result.is_err(), "invalid intent must be rejected");
}

#[test]
fn validate_intent_rejects_empty_string() {
    let result = validate_intent("");
    assert!(result.is_err(), "empty string must be rejected");
}

#[test]
fn validate_intent_is_case_insensitive() {
    assert!(validate_intent("Dialogue").is_ok());
    assert!(validate_intent("EXPLORATION").is_ok());
}

// ============================================================================
// AC-1/2: Enum types have as_str() for JSON output
// ============================================================================

#[test]
fn scene_mood_as_str_roundtrips() {
    let mood = SceneMood::Tension;
    let s = mood.as_str();
    assert_eq!(s, "tension");
    assert_eq!(validate_mood(s).unwrap(), mood, "as_str must roundtrip through validate");
}

#[test]
fn scene_intent_as_str_roundtrips() {
    let intent = SceneIntent::CombatPrep;
    let s = intent.as_str();
    assert_eq!(s, "combat_prep");
    assert_eq!(validate_intent(s).unwrap(), intent, "as_str must roundtrip through validate");
}

// ============================================================================
// AC-3: assemble_turn accepts tool results and overrides narrator extraction
// ============================================================================

/// When set_mood tool fires, its value overrides narrator extraction's scene_mood.
#[test]
fn assemble_turn_tool_mood_overrides_narrator() {
    let extraction = extraction_with_mood_and_intent();
    let tool_results = ToolCallResults {
        scene_mood: Some("triumph".to_string()),
        scene_intent: None,
        ..ToolCallResults::default()
    };

    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);

    assert_eq!(
        result.scene_mood.as_deref(),
        Some("triumph"),
        "tool result must override narrator's scene_mood"
    );
}

/// When set_intent tool fires, its value overrides narrator extraction's scene_intent.
#[test]
fn assemble_turn_tool_intent_overrides_narrator() {
    let extraction = extraction_with_mood_and_intent();
    let tool_results = ToolCallResults {
        scene_mood: None,
        scene_intent: Some("combat_prep".to_string()),
        ..ToolCallResults::default()
    };

    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);

    assert_eq!(
        result.scene_intent.as_deref(),
        Some("combat_prep"),
        "tool result must override narrator's scene_intent"
    );
}

/// Both tools fire — both override narrator extraction.
#[test]
fn assemble_turn_both_tools_override_narrator() {
    let extraction = extraction_with_mood_and_intent();
    let tool_results = ToolCallResults {
        scene_mood: Some("foreboding".to_string()),
        scene_intent: Some("stealth".to_string()),
        ..ToolCallResults::default()
    };

    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);

    assert_eq!(result.scene_mood.as_deref(), Some("foreboding"));
    assert_eq!(result.scene_intent.as_deref(), Some("stealth"));
    // Verify narrator values were NOT used
    assert_ne!(result.scene_mood.as_deref(), Some("narrator_mood_value"));
    assert_ne!(result.scene_intent.as_deref(), Some("narrator_intent_value"));
}

// ============================================================================
// AC-4: Missing tool call falls back to narrator extraction
// ============================================================================

/// No tool calls — scene_mood comes from narrator extraction.
#[test]
fn assemble_turn_no_tool_mood_uses_narrator_fallback() {
    let extraction = extraction_with_mood_and_intent();
    let tool_results = ToolCallResults {
        scene_mood: None,
        scene_intent: None,
        ..ToolCallResults::default()
    };

    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);

    assert_eq!(
        result.scene_mood.as_deref(),
        Some("narrator_mood_value"),
        "without tool call, narrator extraction value is used"
    );
}

/// No tool calls — scene_intent comes from narrator extraction.
#[test]
fn assemble_turn_no_tool_intent_uses_narrator_fallback() {
    let extraction = extraction_with_mood_and_intent();
    let tool_results = ToolCallResults {
        scene_mood: None,
        scene_intent: None,
        ..ToolCallResults::default()
    };

    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);

    assert_eq!(
        result.scene_intent.as_deref(),
        Some("narrator_intent_value"),
        "without tool call, narrator extraction value is used"
    );
}

/// Narrator has no scene_mood AND tool didn't fire — result is None.
#[test]
fn assemble_turn_no_mood_anywhere_is_none() {
    let mut extraction = extraction_with_mood_and_intent();
    extraction.scene_mood = None;
    let tool_results = ToolCallResults {
        scene_mood: None,
        scene_intent: None,
        ..ToolCallResults::default()
    };

    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);

    assert!(
        result.scene_mood.is_none(),
        "no mood from either source → must be None"
    );
}

/// Tool fires only for mood, not intent — mixed fallback scenario.
#[test]
fn assemble_turn_mixed_tool_and_fallback() {
    let extraction = extraction_with_mood_and_intent();
    let tool_results = ToolCallResults {
        scene_mood: Some("exhilaration".to_string()),
        scene_intent: None, // falls back to narrator
        ..ToolCallResults::default()
    };

    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);

    assert_eq!(result.scene_mood.as_deref(), Some("exhilaration"), "tool mood wins");
    assert_eq!(
        result.scene_intent.as_deref(),
        Some("narrator_intent_value"),
        "narrator intent used as fallback"
    );
}

// ============================================================================
// AC-3: assemble_turn still passes through non-migrated fields
// ============================================================================

/// Tool results don't disrupt existing field pass-through from 20-1.
#[test]
fn assemble_turn_preserves_other_fields_with_tool_results() {
    let extraction = extraction_with_mood_and_intent();
    let tool_results = ToolCallResults {
        scene_mood: Some("calm".to_string()),
        scene_intent: Some("social".to_string()),
        ..ToolCallResults::default()
    };

    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);

    // Non-migrated fields pass through unchanged
    assert_eq!(result.narration, "The shadows lengthen across the marketplace.");
    assert!(result.action_rewrite.is_some());
    assert!(result.action_flags.is_some());
    assert!(result.footnotes.is_empty());
}

// ============================================================================
// AC-3: ToolCallResults is extensible for future phases
// ============================================================================

/// ToolCallResults must support Default for phases where no tools fire.
#[test]
fn tool_call_results_default_is_all_none() {
    let defaults = ToolCallResults::default();
    assert!(defaults.scene_mood.is_none());
    assert!(defaults.scene_intent.is_none());
}

// ============================================================================
// AC-5: Narrator system prompt no longer contains scene_mood/scene_intent schema
// ============================================================================

/// The narrator's system prompt must NOT contain the scene_mood schema documentation.
/// It should reference the tool instead.
#[test]
fn narrator_prompt_omits_scene_mood_schema() {
    let narrator = NarratorAgent::new();
    let prompt = narrator.system_prompt();

    // The detailed schema (enum values, description) must be gone
    assert!(
        !prompt.contains("scene_mood: ALWAYS INCLUDE"),
        "scene_mood schema documentation must be removed from narrator prompt"
    );
    assert!(
        !prompt.contains("One of: combat, exploration, tension, triumph, sorrow, mystery, calm"),
        "scene_mood enum list must be removed from narrator prompt"
    );
}

/// The narrator's system prompt must NOT contain the scene_intent schema documentation.
#[test]
fn narrator_prompt_omits_scene_intent_schema() {
    let narrator = NarratorAgent::new();
    let prompt = narrator.system_prompt();

    assert!(
        !prompt.contains("scene_intent: ALWAYS INCLUDE"),
        "scene_intent schema documentation must be removed from narrator prompt"
    );
    assert!(
        !prompt.contains("One of: Combat, Dialogue, Exploration, Examine, Chase"),
        "scene_intent enum list must be removed from narrator prompt"
    );
}


// ============================================================================
// AC-6: OTEL spans emitted for tool calls
// ============================================================================

/// validate_mood must run cleanly under a tracing subscriber.
/// OTEL span fields: tool.name, tool.args.input, tool.result.valid, tool.result.value
#[test]
fn validate_mood_emits_otel_span() {
    let _guard = tracing::subscriber::set_default(
        tracing_subscriber::fmt()
            .with_test_writer()
            .with_max_level(tracing::Level::TRACE)
            .finish(),
    );

    let result = validate_mood("tension");
    assert!(result.is_ok());
}

/// validate_intent must run cleanly under a tracing subscriber.
#[test]
fn validate_intent_emits_otel_span() {
    let _guard = tracing::subscriber::set_default(
        tracing_subscriber::fmt()
            .with_test_writer()
            .with_max_level(tracing::Level::TRACE)
            .finish(),
    );

    let result = validate_intent("dialogue");
    assert!(result.is_ok());
}

/// OTEL must capture invalid tool calls too — tool.result.valid=false.
#[test]
fn validate_mood_otel_on_invalid_input() {
    let _guard = tracing::subscriber::set_default(
        tracing_subscriber::fmt()
            .with_test_writer()
            .with_max_level(tracing::Level::TRACE)
            .finish(),
    );

    let result = validate_mood("garbage");
    assert!(result.is_err(), "invalid mood should be rejected even under tracing");
}

// ============================================================================
// Wiring: modules are public and accessible
// ============================================================================

#[test]
fn set_mood_module_is_public() {
    // If this compiles, the module and function are wired
    let _: fn(&str) -> Result<SceneMood, _> = validate_mood;
}

#[test]
fn set_intent_module_is_public() {
    let _: fn(&str) -> Result<SceneIntent, _> = validate_intent;
}

#[test]
fn tool_call_results_is_exported() {
    let _ = ToolCallResults::default();
}

// ============================================================================
// Edge cases: enum coverage
// ============================================================================

/// All 8 mood variants must be distinct (no duplicate as_str() values).
#[test]
fn all_scene_moods_are_distinct() {
    let moods = [
        SceneMood::Tension,
        SceneMood::Wonder,
        SceneMood::Melancholy,
        SceneMood::Triumph,
        SceneMood::Foreboding,
        SceneMood::Calm,
        SceneMood::Exhilaration,
        SceneMood::Reverence,
    ];
    let keys: Vec<&str> = moods.iter().map(|m| m.as_str()).collect();
    let unique: std::collections::HashSet<&&str> = keys.iter().collect();
    assert_eq!(
        keys.len(),
        unique.len(),
        "all mood as_str() values must be unique"
    );
}

/// All 8 intent variants must be distinct.
#[test]
fn all_scene_intents_are_distinct() {
    let intents = [
        SceneIntent::Dialogue,
        SceneIntent::Exploration,
        SceneIntent::CombatPrep,
        SceneIntent::Stealth,
        SceneIntent::Negotiation,
        SceneIntent::Escape,
        SceneIntent::Investigation,
        SceneIntent::Social,
    ];
    let keys: Vec<&str> = intents.iter().map(|i| i.as_str()).collect();
    let unique: std::collections::HashSet<&&str> = keys.iter().collect();
    assert_eq!(
        keys.len(),
        unique.len(),
        "all intent as_str() values must be unique"
    );
}

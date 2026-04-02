//! Story 20-7: personality_event, resource_change, play_sfx tools
//!
//! RED phase — tests for Phase 7 of ADR-057 (Narrator Crunch Separation).
//! Migrates personality_events, resource_deltas, and sfx_triggers from the
//! narrator's monolithic JSON block to discrete tool calls. The LLM decides
//! THAT something happened; the tool validates and structures.
//!
//! ACs tested:
//!   1. personality_event validates event_type enum and returns PersonalityEvent JSON
//!   2. resource_change validates resource name against genre declarations and returns delta JSON
//!   3. play_sfx validates SFX ID against loaded library and returns trigger JSON
//!   4. All three removed from narrator JSON schema documentation
//!   5. assemble_turn collects all three into their respective ActionResult fields
//!   6. OTEL spans for each invocation

use std::collections::HashMap;
use std::io::Write;

use sidequest_agents::agent::Agent;
use sidequest_agents::agents::narrator::NarratorAgent;
use sidequest_agents::orchestrator::{
    ActionFlags, ActionRewrite, NarratorExtraction, PersonalityEvent,
};
use sidequest_agents::tools::assemble_turn::{assemble_turn, ToolCallResults};
use sidequest_agents::tools::personality_event::{validate_personality_event, PersonalityEventResult};
use sidequest_agents::tools::resource_change::{validate_resource_change, ResourceChangeResult};
use sidequest_agents::tools::play_sfx::{validate_play_sfx, PlaySfxResult};
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

fn extraction_with_personality_events() -> NarratorExtraction {
    NarratorExtraction {
        prose: "The betrayal cuts deep.".to_string(),
        footnotes: vec![],
        items_gained: vec![],
        npcs_present: vec![],
        quest_updates: HashMap::new(),
        visual_scene: None,
        scene_mood: None,
        personality_events: vec![PersonalityEvent {
            npc: "Toggler Copperjaw".to_string(),
            event_type: sidequest_agents::sidequest_game::PersonalityEvent::Betrayal,
            description: "Toggler betrayed the party".to_string(),
        }],
        scene_intent: None,
        resource_deltas: HashMap::new(),
        lore_established: None,
        merchant_transactions: vec![],
        sfx_triggers: vec![],
        action_rewrite: None,
        action_flags: None,
        tier: 1,
    }
}

fn extraction_with_resource_deltas() -> NarratorExtraction {
    let mut deltas = HashMap::new();
    deltas.insert("luck".to_string(), -1.0);
    NarratorExtraction {
        prose: "You spend a point of Luck.".to_string(),
        footnotes: vec![],
        items_gained: vec![],
        npcs_present: vec![],
        quest_updates: HashMap::new(),
        visual_scene: None,
        scene_mood: None,
        personality_events: vec![],
        scene_intent: None,
        resource_deltas: deltas,
        lore_established: None,
        merchant_transactions: vec![],
        sfx_triggers: vec![],
        action_rewrite: None,
        action_flags: None,
        tier: 1,
    }
}

fn extraction_with_sfx() -> NarratorExtraction {
    NarratorExtraction {
        prose: "The sword clangs against the shield.".to_string(),
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
        sfx_triggers: vec!["sword_clash".to_string()],
        action_rewrite: None,
        action_flags: None,
        tier: 1,
    }
}

fn extraction_empty() -> NarratorExtraction {
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
        tier: 1,
    }
}

/// Sample valid resource names for a genre pack.
fn sample_resource_names() -> Vec<String> {
    vec![
        "luck".to_string(),
        "humanity".to_string(),
        "heat".to_string(),
    ]
}

/// Sample valid SFX IDs for a genre pack.
fn sample_sfx_library() -> Vec<String> {
    vec![
        "sword_clash".to_string(),
        "door_creak".to_string(),
        "coin_drop".to_string(),
        "thunder_rumble".to_string(),
    ]
}

// ============================================================================
// AC-1: personality_event validates event_type enum and returns PersonalityEvent JSON
// ============================================================================

/// Valid betrayal event must be accepted.
#[test]
fn validate_personality_event_betrayal() {
    let result = validate_personality_event("Toggler Copperjaw", "betrayal", "Toggler betrayed the party");
    assert!(result.is_ok(), "valid betrayal event must succeed");
    let event = result.unwrap();
    assert_eq!(event.npc(), "Toggler Copperjaw");
    assert_eq!(event.event_type_str(), "betrayal");
    assert_eq!(event.description(), "Toggler betrayed the party");
}

/// Valid near_death event must be accepted.
#[test]
fn validate_personality_event_near_death() {
    let result = validate_personality_event("Reva", "near_death", "Reva nearly fell into the abyss");
    assert!(result.is_ok());
    let event = result.unwrap();
    assert_eq!(event.event_type_str(), "near_death");
}

/// Valid victory event must be accepted.
#[test]
fn validate_personality_event_victory() {
    let result = validate_personality_event("Kael", "victory", "Kael defeated the shadow knight");
    assert!(result.is_ok());
    assert_eq!(result.unwrap().event_type_str(), "victory");
}

/// Valid defeat event must be accepted.
#[test]
fn validate_personality_event_defeat() {
    let result = validate_personality_event("Mirova", "defeat", "Mirova lost the grove");
    assert!(result.is_ok());
    assert_eq!(result.unwrap().event_type_str(), "defeat");
}

/// Valid social_bonding event must be accepted.
#[test]
fn validate_personality_event_social_bonding() {
    let result = validate_personality_event("Patchwork", "social_bonding", "Patchwork and Kael shared a quiet moment");
    assert!(result.is_ok());
    assert_eq!(result.unwrap().event_type_str(), "social_bonding");
}

/// Case-insensitive event_type matching (LLM may capitalize).
#[test]
fn validate_personality_event_case_insensitive() {
    let result = validate_personality_event("Toggler", "BETRAYAL", "betrayal event");
    assert!(result.is_ok(), "event_type matching must be case-insensitive");
    assert_eq!(result.unwrap().event_type_str(), "betrayal");
}

/// Invalid event_type must be rejected.
#[test]
fn validate_personality_event_rejects_invalid_type() {
    let result = validate_personality_event("Toggler", "love", "not a valid event");
    assert!(result.is_err(), "invalid event_type must be rejected");
}

/// Empty event_type must be rejected.
#[test]
fn validate_personality_event_rejects_empty_type() {
    let result = validate_personality_event("Toggler", "", "some event");
    assert!(result.is_err(), "empty event_type must be rejected");
}

/// Empty NPC name must be rejected.
#[test]
fn validate_personality_event_rejects_empty_npc() {
    let result = validate_personality_event("", "betrayal", "some event");
    assert!(result.is_err(), "empty NPC name must be rejected");
}

/// Whitespace-only NPC name must be rejected.
#[test]
fn validate_personality_event_rejects_whitespace_npc() {
    let result = validate_personality_event("   ", "betrayal", "some event");
    assert!(result.is_err(), "whitespace-only NPC name must be rejected");
}

/// NPC name should be trimmed.
#[test]
fn validate_personality_event_trims_npc_name() {
    let result = validate_personality_event("  Toggler Copperjaw  ", "betrayal", "betrayal event");
    assert!(result.is_ok());
    assert_eq!(result.unwrap().npc(), "Toggler Copperjaw");
}

/// Description can be empty (it's optional context for OTEL).
#[test]
fn validate_personality_event_allows_empty_description() {
    let result = validate_personality_event("Toggler", "betrayal", "");
    assert!(result.is_ok(), "empty description should be allowed — it's optional OTEL context");
}

/// PersonalityEventResult must serialize to the expected JSON shape.
#[test]
fn personality_event_serializes_to_json() {
    let event = validate_personality_event("Toggler Copperjaw", "betrayal", "Toggler betrayed trust").unwrap();
    let json = serde_json::to_value(&event).expect("PersonalityEventResult must serialize");
    assert_eq!(json["npc"], "Toggler Copperjaw");
    assert_eq!(json["event_type"], "betrayal");
    assert_eq!(json["description"], "Toggler betrayed trust");
}

// ============================================================================
// AC-2: resource_change validates resource name against genre declarations
// ============================================================================

/// Valid resource change must be accepted.
#[test]
fn validate_resource_change_valid() {
    let valid_resources = sample_resource_names();
    let result = validate_resource_change("luck", -1.0, &valid_resources);
    assert!(result.is_ok(), "valid resource change must succeed");
    let change = result.unwrap();
    assert_eq!(change.resource(), "luck");
    assert!((change.delta() - (-1.0)).abs() < f64::EPSILON);
}

/// Positive delta must be accepted.
#[test]
fn validate_resource_change_positive_delta() {
    let valid_resources = sample_resource_names();
    let result = validate_resource_change("heat", 0.5, &valid_resources);
    assert!(result.is_ok());
    let change = result.unwrap();
    assert_eq!(change.resource(), "heat");
    assert!((change.delta() - 0.5).abs() < f64::EPSILON);
}

/// Resource name must match declared resources (case-insensitive).
#[test]
fn validate_resource_change_case_insensitive() {
    let valid_resources = sample_resource_names();
    let result = validate_resource_change("LUCK", -1.0, &valid_resources);
    assert!(result.is_ok(), "resource name matching must be case-insensitive");
    assert_eq!(result.unwrap().resource(), "luck");
}

/// Unknown resource name must be rejected.
#[test]
fn validate_resource_change_rejects_unknown_resource() {
    let valid_resources = sample_resource_names();
    let result = validate_resource_change("mana", -1.0, &valid_resources);
    assert!(result.is_err(), "unknown resource name must be rejected");
}

/// Empty resource name must be rejected.
#[test]
fn validate_resource_change_rejects_empty_name() {
    let valid_resources = sample_resource_names();
    let result = validate_resource_change("", -1.0, &valid_resources);
    assert!(result.is_err(), "empty resource name must be rejected");
}

/// Whitespace-only resource name must be rejected.
#[test]
fn validate_resource_change_rejects_whitespace_name() {
    let valid_resources = sample_resource_names();
    let result = validate_resource_change("   ", -1.0, &valid_resources);
    assert!(result.is_err(), "whitespace-only resource name must be rejected");
}

/// Zero delta must be accepted (LLM might emit zero — it's harmless).
#[test]
fn validate_resource_change_zero_delta() {
    let valid_resources = sample_resource_names();
    let result = validate_resource_change("luck", 0.0, &valid_resources);
    assert!(result.is_ok(), "zero delta should be accepted");
}

/// Resource name should be trimmed.
#[test]
fn validate_resource_change_trims_name() {
    let valid_resources = sample_resource_names();
    let result = validate_resource_change("  luck  ", -1.0, &valid_resources);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().resource(), "luck");
}

/// ResourceChangeResult must serialize to the expected JSON shape.
#[test]
fn resource_change_serializes_to_json() {
    let valid_resources = sample_resource_names();
    let change = validate_resource_change("luck", -1.0, &valid_resources).unwrap();
    let json = serde_json::to_value(&change).expect("ResourceChangeResult must serialize");
    assert_eq!(json["resource"], "luck");
    assert_eq!(json["delta"], -1.0);
}

/// NaN delta must be rejected.
#[test]
fn validate_resource_change_rejects_nan() {
    let valid_resources = sample_resource_names();
    let result = validate_resource_change("luck", f64::NAN, &valid_resources);
    assert!(result.is_err(), "NaN delta must be rejected");
}

/// Infinity delta must be rejected.
#[test]
fn validate_resource_change_rejects_infinity() {
    let valid_resources = sample_resource_names();
    let result = validate_resource_change("luck", f64::INFINITY, &valid_resources);
    assert!(result.is_err(), "infinite delta must be rejected");
}

// ============================================================================
// AC-3: play_sfx validates SFX ID against loaded library
// ============================================================================

/// Valid SFX ID must be accepted.
#[test]
fn validate_play_sfx_valid() {
    let library = sample_sfx_library();
    let result = validate_play_sfx("sword_clash", &library);
    assert!(result.is_ok(), "valid SFX ID must succeed");
    assert_eq!(result.unwrap().sfx_id(), "sword_clash");
}

/// Case-insensitive SFX ID matching.
#[test]
fn validate_play_sfx_case_insensitive() {
    let library = sample_sfx_library();
    let result = validate_play_sfx("SWORD_CLASH", &library);
    assert!(result.is_ok(), "SFX ID matching must be case-insensitive");
    assert_eq!(result.unwrap().sfx_id(), "sword_clash");
}

/// Unknown SFX ID must be rejected.
#[test]
fn validate_play_sfx_rejects_unknown_id() {
    let library = sample_sfx_library();
    let result = validate_play_sfx("explosion_mega", &library);
    assert!(result.is_err(), "unknown SFX ID must be rejected");
}

/// Empty SFX ID must be rejected.
#[test]
fn validate_play_sfx_rejects_empty() {
    let library = sample_sfx_library();
    let result = validate_play_sfx("", &library);
    assert!(result.is_err(), "empty SFX ID must be rejected");
}

/// Whitespace-only SFX ID must be rejected.
#[test]
fn validate_play_sfx_rejects_whitespace() {
    let library = sample_sfx_library();
    let result = validate_play_sfx("   ", &library);
    assert!(result.is_err(), "whitespace-only SFX ID must be rejected");
}

/// SFX ID should be trimmed.
#[test]
fn validate_play_sfx_trims_id() {
    let library = sample_sfx_library();
    let result = validate_play_sfx("  sword_clash  ", &library);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().sfx_id(), "sword_clash");
}

/// PlaySfxResult must serialize to expected JSON shape.
#[test]
fn play_sfx_serializes_to_json() {
    let library = sample_sfx_library();
    let sfx = validate_play_sfx("coin_drop", &library).unwrap();
    let json = serde_json::to_value(&sfx).expect("PlaySfxResult must serialize");
    assert_eq!(json["sfx_id"], "coin_drop");
}

// ============================================================================
// AC-4: All three removed from narrator JSON schema documentation
// ============================================================================

/// personality_events field must be removed from narrator prompt JSON block.
#[test]
fn narrator_prompt_omits_personality_events_schema() {
    let narrator = NarratorAgent::new();
    let prompt = narrator.system_prompt();

    assert!(
        !prompt.contains("personality_events: list of NPC personality-changing moments"),
        "personality_events schema documentation must be removed from narrator prompt"
    );
    assert!(
        !prompt.contains("event_type MUST be one of exactly these values"),
        "event_type enum documentation must be removed from narrator prompt"
    );
}

/// resource_deltas field must be removed from narrator prompt JSON block.
#[test]
fn narrator_prompt_omits_resource_deltas_schema() {
    let narrator = NarratorAgent::new();
    let prompt = narrator.system_prompt();

    assert!(
        !prompt.contains("resource_deltas: object mapping resource names"),
        "resource_deltas schema documentation must be removed from narrator prompt"
    );
    assert!(
        !prompt.contains("Resource names must match the genre's declared resource names exactly"),
        "resource validation documentation must be removed from narrator prompt"
    );
}

/// sfx_triggers field must be removed from narrator prompt JSON block.
#[test]
fn narrator_prompt_omits_sfx_triggers_schema() {
    let narrator = NarratorAgent::new();
    let prompt = narrator.system_prompt();

    assert!(
        !prompt.contains("sfx_triggers: list of SFX IDs to play this turn"),
        "sfx_triggers schema documentation must be removed from narrator prompt"
    );
    assert!(
        !prompt.contains("Match the action, not the noun"),
        "SFX matching guidance must be removed from narrator prompt"
    );
}

/// Non-migrated fields must still be present in the narrator prompt.
#[test]
fn narrator_prompt_retains_non_migrated_fields() {
    let narrator = NarratorAgent::new();
    let prompt = narrator.system_prompt();

    // These fields are NOT migrated in any prior phase — they must remain
    assert!(
        prompt.contains("merchant_transactions"),
        "merchant_transactions is NOT migrated yet — must remain"
    );
    assert!(
        prompt.contains("footnotes"),
        "footnotes must remain in narrator prompt"
    );
    assert!(
        prompt.contains("items_gained"),
        "items_gained must remain in narrator prompt"
    );
}

// ============================================================================
// AC-5: assemble_turn collects all three into respective ActionResult fields
// ============================================================================

// --- personality_events ---

/// Tool personality events override narrator extraction.
#[test]
fn assemble_turn_tool_personality_events_override_narrator() {
    let extraction = extraction_with_personality_events();
    let tool_events = vec![PersonalityEvent {
        npc: "Reva".to_string(),
        event_type: sidequest_agents::sidequest_game::PersonalityEvent::Victory,
        description: "Reva won the trial".to_string(),
    }];

    let tool_results = ToolCallResults {
        personality_events: Some(tool_events),
        ..ToolCallResults::default()
    };

    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);

    assert_eq!(result.personality_events.len(), 1, "tool events must replace narrator events");
    assert_eq!(result.personality_events[0].npc, "Reva");
}

/// No personality_event tools fired — narrator extraction falls through.
#[test]
fn assemble_turn_no_personality_tool_uses_narrator_fallback() {
    let extraction = extraction_with_personality_events();
    let tool_results = ToolCallResults::default();

    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);

    assert_eq!(result.personality_events.len(), 1);
    assert_eq!(result.personality_events[0].npc, "Toggler Copperjaw");
}

/// Multiple personality events from tools.
#[test]
fn assemble_turn_multiple_personality_events() {
    let extraction = extraction_empty();
    let tool_events = vec![
        PersonalityEvent {
            npc: "Toggler".to_string(),
            event_type: sidequest_agents::sidequest_game::PersonalityEvent::Betrayal,
            description: "betrayal".to_string(),
        },
        PersonalityEvent {
            npc: "Reva".to_string(),
            event_type: sidequest_agents::sidequest_game::PersonalityEvent::SocialBonding,
            description: "bonding".to_string(),
        },
    ];

    let tool_results = ToolCallResults {
        personality_events: Some(tool_events),
        ..ToolCallResults::default()
    };

    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);
    assert_eq!(result.personality_events.len(), 2);
}

/// Some(empty vec) from tools means "tools fired, no events" — overrides narrator.
#[test]
fn assemble_turn_empty_tool_personality_events_overrides_narrator() {
    let extraction = extraction_with_personality_events();
    let tool_results = ToolCallResults {
        personality_events: Some(vec![]),
        ..ToolCallResults::default()
    };

    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);
    assert!(
        result.personality_events.is_empty(),
        "Some(empty) tool personality_events must override narrator's events"
    );
}

// --- resource_deltas ---

/// Tool resource deltas override narrator extraction.
#[test]
fn assemble_turn_tool_resource_deltas_override_narrator() {
    let extraction = extraction_with_resource_deltas();
    let mut tool_deltas = HashMap::new();
    tool_deltas.insert("heat".to_string(), 0.5);

    let tool_results = ToolCallResults {
        resource_deltas: Some(tool_deltas),
        ..ToolCallResults::default()
    };

    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);

    assert!(result.resource_deltas.contains_key("heat"));
    assert!(!result.resource_deltas.contains_key("luck"), "narrator deltas must be overridden");
    assert_eq!(result.resource_deltas.len(), 1);
}

/// No resource_change tools fired — narrator extraction falls through.
#[test]
fn assemble_turn_no_resource_tool_uses_narrator_fallback() {
    let extraction = extraction_with_resource_deltas();
    let tool_results = ToolCallResults::default();

    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);
    assert!(result.resource_deltas.contains_key("luck"));
}

/// Some(empty) means tools fired but no resources changed — overrides narrator.
#[test]
fn assemble_turn_empty_tool_resource_deltas_overrides_narrator() {
    let extraction = extraction_with_resource_deltas();
    let tool_results = ToolCallResults {
        resource_deltas: Some(HashMap::new()),
        ..ToolCallResults::default()
    };

    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);
    assert!(
        result.resource_deltas.is_empty(),
        "Some(empty) tool resource_deltas must override narrator's deltas"
    );
}

/// Multiple resource changes accumulate in one HashMap.
#[test]
fn assemble_turn_multiple_resource_deltas() {
    let extraction = extraction_empty();
    let mut tool_deltas = HashMap::new();
    tool_deltas.insert("luck".to_string(), -1.0);
    tool_deltas.insert("heat".to_string(), 0.5);

    let tool_results = ToolCallResults {
        resource_deltas: Some(tool_deltas),
        ..ToolCallResults::default()
    };

    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);
    assert_eq!(result.resource_deltas.len(), 2);
}

// --- sfx_triggers ---

/// Tool SFX triggers override narrator extraction.
#[test]
fn assemble_turn_tool_sfx_triggers_override_narrator() {
    let extraction = extraction_with_sfx();
    let tool_sfx = vec!["door_creak".to_string()];

    let tool_results = ToolCallResults {
        sfx_triggers: Some(tool_sfx),
        ..ToolCallResults::default()
    };

    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);

    assert_eq!(result.sfx_triggers.len(), 1);
    assert_eq!(result.sfx_triggers[0], "door_creak");
}

/// No play_sfx tools fired — narrator extraction falls through.
#[test]
fn assemble_turn_no_sfx_tool_uses_narrator_fallback() {
    let extraction = extraction_with_sfx();
    let tool_results = ToolCallResults::default();

    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);
    assert_eq!(result.sfx_triggers.len(), 1);
    assert_eq!(result.sfx_triggers[0], "sword_clash");
}

/// Some(empty) means tools fired but no SFX — overrides narrator.
#[test]
fn assemble_turn_empty_tool_sfx_overrides_narrator() {
    let extraction = extraction_with_sfx();
    let tool_results = ToolCallResults {
        sfx_triggers: Some(vec![]),
        ..ToolCallResults::default()
    };

    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);
    assert!(
        result.sfx_triggers.is_empty(),
        "Some(empty) tool sfx_triggers must override narrator's SFX"
    );
}

/// Multiple SFX triggers from tools.
#[test]
fn assemble_turn_multiple_sfx_triggers() {
    let extraction = extraction_empty();
    let tool_sfx = vec!["sword_clash".to_string(), "thunder_rumble".to_string()];

    let tool_results = ToolCallResults {
        sfx_triggers: Some(tool_sfx),
        ..ToolCallResults::default()
    };

    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);
    assert_eq!(result.sfx_triggers.len(), 2);
}

// --- cross-field preservation ---

/// Tool results for all three fields don't disrupt existing fields.
#[test]
fn assemble_turn_all_three_tools_preserve_other_fields() {
    let extraction = extraction_with_personality_events();

    let tool_results = ToolCallResults {
        personality_events: Some(vec![]),
        resource_deltas: Some({
            let mut m = HashMap::new();
            m.insert("luck".to_string(), -1.0);
            m
        }),
        sfx_triggers: Some(vec!["coin_drop".to_string()]),
        ..ToolCallResults::default()
    };

    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);

    // Verify the three fields
    assert!(result.personality_events.is_empty());
    assert_eq!(result.resource_deltas.len(), 1);
    assert_eq!(result.sfx_triggers.len(), 1);

    // Verify other fields still pass through
    assert_eq!(result.narration, "The betrayal cuts deep.");
    assert!(result.action_rewrite.is_some());
}

// ============================================================================
// AC-5: ToolCallResults has new fields
// ============================================================================

/// ToolCallResults must have personality_events field.
#[test]
fn tool_call_results_has_personality_events_field() {
    let results = ToolCallResults {
        personality_events: Some(vec![]),
        ..ToolCallResults::default()
    };
    assert!(results.personality_events.is_some());
}

/// ToolCallResults must have resource_deltas field.
#[test]
fn tool_call_results_has_resource_deltas_field() {
    let results = ToolCallResults {
        resource_deltas: Some(HashMap::new()),
        ..ToolCallResults::default()
    };
    assert!(results.resource_deltas.is_some());
}

/// ToolCallResults must have sfx_triggers field.
#[test]
fn tool_call_results_has_sfx_triggers_field() {
    let results = ToolCallResults {
        sfx_triggers: Some(vec![]),
        ..ToolCallResults::default()
    };
    assert!(results.sfx_triggers.is_some());
}

/// Default ToolCallResults must have all three new fields as None.
#[test]
fn tool_call_results_default_new_fields_are_none() {
    let defaults = ToolCallResults::default();
    assert!(defaults.personality_events.is_none(), "default personality_events must be None");
    assert!(defaults.resource_deltas.is_none(), "default resource_deltas must be None");
    assert!(defaults.sfx_triggers.is_none(), "default sfx_triggers must be None");
}

// ============================================================================
// AC-6: OTEL spans for each invocation
// ============================================================================

/// validate_personality_event must run cleanly under a tracing subscriber.
#[test]
fn validate_personality_event_emits_otel_span() {
    let _guard = tracing::subscriber::set_default(
        tracing_subscriber::fmt()
            .with_test_writer()
            .with_max_level(tracing::Level::TRACE)
            .finish(),
    );

    let result = validate_personality_event("Toggler", "betrayal", "betrayed trust");
    assert!(result.is_ok());
}

/// OTEL must capture invalid personality_event calls too.
#[test]
fn validate_personality_event_otel_on_invalid_input() {
    let _guard = tracing::subscriber::set_default(
        tracing_subscriber::fmt()
            .with_test_writer()
            .with_max_level(tracing::Level::TRACE)
            .finish(),
    );

    let result = validate_personality_event("Toggler", "invalid_event", "something");
    assert!(result.is_err());
}

/// validate_resource_change must run cleanly under a tracing subscriber.
#[test]
fn validate_resource_change_emits_otel_span() {
    let _guard = tracing::subscriber::set_default(
        tracing_subscriber::fmt()
            .with_test_writer()
            .with_max_level(tracing::Level::TRACE)
            .finish(),
    );

    let resources = sample_resource_names();
    let result = validate_resource_change("luck", -1.0, &resources);
    assert!(result.is_ok());
}

/// validate_play_sfx must run cleanly under a tracing subscriber.
#[test]
fn validate_play_sfx_emits_otel_span() {
    let _guard = tracing::subscriber::set_default(
        tracing_subscriber::fmt()
            .with_test_writer()
            .with_max_level(tracing::Level::TRACE)
            .finish(),
    );

    let library = sample_sfx_library();
    let result = validate_play_sfx("sword_clash", &library);
    assert!(result.is_ok());
}

// ============================================================================
// Tool call parser: sidecar JSONL recognition
// ============================================================================

fn test_session_id(test_name: &str) -> String {
    format!("test-20-7-{}-{}", test_name, std::process::id())
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

/// Parser must recognize personality_event records from sidecar.
#[test]
fn parser_extracts_personality_event_from_sidecar() {
    let sid = test_session_id("parse-personality");
    write_sidecar(&sid, &[
        r#"{"tool":"personality_event","result":{"npc":"Toggler","event_type":"betrayal","description":"betrayed trust"}}"#,
    ]);

    let results = parse_tool_results(&sid);
    let events = results.personality_events.expect("personality_event tool result should populate");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].npc, "Toggler");

    cleanup_sidecar(&sid);
}

/// Parser must accumulate multiple personality_event records.
#[test]
fn parser_accumulates_multiple_personality_events() {
    let sid = test_session_id("parse-personality-multi");
    write_sidecar(&sid, &[
        r#"{"tool":"personality_event","result":{"npc":"Toggler","event_type":"betrayal","description":"betrayed"}}"#,
        r#"{"tool":"personality_event","result":{"npc":"Reva","event_type":"victory","description":"won"}}"#,
    ]);

    let results = parse_tool_results(&sid);
    let events = results.personality_events.expect("multiple personality events should accumulate");
    assert_eq!(events.len(), 2);

    cleanup_sidecar(&sid);
}

/// Parser must reject invalid event_type from sidecar.
#[test]
fn parser_rejects_invalid_personality_event_type() {
    let sid = test_session_id("parse-personality-invalid");
    write_sidecar(&sid, &[
        r#"{"tool":"personality_event","result":{"npc":"Toggler","event_type":"love","description":"not valid"}}"#,
    ]);

    let results = parse_tool_results(&sid);
    assert!(
        results.personality_events.is_none(),
        "invalid event_type must be rejected by parser"
    );

    cleanup_sidecar(&sid);
}

/// Parser must recognize resource_change records from sidecar.
#[test]
fn parser_extracts_resource_change_from_sidecar() {
    let sid = test_session_id("parse-resource");
    write_sidecar(&sid, &[
        r#"{"tool":"resource_change","result":{"resource":"luck","delta":-1.0}}"#,
    ]);

    let results = parse_tool_results(&sid);
    let deltas = results.resource_deltas.expect("resource_change tool result should populate");
    assert_eq!(deltas.len(), 1);
    assert!((deltas["luck"] - (-1.0)).abs() < f64::EPSILON);

    cleanup_sidecar(&sid);
}

/// Parser must accumulate multiple resource_change records.
#[test]
fn parser_accumulates_multiple_resource_changes() {
    let sid = test_session_id("parse-resource-multi");
    write_sidecar(&sid, &[
        r#"{"tool":"resource_change","result":{"resource":"luck","delta":-1.0}}"#,
        r#"{"tool":"resource_change","result":{"resource":"heat","delta":0.5}}"#,
    ]);

    let results = parse_tool_results(&sid);
    let deltas = results.resource_deltas.expect("multiple resource changes should accumulate");
    assert_eq!(deltas.len(), 2);

    cleanup_sidecar(&sid);
}

/// Parser must recognize play_sfx records from sidecar.
#[test]
fn parser_extracts_play_sfx_from_sidecar() {
    let sid = test_session_id("parse-sfx");
    write_sidecar(&sid, &[
        r#"{"tool":"play_sfx","result":{"sfx_id":"sword_clash"}}"#,
    ]);

    let results = parse_tool_results(&sid);
    let sfx = results.sfx_triggers.expect("play_sfx tool result should populate");
    assert_eq!(sfx.len(), 1);
    assert_eq!(sfx[0], "sword_clash");

    cleanup_sidecar(&sid);
}

/// Parser must accumulate multiple play_sfx records.
#[test]
fn parser_accumulates_multiple_sfx() {
    let sid = test_session_id("parse-sfx-multi");
    write_sidecar(&sid, &[
        r#"{"tool":"play_sfx","result":{"sfx_id":"sword_clash"}}"#,
        r#"{"tool":"play_sfx","result":{"sfx_id":"door_creak"}}"#,
    ]);

    let results = parse_tool_results(&sid);
    let sfx = results.sfx_triggers.expect("multiple SFX triggers should accumulate");
    assert_eq!(sfx.len(), 2);

    cleanup_sidecar(&sid);
}

/// Mixed tool records in one sidecar file — all three populate correctly.
#[test]
fn parser_handles_mixed_tool_records() {
    let sid = test_session_id("parse-mixed");
    write_sidecar(&sid, &[
        r#"{"tool":"personality_event","result":{"npc":"Toggler","event_type":"betrayal","description":"betrayed"}}"#,
        r#"{"tool":"resource_change","result":{"resource":"luck","delta":-1.0}}"#,
        r#"{"tool":"play_sfx","result":{"sfx_id":"sword_clash"}}"#,
        r#"{"tool":"set_mood","result":{"mood":"tension"}}"#,
    ]);

    let results = parse_tool_results(&sid);
    assert!(results.personality_events.is_some(), "personality_events should be populated");
    assert!(results.resource_deltas.is_some(), "resource_deltas should be populated");
    assert!(results.sfx_triggers.is_some(), "sfx_triggers should be populated");
    assert_eq!(results.scene_mood.as_deref(), Some("tension"), "existing tools must still work");

    cleanup_sidecar(&sid);
}

// ============================================================================
// End-to-end: sidecar → parse → assemble → ActionResult
// ============================================================================

/// E2E for personality_event: sidecar → parse → assemble → ActionResult.personality_events.
#[test]
fn personality_event_e2e_sidecar_to_action_result() {
    let sid = test_session_id("e2e-personality");
    write_sidecar(&sid, &[
        r#"{"tool":"personality_event","result":{"npc":"Toggler","event_type":"betrayal","description":"betrayed the party"}}"#,
    ]);

    let tool_results = parse_tool_results(&sid);
    let extraction = extraction_with_personality_events(); // has narrator fallback event
    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);

    assert_eq!(result.personality_events.len(), 1);
    assert_eq!(result.personality_events[0].npc, "Toggler");

    cleanup_sidecar(&sid);
}

/// E2E for resource_change: sidecar → parse → assemble → ActionResult.resource_deltas.
#[test]
fn resource_change_e2e_sidecar_to_action_result() {
    let sid = test_session_id("e2e-resource");
    write_sidecar(&sid, &[
        r#"{"tool":"resource_change","result":{"resource":"heat","delta":0.5}}"#,
    ]);

    let tool_results = parse_tool_results(&sid);
    let extraction = extraction_with_resource_deltas(); // has narrator fallback delta
    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);

    assert!(result.resource_deltas.contains_key("heat"));
    assert!(!result.resource_deltas.contains_key("luck"), "narrator deltas must be overridden");

    cleanup_sidecar(&sid);
}

/// E2E for play_sfx: sidecar → parse → assemble → ActionResult.sfx_triggers.
#[test]
fn play_sfx_e2e_sidecar_to_action_result() {
    let sid = test_session_id("e2e-sfx");
    write_sidecar(&sid, &[
        r#"{"tool":"play_sfx","result":{"sfx_id":"door_creak"}}"#,
    ]);

    let tool_results = parse_tool_results(&sid);
    let extraction = extraction_with_sfx(); // has narrator fallback SFX
    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);

    assert_eq!(result.sfx_triggers.len(), 1);
    assert_eq!(result.sfx_triggers[0], "door_creak");

    cleanup_sidecar(&sid);
}

// ============================================================================
// Wiring: modules are public and accessible
// ============================================================================

#[test]
fn personality_event_module_is_public() {
    let _: fn(&str, &str, &str) -> Result<PersonalityEventResult, _> = validate_personality_event;
}

#[test]
fn resource_change_module_is_public() {
    let _: fn(&str, f64, &[String]) -> Result<ResourceChangeResult, _> = validate_resource_change;
}

#[test]
fn play_sfx_module_is_public() {
    let _: fn(&str, &[String]) -> Result<PlaySfxResult, _> = validate_play_sfx;
}

// ============================================================================
// Rule coverage: Rust review checklist (lang-review/rust.md)
// ============================================================================

// Rule #2: non_exhaustive — PersonalityEvent enum in sidequest-game already
// has it via serde rename. The tool result types are not public enums — N/A.

// Rule #5: Validated constructors — all three validate_ functions return Result.
// (Already covered above — rejects_empty, rejects_invalid tests.)

// Rule #6: Test quality self-check — all tests have meaningful assert_eq!/assert!
// with specific values, not vacuous assertions.

// Rule #9: Public fields — tool result types should have private fields + getters.
// (Validated via getter method calls: .npc(), .event_type_str(), .resource(), etc.)

// Rule #13: Constructor/Deserialize consistency — these types don't derive Deserialize
// directly (they're produced by validate_* functions, not deserialized). N/A.

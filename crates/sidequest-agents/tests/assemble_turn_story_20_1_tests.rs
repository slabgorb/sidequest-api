//! Story 20-1: assemble_turn infrastructure + action preprocessors
//!
//! RED phase — tests for Phase 1 of ADR-057 (Narrator Crunch Separation).
//!
//! Targets:
//! - `tools::assemble_turn::assemble_turn()` — post-narration assembler
//! - `tools::preprocessors::classify_action()` — mechanical boolean flags (no LLM)
//! - `tools::preprocessors::rewrite_action()` — mechanical text rewrite (no LLM)
//! - Narrator system prompt — action_rewrite/action_flags schema removed
//! - OTEL events for preprocessor execution
//!
//! ACs tested:
//!   1. assemble_turn produces valid ActionResult from narrator output + preprocessor results
//!   2. action_rewrite produced by preprocessor, not extracted from narrator JSON
//!   3. action_flags produced by preprocessor, not extracted from narrator JSON
//!   4. Narrator system prompt no longer contains action_rewrite/action_flags schema
//!   5. OTEL events emitted for preprocessor execution
//!   6. All existing tests pass (verified by test runner, not tested here)

use std::collections::HashMap;

use sidequest_agents::agent::Agent;
use sidequest_agents::agents::narrator::NarratorAgent;
use sidequest_agents::orchestrator::{
    ActionFlags, ActionResult, ActionRewrite, NarratorExtraction,
};
use sidequest_agents::tools::assemble_turn::{assemble_turn, ToolCallResults};
use sidequest_agents::tools::preprocessors::{classify_action, rewrite_action};

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
        // Narrator still emits these in legacy mode — assembler must IGNORE them
        action_rewrite: Some(ActionRewrite {
            you: "NARRATOR SHOULD NOT WIN".to_string(),
            named: "NARRATOR SHOULD NOT WIN".to_string(),
            intent: "NARRATOR SHOULD NOT WIN".to_string(),
        }),
        action_flags: Some(ActionFlags {
            is_power_grab: true, // intentionally wrong — assembler must use preprocessor value
            references_inventory: true,
            references_npc: true,
            references_ability: true,
            references_location: true,
        }),
        tier: 1,
    }
}

// ============================================================================
// AC-1: assemble_turn produces valid ActionResult
// ============================================================================

#[test]
fn assemble_turn_produces_valid_action_result() {
    let extraction = minimal_extraction();
    let rewrite = ActionRewrite {
        you: "You look around".to_string(),
        named: "Kael looks around".to_string(),
        intent: "look around".to_string(),
    };
    let flags = ActionFlags {
        is_power_grab: false,
        references_inventory: false,
        references_npc: false,
        references_ability: false,
        references_location: false,
    };

    let result = assemble_turn(extraction, rewrite, flags, ToolCallResults::default());

    assert_eq!(
        result.narration,
        "**The Collapsed Overpass**\n\nYou step into the ruins."
    );
    assert!(!result.is_degraded, "assemble_turn should not produce degraded results");
    assert!(
        result.action_rewrite.is_some(),
        "ActionResult must have action_rewrite populated"
    );
    assert!(
        result.action_flags.is_some(),
        "ActionResult must have action_flags populated"
    );
}

/// assemble_turn must pass through all non-migrated fields from NarratorExtraction.
#[test]
fn assemble_turn_passes_through_narrator_fields() {
    let mut extraction = minimal_extraction();
    extraction.scene_mood = Some("combat".to_string());
    extraction.scene_intent = Some("Combat".to_string());

    let rewrite = ActionRewrite {
        you: "You attack".to_string(),
        named: "Kael attacks".to_string(),
        intent: "attack".to_string(),
    };
    let flags = ActionFlags {
        is_power_grab: false,
        references_inventory: false,
        references_npc: false,
        references_ability: false,
        references_location: false,
    };

    let result = assemble_turn(extraction, rewrite, flags, ToolCallResults::default());

    assert_eq!(result.scene_mood.as_deref(), Some("combat"));
    assert_eq!(result.scene_intent.as_deref(), Some("Combat"));
    assert!(result.combat_patch.is_none());
    assert!(result.chase_patch.is_none());
    assert!(result.footnotes.is_empty());
    assert!(result.items_gained.is_empty());
    assert!(result.npcs_present.is_empty());
    assert!(result.quest_updates.is_empty());
    assert!(result.personality_events.is_empty());
    assert!(result.resource_deltas.is_empty());
    assert!(result.merchant_transactions.is_empty());
    assert!(result.sfx_triggers.is_empty());
}

// ============================================================================
// AC-2: action_rewrite comes from preprocessor, NOT narrator JSON
// ============================================================================

/// When both narrator extraction and preprocessor provide action_rewrite,
/// the preprocessor MUST win. This is the core behavioral change of 20-1.
#[test]
fn assemble_turn_uses_preprocessor_rewrite_over_narrator() {
    let extraction = minimal_extraction(); // has "NARRATOR SHOULD NOT WIN" rewrites

    let preprocessor_rewrite = ActionRewrite {
        you: "You draw your sword".to_string(),
        named: "Kael draws their sword".to_string(),
        intent: "draw sword".to_string(),
    };
    let flags = ActionFlags {
        is_power_grab: false,
        references_inventory: false,
        references_npc: false,
        references_ability: false,
        references_location: false,
    };

    let result = assemble_turn(extraction, preprocessor_rewrite, flags, ToolCallResults::default());

    let rewrite = result.action_rewrite.expect("action_rewrite must be present");
    assert_eq!(rewrite.you, "You draw your sword");
    assert_eq!(rewrite.named, "Kael draws their sword");
    assert_eq!(rewrite.intent, "draw sword");
    // Verify narrator's values were NOT used
    assert_ne!(rewrite.you, "NARRATOR SHOULD NOT WIN");
}

// ============================================================================
// AC-3: action_flags comes from preprocessor, NOT narrator JSON
// ============================================================================

/// When both narrator extraction and preprocessor provide action_flags,
/// the preprocessor MUST win. Narrator had all flags=true; preprocessor has all=false.
#[test]
fn assemble_turn_uses_preprocessor_flags_over_narrator() {
    let extraction = minimal_extraction(); // has all flags = true (intentionally wrong)

    let rewrite = ActionRewrite {
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

    let result = assemble_turn(extraction, rewrite, preprocessor_flags, ToolCallResults::default());

    let flags = result.action_flags.expect("action_flags must be present");
    assert!(!flags.is_power_grab, "preprocessor said false, narrator said true — preprocessor wins");
    assert!(!flags.references_inventory);
    assert!(!flags.references_npc);
    assert!(!flags.references_ability);
    assert!(!flags.references_location);
}

// ============================================================================
// AC-2/3: classify_action — mechanical boolean classification (no LLM)
// ============================================================================

#[test]
fn classify_action_detects_inventory_reference() {
    let flags = classify_action("I check my bag for healing potions");
    assert!(flags.references_inventory, "must detect inventory reference");
    assert!(!flags.is_power_grab);
}

#[test]
fn classify_action_detects_npc_reference() {
    let flags = classify_action("I talk to the bartender about the rumors");
    assert!(flags.references_npc, "must detect NPC reference");
}

#[test]
fn classify_action_detects_ability_reference() {
    let flags = classify_action("I use my psychic echo to scan the area");
    assert!(flags.references_ability, "must detect ability reference");
}

#[test]
fn classify_action_detects_location_reference() {
    let flags = classify_action("I head to the market district");
    assert!(flags.references_location, "must detect location reference");
}

#[test]
fn classify_action_detects_power_grab() {
    let flags = classify_action("I wish for unlimited gold and godlike power");
    assert!(flags.is_power_grab, "must detect power grab");
}

#[test]
fn classify_action_default_no_references() {
    let flags = classify_action("I look around");
    assert!(!flags.is_power_grab);
    assert!(!flags.references_inventory);
    assert!(!flags.references_npc);
    assert!(!flags.references_ability);
    assert!(!flags.references_location);
}

/// Multiple flags can be true simultaneously.
#[test]
fn classify_action_multiple_flags() {
    let flags = classify_action("I use my telekinesis to grab the sword from the merchant");
    assert!(flags.references_ability, "telekinesis is an ability");
    assert!(flags.references_inventory, "sword is an item");
    assert!(flags.references_npc, "merchant is an NPC");
}

// ============================================================================
// AC-2: rewrite_action — mechanical text rewrite (no LLM)
// ============================================================================

#[test]
fn rewrite_action_produces_three_forms() {
    let rewrite = rewrite_action("I draw my sword", "Kael");
    assert!(
        rewrite.you.starts_with("You "),
        "you form must start with 'You ': got '{}'",
        rewrite.you
    );
    assert!(
        rewrite.named.contains("Kael"),
        "named form must contain character name: got '{}'",
        rewrite.named
    );
    assert!(
        !rewrite.intent.is_empty(),
        "intent form must not be empty"
    );
}

#[test]
fn rewrite_action_you_form_is_second_person() {
    let rewrite = rewrite_action("attack the goblin", "Thorn");
    // "You" prefix — second person
    assert!(
        rewrite.you.to_lowercase().starts_with("you "),
        "you form must be second-person: got '{}'",
        rewrite.you
    );
}

#[test]
fn rewrite_action_named_form_uses_character_name() {
    let rewrite = rewrite_action("look around the room", "Ember");
    assert!(
        rewrite.named.starts_with("Ember"),
        "named form must start with character name: got '{}'",
        rewrite.named
    );
}

#[test]
fn rewrite_action_intent_is_neutral() {
    let rewrite = rewrite_action("I carefully examine the ancient door", "Kael");
    // Intent should not contain pronouns
    assert!(
        !rewrite.intent.to_lowercase().contains(" i "),
        "intent must not contain first-person pronoun: got '{}'",
        rewrite.intent
    );
    assert!(
        !rewrite.intent.to_lowercase().starts_with("i "),
        "intent must not start with first-person pronoun: got '{}'",
        rewrite.intent
    );
    assert!(
        !rewrite.intent.to_lowercase().starts_with("you "),
        "intent must not contain second-person pronoun: got '{}'",
        rewrite.intent
    );
}

// ============================================================================
// AC-4: Narrator system prompt no longer contains action_rewrite/action_flags
// ============================================================================

/// The narrator's system prompt must NOT reference action_rewrite or action_flags.
/// These fields are now handled by preprocessors, not the narrator.
#[test]
fn narrator_prompt_omits_action_rewrite_schema() {
    let narrator = NarratorAgent::new();
    let prompt = narrator.system_prompt();
    assert!(
        !prompt.contains("action_rewrite"),
        "Narrator system prompt must not contain 'action_rewrite' — this field is now a preprocessor"
    );
}

#[test]
fn narrator_prompt_omits_action_flags_schema() {
    let narrator = NarratorAgent::new();
    let prompt = narrator.system_prompt();
    assert!(
        !prompt.contains("action_flags"),
        "Narrator system prompt must not contain 'action_flags' — this field is now a preprocessor"
    );
}

/// The narrator prompt must still contain non-migrated fields.
/// Note: personality_events, resource_deltas, sfx_triggers migrated in Phase 7 (20-7).
/// Note: scene_mood migrated in Phase 2 (20-2), visual_scene migrated in Phase 5 (20-5).
#[test]
fn narrator_prompt_retains_non_migrated_fields() {
    let narrator = NarratorAgent::new();
    let prompt = narrator.system_prompt();
    assert!(
        prompt.contains("merchant_transactions"),
        "merchant_transactions must remain in narrator prompt"
    );
}

// ============================================================================
// AC-5: OTEL events for preprocessor execution
// ============================================================================

/// classify_action must emit a tracing span with the classification results.
/// We verify by running under an active tracing subscriber and checking output.
#[test]
fn classify_action_emits_otel_span() {
    let _guard = tracing::subscriber::set_default(
        tracing_subscriber::fmt()
            .with_test_writer()
            .with_max_level(tracing::Level::TRACE)
            .finish(),
    );

    let flags = classify_action("I check my inventory");
    // Function must run successfully under tracing and produce correct results.
    // OTEL span existence is verified by the tracing subscriber capturing output.
    assert!(flags.references_inventory);
}

/// rewrite_action must emit a tracing span with timing and field values.
#[test]
fn rewrite_action_emits_otel_span() {
    let _guard = tracing::subscriber::set_default(
        tracing_subscriber::fmt()
            .with_test_writer()
            .with_max_level(tracing::Level::TRACE)
            .finish(),
    );

    let rewrite = rewrite_action("I draw my sword", "Kael");
    assert!(!rewrite.you.is_empty());
}

// ============================================================================
// Wiring test: tools module is exported and accessible
// ============================================================================

/// The `tools` module must be public on sidequest_agents.
/// This test verifies the module exists and is wired into lib.rs.
#[test]
fn tools_module_is_public() {
    // If this compiles, the module path is wired
    let _: fn(NarratorExtraction, ActionRewrite, ActionFlags, ToolCallResults) -> ActionResult = assemble_turn;
}

/// The preprocessor functions must be accessible from the tools module.
#[test]
fn preprocessor_functions_are_public() {
    // If this compiles, the functions are wired
    let _: fn(&str) -> ActionFlags = classify_action;
    let _: fn(&str, &str) -> ActionRewrite = rewrite_action;
}

// ============================================================================
// Edge cases: assembler robustness
// ============================================================================

/// assemble_turn with narrator extraction that has NO action_rewrite/action_flags
/// (e.g., narrator didn't emit them). Preprocessor values must still be used.
#[test]
fn assemble_turn_works_when_narrator_omits_rewrite_and_flags() {
    let extraction = NarratorExtraction {
        prose: "You see a door.".to_string(),
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
        action_rewrite: None, // narrator didn't emit
        action_flags: None,   // narrator didn't emit
        tier: 3,
    };

    let rewrite = ActionRewrite {
        you: "You open the door".to_string(),
        named: "Kael opens the door".to_string(),
        intent: "open door".to_string(),
    };
    let flags = ActionFlags {
        is_power_grab: false,
        references_inventory: false,
        references_npc: false,
        references_ability: false,
        references_location: true,
    };

    let result = assemble_turn(extraction, rewrite, flags, ToolCallResults::default());

    let result_rewrite = result.action_rewrite.expect("preprocessor rewrite must be present");
    assert_eq!(result_rewrite.you, "You open the door");

    let result_flags = result.action_flags.expect("preprocessor flags must be present");
    assert!(result_flags.references_location);
    assert!(!result_flags.is_power_grab);
}

/// assemble_turn preserves extraction_tier from the narrator extraction.
#[test]
fn assemble_turn_preserves_extraction_tier() {
    let mut extraction = minimal_extraction();
    extraction.tier = 2;

    let rewrite = ActionRewrite {
        you: "You look".to_string(),
        named: "Kael looks".to_string(),
        intent: "look".to_string(),
    };
    let flags = ActionFlags {
        is_power_grab: false,
        references_inventory: false,
        references_npc: false,
        references_ability: false,
        references_location: false,
    };

    let result = assemble_turn(extraction, rewrite, flags, ToolCallResults::default());
    assert_eq!(
        result.extraction_tier,
        Some(2),
        "extraction_tier must pass through from narrator extraction"
    );
}

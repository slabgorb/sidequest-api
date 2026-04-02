//! Story 20-9: Wire assemble_turn into dispatch pipeline
//!
//! RED phase — tests that verify assemble_turn is actually CALLED by the
//! orchestrator's process_action() method, not just existing as dead code.
//!
//! ACs tested:
//!   1. assemble_turn is a public API exported from sidequest-agents crate
//!   2. orchestrator.rs imports and calls assemble_turn()
//!   3. ActionResult fields are identical pre- vs. post-refactor (no-op with default ToolCallResults)
//!   4. All existing tests pass without modification (verified by test runner)
//!   5. Wiring verified — non-test consumers (orchestrator) call assemble_turn()

use std::collections::HashMap;

use sidequest_agents::orchestrator::{
    ActionFlags, ActionResult, ActionRewrite, NarratorExtraction,
};
use sidequest_agents::tools::assemble_turn::{assemble_turn, ToolCallResults};

// ============================================================================
// AC-2 + AC-5: orchestrator.rs calls assemble_turn (source-level wiring test)
// ============================================================================

/// The orchestrator's process_action() method MUST call assemble_turn().
/// This is the core wiring assertion of story 20-9 — without this, assemble_turn
/// remains dead code and story 20-10 (tool call parsing) can't reach the game.
///
/// We verify by scanning the orchestrator source for the function call.
/// This is a wiring test per CLAUDE.md: "Every test suite needs a wiring test."
#[test]
fn orchestrator_calls_assemble_turn() {
    let orchestrator_source =
        std::fs::read_to_string("crates/sidequest-agents/src/orchestrator.rs")
            .expect("orchestrator.rs must exist");

    assert!(
        orchestrator_source.contains("assemble_turn("),
        "orchestrator.rs must call assemble_turn() — currently builds ActionResult directly \
         at lines 704-730. Story 20-9 wires assemble_turn into the dispatch pipeline."
    );
}

/// The orchestrator must import ToolCallResults to construct the default.
#[test]
fn orchestrator_imports_tool_call_results() {
    let orchestrator_source =
        std::fs::read_to_string("crates/sidequest-agents/src/orchestrator.rs")
            .expect("orchestrator.rs must exist");

    assert!(
        orchestrator_source.contains("ToolCallResults"),
        "orchestrator.rs must reference ToolCallResults — needed to pass default tool results \
         to assemble_turn(). Without this import, the wiring is incomplete."
    );
}

/// The orchestrator must import from the tools module, not re-implement assembly.
#[test]
fn orchestrator_imports_from_tools_module() {
    let orchestrator_source =
        std::fs::read_to_string("crates/sidequest-agents/src/orchestrator.rs")
            .expect("orchestrator.rs must exist");

    assert!(
        orchestrator_source.contains("tools::assemble_turn")
            || orchestrator_source.contains("use crate::tools"),
        "orchestrator.rs must import from the tools module — assemble_turn lives in \
         crate::tools::assemble_turn, not in orchestrator.rs itself."
    );
}

// ============================================================================
// AC-3: No-op behavior — default ToolCallResults produces identical output
// ============================================================================

/// With ToolCallResults::default() (all None), assemble_turn must produce the same
/// scene_mood and scene_intent as the narrator extraction. This proves the wiring
/// is a no-op refactor — existing game behavior is unchanged.
#[test]
fn default_tool_results_preserves_scene_mood_from_extraction() {
    let extraction = NarratorExtraction {
        prose: "The neon signs flicker.".to_string(),
        footnotes: vec![],
        items_gained: vec![],
        npcs_present: vec![],
        quest_updates: HashMap::new(),
        visual_scene: None,
        scene_mood: Some("tension".to_string()),
        personality_events: vec![],
        scene_intent: Some("Exploration".to_string()),
        resource_deltas: HashMap::new(),
        lore_established: None,
        merchant_transactions: vec![],
        sfx_triggers: vec![],
        action_rewrite: None,
        action_flags: None,
        tier: 1,
    };

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

    // These must match what orchestrator previously built directly from extraction
    assert_eq!(
        result.scene_mood.as_deref(),
        Some("tension"),
        "scene_mood must pass through from extraction when no tool override"
    );
    assert_eq!(
        result.scene_intent.as_deref(),
        Some("Exploration"),
        "scene_intent must pass through from extraction when no tool override"
    );
}

/// With default ToolCallResults, footnotes must be empty (no-fallback rule).
/// This is the known behavioral change — narrator footnotes are discarded when
/// lore_mark tool hasn't fired. Correct per AC-6 of story 20-1.
#[test]
fn default_tool_results_discards_narrator_footnotes() {
    let extraction = NarratorExtraction {
        prose: "You discover ancient lore.".to_string(),
        footnotes: vec![sidequest_protocol::Footnote {
            category: "lore".to_string(),
            text: "The old gods slumber.".to_string(),
        }],
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
    };

    let rewrite = ActionRewrite {
        you: "You examine".to_string(),
        named: "Kael examines".to_string(),
        intent: "examine".to_string(),
    };
    let flags = ActionFlags {
        is_power_grab: false,
        references_inventory: false,
        references_npc: false,
        references_ability: false,
        references_location: false,
    };

    let result = assemble_turn(extraction, rewrite, flags, ToolCallResults::default());

    assert!(
        result.footnotes.is_empty(),
        "footnotes must be empty when lore_mark tool didn't fire — no-fallback rule (AC-6 of 20-1). \
         Narrator footnotes are discarded. Got {} footnotes.",
        result.footnotes.len()
    );
}

/// With default ToolCallResults, all pass-through fields from extraction must survive.
/// This verifies the full field set, not just mood/intent.
#[test]
fn default_tool_results_passes_through_all_extraction_fields() {
    let extraction = NarratorExtraction {
        prose: "**The Market** You haggle with a vendor.".to_string(),
        footnotes: vec![],
        items_gained: vec![sidequest_protocol::ItemGained {
            name: "Rusty Blade".to_string(),
            category: "weapon".to_string(),
        }],
        npcs_present: vec![],
        quest_updates: {
            let mut m = HashMap::new();
            m.insert("find_blade".to_string(), "Acquired the Rusty Blade".to_string());
            m
        },
        visual_scene: Some("A dusty market square with ramshackle stalls.".to_string()),
        scene_mood: Some("commerce".to_string()),
        personality_events: vec![],
        scene_intent: Some("Trade".to_string()),
        resource_deltas: {
            let mut m = HashMap::new();
            m.insert("gold".to_string(), -15);
            m
        },
        lore_established: Some("The vendor speaks of the old keep.".to_string()),
        merchant_transactions: vec![],
        sfx_triggers: vec![],
        action_rewrite: None,
        action_flags: None,
        tier: 1,
    };

    let rewrite = ActionRewrite {
        you: "You haggle".to_string(),
        named: "Kael haggles".to_string(),
        intent: "haggle with vendor".to_string(),
    };
    let flags = ActionFlags {
        is_power_grab: false,
        references_inventory: true,
        references_npc: true,
        references_ability: false,
        references_location: false,
    };

    let result = assemble_turn(extraction, rewrite, flags, ToolCallResults::default());

    // Verify all pass-through fields match what orchestrator used to set directly
    assert_eq!(result.narration, "**The Market** You haggle with a vendor.");
    assert_eq!(result.items_gained.len(), 1);
    assert_eq!(result.items_gained[0].name, "Rusty Blade");
    assert_eq!(result.quest_updates.get("find_blade").map(|s| s.as_str()), Some("Acquired the Rusty Blade"));
    assert_eq!(result.visual_scene.as_deref(), Some("A dusty market square with ramshackle stalls."));
    assert_eq!(result.resource_deltas.get("gold"), Some(&-15));
    assert_eq!(result.lore_established.as_deref(), Some("The vendor speaks of the old keep."));
    assert_eq!(result.extraction_tier, Some(1));

    // Verify fields that assemble_turn sets to None (orchestrator fills these separately)
    assert!(result.combat_patch.is_none(), "assemble_turn must not set combat_patch");
    assert!(result.chase_patch.is_none(), "assemble_turn must not set chase_patch");
    assert!(!result.is_degraded, "assemble_turn must not produce degraded results");

    // classified_intent and agent_name are set by orchestrator, not assemble_turn
    assert!(result.classified_intent.is_none(), "assemble_turn must not set classified_intent");
    assert!(result.agent_name.is_none(), "assemble_turn must not set agent_name");
}

// ============================================================================
// AC-1: assemble_turn is publicly accessible (compile-time wiring)
// ============================================================================

/// Verify the public API surface exists and is callable from external crates.
/// If this compiles, assemble_turn + ToolCallResults are properly exported.
#[test]
fn assemble_turn_public_api_is_accessible() {
    // Type-check: assemble_turn has the expected signature
    let _: fn(NarratorExtraction, ActionRewrite, ActionFlags, ToolCallResults) -> ActionResult =
        assemble_turn;

    // Type-check: ToolCallResults::default() works
    let defaults = ToolCallResults::default();
    assert!(defaults.scene_mood.is_none(), "default scene_mood must be None");
    assert!(defaults.scene_intent.is_none(), "default scene_intent must be None");
    assert!(defaults.footnotes.is_none(), "default footnotes must be None");
}

// ============================================================================
// Behavioral equivalence: orchestrator's old fields vs assemble_turn output
// ============================================================================

/// The orchestrator previously set action_rewrite and action_flags directly from
/// extraction. After 20-9, they come from preprocessor args to assemble_turn.
/// With default preprocessor values, the result must still have Some() for both.
#[test]
fn assemble_turn_always_wraps_preprocessor_values_in_some() {
    let extraction = NarratorExtraction {
        prose: "You wait.".to_string(),
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
    };

    let rewrite = ActionRewrite {
        you: String::new(),
        named: String::new(),
        intent: String::new(),
    };
    let flags = ActionFlags {
        is_power_grab: false,
        references_inventory: false,
        references_npc: false,
        references_ability: false,
        references_location: false,
    };

    let result = assemble_turn(extraction, rewrite, flags, ToolCallResults::default());

    // Preprocessor values are ALWAYS wrapped in Some — even if default
    assert!(
        result.action_rewrite.is_some(),
        "action_rewrite must be Some even with default preprocessor — \
         assemble_turn wraps the preprocessor value unconditionally"
    );
    assert!(
        result.action_flags.is_some(),
        "action_flags must be Some even with default preprocessor — \
         assemble_turn wraps the preprocessor value unconditionally"
    );
}

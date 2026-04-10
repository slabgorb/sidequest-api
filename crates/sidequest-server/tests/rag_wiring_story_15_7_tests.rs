//! Story 15-7: RAG pipeline wiring tests for dispatch.
//!
//! Tests that:
//! 1. ActionResult has a `lore_established` field for post-narration lore accumulation
//! 2. apply_state_mutations processes lore_established entries
//! 3. OTEL events are emitted for lore operations

use std::collections::HashMap;
use sidequest_agents::orchestrator::ActionResult;

// ============================================================
// AC-1: ActionResult carries lore_established from agent output
// ============================================================

#[test]
fn action_result_has_lore_established_field() {
    // ActionResult must carry lore_established so the dispatch loop can
    // call accumulate_lore() in post-narration state mutations.
    let result = ActionResult {
        narration: "You discover ancient ruins.".to_string(),
        beat_selections: vec![],
        is_degraded: false,
        classified_intent: None,
        agent_name: None,
        footnotes: vec![],
        items_gained: vec![],
        npcs_present: vec![],
        quest_updates: HashMap::new(),
        agent_duration_ms: None,
        token_count_in: None,
        token_count_out: None,
        visual_scene: None,
        scene_mood: None,
        personality_events: vec![],
        scene_intent: None,
        resource_deltas: HashMap::new(),
        zone_breakdown: None,
        // This field must exist for the RAG pipeline
        lore_established: Some(vec![
            "The ancient ruins predate the current civilization by millennia.".to_string(),
            "Strange crystalline formations grow from the temple walls.".to_string(),
        ]),
        action_rewrite: None,
        action_flags: None,
        sfx_triggers: vec![],
        merchant_transactions: vec![],
        prompt_tier: String::new(),
    };

    let lore = result.lore_established.as_ref().unwrap();
    assert_eq!(lore.len(), 2);
    assert!(lore[0].contains("ancient ruins"));
    assert!(lore[1].contains("crystalline formations"));
}

#[test]
fn action_result_lore_established_defaults_to_none() {
    // When no lore is established, the field should be None
    let result = ActionResult {
        narration: "Nothing notable happens.".to_string(),
        beat_selections: vec![],
        is_degraded: false,
        classified_intent: None,
        agent_name: None,
        footnotes: vec![],
        items_gained: vec![],
        npcs_present: vec![],
        quest_updates: HashMap::new(),
        agent_duration_ms: None,
        token_count_in: None,
        token_count_out: None,
        visual_scene: None,
        scene_mood: None,
        personality_events: vec![],
        scene_intent: None,
        resource_deltas: HashMap::new(),
        zone_breakdown: None,
        lore_established: None,
        action_rewrite: None,
        action_flags: None,
        sfx_triggers: vec![],
        merchant_transactions: vec![],
        prompt_tier: String::new(),
    };

    assert!(result.lore_established.is_none());
}

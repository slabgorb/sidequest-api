//! Post-narration assembler — merges narrator extraction with preprocessor results.
//!
//! ADR-057 Phase 1: `assemble_turn` is a deterministic function that takes
//! the narrator's `NarratorExtraction` and the preprocessor-produced
//! `ActionRewrite` and `ActionFlags`, then assembles a complete `ActionResult`.
//!
//! **Key invariant:** Preprocessor values for `action_rewrite` and `action_flags`
//! always override whatever the narrator emitted. The narrator's versions of
//! those fields are discarded.

use crate::orchestrator::{ActionFlags, ActionRewrite, ActionResult, NarratorExtraction};

/// Assemble a complete `ActionResult` from narrator extraction and preprocessor outputs.
///
/// Preprocessor `rewrite` and `flags` always win over any values the narrator
/// may have emitted in `extraction.action_rewrite` / `extraction.action_flags`.
/// All other fields pass through from the narrator extraction unchanged.
pub fn assemble_turn(
    extraction: NarratorExtraction,
    rewrite: ActionRewrite,
    flags: ActionFlags,
) -> ActionResult {
    ActionResult {
        narration: extraction.prose,
        combat_patch: None,
        chase_patch: None,
        is_degraded: false,
        classified_intent: None,
        agent_name: None,
        footnotes: extraction.footnotes,
        items_gained: extraction.items_gained,
        npcs_present: extraction.npcs_present,
        quest_updates: extraction.quest_updates,
        agent_duration_ms: None,
        token_count_in: None,
        token_count_out: None,
        extraction_tier: Some(extraction.tier),
        visual_scene: extraction.visual_scene,
        scene_mood: extraction.scene_mood,
        personality_events: extraction.personality_events,
        scene_intent: extraction.scene_intent,
        resource_deltas: extraction.resource_deltas,
        zone_breakdown: None,
        lore_established: extraction.lore_established,
        merchant_transactions: extraction.merchant_transactions,
        sfx_triggers: extraction.sfx_triggers,
        // Preprocessor values always win — narrator's action_rewrite/action_flags are discarded
        action_rewrite: Some(rewrite),
        action_flags: Some(flags),
    }
}

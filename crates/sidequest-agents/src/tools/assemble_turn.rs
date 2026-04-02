//! Post-narration assembler — merges narrator extraction with preprocessor and tool results.
//!
//! ADR-057: `assemble_turn` is a deterministic function that takes the narrator's
//! `NarratorExtraction`, preprocessor-produced `ActionRewrite` and `ActionFlags`,
//! and tool call results (`ToolCallResults`), then assembles a complete `ActionResult`.
//!
//! **Priority:** Tool call results > Preprocessor results > Narrator extraction.
//! For action_rewrite/action_flags: preprocessor always wins.
//! For scene_mood/scene_intent: tool call wins if present, else narrator fallback.

use crate::orchestrator::{ActionFlags, ActionRewrite, ActionResult, NarratorExtraction, VisualScene};

/// Collected results from tool calls made during the narrator turn.
///
/// Each field is `Option` — `None` means the tool didn't fire and the
/// assembler should fall back to the narrator extraction value.
/// Derives `Default` so callers can use `ToolCallResults::default()` when
/// no tools fired.
#[derive(Debug, Clone, Default)]
pub struct ToolCallResults {
    /// Scene mood from `set_mood` tool call. Overrides narrator's `scene_mood`.
    pub scene_mood: Option<String>,
    /// Scene intent from `set_intent` tool call. Overrides narrator's `scene_intent`.
    pub scene_intent: Option<String>,
    /// Visual scene from `scene_render` tool call. Overrides narrator's `visual_scene`.
    pub visual_scene: Option<VisualScene>,
}

/// Assemble a complete `ActionResult` from narrator extraction, preprocessor outputs,
/// and tool call results.
///
/// **Override rules:**
/// - `action_rewrite` / `action_flags`: preprocessor always wins (Phase 1).
/// - `scene_mood` / `scene_intent`: tool call wins if present, else narrator fallback (Phase 2).
/// - All other fields: pass through from narrator extraction.
pub fn assemble_turn(
    extraction: NarratorExtraction,
    rewrite: ActionRewrite,
    flags: ActionFlags,
    tool_results: ToolCallResults,
) -> ActionResult {
    // Scene mood: tool call > narrator extraction
    let scene_mood = tool_results.scene_mood.or(extraction.scene_mood);
    // Scene intent: tool call > narrator extraction
    let scene_intent = tool_results.scene_intent.or(extraction.scene_intent);
    // Visual scene: tool call > narrator extraction
    let visual_scene = tool_results.visual_scene.or(extraction.visual_scene);

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
        visual_scene,
        scene_mood,
        personality_events: extraction.personality_events,
        scene_intent,
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

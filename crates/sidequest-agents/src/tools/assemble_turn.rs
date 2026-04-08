//! Post-narration assembler — merges narrator extraction with preprocessor and tool results.
//!
//! ADR-057: `assemble_turn` is a deterministic function that takes the narrator's
//! `NarratorExtraction`, preprocessor-produced `ActionRewrite` and `ActionFlags`,
//! and tool call results (`ToolCallResults`), then assembles a complete `ActionResult`.
//!
//! **Priority:** Tool call results > Preprocessor results > Narrator extraction.
//! For action_rewrite/action_flags: preprocessor always wins.
//! For scene_mood/scene_intent: tool call wins if present, else narrator fallback.

use std::collections::HashMap;

use crate::orchestrator::{ActionFlags, ActionRewrite, ActionResult, NarratorExtraction, PersonalityEvent, VisualScene};

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
    /// Quest updates from `quest_update` tool calls. Overrides narrator's `quest_updates`.
    /// `None` means no quest_update tools fired (use narrator fallback).
    /// `Some(map)` means tools fired — use this map even if empty.
    pub quest_updates: Option<HashMap<String, String>>,
    /// Personality events from `personality_event` tool calls. Overrides narrator's `personality_events`.
    /// `None` means no personality_event tools fired (use narrator fallback).
    /// `Some(vec)` means tools fired — use this vec even if empty.
    pub personality_events: Option<Vec<PersonalityEvent>>,
    /// Resource deltas from `resource_change` tool calls. Overrides narrator's `resource_deltas`.
    /// `None` means no resource_change tools fired (use narrator fallback).
    /// `Some(map)` means tools fired — use this map even if empty.
    pub resource_deltas: Option<HashMap<String, f64>>,
    /// SFX triggers from `play_sfx` tool calls. Overrides narrator's `sfx_triggers`.
    /// `None` means no play_sfx tools fired (use narrator fallback).
    /// `Some(vec)` means tools fired — use this vec even if empty.
    pub sfx_triggers: Option<Vec<String>>,
    /// Items acquired from `item_acquire` tool calls. Overrides narrator's `items_gained`.
    /// `None` means no item_acquire tools fired (use narrator fallback).
    /// `Some(vec)` means tools fired — use this vec even if empty.
    pub items_acquired: Option<Vec<sidequest_protocol::ItemGained>>,
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
    let span = tracing::info_span!("turn.assemble");
    let _guard = span.enter();

    let mut override_count: u32 = 0;

    // Scene mood: tool call > narrator extraction
    if tool_results.scene_mood.is_some() {
        tracing::info!(
            source = "tool_call",
            value = %tool_results.scene_mood.as_ref().unwrap(),
            "assemble.override.scene_mood"
        );
        override_count += 1;
    }
    let scene_mood = tool_results.scene_mood.or(extraction.scene_mood);

    // Scene intent: tool call > narrator extraction
    if tool_results.scene_intent.is_some() {
        tracing::info!(
            source = "tool_call",
            value = %tool_results.scene_intent.as_ref().unwrap(),
            "assemble.override.scene_intent"
        );
        override_count += 1;
    }
    let scene_intent = tool_results.scene_intent.or(extraction.scene_intent);

    // Visual scene: tool call > narrator extraction
    if tool_results.visual_scene.is_some() {
        tracing::info!(source = "tool_call", "assemble.override.visual_scene");
        override_count += 1;
    }
    let visual_scene = tool_results.visual_scene.or(extraction.visual_scene);

    // Quest updates: tool calls > narrator extraction
    if tool_results.quest_updates.is_some() {
        tracing::info!(
            source = "tool_call",
            count = tool_results.quest_updates.as_ref().unwrap().len(),
            "assemble.override.quest_updates"
        );
        override_count += 1;
    }
    let quest_updates = tool_results.quest_updates.unwrap_or(extraction.quest_updates);

    // Personality events: tool calls > narrator extraction
    if tool_results.personality_events.is_some() {
        tracing::info!(
            source = "tool_call",
            count = tool_results.personality_events.as_ref().unwrap().len(),
            "assemble.override.personality_events"
        );
        override_count += 1;
    }
    let personality_events = tool_results.personality_events.unwrap_or(extraction.personality_events);

    // Resource deltas: tool calls > narrator extraction
    if tool_results.resource_deltas.is_some() {
        tracing::info!(
            source = "tool_call",
            count = tool_results.resource_deltas.as_ref().unwrap().len(),
            "assemble.override.resource_deltas"
        );
        override_count += 1;
    }
    let resource_deltas = tool_results.resource_deltas.unwrap_or(extraction.resource_deltas);

    // SFX triggers: tool calls > narrator extraction
    if tool_results.sfx_triggers.is_some() {
        tracing::info!(
            source = "tool_call",
            count = tool_results.sfx_triggers.as_ref().unwrap().len(),
            "assemble.override.sfx_triggers"
        );
        override_count += 1;
    }
    let sfx_triggers = tool_results.sfx_triggers.unwrap_or(extraction.sfx_triggers);

    // Items gained: tool calls > narrator extraction
    if tool_results.items_acquired.is_some() {
        tracing::info!(
            source = "tool_call",
            count = tool_results.items_acquired.as_ref().unwrap().len(),
            "assemble.override.items_acquired"
        );
        override_count += 1;
    }
    let items_gained = tool_results.items_acquired.unwrap_or(extraction.items_gained);

    tracing::info!(
        tool_overrides = override_count,
        narration_len = extraction.prose.len(),
        items_count = items_gained.len(),
        npcs_count = extraction.npcs_present.len(),
        quests_count = quest_updates.len(),
        personality_events_count = personality_events.len(),
        resource_deltas_count = resource_deltas.len(),
        "assemble.complete"
    );

    ActionResult {
        narration: extraction.prose,
        beat_selections: vec![],
        is_degraded: false,
        classified_intent: None,
        agent_name: None,
        footnotes: extraction.footnotes,
        items_gained,
        npcs_present: extraction.npcs_present,
        quest_updates,
        agent_duration_ms: None,
        token_count_in: None,
        token_count_out: None,
        visual_scene,
        scene_mood,
        personality_events,
        scene_intent,
        resource_deltas,
        zone_breakdown: None,
        lore_established: extraction.lore_established,
        merchant_transactions: extraction.merchant_transactions,
        sfx_triggers,
        // Preprocessor values always win — narrator's action_rewrite/action_flags are discarded
        action_rewrite: Some(rewrite),
        action_flags: Some(flags),
        prompt_tier: String::new(),
        confrontation: extraction.confrontation,
        prompt_text: None,
        raw_response_text: None,
    }
}

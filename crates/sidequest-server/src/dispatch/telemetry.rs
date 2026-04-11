//! GM panel snapshot and turn timing telemetry.

use crate::{Severity, WatcherEventBuilder, WatcherEventType};

use super::DispatchContext;

/// Emit GM panel snapshot and turn timing telemetry.
///
/// Playtest 2026-04-11: this function is now the SINGLE SOURCE OF TRUTH for
/// the `WatcherEventType::TurnComplete` event consumed by the OTEL dashboard.
/// Previously there were TWO emitters of TurnComplete per real player turn:
///   1. This function (component: "game") — fired in the dispatch hot path
///   2. main.rs::turn_record_bridge (component: "orchestrator") — fired
///      asynchronously from the TurnRecord mpsc channel
/// Both fired per real turn → 2× rows in the dashboard timeline. SM diagnosed
/// this from a server log showing two narrator-component spans per turn with
/// identical durations but different turn_numbers.
///
/// Fix: this emitter now carries ALL the fields the dashboard reads (patches,
/// beats_fired, delta_empty, narration_len in addition to the existing
/// turn_id/agent/duration/tokens), and main.rs has been updated to NOT emit
/// the duplicate event. The TurnRecord bridge in main.rs is still alive — it
/// continues to drive ADR-073 JSONL training data persistence and the
/// SubsystemTracker — but no longer emits a competing WatcherEvent.
pub(super) fn emit_telemetry(
    ctx: &mut DispatchContext<'_>,
    turn_number: u64,
    result: &sidequest_agents::orchestrator::ActionResult,
    turn_start: std::time::Instant,
    preprocess_done: std::time::Instant,
    agent_done: std::time::Instant,
    game_delta: &sidequest_game::StateDelta,
    patches_applied: &[sidequest_agents::turn_record::PatchSummary],
    beats_fired: &[(String, f32)],
) {
    // GM Panel: emit full game state snapshot after all mutations
    {
        let turn_approx = ctx.turn_manager.interaction() as u32;
        let npc_data: Vec<serde_json::Value> = ctx
            .npc_registry
            .iter()
            .map(|e| {
                serde_json::json!({
                    "name": e.name,
                    "pronouns": e.pronouns,
                    "role": e.role,
                    "location": e.location,
                    "last_seen_turn": e.last_seen_turn,
                })
            })
            .collect();
        let inventory_names: Vec<String> = ctx
            .inventory
            .carried()
            .map(|i| i.name.as_str().to_string())
            .collect();
        let active_tropes: Vec<serde_json::Value> = ctx
            .trope_states
            .iter()
            .map(|ts| {
                serde_json::json!({
                    "id": ts.trope_definition_id(),
                    "progression": ts.progression(),
                    "status": format!("{:?}", ts.status()),
                })
            })
            .collect();
        let snapshot = serde_json::json!({
            "turn_number": turn_approx,
            "location": ctx.current_location.as_str(),
            "hp": *ctx.hp,
            "max_hp": *ctx.max_hp,
            "level": *ctx.level,
            "xp": *ctx.xp,
            "inventory": inventory_names,
            "npc_registry": npc_data,
            "active_tropes": active_tropes,
            "in_combat": ctx.in_combat(),
            "player_id": ctx.player_id,
            "character": ctx.char_name,
        });
        WatcherEventBuilder::new("game", WatcherEventType::GameStateSnapshot)
            .field("turn_number", turn_approx)
            .field("snapshot", &snapshot)
            .send();
    }

    // Build timing spans for flame chart visualization
    let state_done = std::time::Instant::now();
    let preprocess_ms = preprocess_done.duration_since(turn_start).as_millis() as u64;
    let agent_ms = result
        .agent_duration_ms
        .unwrap_or_else(|| agent_done.duration_since(preprocess_done).as_millis() as u64);
    let agent_start_ms = preprocess_ms;
    let state_start_ms = agent_start_ms + agent_ms;
    let state_ms = state_done.duration_since(agent_done).as_millis() as u64;
    let total_ms = state_done.duration_since(turn_start).as_millis() as u64;

    let spans = serde_json::json!([
        { "name": "preprocessor", "component": "preprocessor", "start_ms": 0, "duration_ms": preprocess_ms },
        { "name": "agent_llm", "component": result.agent_name.as_deref().unwrap_or("narrator"), "start_ms": agent_start_ms, "duration_ms": agent_ms },
        { "name": "state_patch", "component": "state", "start_ms": state_start_ms, "duration_ms": state_ms },
    ]);

    {
        // Render patches/beats into JSON the dashboard's TurnCompleteFields
        // expects. These were previously only emitted via main.rs::turn_record_bridge
        // — now bundled into the single TurnComplete event source. See the
        // function-level doc comment for the consolidation rationale.
        let patches_json: Vec<serde_json::Value> = patches_applied
            .iter()
            .map(|p| {
                serde_json::json!({
                    "patch_type": p.patch_type,
                    "fields_changed": p.fields_changed,
                })
            })
            .collect();
        let beats_json: Vec<serde_json::Value> = beats_fired
            .iter()
            .map(|(name, thresh)| serde_json::json!({"trope": name, "threshold": thresh}))
            .collect();

        let mut builder = WatcherEventBuilder::new("game", WatcherEventType::TurnComplete)
            .field("turn_id", turn_number)
            .field("turn_number", turn_number)
            .field("player_input", ctx.action)
            .field_opt("classified_intent", &result.classified_intent)
            .field_opt("agent_name", &result.agent_name)
            .field("agent_duration_ms", agent_ms)
            .field("is_degraded", result.is_degraded)
            .field("player_id", ctx.player_id)
            .field_opt("token_count_in", &result.token_count_in)
            .field_opt("token_count_out", &result.token_count_out)
            // Playtest 2026-04-11: extraction_tier was previously only emitted on
            // the AgentSpanClose event (dispatch/mod.rs:1127), but the OTEL
            // dashboard's TimelineTab reads it from the TurnComplete fields. Net
            // effect: every Turn Details panel showed `Tier: ?` because the field
            // didn't exist on the event the dashboard was reading. Adding it here
            // restores the per-turn tier visibility (full vs delta per ADR-066),
            // which is critical for diagnosing prompt-cost regressions during
            // playtests.
            .field("extraction_tier", &result.prompt_tier)
            // Genre and world are added so the dashboard can group turns by
            // session/world for the "Turn # collides across sessions" bug.
            // The dashboard already has player_id; (player_id, genre, world) is
            // a stable session identifier across reconnects of the same
            // character into the same world.
            .field("genre", ctx.genre_slug)
            .field("world", ctx.world_slug)
            // Playtest 2026-04-11 follow-up: ported from main.rs's turn_record_bridge
            // emission (now disabled) to consolidate to a single TurnComplete source.
            // Without these fields the dashboard's Turn Details panel would display
            // "Patches: none" / "Beats: none" / wrong delta_empty / wrong narration_len
            // for every turn after the consolidation.
            .field("patches", &patches_json)
            .field("beats_fired", &beats_json)
            .field("delta_empty", game_delta.is_empty())
            .field("narration_len", result.narration.len())
            .field("spans", &spans)
            .field("total_duration_ms", total_ms);
        if result.is_degraded {
            builder = builder.severity(Severity::Warn);
        }
        builder.send();
    }
}

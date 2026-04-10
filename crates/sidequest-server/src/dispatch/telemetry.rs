//! GM panel snapshot and turn timing telemetry.

use crate::{Severity, WatcherEventBuilder, WatcherEventType};

use super::DispatchContext;

/// Emit GM panel snapshot and turn timing telemetry.
pub(super) fn emit_telemetry(
    ctx: &mut DispatchContext<'_>,
    turn_number: u64,
    result: &sidequest_agents::orchestrator::ActionResult,
    turn_start: std::time::Instant,
    preprocess_done: std::time::Instant,
    agent_done: std::time::Instant,
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
            .field("spans", &spans)
            .field("total_duration_ms", total_ms);
        if result.is_degraded {
            builder = builder.severity(Severity::Warn);
        }
        builder.send();
    }
}

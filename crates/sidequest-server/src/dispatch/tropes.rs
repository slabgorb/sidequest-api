//! Trope engine — keyword scanning, activation, tick, and escalation.

use std::collections::HashMap;

use sidequest_protocol::GameMessage;

use crate::{Severity, WatcherEvent, WatcherEventType};

use super::DispatchContext;

/// Scan narration for trope triggers, tick the trope engine.
pub(crate) fn process_tropes(
    ctx: &mut DispatchContext<'_>,
    clean_narration: &str,
    _messages: &mut Vec<GameMessage>,
) {
    let _span =
        tracing::info_span!("turn.tropes", active_count = ctx.trope_states.len(),).entered();

    let narration_lower = clean_narration.to_lowercase();
    tracing::debug!(
        narration_len = narration_lower.len(),
        active_tropes = ctx.trope_states.len(),
        total_defs = ctx.trope_defs.len(),
        "Trope keyword scan starting"
    );
    for def in ctx.trope_defs.iter() {
        let id = match &def.id {
            Some(id) => id,
            None => continue,
        };
        // Skip already active tropes
        if ctx
            .trope_states
            .iter()
            .any(|ts| ts.trope_definition_id() == id)
        {
            continue;
        }
        // Check if any trigger keyword appears in the narration
        let triggered = def
            .triggers
            .iter()
            .any(|t| narration_lower.contains(&t.to_lowercase()));
        if triggered {
            sidequest_game::trope::TropeEngine::activate(ctx.trope_states, id);
            tracing::info!(trope_id = %id, "Trope activated by narration keyword");
            ctx.state.send_watcher_event(WatcherEvent {
                timestamp: chrono::Utc::now(),
                component: "trope".to_string(),
                event_type: WatcherEventType::StateTransition,
                severity: Severity::Info,
                fields: {
                    let mut f = HashMap::new();
                    f.insert(
                        "event".to_string(),
                        serde_json::Value::String("trope_activated".to_string()),
                    );
                    f.insert(
                        "trope_id".to_string(),
                        serde_json::Value::String(id.clone()),
                    );
                    f.insert(
                        "trigger".to_string(),
                        serde_json::Value::String("narration_keyword".to_string()),
                    );
                    f
                },
            });
        }
    }

    // Trope engine tick — uses persistent per-session trope state and genre pack defs
    // Log pre-tick state for debugging
    for ts in ctx.trope_states.iter() {
        tracing::info!(
            trope_id = %ts.trope_definition_id(),
            status = ?ts.status(),
            progression = ts.progression(),
            fired_beats = ts.fired_beats().len(),
            "Trope pre-tick state"
        );
    }
    let fired = sidequest_game::trope::TropeEngine::tick(ctx.trope_states, ctx.trope_defs);
    sidequest_game::trope::TropeEngine::apply_keyword_modifiers(
        ctx.trope_states,
        ctx.trope_defs,
        clean_narration,
    );
    tracing::info!(
        active_tropes = ctx.trope_states.len(),
        fired_beats = fired.len(),
        "Trope tick complete"
    );
    // Log post-tick state
    for ts in ctx.trope_states.iter() {
        tracing::debug!(
            trope_id = %ts.trope_definition_id(),
            status = ?ts.status(),
            progression = ts.progression(),
            "Trope post-tick state"
        );
    }
    for beat in &fired {
        tracing::info!(trope = %beat.trope_name, "Trope beat fired");
        ctx.state.send_watcher_event(WatcherEvent {
            timestamp: chrono::Utc::now(),
            component: "trope".to_string(),
            event_type: WatcherEventType::AgentSpanOpen,
            severity: Severity::Info,
            fields: {
                let mut f = HashMap::new();
                f.insert(
                    "trope".to_string(),
                    serde_json::Value::String(beat.trope_name.clone()),
                );
                f.insert(
                    "trope_id".to_string(),
                    serde_json::Value::String(beat.trope_id.clone()),
                );
                f
            },
        });
    }
}

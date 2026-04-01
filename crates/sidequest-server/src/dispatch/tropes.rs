//! Trope engine — LLM trigger evaluation, activation, tick, and escalation.

use std::collections::{HashMap, HashSet};

use sidequest_agents::agents::troper::TroperAgent;
use sidequest_agents::client::ClaudeClient;
use sidequest_game::achievement::Achievement;
use sidequest_game::trope::{FiredBeat, TropeEngine};
use sidequest_protocol::GameMessage;

use crate::{Severity, WatcherEvent, WatcherEventType};

use super::DispatchContext;

/// Evaluate trope triggers via LLM, tick the trope engine, return fired beats.
///
/// Replaces the old keyword substring scan with Claude-based semantic evaluation.
/// Returns `Vec<FiredBeat>` so the caller can store beat context for next turn's
/// narrator prompt injection.
pub(crate) fn process_tropes(
    ctx: &mut DispatchContext<'_>,
    clean_narration: &str,
    messages: &mut Vec<GameMessage>,
) -> (Vec<FiredBeat>, Vec<Achievement>) {
    let span = tracing::info_span!(
        "turn.tropes",
        active_count = ctx.trope_states.len(),
        activations_from_llm = tracing::field::Empty,
        beats_fired = tracing::field::Empty,
    );
    let _guard = span.enter();

    // --- Phase 1: LLM-based trigger evaluation ---
    let active_ids: HashSet<String> = ctx
        .trope_states
        .iter()
        .map(|ts| ts.trope_definition_id().to_string())
        .collect();

    let client = ClaudeClient::new();
    let activations =
        TroperAgent::evaluate_triggers(&client, clean_narration, ctx.trope_defs, &active_ids);

    span.record("activations_from_llm", activations.len() as u64);

    for id in &activations {
        TropeEngine::activate_and_check_achievements(ctx.trope_states, id, ctx.achievement_tracker);
        tracing::info!(trope_id = %id, "Trope activated by LLM evaluation");
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
                    serde_json::Value::String("llm_evaluation".to_string()),
                );
                f
            },
        });
    }

    // --- Phase 2: Trope engine tick ---
    for ts in ctx.trope_states.iter() {
        tracing::debug!(
            trope_id = %ts.trope_definition_id(),
            status = ?ts.status(),
            progression = ts.progression(),
            fired_beats = ts.fired_beats().len(),
            "Trope pre-tick state"
        );
    }

    let (fired, earned) = TropeEngine::tick_and_check_achievements(
        ctx.trope_states,
        ctx.trope_defs,
        ctx.achievement_tracker,
    );

    tracing::info!(
        active_tropes = ctx.trope_states.len(),
        fired_beats = fired.len(),
        achievements_earned = earned.len(),
        "Trope tick complete"
    );

    for ts in ctx.trope_states.iter() {
        tracing::debug!(
            trope_id = %ts.trope_definition_id(),
            status = ?ts.status(),
            progression = ts.progression(),
            "Trope post-tick state"
        );
    }

    // --- Phase 3: Emit watcher events for fired beats ---
    for beat in &fired {
        tracing::info!(
            trope = %beat.trope_name,
            trope_id = %beat.trope_id,
            threshold = beat.beat.at,
            "Trope beat fired"
        );
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
                f.insert(
                    "threshold".to_string(),
                    serde_json::Value::Number(
                        serde_json::Number::from_f64(beat.beat.at).unwrap_or(serde_json::Number::from(0)),
                    ),
                );
                f
            },
        });
    }

    // --- Phase 4: Broadcast earned achievements + emit watcher events ---
    for achievement in &earned {
        // Broadcast to all session players via GameMessage
        messages.push(GameMessage::AchievementEarned {
            payload: sidequest_protocol::AchievementEarnedPayload {
                achievement_id: achievement.id.clone(),
                name: achievement.name.clone(),
                description: achievement.description.clone(),
                trope_id: achievement.trope_id.clone(),
                trigger: achievement.trigger_status.clone(),
                emoji: achievement.emoji.clone(),
            },
            player_id: "server".to_string(),
        });

        // Emit watcher event for GM panel
        ctx.state.send_watcher_event(WatcherEvent {
            timestamp: chrono::Utc::now(),
            component: "achievement".to_string(),
            event_type: WatcherEventType::StateTransition,
            severity: Severity::Info,
            fields: {
                let mut f = HashMap::new();
                f.insert(
                    "event".to_string(),
                    serde_json::Value::String("achievement_earned".to_string()),
                );
                f.insert(
                    "achievement_id".to_string(),
                    serde_json::Value::String(achievement.id.clone()),
                );
                f.insert(
                    "achievement_name".to_string(),
                    serde_json::Value::String(achievement.name.clone()),
                );
                f.insert(
                    "trope_id".to_string(),
                    serde_json::Value::String(achievement.trope_id.clone()),
                );
                f.insert(
                    "trigger_type".to_string(),
                    serde_json::Value::String(achievement.trigger_status.clone()),
                );
                f
            },
        });
    }

    span.record("beats_fired", fired.len() as u64);
    (fired, earned)
}

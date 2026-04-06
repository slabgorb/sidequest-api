//! Trope engine — LLM trigger evaluation, activation, tick, and escalation.

use std::collections::HashSet;

use sidequest_agents::agents::troper::TroperAgent;
use sidequest_game::achievement::Achievement;
use sidequest_game::engagement::engagement_multiplier;
use sidequest_game::trope::{FiredBeat, TropeEngine};
use sidequest_protocol::GameMessage;

use crate::{WatcherEventBuilder, WatcherEventType};

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

    let client = ctx.state.create_claude_client();
    let activations =
        TroperAgent::evaluate_triggers(&client, clean_narration, ctx.trope_defs, &active_ids);

    span.record("activations_from_llm", activations.len() as u64);

    for id in &activations {
        TropeEngine::activate_and_check_achievements(ctx.trope_states, id, ctx.achievement_tracker);
        tracing::info!(trope_id = %id, "Trope activated by LLM evaluation");
        WatcherEventBuilder::new("trope", WatcherEventType::StateTransition)
            .field("event", "trope_activated")
            .field("trope_id", id)
            .field("trigger", "llm_evaluation")
            .send();
    }

    // --- Phase 2: Trope engine tick with engagement multiplier (story 6-3) ---
    let multiplier = engagement_multiplier(ctx.snapshot.turns_since_meaningful) as f64;

    for ts in ctx.trope_states.iter() {
        tracing::debug!(
            trope_id = %ts.trope_definition_id(),
            status = ?ts.status(),
            progression = ts.progression(),
            fired_beats = ts.fired_beats().len(),
            "Trope pre-tick state"
        );
    }

    let (fired, earned) = TropeEngine::tick_and_check_achievements_with_multiplier(
        ctx.trope_states,
        ctx.trope_defs,
        ctx.achievement_tracker,
        multiplier,
    );

    tracing::info!(
        active_tropes = ctx.trope_states.len(),
        fired_beats = fired.len(),
        achievements_earned = earned.len(),
        engagement_multiplier = multiplier,
        turns_since_meaningful = ctx.snapshot.turns_since_meaningful,
        "Trope tick complete"
    );

    // Unconditional watcher event — GM panel sees the engine is engaged every turn
    WatcherEventBuilder::new("trope", WatcherEventType::SubsystemExerciseSummary)
        .field("event", "trope.tick")
        .field("active_tropes", ctx.trope_states.len())
        .field("activations_from_llm", activations.len())
        .field("beats_fired", fired.len())
        .field("achievements_earned", earned.len())
        .field("engagement_multiplier", multiplier)
        .field("turns_since_meaningful", ctx.snapshot.turns_since_meaningful)
        .send();

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
        WatcherEventBuilder::new("trope", WatcherEventType::AgentSpanOpen)
            .field("trope", &beat.trope_name)
            .field("trope_id", &beat.trope_id)
            .field("threshold", beat.beat.at)
            .send();
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
        WatcherEventBuilder::new("achievement", WatcherEventType::StateTransition)
            .field("event", "achievement_earned")
            .field("achievement_id", &achievement.id)
            .field("achievement_name", &achievement.name)
            .field("trope_id", &achievement.trope_id)
            .field("trigger_type", &achievement.trigger_status)
            .send();
    }

    span.record("beats_fired", fired.len() as u64);
    (fired, earned)
}

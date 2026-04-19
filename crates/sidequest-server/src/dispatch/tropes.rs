//! Trope engine — LLM trigger evaluation, activation, tick, and escalation.

use std::collections::HashSet;

use sidequest_agents::agents::troper::TroperAgent;
use sidequest_game::achievement::Achievement;
use sidequest_game::engagement::engagement_multiplier;
use sidequest_game::trope::{FiredBeat, TropeEngine, TropeStatus};
use sidequest_protocol::GameMessage;

use crate::{WatcherEventBuilder, WatcherEventType};

use super::DispatchContext;

/// Return the engagement category (stealth / confrontation / evasion) for a
/// trope based on its tags, for GM-panel OTEL visibility. First qualifying
/// tag wins; `combat` is folded into `confrontation` and `chase` into
/// `evasion` to match tag conventions already in use across genre packs.
///
/// Returns `None` in two distinct cases:
///  - the trope is not an engagement type (no qualifying tag) — legitimate,
///    the caller should skip the `trope.engagement_outcome` span rather than
///    emit a misleading `"other"` value;
///  - the trope id could not be resolved to a definition — a warning is
///    logged, because the engine only invokes this helper for tropes it
///    believes are active, so a resolve miss signals a tag-typo, id/name
///    drift, or a trope pruned between tick and classify.
fn classify_engagement_kind(
    trope_id: &str,
    trope_defs: &[sidequest_genre::TropeDefinition],
) -> Option<&'static str> {
    let def = match trope_defs
        .iter()
        .find(|d| d.id.as_deref() == Some(trope_id) || d.name.as_str() == trope_id)
    {
        Some(d) => d,
        None => {
            tracing::warn!(
                trope_id,
                "engagement classify: trope_id not resolvable in trope_defs — span skipped"
            );
            return None;
        }
    };
    for tag in &def.tags {
        let t = tag.to_ascii_lowercase();
        if t == "stealth" {
            return Some("stealth");
        }
        if t == "confrontation" || t == "combat" {
            return Some("confrontation");
        }
        if t == "evasion" || t == "chase" {
            return Some("evasion");
        }
    }
    None
}

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
    // Compose engagement multiplier with encumbrance multiplier (story 19-7).
    // When overencumbered in room_graph mode, tropes tick 1.5x faster.
    let engagement = engagement_multiplier(ctx.snapshot.turns_since_meaningful) as f64;
    let encumbrance = if !ctx.rooms.is_empty() {
        // room_graph mode — apply encumbrance multiplier
        ctx.weight_limit
            .map(|wl| ctx.inventory.encumbrance_multiplier(wl))
            .unwrap_or(1.0)
    } else {
        1.0
    };
    let multiplier = engagement * encumbrance;

    if encumbrance > 1.0 {
        WatcherEventBuilder::new("encumbrance", WatcherEventType::StateTransition)
            .field("event", "overencumbered_trope_tick")
            .field("total_weight", ctx.inventory.total_weight())
            .field("weight_limit", ctx.weight_limit.unwrap_or(0.0))
            .field("encumbrance_multiplier", encumbrance)
            .field("combined_multiplier", multiplier)
            .send();
    }

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
        .field(
            "turns_since_meaningful",
            ctx.snapshot.turns_since_meaningful,
        )
        .send();

    for ts in ctx.trope_states.iter() {
        tracing::debug!(
            trope_id = %ts.trope_definition_id(),
            status = ?ts.status(),
            progression = ts.progression(),
            "Trope post-tick state"
        );
    }

    // --- Phase 2b: Trope-encounter handshake (story 37-15) ---
    // If any trope just auto-resolved and there's an active encounter,
    // signal the encounter to resolve.
    for ts in ctx.trope_states.iter() {
        if ts.status() == TropeStatus::Resolved && ts.progression() >= 1.0 {
            if let Some(encounter) = ctx.snapshot.encounter.as_mut() {
                if !encounter.resolved {
                    encounter.resolve_from_trope(ts.trope_definition_id());
                }
            }
            // Story 37-24: emit engagement outcome span if this resolved trope
            // is tagged as a stealth / confrontation / evasion engagement.
            if let Some(kind) = classify_engagement_kind(ts.trope_definition_id(), ctx.trope_defs) {
                crate::emit_trope_engagement_outcome(
                    ts.trope_definition_id(),
                    kind,
                    "success",
                    ts.progression(),
                );
            }
        }
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
        // Story 37-24: when a fired beat belongs to a stealth / confrontation /
        // evasion trope, emit the engagement-outcome span. Outcome is
        // "escalation" — a beat firing is progression advancing, not terminal
        // success/failure. Terminal outcomes fire at the auto-resolve site above.
        if let Some(kind) = classify_engagement_kind(&beat.trope_id, ctx.trope_defs) {
            crate::emit_trope_engagement_outcome(&beat.trope_id, kind, "escalation", beat.beat.at);
        }
    }

    // --- Phase 4: Broadcast earned achievements + emit watcher events ---
    for achievement in &earned {
        // Broadcast to all session players via GameMessage
        let achievement_id = sidequest_protocol::NonBlankString::new(&achievement.id)
            .expect("achievement.id is non-empty by YAML schema");
        let name = sidequest_protocol::NonBlankString::new(&achievement.name)
            .expect("achievement.name is non-empty by YAML schema");
        let description = sidequest_protocol::NonBlankString::new(&achievement.description)
            .expect("achievement.description is non-empty by YAML schema");
        let trope_id = sidequest_protocol::NonBlankString::new(&achievement.trope_id)
            .expect("achievement.trope_id is non-empty by YAML schema");
        messages.push(GameMessage::AchievementEarned {
            payload: sidequest_protocol::AchievementEarnedPayload {
                achievement_id,
                name,
                description,
                trope_id,
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

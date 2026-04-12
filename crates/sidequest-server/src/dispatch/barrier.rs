//! Turn barrier coordination for structured/cinematic multiplayer turns.

use std::collections::HashMap;

use sidequest_protocol::{
    ActionRevealPayload, GameMessage, PlayerActionEntry, SessionEventPayload, TurnStatusPayload,
};

use super::DispatchContext;

/// Parse structured status effect codes into typed perceptual effects.
///
/// Status codes are set by the narrator in game_patch and stored on
/// CreatureCore.statuses. Expected formats: "blinded", "deafened",
/// "hallucinating", "charmed_by:<source>", "dominated_by:<controller>".
///
/// Unrecognized codes emit an OTEL warning — no silent drop, no keyword
/// fuzzy match. This replaces the `lower.contains("blind")` pattern that
/// violated the Zork Problem (ADR-010/032) and the "no silent fallbacks"
/// rule (CLAUDE.md).
pub(super) fn map_statuses_to_perceptual_effects(
    statuses: &[String],
) -> Vec<sidequest_game::perception::PerceptualEffect> {
    use sidequest_game::perception::PerceptualEffect;

    statuses
        .iter()
        .filter_map(|s| {
            match PerceptualEffect::from_status_code(s) {
                Some(effect) => Some(effect),
                None => {
                    // FAIL LOUD — narrator emitted an unknown status effect code.
                    // Don't silently drop it. OTEL warning so the GM panel sees it.
                    crate::WatcherEventBuilder::new("perception", crate::WatcherEventType::ValidationWarning)
                        .field("event", "perception.unknown_status_code")
                        .field("status_code", s.as_str())
                        .field("expected_formats", "blinded|deafened|hallucinating|charmed_by:<source>|dominated_by:<controller>")
                        .severity(crate::Severity::Warn)
                        .send();
                    tracing::warn!(
                        status_code = %s,
                        "perception.unknown_status_code — narrator emitted unrecognized status effect, no fallback"
                    );
                    None
                }
            }
        })
        .collect()
}

/// Outcome of barrier resolution — includes claim election flag and barrier
/// handle for post-narrator `store_resolution_narration()`.
pub(super) struct BarrierOutcome {
    pub combined_action: String,
    pub claimed_resolution: bool,
    pub barrier: sidequest_game::barrier::TurnBarrier,
}

pub(super) async fn handle_barrier(
    ctx: &mut DispatchContext<'_>,
    state_summary: &mut String,
) -> Option<BarrierOutcome> {
    let holder = ctx.shared_session_holder.lock().await;
    if let Some(ref ss_arc) = *holder {
        let ss = ss_arc.lock().await;
        tracing::debug!(
            turn_mode = ?ss.turn_mode,
            player_count = ss.players.len(),
            has_barrier = ss.turn_barrier.is_some(),
            "barrier.check — if barrier exists, use it"
        );
        {
            if let Some(ref barrier) = ss.turn_barrier {
                tracing::info!(player_id = %ctx.player_id, "barrier.submit — action submitted, waiting for other players");
                barrier.submit_action(ctx.player_id, ctx.action);

                let turn_submitted = GameMessage::TurnStatus {
                    payload: TurnStatusPayload {
                        player_name: ctx.player_name_for_save.to_string(),
                        status: "submitted".into(),
                        state_delta: None,
                    },
                    player_id: ctx.player_id.to_string(),
                };
                let _ = ctx.state.broadcast(turn_submitted);
                tracing::info!(player_name = %ctx.player_name_for_save, "barrier.turn_status.submitted — player sealed their letter");
                let barrier_clone = barrier.clone();

                ss.send_to_player(
                    GameMessage::SessionEvent {
                        payload: SessionEventPayload {
                            event: "waiting".to_string(),
                            player_name: None,
                            genre: None,
                            world: None,
                            has_character: None,
                            initial_state: None,
                            css: None,
                            narrator_verbosity: None,
                            narrator_vocabulary: None,
                            image_cooldown_seconds: None,
                        },
                        player_id: ctx.player_id.to_string(),
                    },
                    ctx.player_id.to_string(),
                );

                drop(ss);
                drop(holder);

                let result = barrier_clone.wait_for_turn().await;
                let claimed = result.claimed_resolution;
                tracing::info!(
                    timed_out = result.timed_out,
                    missing = ?result.missing_players,
                    claimed_resolution = claimed,
                    genre = %ctx.genre_slug,
                    world = %ctx.world_slug,
                    "Turn barrier resolved"
                );
                crate::WatcherEventBuilder::new("multiplayer", crate::WatcherEventType::StateTransition)
                    .field("event", "sealed_round.claim_election")
                    .field("player_id", ctx.player_id)
                    .field("claimed", claimed)
                    .field("timed_out", result.timed_out)
                    .field("missing_players", format!("{:?}", result.missing_players))
                    .send();

                let auto_resolved_names = result.auto_resolved_character_names();
                let auto_resolved_context = result.format_auto_resolved_context();

                // Read actions from the BARRIER's internal session, not
                // ss.multiplayer — submit_action() records into the barrier's
                // own MultiplayerSession, so ss.multiplayer has no actions.
                let named_actions = barrier_clone.named_actions();
                let player_stats = {
                    let holder = ctx.shared_session_holder.lock().await;
                    if let Some(ref ss_arc) = *holder {
                        let ss = ss_arc.lock().await;
                        let stats: HashMap<String, HashMap<String, i32>> = ss
                            .players
                            .values()
                            .filter_map(|ps| {
                                let name = ps.character_name.as_ref()?;
                                let json = ps.character_json.as_ref()?;
                                let char_stats: HashMap<String, i32> = json
                                    .get("stats")
                                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                                    .unwrap_or_default();
                                Some((name.clone(), char_stats))
                            })
                            .collect();
                        stats
                    } else {
                        HashMap::new()
                    }
                };

                let initiative_rules = {
                    let genre_dir = ctx.state.genre_packs_path().join(ctx.genre_slug);
                    sidequest_genre::load_genre_pack(&genre_dir)
                        .map(|pack| pack.rules.initiative_rules.clone())
                        .unwrap_or_default()
                };

                let encounter_type = ctx
                    .snapshot
                    .encounter
                    .as_ref()
                    .map(|e| e.encounter_type.as_str())
                    .unwrap_or("exploration");

                let sealed_ctx = sidequest_game::sealed_round::build_sealed_round_context(
                    &named_actions,
                    encounter_type,
                    &initiative_rules,
                    &player_stats,
                );

                {
                    let span = tracing::info_span!(
                        "narrator.sealed_round",
                        encounter_type = encounter_type,
                        player_count = sealed_ctx.player_count(),
                        action_count = sealed_ctx.action_count(),
                        has_initiative = initiative_rules.contains_key(encounter_type),
                    );
                    let _guard = span.enter();
                }

                let sealed_prompt = sealed_ctx.to_prompt_section();

                let turn_number = barrier_clone.turn_number().saturating_sub(1);
                let action_entries: Vec<PlayerActionEntry> = named_actions
                    .iter()
                    .map(|(name, action)| PlayerActionEntry {
                        character_name: name.clone(),
                        player_id: String::new(),
                        action: action.clone(),
                    })
                    .collect();
                let reveal = GameMessage::ActionReveal {
                    payload: ActionRevealPayload {
                        actions: action_entries,
                        turn_number,
                        auto_resolved: auto_resolved_names.clone(),
                    },
                    player_id: "server".to_string(),
                };
                let _ = ctx.state.broadcast(reveal);
                tracing::info!(auto_resolved = ?auto_resolved_names, "barrier.action_reveal — broadcast with auto-resolved");

                for name in &auto_resolved_names {
                    let turn_auto = GameMessage::TurnStatus {
                        payload: TurnStatusPayload {
                            player_name: name.clone(),
                            status: "auto_resolved".into(),
                            state_delta: None,
                        },
                        player_id: "server".to_string(),
                    };
                    let _ = ctx.state.broadcast(turn_auto);
                }

                let auto_ctx = if auto_resolved_context.is_empty() {
                    String::new()
                } else {
                    format!("\n{}\n", auto_resolved_context)
                };
                *state_summary = format!("{}\n{}\n{}", sealed_prompt, auto_ctx, state_summary);

                let combined = named_actions
                    .iter()
                    .map(|(name, act)| format!("{}: {}", name, act))
                    .collect::<Vec<_>>()
                    .join("\n");
                return Some(BarrierOutcome {
                    combined_action: combined,
                    claimed_resolution: claimed,
                    barrier: barrier_clone,
                });
            }
        }
    }
    None
}

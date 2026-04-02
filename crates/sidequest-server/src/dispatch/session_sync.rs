//! Shared session synchronization — sync state back and broadcast to other players.

use sidequest_protocol::{
    GameMessage, NarrationEndPayload, PartyMember, PartyStatusPayload, TurnStatusPayload,
};

use crate::{WatcherEventBuilder, WatcherEventType};

use super::DispatchContext;

/// Sync state back to shared session and broadcast messages to other players.
#[tracing::instrument(name = "turn.sync_session", skip_all)]
pub(crate) async fn sync_back_to_shared_session(
    ctx: &mut DispatchContext<'_>,
    messages: &[GameMessage],
    _clean_narration: &str,
    char_class: &str,
    effective_action: &str,
) {
    let holder = ctx.shared_session_holder.lock().await;
    if let Some(ref ss_arc) = *holder {
        let mut ss = ss_arc.lock().await;
        ss.sync_from_locals(
            ctx.current_location,
            ctx.npc_registry,
            ctx.narration_history,
            ctx.discovered_regions,
            ctx.trope_states,
            ctx.player_id,
        );

        WatcherEventBuilder::new("session_sync", WatcherEventType::StateTransition)
            .field("action", "sync_from_locals")
            .field("player_id", ctx.player_id)
            .field("player_count", ss.player_count())
            .field("npc_count", ctx.npc_registry.len())
            .field("location", ctx.current_location.as_str())
            .send(ctx.state);

        // Sync acting player's character data to PlayerState for other players' PARTY_STATUS
        if let Some(ps) = ss.players.get_mut(ctx.player_id) {
            ps.character_hp = *ctx.hp;
            ps.character_max_hp = *ctx.max_hp;
            ps.character_level = *ctx.level;
            ps.character_xp = *ctx.xp;
            ps.character_class = char_class.to_string();
            ps.inventory = ctx.inventory.clone();
            ps.combat_state = ctx.combat_state.clone();
            ps.chase_state = ctx.chase_state.clone();
            if ps.character_name.is_none() {
                ps.character_name = Some(ctx.char_name.to_string());
            }
        }
        // Route messages to session members.
        // The acting player already receives via their direct tx channel (mpsc).
        // Other players get narration (without state_delta) via the session broadcast channel.
        // Fall back to all session members when cartography regions aren't available.
        let co_located = ss.co_located_players(ctx.player_id);
        let other_players: Vec<String> = if co_located.is_empty() {
            // No region data — fall back to all other session members
            ss.players
                .keys()
                .filter(|pid| pid.as_str() != ctx.player_id)
                .cloned()
                .collect()
        } else {
            co_located
        };
        for msg in messages {
            match msg {
                GameMessage::Narration { payload, .. } => {
                    // Send the acting player's action to observers FIRST.
                    // This creates a turn boundary in NarrativeView (PLAYER_ACTION triggers flushChunks).
                    let observer_action = GameMessage::PlayerAction {
                        payload: sidequest_protocol::PlayerActionPayload {
                            action: format!("{} — {}", ctx.char_name, effective_action),
                            aside: false,
                        },
                        player_id: ctx.player_id.to_string(),
                    };
                    tracing::info!(
                        ctx.char_name = %ctx.char_name,
                        observer_count = other_players.len(),
                        "multiplayer.observer_action — broadcasting PLAYER_ACTION to observers"
                    );
                    for target_id in &other_players {
                        ss.send_to_player(observer_action.clone(), target_id.clone());
                    }
                    // Send narration (state_delta stripped) to other players.
                    // Apply perception rewriting if active filters exist (Story 15-4).
                    for target_id in &other_players {
                        let text = if let Some(filter) = ss.perception_filters.get(target_id) {
                            if filter.has_effects() {
                                // Use Claude-backed perception rewriter for actual narration variant
                                let client = if let Some(endpoint) = ctx.state.otel_endpoint() {
                                    sidequest_agents::client::ClaudeClient::builder()
                                        .otel_endpoint(endpoint.to_string())
                                        .build()
                                } else {
                                    sidequest_agents::client::ClaudeClient::new()
                                };
                                let strategy =
                                    sidequest_agents::agents::resonator::ClaudeRewriteStrategy::new(
                                        client,
                                    );
                                let rewriter = sidequest_game::perception::PerceptionRewriter::new(
                                    Box::new(strategy),
                                );
                                match rewriter.rewrite(&payload.text, filter, ctx.genre_slug) {
                                    Ok(rewritten) => {
                                        tracing::info!(
                                            target_player = %target_id,
                                            effects = %sidequest_game::perception::PerceptionRewriter::describe_effects(filter.effects()),
                                            "perception.rewrite — narration rewritten for player"
                                        );
                                        rewritten
                                    }
                                    Err(e) => {
                                        // Graceful degradation per ADR-006: fall back to annotated narration
                                        tracing::warn!(
                                            target_player = %target_id,
                                            error = %e,
                                            "perception.rewrite_failed — falling back to base narration"
                                        );
                                        let effects_desc = sidequest_game::perception::PerceptionRewriter::describe_effects(filter.effects());
                                        format!(
                                            "[Your perception is altered: {}]\n\n{}",
                                            effects_desc, payload.text
                                        )
                                    }
                                }
                            } else {
                                payload.text.clone()
                            }
                        } else {
                            payload.text.clone()
                        };
                        let narration_msg = GameMessage::Narration {
                            payload: sidequest_protocol::NarrationPayload {
                                text,
                                state_delta: None,
                                footnotes: payload.footnotes.clone(),
                            },
                            player_id: target_id.clone(),
                        };
                        ss.send_to_player(narration_msg, target_id.clone());
                    }
                    tracing::info!(
                        observer_count = other_players.len(),
                        text_len = payload.text.len(),
                        "multiplayer.narration_broadcast — sent to observers via session channel"
                    );
                }
                GameMessage::NarrationEnd { .. } => {
                    // Broadcast NarrationEnd to all players so TTS sync works correctly
                    let player_ids: Vec<String> = ss.players.keys().cloned().collect();
                    for target_pid in &player_ids {
                        let end_msg = GameMessage::NarrationEnd {
                            payload: NarrationEndPayload { state_delta: None },
                            player_id: target_pid.clone(),
                        };
                        ss.send_to_player(end_msg, target_pid.clone());
                    }
                    // TURN_STATUS "resolved" — unlock input for all players after narration completes.
                    // Use global broadcast (not session channel) for reliability — session
                    // subscribers may miss messages sent before subscription.
                    if ss.players.len() > 1 {
                        let resolved_msg = GameMessage::TurnStatus {
                            payload: TurnStatusPayload {
                                player_name: ctx.player_name_for_save.to_string(),
                                status: "resolved".into(),
                                state_delta: None,
                            },
                            player_id: ctx.player_id.to_string(),
                        };
                        let _ = ctx.state.broadcast(resolved_msg);
                        tracing::info!(player_name = %ctx.player_name_for_save, "turn_status.resolved broadcast to all clients");
                    }
                }
                GameMessage::ChapterMarker { ref payload, .. } => {
                    // Send to other players only — acting player already received via direct channel
                    for target_pid in ss
                        .players
                        .keys()
                        .filter(|pid| pid.as_str() != ctx.player_id)
                    {
                        let marker = GameMessage::ChapterMarker {
                            payload: payload.clone(),
                            player_id: target_pid.clone(),
                        };
                        ss.send_to_player(marker, target_pid.clone());
                    }
                }
                GameMessage::PartyStatus { .. } => {
                    // Build targeted PARTY_STATUS per player so every player's
                    // player_id is set correctly (client HUD uses this for identity).
                    let members: Vec<PartyMember> = ss
                        .players
                        .iter()
                        .map(|(pid, ps)| PartyMember {
                            player_id: pid.clone(),
                            name: ps.player_name.clone(),
                            character_name: ps
                                .character_name
                                .clone()
                                .unwrap_or_else(|| ps.player_name.clone()),
                            current_hp: ps.character_hp,
                            max_hp: ps.character_max_hp,
                            statuses: vec![],
                            class: ps.character_class.clone(),
                            level: ps.character_level,
                            portrait_url: None,
                            current_location: ps.display_location.clone(),
                        })
                        .collect();
                    let player_ids: Vec<String> = ss.players.keys().cloned().collect();
                    for target_pid in &player_ids {
                        let party_msg = GameMessage::PartyStatus {
                            payload: PartyStatusPayload {
                                members: members.clone(),
                            },
                            player_id: target_pid.clone(),
                        };
                        ss.send_to_player(party_msg, target_pid.clone());
                    }
                }
                _ => {}
            }
        }
    }
}

//! Combat and chase detection, state tracking, overlay messages.

use std::collections::HashMap;

use sidequest_protocol::{CombatEventPayload, GameMessage};

use crate::{Severity, WatcherEvent, WatcherEventType};

use super::DispatchContext;

/// Combat detection, combat tick, combat overlay, chase detection.
#[tracing::instrument(name = "turn.combat_and_chase", skip_all)]
pub(crate) async fn process_combat_and_chase(
    ctx: &mut DispatchContext<'_>,
    clean_narration: &str,
    result: &sidequest_agents::orchestrator::ActionResult,
    messages: &mut Vec<GameMessage>,
) {
    // Combat engagement is driven by CombatPatch from creature_smith (in apply_state_mutations),
    // NOT by intent classification. Intent classification routes to the right agent, but the
    // agent decides whether combat actually starts via the in_combat field in its patch.
    //
    // Keyword fallback: narration explicitly describes combat starting (emergency only).
    // This catches cases where the narrator (not creature_smith) describes combat beginning.
    if !ctx.combat_state.in_combat() {
        let narr_lower = clean_narration.to_lowercase();
        let combat_start_keywords = ["roll for initiative", "combat begins", "enters combat"];
        if combat_start_keywords
            .iter()
            .any(|kw| narr_lower.contains(kw))
        {
            // Extract combatant names from hp_changes in combat patch (if present),
            // otherwise use only the player name — don't dump all known NPCs.
            let mut combatants: Vec<String> = vec![ctx.char_name.to_string()];
            if let Some(ref combat_patch) = result.combat_patch {
                if let Some(ref hp_changes) = combat_patch.hp_changes {
                    for target in hp_changes.keys() {
                        if !combatants.iter().any(|c| c.eq_ignore_ascii_case(target)) {
                            combatants.push(target.clone());
                        }
                    }
                }
            }
            ctx.combat_state.engage(combatants);
            tracing::info!(
                source = "keyword",
                turn_order = ?ctx.combat_state.turn_order(),
                current_turn = ?ctx.combat_state.current_turn(),
                "combat.engaged"
            );
            // Transition turn mode: FreePlay → Structured
            {
                let holder = ctx.shared_session_holder.lock().await;
                if let Some(ref ss_arc) = *holder {
                    let mut ss = ss_arc.lock().await;
                    let old_mode = std::mem::take(&mut ss.turn_mode);
                    ss.turn_mode = old_mode
                        .apply(sidequest_game::turn_mode::TurnModeTransition::CombatStarted);
                    tracing::info!(new_mode = ?ss.turn_mode, "Turn mode transitioned on combat start");
                    if ss.turn_mode.should_use_barrier() && ss.turn_barrier.is_none() {
                        let mp_session =
                            sidequest_game::multiplayer::MultiplayerSession::with_player_ids(
                                ss.players.keys().cloned(),
                            );
                        let adaptive = sidequest_game::barrier::AdaptiveTimeout::default();
                        ss.turn_barrier =
                            Some(sidequest_game::barrier::TurnBarrier::with_adaptive(
                                mp_session, adaptive,
                            ));
                    }
                }
            }
        }
    }

    // Combat end detection — keyword fallback for narration-driven combat end
    if ctx.combat_state.in_combat() {
        let narr_lower = clean_narration.to_lowercase();
        let combat_end_keywords = [
            "combat ends",
            "battle is over",
            "enemies defeated",
            "falls unconscious",
            "retreats",
            "flees",
            "surrenders",
            "combat resolved",
            "the fight is over",
        ];
        if combat_end_keywords.iter().any(|kw| narr_lower.contains(kw)) {
            ctx.combat_state.disengage();
            tracing::info!("combat.disengaged — detected end keyword in narration");
            // Transition turn mode: Structured → FreePlay
            {
                let holder = ctx.shared_session_holder.lock().await;
                if let Some(ref ss_arc) = *holder {
                    let mut ss = ss_arc.lock().await;
                    let old_mode = std::mem::take(&mut ss.turn_mode);
                    ss.turn_mode =
                        old_mode.apply(sidequest_game::turn_mode::TurnModeTransition::CombatEnded);
                    tracing::info!(new_mode = ?ss.turn_mode, "Turn mode transitioned on combat end");
                }
            }
        }
    }

    // Combat tick — tick status effects (round advancement handled by advance_turn in apply_state_mutations)
    let was_in_combat = ctx.combat_state.in_combat();
    tracing::debug!(
        in_combat = was_in_combat,
        round = ctx.combat_state.round(),
        drama_weight = ctx.combat_state.drama_weight(),
        "combat.pre_tick"
    );
    if ctx.combat_state.in_combat() {
        ctx.combat_state.tick_effects();
        ctx.state.send_watcher_event(WatcherEvent {
            timestamp: chrono::Utc::now(),
            component: "combat".to_string(),
            event_type: WatcherEventType::AgentSpanOpen,
            severity: Severity::Info,
            fields: {
                let mut f = HashMap::new();
                f.insert(
                    "round".to_string(),
                    serde_json::json!(ctx.combat_state.round()),
                );
                f.insert(
                    "drama_weight".to_string(),
                    serde_json::json!(ctx.combat_state.drama_weight()),
                );
                f.insert(
                    "turn_order".to_string(),
                    serde_json::json!(ctx.combat_state.turn_order()),
                );
                f.insert(
                    "current_turn".to_string(),
                    serde_json::json!(ctx.combat_state.current_turn()),
                );
                f
            },
        });
    }

    // Combat overlay — send populated CombatEvent with enemies, turn order, current turn
    if was_in_combat || ctx.combat_state.in_combat() {
        let enemies: Vec<sidequest_protocol::CombatEnemy> = ctx
            .npc_registry
            .iter()
            .filter(|_| ctx.combat_state.in_combat()) // only show enemies during active combat
            .map(|entry| sidequest_protocol::CombatEnemy {
                name: entry.name.clone(),
                hp: entry.hp,
                max_hp: entry.max_hp,
                ac: None,
            })
            .collect();
        messages.push(GameMessage::CombatEvent {
            payload: CombatEventPayload {
                in_combat: ctx.combat_state.in_combat(),
                enemies,
                turn_order: ctx.combat_state.turn_order().to_vec(),
                current_turn: ctx.combat_state.current_turn().unwrap_or("").to_string(),
            },
            player_id: ctx.player_id.to_string(),
        });
    }

    // Bug 6: Chase detection and state tracking
    {
        let narr_lower = clean_narration.to_lowercase();
        let chase_start_keywords = [
            "chase begins",
            "gives chase",
            "starts chasing",
            "run!",
            "flee!",
            "pursues you",
            "pursuit begins",
            "races after",
            "sprints after",
            "bolts away",
        ];
        let chase_end_keywords = [
            "escape",
            "lost them",
            "chase ends",
            "caught up",
            "stopped running",
            "pursuit ends",
            "safe now",
            "shakes off",
            "outrun",
        ];

        if let Some(ref mut cs) = ctx.chase_state {
            // Update active chase
            if chase_end_keywords.iter().any(|kw| narr_lower.contains(kw)) {
                ctx.state.send_watcher_event(WatcherEvent {
                    timestamp: chrono::Utc::now(),
                    component: "chase".to_string(),
                    event_type: WatcherEventType::StateTransition,
                    severity: Severity::Info,
                    fields: {
                        let mut f = HashMap::new();
                        f.insert("action".to_string(), serde_json::json!("chase_resolved"));
                        f.insert("rounds".to_string(), serde_json::json!(cs.round()));
                        f.insert("final_separation".to_string(), serde_json::json!(cs.separation()));
                        f
                    },
                });
                tracing::info!(rounds = cs.round(), "Chase resolved");
                *ctx.chase_state = None;
            } else {
                // Advance chase round, adjust separation based on narration
                let gain = if narr_lower.contains("gaining") || narr_lower.contains("closing") {
                    -1
                } else if narr_lower.contains("widening") || narr_lower.contains("pulling ahead") {
                    1
                } else {
                    0
                };
                cs.set_separation(cs.separation() + gain);
                cs.record_roll(0.5); // placeholder roll
                tracing::info!(
                    round = cs.round(),
                    separation = cs.separation(),
                    gain,
                    "chase.tick — round advanced"
                );
                ctx.state.send_watcher_event(WatcherEvent {
                    timestamp: chrono::Utc::now(),
                    component: "chase".to_string(),
                    event_type: WatcherEventType::StateTransition,
                    severity: Severity::Info,
                    fields: {
                        let mut f = HashMap::new();
                        f.insert("action".to_string(), serde_json::json!("chase_tick"));
                        f.insert("round".to_string(), serde_json::json!(cs.round()));
                        f.insert("separation".to_string(), serde_json::json!(cs.separation()));
                        f.insert("gain".to_string(), serde_json::json!(gain));
                        f
                    },
                });
            }
        } else if chase_start_keywords
            .iter()
            .any(|kw| narr_lower.contains(kw))
        {
            let cs = sidequest_game::ChaseState::new(sidequest_game::ChaseType::Footrace, 0.5);
            tracing::info!("Chase started — detected chase keyword in narration");
            *ctx.chase_state = Some(cs);
            ctx.state.send_watcher_event(WatcherEvent {
                timestamp: chrono::Utc::now(),
                component: "chase".to_string(),
                event_type: WatcherEventType::StateTransition,
                severity: Severity::Info,
                fields: {
                    let mut f = HashMap::new();
                    f.insert("action".to_string(), serde_json::json!("chase_started"));
                    f.insert("chase_type".to_string(), serde_json::json!("Footrace"));
                    f
                },
            });
        }
    }
}

//! Slash command interception — route /commands to mechanical handlers, not the LLM.

use sidequest_protocol::{GameMessage, NarrationEndPayload, NarrationPayload};

use crate::{WatcherEventBuilder, WatcherEventType};

use super::DispatchContext;

/// Slash command interception — route /commands to mechanical handlers, not the LLM.
/// Returns `Some(messages)` for early return, `None` to continue normal dispatch.
pub(crate) fn handle_slash_command(ctx: &mut DispatchContext<'_>) -> Option<Vec<GameMessage>> {
    if !ctx.action.starts_with('/') {
        return None;
    }
    let _span = tracing::info_span!("turn.slash_command", command = %ctx.action).entered();

    use sidequest_game::commands::{
        GmCommand, InventoryCommand, MapCommand, QuestsCommand, SaveCommand, StatusCommand,
    };
    use sidequest_game::slash_router::SlashRouter;
    use sidequest_game::state::GameSnapshot;

    let mut router = SlashRouter::new();
    router.register(Box::new(StatusCommand));
    router.register(Box::new(InventoryCommand));
    router.register(Box::new(MapCommand));
    router.register(Box::new(QuestsCommand));
    router.register(Box::new(SaveCommand));
    router.register(Box::new(GmCommand));
    if let Some(ref ac) = ctx.axes_config {
        router.register(Box::new(sidequest_game::ToneCommand::new(ac.clone())));
    }

    // Build a minimal GameSnapshot from the local session state.
    let snapshot = {
        let mut snap = GameSnapshot {
            genre_slug: ctx.genre_slug.to_string(),
            world_slug: ctx.world_slug.to_string(),
            location: ctx.current_location.clone(),
            current_region: ctx.current_location.clone(),
            discovered_regions: ctx.discovered_regions.clone(),
            encounter: ctx.snapshot.encounter.clone(),
            axis_values: ctx.axis_values.clone(),
            active_tropes: ctx.trope_states.clone(),
            quest_log: ctx.quest_log.clone(),
            npc_registry: ctx.npc_registry.clone(),
            ..GameSnapshot::default()
        };
        // Reconstruct a minimal Character from loose variables.
        if let Some(ref cj) = ctx.character_json {
            if let Ok(mut ch) = serde_json::from_value::<sidequest_game::Character>(cj.clone()) {
                // Sync mutable fields that may have diverged from the JSON snapshot.
                ch.core.hp = *ctx.hp;
                ch.core.max_hp = *ctx.max_hp;
                ch.core.level = *ctx.level;
                ch.core.inventory = ctx.inventory.clone();
                snap.characters.push(ch);
            }
        }
        snap
    };

    // Story 7-9: /accuse command — route to ScenarioState::handle_accusation().
    // Parsed format: /accuse <npc_name> <reason>
    if ctx.action.starts_with("/accuse") {
        let _span = tracing::info_span!("scenario.accusation", command = %ctx.action).entered();
        let parts: Vec<&str> = ctx.action.splitn(3, ' ').collect();
        if parts.len() < 2 {
            return Some(vec![
                GameMessage::Narration {
                    payload: NarrationPayload {
                        text: "Usage: /accuse <npc_name> [reason]".to_string(),
                        state_delta: None,
                        footnotes: vec![],
                    },
                    player_id: ctx.player_id.to_string(),
                },
                GameMessage::NarrationEnd {
                    payload: NarrationEndPayload { state_delta: None },
                    player_id: ctx.player_id.to_string(),
                },
            ]);
        }

        let accused_npc_name = parts[1].to_string();
        let stated_reason = if parts.len() >= 3 {
            parts[2].to_string()
        } else {
            "No reason given.".to_string()
        };

        // Guard: validate accused NPC exists in the roster before resolving.
        // A typo would permanently resolve the scenario against a phantom NPC.
        let npc_exists = ctx
            .snapshot
            .npcs
            .iter()
            .any(|n| n.core.name.as_str() == accused_npc_name);
        if !npc_exists {
            let valid_names: Vec<String> = ctx
                .snapshot
                .npcs
                .iter()
                .map(|n| n.core.name.to_string())
                .collect();
            return Some(vec![
                GameMessage::Narration {
                    payload: NarrationPayload {
                        text: format!(
                            "No NPC named '{}' found. Known NPCs: {}",
                            accused_npc_name,
                            valid_names.join(", ")
                        ),
                        state_delta: None,
                        footnotes: vec![],
                    },
                    player_id: ctx.player_id.to_string(),
                },
                GameMessage::NarrationEnd {
                    payload: NarrationEndPayload { state_delta: None },
                    player_id: ctx.player_id.to_string(),
                },
            ]);
        }

        // Guard: prevent re-accusation after scenario is already resolved.
        if let Some(ref scenario) = ctx.snapshot.scenario_state {
            if scenario.is_resolved() {
                return Some(vec![
                    GameMessage::Narration {
                        payload: NarrationPayload {
                            text: "The scenario has already been resolved. No further accusations possible.".to_string(),
                            state_delta: None,
                            footnotes: vec![],
                        },
                        player_id: ctx.player_id.to_string(),
                    },
                    GameMessage::NarrationEnd {
                        payload: NarrationEndPayload { state_delta: None },
                        player_id: ctx.player_id.to_string(),
                    },
                ]);
            }
        }

        let accusation = sidequest_game::Accusation::new(
            ctx.char_name.to_string(),
            accused_npc_name.clone(),
            stated_reason,
        );

        // Clone NPCs to avoid split borrow (scenario_state is &mut, npcs is &).
        let npcs_snapshot = ctx.snapshot.npcs.clone();
        if let Some(ref mut scenario) = ctx.snapshot.scenario_state {
            let result: sidequest_game::AccusationResult =
                scenario.handle_accusation(&accusation, &npcs_snapshot);

            WatcherEventBuilder::new("scenario", WatcherEventType::StateTransition)
                .field("event", "scenario.accusation_resolved")
                .field("accused", &accused_npc_name)
                .field("is_correct", result.is_correct)
                .field("quality", format!("{:?}", result.quality))
                .send();

            // Story 35-3: Score the scenario after accusation resolution.
            let total_turns = ctx.turn_manager.interaction();
            let questioned: Vec<String> = scenario.questioned_npcs().iter().cloned().collect();
            let score_input = sidequest_game::ScenarioScoreInput {
                scenario_state: scenario,
                accusation_result: &result,
                total_turns,
                npcs_questioned: &questioned,
            };
            let score = sidequest_game::score_scenario(&score_input);

            WatcherEventBuilder::new("scenario", WatcherEventType::StateTransition)
                .field("event", "scenario.scored")
                .field("grade", format!("{:?}", score.grade()))
                .field(
                    "evidence_coverage",
                    format!("{:.0}%", score.evidence_coverage() * 100.0),
                )
                .field(
                    "interrogation_breadth",
                    format!("{:.0}%", score.interrogation_breadth() * 100.0),
                )
                .field(
                    "deduction_quality",
                    format!("{:?}", score.deduction_quality()),
                )
                .field("total_turns", score.total_turns())
                .send();

            let score_summary = format!(
                "\n\n**Scenario Score:** {:?} — Evidence: {:.0}%, Interrogation: {:.0}%, Deduction: {:?} ({} turns)",
                score.grade(),
                score.evidence_coverage() * 100.0,
                score.interrogation_breadth() * 100.0,
                score.deduction_quality(),
                score.total_turns(),
            );

            let text = if result.is_correct {
                format!(
                    "**ACCUSATION CORRECT!** {} has been identified as the culprit. Evidence quality: {:?}.\n\n{}{}",
                    accused_npc_name, result.quality, result.narrative_prompt, score_summary
                )
            } else {
                format!(
                    "**ACCUSATION INCORRECT.** {} is not the guilty party. Evidence quality: {:?}.\n\n{}{}",
                    accused_npc_name, result.quality, result.narrative_prompt, score_summary
                )
            };

            return Some(vec![
                GameMessage::Narration {
                    payload: NarrationPayload {
                        text,
                        state_delta: None,
                        footnotes: vec![],
                    },
                    player_id: ctx.player_id.to_string(),
                },
                GameMessage::NarrationEnd {
                    payload: NarrationEndPayload { state_delta: None },
                    player_id: ctx.player_id.to_string(),
                },
            ]);
        } else {
            return Some(vec![
                GameMessage::Narration {
                    payload: NarrationPayload {
                        text:
                            "No active scenario — /accuse is only available during scenario play."
                                .to_string(),
                        state_delta: None,
                        footnotes: vec![],
                    },
                    player_id: ctx.player_id.to_string(),
                },
                GameMessage::NarrationEnd {
                    payload: NarrationEndPayload { state_delta: None },
                    player_id: ctx.player_id.to_string(),
                },
            ]);
        }
    }

    if let Some(cmd_result) = router.try_dispatch(ctx.action, &snapshot) {
        tracing::info!(command = %ctx.action, result_type = ?std::mem::discriminant(&cmd_result), "slash_command.dispatched");
        let text = match &cmd_result {
            sidequest_game::slash_router::CommandResult::Display(t) => t.clone(),
            sidequest_game::slash_router::CommandResult::Error(e) => e.clone(),
            sidequest_game::slash_router::CommandResult::StateMutation(patch) => {
                // Apply location/region changes from /gm commands.
                if let Some(ref loc) = patch.location {
                    *ctx.current_location = loc.clone();
                }
                if let Some(ref hp_changes) = patch.hp_changes {
                    for delta in hp_changes.values() {
                        *ctx.hp = (*ctx.hp + delta).max(0);
                    }
                }
                "GM command applied.".to_string()
            }
            sidequest_game::slash_router::CommandResult::ToneChange(new_values) => {
                *ctx.axis_values = new_values.clone();
                "Tone updated.".to_string()
            }
            _ => "Command executed.".to_string(),
        };

        // Watcher: slash command handled
        WatcherEventBuilder::new("game", WatcherEventType::AgentSpanClose)
            .field("slash_command", ctx.action)
            .field("result_len", text.len())
            .send();

        return Some(vec![
            GameMessage::Narration {
                payload: NarrationPayload {
                    text,
                    state_delta: None,
                    footnotes: vec![],
                },
                player_id: ctx.player_id.to_string(),
            },
            GameMessage::NarrationEnd {
                payload: NarrationEndPayload { state_delta: None },
                player_id: ctx.player_id.to_string(),
            },
        ]);
    }

    None
}

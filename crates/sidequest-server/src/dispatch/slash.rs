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
            combat: ctx.combat_state.clone(),
            chase: ctx.chase_state.clone(),
            axis_values: ctx.axis_values.clone(),
            active_tropes: ctx.trope_states.clone(),
            quest_log: ctx.quest_log.clone(),
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
                    for (_target, delta) in hp_changes {
                        *ctx.hp = (*ctx.hp + delta).max(0);
                    }
                }
                format!("GM command applied.")
            }
            sidequest_game::slash_router::CommandResult::ToneChange(new_values) => {
                *ctx.axis_values = new_values.clone();
                format!("Tone updated.")
            }
            _ => "Command executed.".to_string(),
        };

        // Watcher: slash command handled
        WatcherEventBuilder::new("game", WatcherEventType::AgentSpanClose)
            .field("slash_command", ctx.action)
            .field("result_len", text.len())
            .send(ctx.state);

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

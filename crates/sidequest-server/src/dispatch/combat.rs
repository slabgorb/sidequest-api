//! Combat tick effects, UI overlays, and chase state tracking.
//!
//! Combat engagement/disengagement and turn mode transitions are driven entirely
//! by CombatPatch from creature_smith, applied in state_mutations.rs.
//! This module handles post-mutation tick effects and UI overlay messages.
//! No keyword/string matching — all state decisions come from typed patches.

use std::collections::HashMap;

use sidequest_protocol::{CombatEventPayload, GameMessage};

use crate::{Severity, WatcherEvent, WatcherEventType};

use super::DispatchContext;

/// Process combat tick effects, overlays, and chase state.
///
/// Called AFTER `apply_state_mutations` — combat_state and turn mode are already
/// updated from CombatPatch. This function handles tick effects and UI overlays.
#[tracing::instrument(name = "turn.combat_and_chase", skip_all)]
pub(crate) async fn process_combat_and_chase(
    ctx: &mut DispatchContext<'_>,
    _clean_narration: &str,
    _result: &sidequest_agents::orchestrator::ActionResult,
    messages: &mut Vec<GameMessage>,
    combat_just_ended: bool,
) {
    let now_in_combat = ctx.combat_state.in_combat();

    // Combat tick — tick status effects (round advancement handled by advance_turn in apply_state_mutations)
    tracing::debug!(
        in_combat = now_in_combat,
        round = ctx.combat_state.round(),
        drama_weight = ctx.combat_state.drama_weight(),
        "combat.pre_tick"
    );
    if now_in_combat {
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
    if now_in_combat || combat_just_ended {
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

    // Chase state tracking is driven by ChasePatch from the dialectician agent,
    // applied in state_mutations.rs. See Fix 3 in the keyword elimination plan.
}

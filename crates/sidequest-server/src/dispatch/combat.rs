//! Combat tick effects, UI overlays, and chase state tracking.
//!
//! Combat engagement/disengagement and turn mode transitions are driven entirely
//! by CombatPatch from creature_smith, applied in state_mutations.rs.
//! This module handles post-mutation tick effects and UI overlay messages.
//! No keyword/string matching — all state decisions come from typed patches.

use sidequest_protocol::{CombatEventPayload, GameMessage};

use crate::{WatcherEventBuilder, WatcherEventType};

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
    combat_just_started: bool,
) {
    let now_in_combat = ctx.combat_state.in_combat();

    // Combat tick — tick status effects (round advancement handled by advance_turn in apply_state_mutations)
    tracing::debug!(
        in_combat = now_in_combat,
        round = ctx.combat_state.round(),
        drama_weight = ctx.combat_state.drama_weight(),
        "combat.pre_tick"
    );

    // OTEL: combat state on every turn (so dashboard always shows current combat status)
    WatcherEventBuilder::new("combat", WatcherEventType::AgentSpanOpen)
        .field("action", "combat_tick")
        .field("in_combat", now_in_combat)
        .field("combat_just_ended", combat_just_ended)
        .field("combat_just_started", combat_just_started)
        .field("round", ctx.combat_state.round())
        .field("drama_weight", ctx.combat_state.drama_weight())
        .field("turn_order", ctx.combat_state.turn_order())
        .field("current_turn", ctx.combat_state.current_turn())
        .field("enemy_count", ctx.combat_state.turn_order().iter()
            .filter(|n| !n.eq_ignore_ascii_case(ctx.char_name))
            .count())
        .field("damage_log_len", ctx.combat_state.damage_log().len())
        .send();

    if now_in_combat {
        ctx.combat_state.tick_effects();
    }

    // OTEL: active status effects visibility
    if now_in_combat {
        let active_effects: Vec<serde_json::Value> = ctx.combat_state.turn_order().iter()
            .flat_map(|name| {
                ctx.combat_state.effects_on(name).into_iter().map(move |e| {
                    serde_json::json!({
                        "target": name,
                        "kind": format!("{:?}", e.kind()),
                        "remaining_rounds": e.remaining_rounds(),
                    })
                })
            })
            .collect();
        if !active_effects.is_empty() {
            WatcherEventBuilder::new("combat", WatcherEventType::StateTransition)
                .field("action", "status_effects_active")
                .field("effects", &active_effects)
                .field("effect_count", active_effects.len())
                .send();
        }
    }

    // Combat overlay — send populated CombatEvent with enemies, turn order, current turn
    if now_in_combat || combat_just_ended {
        // Build enemies from turn_order (actual combatants), not npc_registry (all known NPCs).
        // Look up each combatant's HP/effects from npc_registry by name match.
        // Skip the player character (they're in turn_order but aren't an "enemy").
        let enemies: Vec<sidequest_protocol::CombatEnemy> = ctx
            .combat_state
            .turn_order()
            .iter()
            .filter(|name| !name.eq_ignore_ascii_case(ctx.char_name))
            .filter_map(|name| {
                ctx.npc_registry
                    .iter()
                    .find(|entry| entry.name.eq_ignore_ascii_case(name))
                    .map(|entry| sidequest_protocol::CombatEnemy {
                        name: entry.name.clone(),
                        hp: entry.hp,
                        max_hp: entry.max_hp,
                        ac: None,
                        status_effects: ctx.combat_state.effects_on(&entry.name)
                            .iter()
                            .map(|e| sidequest_protocol::StatusEffectInfo {
                                kind: format!("{:?}", e.kind()),
                                remaining_rounds: e.remaining_rounds(),
                            })
                            .collect(),
                    })
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

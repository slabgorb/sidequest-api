//! Encounter beat selection dispatch (story 28-5).

use crate::{Severity, WatcherEventBuilder, WatcherEventType};

use super::DispatchContext;

/// Dispatch a beat selection to the encounter engine.
///
/// Routes beat_id through apply_beat() on the live StructuredEncounter,
/// then resolves stat_check-specific mechanics (attack → apply_hp_delta,
/// escape → separation metric, others → metric_delta only via apply_beat).
/// Checks resolution after apply_beat and handles escalation if needed.
///
/// Emits OTEL events: encounter.beat_dispatched, encounter.stat_check_resolved.
pub(super) fn dispatch_beat_selection(ctx: &mut DispatchContext<'_>, beat_id: &str) {
    let encounter_type = match ctx.snapshot.encounter {
        Some(ref enc) => enc.encounter_type.clone(),
        None => {
            tracing::warn!(beat_id = %beat_id, "beat_selection with no active encounter");
            return;
        }
    };

    let def = match crate::find_confrontation_def(&ctx.confrontation_defs, &encounter_type) {
        Some(d) => d.clone(),
        None => {
            tracing::warn!(
                beat_id = %beat_id,
                encounter_type = %encounter_type,
                "beat_selection: no confrontation def found"
            );
            return;
        }
    };

    // Strict match — NO label fallback, NO snake_case normalization.
    // The label_fallback was a silent fallback that fuzzy-matched narrator-emitted
    // beat labels back to IDs via `b.label.to_lowercase().replace(' ', "_")`.
    // This violated: no keyword matching (Zork Problem, ADR-010/032), no silent
    // fallbacks (CLAUDE.md × 4 repos). Deleted in confrontation wiring repair.
    // If the narrator emits an unknown beat_id, it fails loud with an OTEL warning.
    let beat = match def.beats.iter().find(|b| b.id == beat_id) {
        Some(b) => b,
        None => {
            tracing::warn!(
                beat_id = %beat_id,
                encounter_type = %encounter_type,
                available_ids = ?def.beats.iter().map(|b| &b.id).collect::<Vec<_>>(),
                "beat_selection: beat_id not found — NO FALLBACK"
            );
            WatcherEventBuilder::new("encounter", WatcherEventType::ValidationWarning)
                .field("event", "beat_id.unknown")
                .field("submitted", beat_id)
                .field("encounter_type", &encounter_type)
                .field(
                    "available_ids",
                    def.beats
                        .iter()
                        .map(|b| b.id.as_str())
                        .collect::<Vec<_>>()
                        .join(","),
                )
                .severity(Severity::Error)
                .send();
            return;
        }
    };
    let stat_check = beat.stat_check.clone();
    let metric_delta = beat.metric_delta;
    let gold_delta = beat.gold_delta;
    let resolved_beat_id = beat.id.clone();

    if let Some(gd) = gold_delta {
        ctx.inventory.gold = (ctx.inventory.gold + gd as i64).max(0);
        if let Some(ch) = ctx.snapshot.characters.first_mut() {
            ch.core.inventory.gold = ctx.inventory.gold;
        }
        WatcherEventBuilder::new("encounter", WatcherEventType::StateTransition)
            .field("event", "encounter.gold_delta")
            .field("beat_id", &resolved_beat_id)
            .field("gold_delta", gd)
            .field("gold_after", ctx.inventory.gold)
            .field("encounter_type", &encounter_type)
            .send();
        tracing::info!(
            beat_id = %resolved_beat_id,
            gold_delta = gd,
            gold_after = ctx.inventory.gold,
            "encounter.gold_delta — inventory updated"
        );
    }

    let resolver = match stat_check.to_lowercase().as_str() {
        "attack" | "strength" => "resolve_attack",
        "escape" => "escape",
        _ => "metric_delta",
    };

    WatcherEventBuilder::new("encounter", WatcherEventType::StateTransition)
        .field("event", "encounter.beat_dispatched")
        .field("beat_id", beat_id)
        .field("stat_check", &stat_check)
        .field("resolver", resolver)
        .field("encounter_type", &encounter_type)
        .send();

    if let Some(ref mut encounter) = ctx.snapshot.encounter {
        match encounter.apply_beat(beat_id, &def) {
            Ok(()) => {
                tracing::info!(
                    beat_id = %beat_id,
                    stat_check = %stat_check,
                    metric_current = encounter.metric.current,
                    resolved = encounter.resolved,
                    "encounter.beat_applied"
                );
            }
            Err(e) => {
                tracing::warn!(beat_id = %beat_id, error = %e, "encounter.beat_apply_failed");
                return;
            }
        }
    }

    match resolver {
        "resolve_attack" => {
            if metric_delta != 0 {
                tracing::info!(
                    delta = metric_delta,
                    "encounter.stat_check.resolve_attack — HP delta via encounter metric"
                );
            }
        }
        "escape" => {
            if let Some(ref encounter) = ctx.snapshot.encounter {
                tracing::info!(
                    separation = encounter.metric.current,
                    "encounter.stat_check.escape — separation metric updated"
                );
            }
        }
        _ => {}
    }

    let metric_current = ctx
        .snapshot
        .encounter
        .as_ref()
        .map(|e| e.metric.current)
        .unwrap_or(0);
    let is_resolved = ctx
        .snapshot
        .encounter
        .as_ref()
        .map(|e| e.resolved)
        .unwrap_or(false);
    WatcherEventBuilder::new("encounter", WatcherEventType::StateTransition)
        .field("event", "encounter.stat_check_resolved")
        .field("stat_check", &stat_check)
        .field("resolver", resolver)
        .field("metric_current", metric_current)
        .field("resolved", is_resolved)
        .send();

    if is_resolved {
        tracing::info!(
            encounter_type = %encounter_type,
            "encounter.resolved — checking escalation"
        );

        if let Some(ref encounter) = ctx.snapshot.encounter {
            if let Some(escalation_target) = encounter.escalation_target(&def) {
                tracing::info!(
                    escalates_to = %escalation_target,
                    "encounter.escalation_triggered"
                );
                if let Some(escalated) = encounter.escalate_to_combat() {
                    ctx.snapshot.encounter = Some(escalated);
                    WatcherEventBuilder::new("encounter", WatcherEventType::StateTransition)
                        .field("event", "encounter.escalation_started")
                        .field("from_type", &encounter_type)
                        .field("to_type", &escalation_target)
                        .send();
                } else {
                    tracing::warn!(
                        escalates_to = %escalation_target,
                        encounter_type = %encounter_type,
                        "encounter.escalation_failed — escalate_to_combat returned None"
                    );
                }
            }
        }
    }
}

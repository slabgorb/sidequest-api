//! Encounter beat selection dispatch.
//!
//! Story 28-5 introduced beat dispatch. Story 37-14 extracted the per-branch
//! decision into [`apply_beat_dispatch`] so every outcome is observable on
//! the GM panel via a single canonical `event=` field, mirroring 37-13's
//! `apply_confrontation_gate`.
//!
//! The five outcomes:
//!
//! | Variant         | Trigger                                              | Event                          |
//! |-----------------|------------------------------------------------------|--------------------------------|
//! | `Applied`       | def + beat found, `apply_beat()` Ok                  | `encounter.beat_applied`       |
//! | `NoEncounter`   | `snapshot.encounter` is `None`                       | `encounter.beat_no_encounter`  |
//! | `NoDef`         | encounter live, no `ConfrontationDef` for its type   | `encounter.beat_no_def`        |
//! | `UnknownBeatId` | def found, `beat_id` not in `def.beats`              | `encounter.beat_id.unknown`    |
//! | `ApplyFailed`   | def + beat found, `apply_beat()` returned `Err`      | `encounter.beat_apply_failed`  |
//!
//! Every event is on the `encounter` component, keyed by `event=` (not
//! `action=`), and carries `source: "narrator_beat_selection"` so the GM
//! panel can attribute it to the narrator-driven beat subsystem (distinct
//! from the structured `BeatSelection` protocol preprocessing in `lib.rs`).
//!
//! The `DispatchContext`-flavoured wrapper that adds gold-delta,
//! resolver-specific tracing, and escalation handling lives in this module
//! as [`handle_applied_side_effects`]. `dispatch/mod.rs` calls
//! `apply_beat_dispatch` directly in its beat-selection loop, then invokes
//! `handle_applied_side_effects` only on the `Applied` outcome. The direct
//! call in `dispatch/mod.rs` also satisfies the story 37-14 wiring test,
//! which scans for `apply_beat_dispatch(` at the dispatch layer.

use sidequest_game::state::GameSnapshot;
use sidequest_genre::ConfrontationDef;

use crate::{Severity, WatcherEventBuilder, WatcherEventType};

use super::DispatchContext;

/// Outcome of `apply_beat_dispatch`. One variant per observable branch.
///
/// `#[non_exhaustive]` matches the convention of `ConfrontationGateOutcome`
/// — the case matrix is expected to grow (e.g., per-actor cooldowns,
/// resource-gated beats) and downstream callers should pattern-match with a
/// wildcard arm so a new variant is a pure additive change.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BeatDispatchOutcome {
    /// def + beat found, `apply_beat()` returned `Ok` and the metric advanced.
    Applied,
    /// `snapshot.encounter` is `None`. The narrator emitted a beat with no
    /// active encounter to apply it to — the playtest 2 silent-drop case.
    NoEncounter,
    /// Encounter live, but `find_confrontation_def` returned `None` for its
    /// `encounter_type`. A configuration drift between game state and the
    /// loaded confrontation defs.
    NoDef,
    /// Def found, but `beat_id` is not present in `def.beats`. The narrator
    /// invented a beat or used a label instead of an id.
    UnknownBeatId,
    /// `apply_beat()` returned `Err` after lookup succeeded. In practice this
    /// fires when the encounter is already resolved — a state invariant
    /// violation that must reach the GM panel.
    ApplyFailed,
}

/// Apply a narrator-emitted beat selection to the live encounter.
///
/// Mutates `snapshot.encounter` only on the `Applied` outcome (via
/// `StructuredEncounter::apply_beat`). **Always emits exactly one
/// `WatcherEvent`** on the `encounter` component using the `event=` field
/// key so the GM panel's standard filter picks it up — this is the primary
/// side-effect contract of the function.
pub(crate) fn apply_beat_dispatch(
    snapshot: &mut GameSnapshot,
    beat_id: &str,
    confrontation_defs: &[ConfrontationDef],
) -> BeatDispatchOutcome {
    // Case: no active encounter. Primary 37-14 silent-drop path.
    let encounter_type = match snapshot.encounter.as_ref() {
        Some(e) => e.encounter_type.clone(),
        None => {
            tracing::warn!(beat_id = %beat_id, "beat_selection with no active encounter");
            WatcherEventBuilder::new("encounter", WatcherEventType::ValidationWarning)
                .field("event", "encounter.beat_no_encounter")
                .field("beat_id", beat_id)
                .field("source", "narrator_beat_selection")
                .severity(Severity::Warn)
                .send();
            return BeatDispatchOutcome::NoEncounter;
        }
    };

    // Case: encounter live, but no confrontation def for its type.
    let def = match crate::find_confrontation_def(confrontation_defs, &encounter_type) {
        Some(d) => d.clone(),
        None => {
            tracing::warn!(
                beat_id = %beat_id,
                encounter_type = %encounter_type,
                "beat_selection: no confrontation def found"
            );
            WatcherEventBuilder::new("encounter", WatcherEventType::ValidationWarning)
                .field("event", "encounter.beat_no_def")
                .field("beat_id", beat_id)
                .field("encounter_type", &encounter_type)
                .field("source", "narrator_beat_selection")
                .severity(Severity::Warn)
                .send();
            return BeatDispatchOutcome::NoDef;
        }
    };

    // Case: beat_id not in def.beats. Strict match — NO label fallback, NO
    // snake_case normalization (deleted in confrontation wiring repair).
    if !def.beats.iter().any(|b| b.id == beat_id) {
        tracing::warn!(
            beat_id = %beat_id,
            encounter_type = %encounter_type,
            available_ids = ?def.beats.iter().map(|b| &b.id).collect::<Vec<_>>(),
            "beat_selection: beat_id not found — NO FALLBACK"
        );
        WatcherEventBuilder::new("encounter", WatcherEventType::ValidationWarning)
            .field("event", "encounter.beat_id.unknown")
            .field("beat_id", beat_id)
            .field("encounter_type", &encounter_type)
            .field(
                "available_ids",
                def.beats
                    .iter()
                    .map(|b| b.id.as_str())
                    .collect::<Vec<_>>()
                    .join(","),
            )
            .field("source", "narrator_beat_selection")
            .severity(Severity::Error)
            .send();
        return BeatDispatchOutcome::UnknownBeatId;
    }

    // Case: lookup succeeded. Apply the beat. `apply_beat()` returns `Err`
    // only when the encounter is already resolved — the beat lookup inside
    // is redundant with the check above, but apply_beat owns its own contract.
    let encounter = snapshot
        .encounter
        .as_mut()
        .expect("encounter presence checked above");
    match encounter.apply_beat(beat_id, &def) {
        Ok(()) => {
            let metric_current = encounter.metric.current;
            let resolved = encounter.resolved;
            tracing::info!(
                beat_id = %beat_id,
                encounter_type = %encounter_type,
                metric_current = metric_current,
                resolved = resolved,
                "encounter.beat_applied"
            );
            WatcherEventBuilder::new("encounter", WatcherEventType::StateTransition)
                .field("event", "encounter.beat_applied")
                .field("beat_id", beat_id)
                .field("encounter_type", &encounter_type)
                .field("metric_current", metric_current)
                .field("resolved", resolved)
                .field("source", "narrator_beat_selection")
                .send();
            BeatDispatchOutcome::Applied
        }
        Err(e) => {
            tracing::warn!(beat_id = %beat_id, error = %e, "encounter.beat_apply_failed");
            WatcherEventBuilder::new("encounter", WatcherEventType::ValidationWarning)
                .field("event", "encounter.beat_apply_failed")
                .field("beat_id", beat_id)
                .field("encounter_type", &encounter_type)
                .field("error", e)
                .field("source", "narrator_beat_selection")
                .severity(Severity::Warn)
                .send();
            BeatDispatchOutcome::ApplyFailed
        }
    }
}

/// Post-apply side effects that fire only when `apply_beat_dispatch` returned
/// `Applied`: gold-delta inventory mutation, resolver classification, the
/// legacy `encounter.beat_dispatched` / `encounter.stat_check_resolved`
/// breadcrumbs, and escalation handling. Split from `apply_beat_dispatch`
/// because these touch the broader `DispatchContext` (inventory, characters)
/// while the helper stays narrow enough to unit-test with a minimal fixture.
///
/// Caller contract: only invoke when the outcome was `Applied`. The function
/// assumes `snapshot.encounter`, the confrontation def, and the beat all
/// exist — violations will `expect`-panic.
pub(super) fn handle_applied_side_effects(ctx: &mut DispatchContext<'_>, beat_id: &str) {
    let encounter_type = ctx
        .snapshot
        .encounter
        .as_ref()
        .map(|e| e.encounter_type.clone())
        .expect("handle_applied_side_effects: encounter must be present on Applied");
    let def = crate::find_confrontation_def(&ctx.confrontation_defs, &encounter_type)
        .expect("handle_applied_side_effects: def must exist on Applied")
        .clone();
    let beat = def
        .beats
        .iter()
        .find(|b| b.id == beat_id)
        .expect("handle_applied_side_effects: beat must exist on Applied");
    let stat_check = beat.stat_check.clone();
    let metric_delta = beat.metric_delta;
    let gold_delta = beat.gold_delta;
    let resolved_beat_id = beat.id.clone();

    // Gold delta: mutate inventory and emit a ledger event.
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

    // Legacy pre-37-14 breadcrumb event. Order is different now — the
    // canonical `encounter.beat_applied` event fires first inside the helper
    // — but downstream tooling may still key on this name for "dispatch was
    // attempted" telemetry, so it's preserved.
    WatcherEventBuilder::new("encounter", WatcherEventType::StateTransition)
        .field("event", "encounter.beat_dispatched")
        .field("beat_id", beat_id)
        .field("stat_check", &stat_check)
        .field("resolver", resolver)
        .field("encounter_type", &encounter_type)
        .send();

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
                        .field("source", "narrator_beat_selection")
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

//! Encounter beat selection dispatch.
//!
//! Story 28-5 introduced beat dispatch. Story 37-14 extracted the per-branch
//! decision into [`apply_beat_dispatch`] so every outcome is observable on
//! the GM panel via a single canonical `event=` field, mirroring 37-13's
//! `apply_confrontation_gate`.
//!
//! The outcomes:
//!
//! | Variant            | Trigger                                              | Event                                 |
//! |--------------------|------------------------------------------------------|---------------------------------------|
//! | `Applied { .. }`   | def + beat found, encounter unresolved, apply OK     | `encounter.beat_applied`              |
//! | `NoEncounter`      | `snapshot.encounter` is `None`                       | `encounter.beat_no_encounter`         |
//! | `NoDef`            | encounter live, no `ConfrontationDef` for its type   | `encounter.beat_no_def`                |
//! | `UnknownBeatId`    | def found, `beat_id` not in `def.beats`              | `encounter.beat_id.unknown`           |
//! | `SkippedResolved`  | encounter already resolved, beat is a legitimate \   | `encounter.beat_skipped_resolved`     |
//! |                    | multi-actor post-resolution emission (not an error)  |                                       |
//! | `ApplyFailed`      | defensive fallback — `apply_beat()` returned `Err` \ | `encounter.beat_apply_failed`         |
//! |                    | despite pre-validation (unreachable today, retained \|                                       |
//! |                    | as a safety net for future Err causes)               |                                       |
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BeatDispatchOutcome {
    /// def + beat found, encounter unresolved, `apply_beat()` returned `Ok`.
    ///
    /// Carries the already-resolved `encounter_type` and `beat_id` so the
    /// post-apply wrapper (`handle_applied_side_effects`) can consume them
    /// without re-deriving the same data from `DispatchContext`. Together
    /// with the short-circuit for pre-resolved encounters, this collapses
    /// the old triple-`.expect()` lookup chain.
    Applied {
        /// The encounter_type the beat was applied to.
        encounter_type: String,
        /// The beat_id that was applied.
        beat_id: String,
    },
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
    /// Encounter is already resolved. This is a legitimate multi-actor
    /// post-resolution emission — e.g., the narrator emits a player beat
    /// that resolves combat, then emits an NPC beat against the now-resolved
    /// encounter in the same turn. The old 37-14 implementation classified
    /// this as `ApplyFailed` which surfaced on the GM panel as an error;
    /// that was a regression against legitimate turn sequences, and the
    /// short-circuit here fixes it.
    SkippedResolved,
    /// Defensive fallback: `apply_beat()` returned `Err` despite the
    /// `SkippedResolved` short-circuit and the beat_id pre-validation.
    ///
    /// This variant is **currently unreachable** — the two pre-checks above
    /// exhaust the Err causes `StructuredEncounter::apply_beat` knows about
    /// (unresolved + beat present in def). It is retained as a non-fatal
    /// safety net so a future Err cause (e.g., a resource-gated beat) can
    /// be observed rather than panicking on `.expect()`. Tests do not cover
    /// it; if a concrete Err cause is added, write a regression test then.
    ApplyFailed,
}

/// Apply a narrator-emitted beat selection to the live encounter.
///
/// Mutates `snapshot.encounter` only on the `Applied` outcome (via
/// `StructuredEncounter::apply_beat`). **Always emits exactly one
/// `WatcherEvent`** on the `encounter` component using the `event=` field
/// key so the GM panel's standard filter picks it up — this is the primary
/// side-effect contract of the function.
pub fn apply_beat_dispatch(
    snapshot: &mut GameSnapshot,
    beat_id: &str,
    confrontation_defs: &[ConfrontationDef],
) -> BeatDispatchOutcome {
    // Case NoEncounter: no active encounter. Primary 37-14 silent-drop path.
    let (encounter_type, is_resolved) = match snapshot.encounter.as_ref() {
        Some(e) => (e.encounter_type.clone(), e.resolved),
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

    // Case NoDef: encounter live, but no confrontation def for its type.
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

    // Case UnknownBeatId: beat_id not in def.beats. Strict match — NO label
    // fallback, NO snake_case normalization (deleted in confrontation wiring
    // repair).
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

    // Case SkippedResolved: encounter already resolved. Short-circuit BEFORE
    // calling apply_beat. This is the pass-2 fix for the pass-1 regression
    // where legitimate multi-actor turns (player beat resolves encounter,
    // narrator emits NPC beat against the resolved encounter in the same
    // turn) fired the misleading `encounter.beat_apply_failed` event.
    // `encounter.beat_skipped_resolved` is a normal end-of-encounter
    // condition, not a narrator error, so it uses Severity::Warn (not Error).
    if is_resolved {
        tracing::info!(
            beat_id = %beat_id,
            encounter_type = %encounter_type,
            "beat_selection on already-resolved encounter — skipping"
        );
        WatcherEventBuilder::new("encounter", WatcherEventType::ValidationWarning)
            .field("event", "encounter.beat_skipped_resolved")
            .field("beat_id", beat_id)
            .field("encounter_type", &encounter_type)
            .field("source", "narrator_beat_selection")
            .severity(Severity::Warn)
            .send();
        return BeatDispatchOutcome::SkippedResolved;
    }

    // Case Applied: all pre-conditions satisfied. Apply the beat. After the
    // NoEncounter/NoDef/UnknownBeatId/SkippedResolved short-circuits, the
    // two Err causes known to `StructuredEncounter::apply_beat` (unresolved
    // check + beat lookup) are already exhausted — the match below keeps a
    // defensive Err arm that emits `encounter.beat_apply_failed` for any
    // future Err cause so we never panic in prod, but the arm is currently
    // unreachable.
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
            BeatDispatchOutcome::Applied {
                encounter_type,
                beat_id: beat_id.to_string(),
            }
        }
        Err(e) => {
            tracing::warn!(
                beat_id = %beat_id,
                error = %e,
                "apply_beat unexpected Err after pre-validation — defensive fallback"
            );
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
/// Caller contract: only invoke when `apply_beat_dispatch` returned
/// `BeatDispatchOutcome::Applied { encounter_type, beat_id }`, and pass
/// both carried fields through. `encounter_type` and `beat_id` are the
/// already-resolved values from that variant, so this function does NOT
/// re-derive them from `DispatchContext`. The def and beat lookups below
/// are guaranteed to succeed because `apply_beat_dispatch` already
/// validated both on the Applied path.
pub(super) fn handle_applied_side_effects(
    ctx: &mut DispatchContext<'_>,
    encounter_type: &str,
    beat_id: &str,
) {
    let def = crate::find_confrontation_def(&ctx.confrontation_defs, encounter_type)
        .expect(
            "handle_applied_side_effects: def must exist (guaranteed by Applied outcome)",
        )
        .clone();
    let beat = def
        .beats
        .iter()
        .find(|b| b.id == beat_id)
        .expect(
            "handle_applied_side_effects: beat must exist (guaranteed by Applied outcome)",
        );
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
            .field("encounter_type", encounter_type)
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
        .field("encounter_type", encounter_type)
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
                        .field("from_type", encounter_type)
                        .field("to_type", &escalation_target)
                        .field("source", "narrator_beat_selection")
                        .send();
                } else {
                    tracing::warn!(
                        escalates_to = %escalation_target,
                        encounter_type = %encounter_type,
                        "encounter.escalation_failed — escalate_to_combat returned None"
                    );
                    // Fix #2: the failure path used to be tracing-only —
                    // a silent drop on the GM panel, which is exactly the
                    // anti-pattern this story exists to eliminate. This
                    // emission mirrors the escalation_started success
                    // event so the panel can diff the two cases.
                    WatcherEventBuilder::new("encounter", WatcherEventType::ValidationWarning)
                        .field("event", "encounter.escalation_failed")
                        .field("from_type", encounter_type)
                        .field("to_type", &escalation_target)
                        .field("source", "narrator_beat_selection")
                        .severity(Severity::Error)
                        .send();
                }
            }
        }
    }
}

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
//!
//! Every non-Applied outcome emits exactly one `WatcherEvent` on the
//! `encounter` component, keyed by `event=` (not `action=`), carrying
//! `source: "narrator_beat_selection"` so the GM panel can attribute it to
//! the narrator-driven beat subsystem (distinct from the structured
//! `BeatSelection` protocol preprocessing in `lib.rs`). On the Applied
//! outcome, `apply_beat_dispatch` emits the dispatch-layer
//! `encounter.beat_applied` AND additionally triggers
//! `StructuredEncounter::apply_beat` inside the game crate, which emits its
//! own state-machine-layer events (`encounter.state.beat_applied`, and
//! conditionally `encounter.state.resolved` /
//! `encounter.state.phase_transition`) on the same component. Operators
//! reading the GM panel see BOTH layers — the `state.` prefix
//! disambiguates.
//!
//! The `DispatchContext`-flavoured wrapper that adds gold-delta,
//! resolver-specific tracing, and escalation handling lives in this module
//! as [`handle_applied_side_effects`]. `dispatch/mod.rs` calls
//! `apply_beat_dispatch` directly in its beat-selection loop, then invokes
//! `handle_applied_side_effects` only on the `Applied` outcome. The direct
//! call in `dispatch/mod.rs` also satisfies the story 37-14 wiring test,
//! which scans for `apply_beat_dispatch(` at the dispatch layer.

use sidequest_game::state::GameSnapshot;
use sidequest_genre::{BeatDef, ConfrontationDef};

use crate::{Severity, WatcherEventBuilder, WatcherEventType};

use super::DispatchContext;

/// Story 39-4: outcome of applying the beat-driven edge and resource
/// deltas carried on a `BeatDef`. Returned by `apply_beat_edge_deltas`
/// so `handle_applied_side_effects` (and integration tests) can observe
/// whether the beat resolved the encounter via composure break.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EdgeDeltaOutcome {
    /// Acting character's new edge.current after `beat.edge_delta` was
    /// applied, or `None` if the beat carried no self-debit.
    pub self_new_current: Option<i32>,
    /// Primary opponent's new edge.current after `beat.target_edge_delta`
    /// was applied, or `None` if the beat carried no target-debit.
    pub target_new_current: Option<i32>,
    /// `true` when either the acting character or the primary opponent
    /// was driven to `edge.current <= 0`, which auto-resolves the
    /// encounter and emits `encounter.composure_break`.
    pub composure_break: bool,
}

/// Story 39-4: apply a beat's `edge_delta`, `target_edge_delta`, and
/// `resource_deltas` to the snapshot.
///
/// Called from `handle_applied_side_effects` on the Applied outcome.
/// Mutates `snapshot.characters[0].core.edge` on self-debit, the primary
/// opponent's `core.edge` on target-debit (first `EncounterActor` with
/// `role == "opponent"`), and `snapshot.resources[name]` for each entry
/// in `resource_deltas`. Sets `snapshot.encounter.resolved = true` on
/// composure break.
///
/// **No silent fallbacks:** if `target_edge_delta` is set but the
/// encounter has no actor with `role == "opponent"`, this function
/// panics. A silent skip would mask a configuration bug — the narrator
/// emitted a target-debit beat into an encounter that has no target.
///
/// **Telemetry:** emits `creature.edge_delta` on each debit (with
/// `source=beat`, `beat_id`, `delta`, `new_current`, `encounter_type`)
/// and `encounter.composure_break` when `edge <= 0` on either side.
pub fn apply_beat_edge_deltas(
    snapshot: &mut GameSnapshot,
    beat: &BeatDef,
    encounter_type: &str,
) -> EdgeDeltaOutcome {
    let mut outcome = EdgeDeltaOutcome::default();
    let mut broken: Option<String> = None;

    // Self-debit — acting character is snapshot.characters[0] (dispatch
    // convention; the builder and dispatch loop keep the acting PC in
    // slot 0).
    if let Some(delta) = beat.edge_delta {
        let (name, new_current) = {
            let ch = snapshot
                .characters
                .first_mut()
                .expect("apply_beat_edge_deltas: self-debit requires an acting character in snapshot.characters[0]");
            let result = ch.core.edge.apply_delta(-delta);
            (ch.core.name.as_str().to_string(), result.new_current)
        };
        emit_edge_delta_event(&name, &beat.id, -delta, new_current, encounter_type);
        outcome.self_new_current = Some(new_current);
        if new_current <= 0 {
            outcome.composure_break = true;
            broken = Some(name);
        }
    }

    // Target-debit — find the primary opponent (first actor with
    // role="opponent"). No silent fallback: if target_edge_delta is set
    // but no opponent is declared, panic.
    if let Some(delta) = beat.target_edge_delta {
        let opponent_name = snapshot
            .encounter
            .as_ref()
            .and_then(|e| e.actors.iter().find(|a| a.role == "opponent"))
            .map(|a| a.name.clone())
            .expect(
                "apply_beat_edge_deltas: beat.target_edge_delta set but no primary opponent \
                 (actor with role=\"opponent\") found in encounter — no silent fallback",
            );
        let npc = snapshot
            .npcs
            .iter_mut()
            .find(|n| n.core.name.as_str() == opponent_name)
            .unwrap_or_else(|| {
                panic!(
                    "apply_beat_edge_deltas: primary opponent \"{}\" named in encounter.actors \
                     is not present in snapshot.npcs — no silent fallback",
                    opponent_name
                )
            });
        let result = npc.core.edge.apply_delta(-delta);
        let new_current = result.new_current;
        emit_edge_delta_event(&opponent_name, &beat.id, -delta, new_current, encounter_type);
        outcome.target_new_current = Some(new_current);
        if new_current <= 0 {
            outcome.composure_break = true;
            broken = Some(opponent_name);
        }
    }

    // Resource deltas — route through the existing ResourcePool path.
    // Silently unknown resources emit a ValidationWarning (same policy
    // as the narrator-emitted path in state_mutations.rs).
    if let Some(ref deltas) = beat.resource_deltas {
        for (resource_name, delta) in deltas {
            let op = if *delta >= 0.0 {
                sidequest_game::ResourcePatchOp::Add
            } else {
                sidequest_game::ResourcePatchOp::Subtract
            };
            let value = delta.abs();
            match snapshot.apply_resource_patch_by_name(resource_name, op, value) {
                Ok(patch) => {
                    WatcherEventBuilder::new(
                        "resource_pool",
                        WatcherEventType::StateTransition,
                    )
                    .field("event", "resource_pool.patched")
                    .field("resource", resource_name.as_str())
                    .field("delta", delta)
                    .field("old_value", patch.old_value)
                    .field("new_value", patch.new_value)
                    .field("source", "beat")
                    .field("beat_id", &beat.id)
                    .send();
                }
                Err(e) => {
                    WatcherEventBuilder::new(
                        "resource_pool",
                        WatcherEventType::ValidationWarning,
                    )
                    .field("event", "resource_pool.delta_rejected")
                    .field("resource", resource_name.as_str())
                    .field("delta", delta)
                    .field("source", "beat")
                    .field("beat_id", &beat.id)
                    .field("error", format!("{e}"))
                    .send();
                }
            }
        }
    }

    if outcome.composure_break {
        if let Some(enc) = snapshot.encounter.as_mut() {
            enc.resolved = true;
        }
        WatcherEventBuilder::new("encounter", WatcherEventType::StateTransition)
            .field("event", "encounter.composure_break")
            .field("beat_id", &beat.id)
            .field("encounter_type", encounter_type)
            .field("broken", broken.as_deref().unwrap_or(""))
            .field("source", "beat")
            .send();
    }

    outcome
}

fn emit_edge_delta_event(
    name: &str,
    beat_id: &str,
    delta: i32,
    new_current: i32,
    encounter_type: &str,
) {
    WatcherEventBuilder::new("creature", WatcherEventType::StateTransition)
        .field("event", "creature.edge_delta")
        .field("source", "beat")
        .field("name", name)
        .field("beat_id", beat_id)
        .field("delta", delta)
        .field("new_current", new_current)
        .field("encounter_type", encounter_type)
        .send();
}

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
    /// encounter in the same turn. Short-circuits before `NoDef` and
    /// `UnknownBeatId` because the encounter is done — no further validation
    /// of the incoming beat is meaningful, and firing a Severity::Error
    /// `UnknownBeatId` on a resolved encounter would be the same class of
    /// false alarm the pass-1 `beat_apply_failed` regression produced.
    SkippedResolved,
}

/// Apply a narrator-emitted beat selection to the live encounter.
///
/// Mutates `snapshot.encounter` only on the `Applied` outcome (via
/// `StructuredEncounter::apply_beat`).
///
/// **Emits exactly one `WatcherEvent` on the `encounter` component for
/// every non-Applied outcome**, keyed by `event=` so the GM panel's
/// standard filter picks it up. On the `Applied` outcome, this function
/// emits the dispatch-layer `encounter.beat_applied` AND additionally
/// triggers `StructuredEncounter::apply_beat` inside the game crate, which
/// emits its own state-machine-layer events
/// (`encounter.state.beat_applied`, plus conditionally
/// `encounter.state.resolved` / `encounter.state.phase_transition`) on the
/// same `encounter` component. The `state.` prefix disambiguates the two
/// layers on the GM panel — a single Applied beat produces at least two
/// `encounter` WatcherEvents, one from each layer.
///
/// **Outcome ordering (higher priority → lower):**
/// 1. `NoEncounter` — `snapshot.encounter` is `None`.
/// 2. `SkippedResolved` — encounter live AND already resolved. Highest-priority
///    short-circuit among the encounter-live branches: once an encounter is
///    done, every incoming beat is skipped regardless of beat_id or def
///    validity. This ordering closes the Reviewer pass-2 finding #1: if
///    `UnknownBeatId` fired before `SkippedResolved`, a narrator beat with
///    a hallucinated beat_id against a resolved encounter would produce a
///    false `Severity::Error` on a normal end-of-encounter condition.
/// 3. `NoDef` — encounter live, unresolved, but no `ConfrontationDef` for
///    its type (config drift).
/// 4. `UnknownBeatId` — def found, unresolved, but `beat_id` not in
///    `def.beats`.
/// 5. `Applied` — all preconditions satisfied, `apply_beat()` returned `Ok`.
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

    // Case SkippedResolved: encounter is already resolved. HIGHEST PRIORITY
    // short-circuit among the encounter-live branches. This must fire BEFORE
    // NoDef and UnknownBeatId — if either of those fired first on a resolved
    // encounter, the GM panel would get a false-alarm error on a legitimate
    // multi-actor post-resolution turn (Reviewer pass-2 finding #1). A
    // resolved encounter means every incoming beat is skipped regardless of
    // further validation; the beat_id and def lookups are moot.
    //
    // Severity::Warn (not Error) + tracing::warn! (not info!) — normal
    // end-of-encounter condition, but still anomalous enough the operator
    // should see it on both the log stream and the GM panel.
    if is_resolved {
        tracing::warn!(
            beat_id = %beat_id,
            encounter_type = %encounter_type,
            "beat_selection on already-resolved encounter — skipping"
        );
        WatcherEventBuilder::new("encounter", WatcherEventType::ValidationWarning)
            .field("event", "encounter.beat_skipped_resolved")
            .field("beat_id", beat_id)
            .field("encounter_type", &encounter_type)
            .field("resolved", true)
            .field("source", "narrator_beat_selection")
            .severity(Severity::Warn)
            .send();
        return BeatDispatchOutcome::SkippedResolved;
    }

    // Case NoDef: encounter live (unresolved), but no confrontation def for
    // its type. Config drift.
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
    //
    // Severity::Warn + tracing::warn! — narrator bad input is 4xx-class
    // (client error), not a 5xx-class server error. Reviewer pass-2 finding
    // #6: the previous Severity::Error + tracing::warn! split lied to the
    // log-stream consumer about the severity class.
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
            .severity(Severity::Warn)
            .send();
        return BeatDispatchOutcome::UnknownBeatId;
    }

    // Case Applied: all pre-conditions satisfied. Apply the beat.
    //
    // `StructuredEncounter::apply_beat` currently has exactly two `Err`
    // causes, both exhausted by the short-circuits above:
    //   1. `if self.resolved { return Err(...) }` — exhausted by
    //      `SkippedResolved` (fires before we reach this match).
    //   2. `def.beats.find(b.id == beat_id).ok_or_else(...)` — exhausted
    //      by the `UnknownBeatId` check above (same predicate).
    //
    // The `.expect()` here is therefore a contract assertion, not a silent
    // fallback. **This is a runtime panic, not a compile-time exhaustiveness
    // check** — if a future `apply_beat()` grows a third `Err` cause, this
    // `.expect()` will panic in production on the first hit. The signal to
    // add a new `BeatDispatchOutcome` variant is (a) this panic landing in
    // logs, or (b) the regression-guard test
    // `wiring_apply_beat_err_causes_exhausted_by_short_circuits` below
    // failing — whichever the developer sees first. `#[non_exhaustive]` on
    // `BeatDispatchOutcome` keeps the variant addition non-breaking for
    // downstream consumers.
    //
    // A stronger design would convert `apply_beat`'s error to a
    // `#[non_exhaustive]` `thiserror`-derived enum and `match` on it with
    // no wildcard arm, making the "new cause requires new variant" rule a
    // compile-time error rather than a runtime panic. That is tracked as a
    // non-blocking delivery finding from Reviewer pass 2 (EncounterApplyError
    // conversion) — the current shape is acceptable because the regression
    // guard test below mechanically verifies the two known causes remain
    // the only two causes.
    let encounter = snapshot
        .encounter
        .as_mut()
        .expect("encounter presence checked above");
    encounter.apply_beat(beat_id, &def).expect(
        "apply_beat: all preconditions validated above \
             (SkippedResolved exhausts `self.resolved`; \
             UnknownBeatId exhausts `def.beats` lookup). \
             A new Err cause requires a new BeatDispatchOutcome variant \
             AND a corresponding short-circuit above this match.",
    );
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
        .expect("handle_applied_side_effects: def must exist (guaranteed by Applied outcome)")
        .clone();
    let beat = def
        .beats
        .iter()
        .find(|b| b.id == beat_id)
        .expect("handle_applied_side_effects: beat must exist (guaranteed by Applied outcome)");
    let stat_check = beat.stat_check.clone();
    let metric_delta = beat.metric_delta;
    let gold_delta = beat.gold_delta;
    let resolved_beat_id = beat.id.clone();
    let beat_for_edge = beat.clone();

    // Story 39-4: beat-driven edge and resource deltas. Runs before the
    // gold-delta block so a composure break (encounter auto-resolve) is
    // visible to downstream escalation checks below.
    apply_beat_edge_deltas(ctx.snapshot, &beat_for_edge, encounter_type);

    // Gold delta: mutate inventory and emit a ledger event.
    if let Some(gd) = gold_delta {
        let gold_before = ctx.inventory.gold;
        if gd >= 0 {
            // Gold gain — always valid.
            ctx.inventory.gold += gd as i64;
            if let Some(ch) = ctx.snapshot.characters.first_mut() {
                ch.core.inventory.gold = ctx.inventory.gold;
            }
            WatcherEventBuilder::new("encounter", WatcherEventType::StateTransition)
                .field("event", "encounter.gold_delta")
                .field("beat_id", &resolved_beat_id)
                .field("gold_delta", gd)
                .field("gold_before", gold_before)
                .field("gold_after", ctx.inventory.gold)
                .field("encounter_type", encounter_type)
                .send();
            tracing::info!(
                beat_id = %resolved_beat_id,
                gold_delta = gd,
                gold_after = ctx.inventory.gold,
                "encounter.gold_delta — gold gained"
            );
        } else {
            // Gold loss — must not exceed balance.
            let spend_amount = (gd as i64).unsigned_abs() as i64;
            match ctx.inventory.spend_gold(spend_amount) {
                Ok(_spent) => {
                    if let Some(ch) = ctx.snapshot.characters.first_mut() {
                        ch.core.inventory.gold = ctx.inventory.gold;
                    }
                    WatcherEventBuilder::new("encounter", WatcherEventType::StateTransition)
                        .field("event", "encounter.gold_delta")
                        .field("beat_id", &resolved_beat_id)
                        .field("gold_delta", gd)
                        .field("gold_before", gold_before)
                        .field("gold_after", ctx.inventory.gold)
                        .field("encounter_type", encounter_type)
                        .send();
                    tracing::info!(
                        beat_id = %resolved_beat_id,
                        gold_delta = gd,
                        gold_after = ctx.inventory.gold,
                        "encounter.gold_delta — gold spent"
                    );
                }
                Err(e) => {
                    WatcherEventBuilder::new("encounter", WatcherEventType::ValidationWarning)
                        .field("event", "encounter.gold_overspend_rejected")
                        .field("beat_id", &resolved_beat_id)
                        .field("gold_delta", gd)
                        .field("gold_before", gold_before)
                        .field("error", format!("{e}"))
                        .field("encounter_type", encounter_type)
                        .send();
                    tracing::warn!(
                        beat_id = %resolved_beat_id,
                        gold_delta = gd,
                        gold_before,
                        error = %e,
                        "encounter.gold_overspend_rejected — beat requested more gold than available"
                    );
                }
            }
        }
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

    // Caller contract: only invoked on Applied outcome, so the encounter
    // is guaranteed present. Use `.expect()` to make contract violations a
    // loud crash rather than silent zero-default telemetry (Reviewer pass-2
    // finding #2 — `.unwrap_or(0)` / `.unwrap_or(false)` here would silently
    // skip the escalation branch below and emit a misleading
    // `metric_current=0 resolved=false` event if the contract ever broke).
    let encounter_after = ctx.snapshot.encounter.as_ref().expect(
        "handle_applied_side_effects: encounter must be present (guaranteed by Applied outcome)",
    );
    let metric_current = encounter_after.metric.current;
    let is_resolved = encounter_after.resolved;
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

        // The encounter was proven present at the top of this function via
        // `.expect()` — reuse that invariant rather than a redundant
        // `if let Some` guard that could silently drop the escalation check
        // if a future refactor nulls `ctx.snapshot.encounter` between the
        // .expect() at the top and here. Structurally consistent with the
        // pass-2 finding #2 fix at the top of this function — panic loudly
        // on contract violation, never silently skip observability.
        let encounter = ctx.snapshot.encounter.as_ref().expect(
            "handle_applied_side_effects: encounter still present at escalation check \
             (invariant established by the .expect() at the top of this function)",
        );
        {
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

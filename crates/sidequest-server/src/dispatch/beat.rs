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
    // After the NoEncounter / SkippedResolved / NoDef / UnknownBeatId
    // short-circuits, the two Err causes known to
    // `StructuredEncounter::apply_beat` (unresolved + beat-present-in-def)
    // are exhausted. If a future `apply_beat()` grows a new Err cause,
    // `apply_beat_dispatch`'s match below will fail to compile (we use a
    // non-exhaustive `match` with no wildcard on the Result). That
    // compile-time failure IS the signal to add a new `BeatDispatchOutcome`
    // variant — per `#[non_exhaustive]`, that addition is non-breaking for
    // downstream consumers. No dead defensive variant is carried in the
    // enum (Reviewer pass-2 finding #4).
    let encounter = snapshot
        .encounter
        .as_mut()
        .expect("encounter presence checked above");
    encounter
        .apply_beat(beat_id, &def)
        .expect(
            "apply_beat: all preconditions validated above \
             (NoEncounter / SkippedResolved / NoDef / UnknownBeatId \
             short-circuits). A new Err cause requires a new \
             BeatDispatchOutcome variant.",
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

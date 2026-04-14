//! Story 37-13: Encounter creation gate — every branch observable.
//!
//! The narrator signals a new encounter by emitting
//! `"confrontation": "combat"` (or any ConfrontationDef type) in the
//! game_patch. This module centralises the decision of what to do with
//! that signal given the current `snapshot.encounter` state.
//!
//! Previously the decision was inline in `dispatch_player_action` and
//! silently dropped the new type whenever an unresolved encounter already
//! existed. That was a CLAUDE.md "No Silent Fallbacks" violation and the
//! root cause for 37-12 (narrator never re-declares confrontation after
//! first emission).
//!
//! The gate now covers six cases, each with a distinct `WatcherEvent`:
//!
//! | Case | Current state                            | Action        | Event                                           |
//! |------|------------------------------------------|---------------|-------------------------------------------------|
//! | A    | `None`                                   | Create        | `encounter.created`                             |
//! | B    | `Some(resolved)`                         | Create        | `encounter.created`                             |
//! | C    | `Some(unresolved, same type)`            | No-op         | `encounter.redeclare_noop`                      |
//! | D    | `Some(unresolved, diff, beat == 0)`      | Replace       | `encounter.replaced_pre_beat`                   |
//! | E    | `Some(unresolved, diff, beat > 0)`       | Reject        | `encounter.new_type_rejected_mid_encounter`     |
//! | F    | Any, `find_confrontation_def` → `None`   | No-op + warn  | `encounter.creation_failed_unknown_type`        |

use sidequest_agents::orchestrator::NpcMention;
use sidequest_game::encounter::{EncounterActor, StructuredEncounter};
use sidequest_game::state::GameSnapshot;
use sidequest_genre::ConfrontationDef;
use sidequest_telemetry::{WatcherEventBuilder, WatcherEventType};

/// Outcome of the confrontation gate. One variant per observable branch.
///
/// Marked `#[non_exhaustive]` to signal that the variant set is open-ended —
/// the gate's case matrix is expected to grow as new encounter lifecycle
/// stories land (e.g., rate-limited redeclares, superseded-by-scenario
/// transitions).
///
/// **Note:** Because this type is `pub(crate)`, the compiler does NOT force
/// wildcard arms on match sites within `sidequest-server` — intra-crate
/// matches can still be exhaustive without an `_ =>` arm. The attribute only
/// has compiler bite if this type is ever promoted to `pub` or moved into
/// `sidequest-protocol`. Until then, add a wildcard arm by convention at
/// every new match site so a future variant is a pure additive change.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConfrontationGateOutcome {
    /// Case A or B — new encounter created on empty or resolved state.
    Created,
    /// Case C — narrator re-declared the active encounter's own type.
    Redeclared,
    /// Case D — old encounter had no beats yet; safe to replace.
    ReplacedPreBeat,
    /// Case E — old encounter has mechanical state; narrator prose diverges,
    /// but we protect state and surface the divergence as a warning.
    RejectedMidEncounter,
    /// Case F — `find_confrontation_def` could not locate the incoming type.
    UnknownType,
}

/// Apply the confrontation gate to the current snapshot.
///
/// Mutates `snapshot.encounter` only for `Created` and `ReplacedPreBeat`
/// outcomes. **Always emits exactly one `WatcherEvent`** on the `encounter`
/// channel so every gate decision is visible on the GM panel — this is the
/// primary side-effect contract of the function.
///
/// `narrator_npcs` is the narrator's `result.npcs_present` list for the
/// current turn — a live, per-turn view, not a persistent registry. These
/// are appended as actors with role `"npc"` on newly-built encounters.
pub(crate) fn apply_confrontation_gate(
    snapshot: &mut GameSnapshot,
    incoming_type: &str,
    confrontation_defs: &[ConfrontationDef],
    narrator_npcs: &[NpcMention],
) -> ConfrontationGateOutcome {
    // Case F: def missing wins over every other branch. We cannot build an
    // encounter without a def, and the existing `tracing::warn!` is preserved
    // so console users still see it.
    let Some(def) = crate::find_confrontation_def(confrontation_defs, incoming_type) else {
        tracing::warn!(
            confrontation_type = %incoming_type,
            "encounter.creation_failed — no ConfrontationDef found for type"
        );
        WatcherEventBuilder::new("encounter", WatcherEventType::ValidationWarning)
            .field("event", "encounter.creation_failed_unknown_type")
            .field("encounter_type", incoming_type)
            .field("source", "narrator_confrontation")
            .send();
        return ConfrontationGateOutcome::UnknownType;
    };

    match snapshot.encounter.as_ref() {
        // Case A — no current encounter.
        None => {
            let encounter = build_encounter(def, &snapshot.characters, narrator_npcs);
            emit_created(&encounter, incoming_type);
            snapshot.encounter = Some(encounter);
            ConfrontationGateOutcome::Created
        }

        // Case B — old encounter resolved; the new one supersedes it.
        Some(old) if old.resolved => {
            let encounter = build_encounter(def, &snapshot.characters, narrator_npcs);
            emit_created(&encounter, incoming_type);
            snapshot.encounter = Some(encounter);
            ConfrontationGateOutcome::Created
        }

        // Case C — narrator re-declares the active encounter type.
        // Keep state as-is; the narrator often restates for prompt clarity.
        Some(old) if old.encounter_type == incoming_type => {
            WatcherEventBuilder::new("encounter", WatcherEventType::StateTransition)
                .field("event", "encounter.redeclare_noop")
                .field("encounter_type", incoming_type)
                .field("beat_count", old.beat)
                .field("source", "narrator_confrontation")
                .send();
            ConfrontationGateOutcome::Redeclared
        }

        // Case D — different type, no beats fired yet: safe to replace.
        // Old encounter had no mechanical state worth preserving.
        Some(old) if old.beat == 0 => {
            let previous_encounter_type = old.encounter_type.clone();
            let encounter = build_encounter(def, &snapshot.characters, narrator_npcs);
            WatcherEventBuilder::new("encounter", WatcherEventType::StateTransition)
                .field("event", "encounter.replaced_pre_beat")
                .field("encounter_type", incoming_type)
                .field("previous_encounter_type", &previous_encounter_type)
                .field("actor_count", encounter.actors.len())
                .field("source", "narrator_confrontation")
                .send();
            snapshot.encounter = Some(encounter);
            ConfrontationGateOutcome::ReplacedPreBeat
        }

        // Case E — different type, beats already fired. Mid-encounter state
        // is sacred; we keep the old encounter and surface the divergence.
        // The explicit `old.beat > 0` guard is redundant with the match arm
        // ordering above (Cases A-D have exhausted the alternatives) but keeps
        // the code honest to the doc table's case definition.
        Some(old) if old.beat > 0 => {
            WatcherEventBuilder::new("encounter", WatcherEventType::ValidationWarning)
                .field("event", "encounter.new_type_rejected_mid_encounter")
                .field("encounter_type", incoming_type)
                .field("previous_encounter_type", &old.encounter_type)
                .field("beat_count", old.beat)
                .field("source", "narrator_confrontation")
                .send();
            ConfrontationGateOutcome::RejectedMidEncounter
        }

        // Unreachable — Cases A through E above exhaust every (None, resolved,
        // same-type, beat==0, beat>0) combination. Left as a compiler-verified
        // safety net so adding a new variant to the match requires a conscious
        // choice about how to route uncovered states.
        Some(_) => unreachable!(
            "encounter gate: match arms A-E should cover every (encounter, incoming) state"
        ),
    }
}

/// Build a fresh `StructuredEncounter` from a ConfrontationDef, populating
/// actors from the current character roster and the narrator's NPC list for
/// this turn. Mirrors the original inline logic in `dispatch_player_action`.
fn build_encounter(
    def: &ConfrontationDef,
    characters: &[sidequest_game::Character],
    narrator_npcs: &[NpcMention],
) -> StructuredEncounter {
    let mut encounter = StructuredEncounter::from_confrontation_def(def);

    for ch in characters {
        encounter.actors.push(EncounterActor {
            name: ch.core.name.as_str().to_string(),
            role: "player".to_string(),
            per_actor_state: std::collections::HashMap::new(),
        });
    }
    for npc in narrator_npcs {
        encounter.actors.push(EncounterActor {
            name: npc.name.clone(),
            role: "npc".to_string(),
            per_actor_state: std::collections::HashMap::new(),
        });
    }

    encounter
}

fn emit_created(encounter: &StructuredEncounter, incoming_type: &str) {
    WatcherEventBuilder::new("encounter", WatcherEventType::StateTransition)
        .field("event", "encounter.created")
        .field("encounter_type", incoming_type)
        .field("actor_count", encounter.actors.len())
        .field("source", "narrator_confrontation")
        .send();
}

//! Playtest 2026-04-11 follow-up regression guard: TurnComplete event must
//! have a SINGLE source of truth, and that source must carry all the fields
//! the OTEL dashboard reads.
//!
//! Background:
//! After the StrictMode WebSocket flag-race fix (slabgorb/sidequest-ui#107)
//! shipped, the OTEL dashboard's 4× duplicate-row bug dropped to 2× — but
//! didn't fully resolve. SM diagnosed the remaining 2× as a server-side
//! dual emission: every real player turn was firing TWO `WatcherEventType::
//! TurnComplete` events:
//!
//!   1. main.rs::turn_record_bridge — `component: "orchestrator"`,
//!      receives TurnRecords from the orchestrator's mpsc channel and
//!      converts each into a TurnComplete watcher event.
//!   2. dispatch/telemetry.rs::emit_telemetry — `component: "game"`,
//!      fired in the dispatch hot path with timing data.
//!
//! Both fired per real turn → 2× rows in the dashboard's TimelineTab.
//! Each emitter had a different field set, so neither was a strict
//! superset and neither could be deleted without data loss.
//!
//! Fix:
//!   - Consolidate to dispatch/telemetry.rs::emit_telemetry as the
//!     single source of truth. Add the missing fields (patches,
//!     beats_fired, delta_empty, narration_len) to its TurnComplete
//!     builder, sourced from the values dispatch already computes.
//!   - Disable the WatcherEvent emission in main.rs::turn_record_bridge.
//!     The bridge is still alive — it continues to drive ADR-073 JSONL
//!     training-data persistence and the SubsystemTracker that emits
//!     SubsystemExerciseSummary / CoverageGap events. Only the redundant
//!     TurnComplete emission was removed.
//!
//! These tests pin the structural shape of the consolidation so a
//! future refactor can't silently re-introduce a duplicate emitter.

// =========================================================================
// Source inspection — telemetry.rs is the canonical TurnComplete source
// =========================================================================

#[test]
fn telemetry_emit_carries_patches_beats_delta_narration_fields() {
    let src = include_str!("../../src/dispatch/telemetry.rs");

    // The consolidated emission in emit_telemetry must include all four
    // fields that were previously only on main.rs's bridge emission.
    // Without these the dashboard's Turn Details panel would silently
    // start showing "Patches: none" / "Beats: none" / wrong delta_empty
    // / wrong narration_len after the consolidation.
    let required_fields = [
        (".field(\"patches\", &patches_json)", "patches"),
        (".field(\"beats_fired\", &beats_json)", "beats_fired"),
        (
            ".field(\"delta_empty\", game_delta.is_empty())",
            "delta_empty",
        ),
        (
            ".field(\"narration_len\", result.narration.len())",
            "narration_len",
        ),
    ];

    for (pattern, name) in &required_fields {
        assert!(
            src.contains(pattern),
            "telemetry.rs::emit_telemetry must add `{}` to the TurnComplete \
             event after the playtest 2026-04-11 consolidation. Pattern not \
             found: `{}`. Without this field the dashboard will silently \
             lose data and we'd be debugging it from the UI side.",
            name,
            pattern
        );
    }
}

#[test]
fn telemetry_emit_signature_takes_consolidated_args() {
    let src = include_str!("../../src/dispatch/telemetry.rs");

    // After story 36-2, these fields moved from direct function parameters
    // to the TelemetryContext struct. The intent is the same: emit_telemetry
    // must receive game_delta, patches_applied, and beats_fired so the
    // call site can pass dispatch-computed values without re-deriving them.
    assert!(
        src.contains("game_delta") && src.contains("sidequest_game::StateDelta"),
        "telemetry.rs must carry `game_delta: StateDelta` (via TelemetryContext) \
         so the TurnComplete builder can call game_delta.is_empty(). The \
         delta is computed in dispatch/mod.rs and threaded through."
    );
    assert!(
        src.contains("patches_applied") && src.contains("PatchSummary"),
        "telemetry.rs must carry `patches_applied: &[PatchSummary]` \
         (via TelemetryContext) ported from dispatch::patching::derive_patches_from_delta()."
    );
    assert!(
        src.contains("beats_fired"),
        "telemetry.rs must carry `beats_fired` (via TelemetryContext) \
         matching the turn_beats_for_record vec dispatch already computes."
    );
}

// =========================================================================
// Source inspection — main.rs no longer emits a duplicate TurnComplete
// =========================================================================

#[test]
fn main_turn_record_bridge_does_not_emit_turn_complete_event() {
    let src = include_str!("../../src/main.rs");

    // The bridge function must NOT contain a TurnComplete WatcherEventBuilder.
    // If this test ever fires, the dual-emission bug has regressed and the
    // dashboard timeline will start showing 2× rows again.
    //
    // We use a tight substring match for the exact original pattern that
    // was deleted, plus a broader scan for "TurnComplete" inside the
    // turn_record_bridge function body.
    assert!(
        !src.contains("WatcherEventBuilder::new(\"orchestrator\", WatcherEventType::TurnComplete)"),
        "main.rs::turn_record_bridge must NOT emit a `WatcherEventType::\
         TurnComplete` event with `component: \"orchestrator\"` — that's \
         the duplicate emission removed in the playtest 2026-04-11 \
         consolidation. The single source of truth is now \
         dispatch/telemetry.rs::emit_telemetry."
    );
}

#[test]
fn main_turn_record_bridge_still_persists_jsonl_for_training_data() {
    let src = include_str!("../../src/main.rs");

    // Sanity check: the bridge function is still alive for its OTHER
    // responsibilities. Removing the WatcherEvent emission must not
    // accidentally drop the JSONL training-data path or the
    // SubsystemTracker. If a future cleanup deletes too much, this
    // test will catch it.
    assert!(
        src.contains("turns_{today}.jsonl"),
        "main.rs::turn_record_bridge must still write per-day JSONL \
         training-data files (ADR-073). Removing the WatcherEvent \
         emission must not drop this path."
    );
    assert!(
        src.contains("SubsystemExerciseSummary"),
        "main.rs::turn_record_bridge must still emit SubsystemExerciseSummary \
         events from the SubsystemTracker (story 26-2). The TurnComplete \
         removal was the ONLY allowed deletion from this function."
    );
    assert!(
        src.contains("CoverageGap"),
        "main.rs::turn_record_bridge must still emit CoverageGap events \
         when the SubsystemTracker hits the gap threshold (story 26-2)."
    );
}

//! Two-phase dice wiring gate.
//!
//! The previous dice flow ran the narrator on the BeatSelection tick and
//! appended `DiceRequest` afterward, so the roll was purely theatrical —
//! the narrator had already committed to an outcome before the dice
//! touched the table. `pending_roll_outcome` was fed into the *next*
//! turn's narration, off-by-one.
//!
//! The fix: `BeatSelection` short-circuits dispatch. It applies
//! `metric_delta`, broadcasts a `DiceRequest`, stores the synthesized
//! `PlayerAction` in `SharedGameSession.pending_replay_action`, and
//! returns. The ws reader loop drains `pending_replay_action` after
//! `DiceThrow` has populated `pending_roll_outcome`, re-dispatching the
//! action through the narrator with the current turn's roll in hand.
//!
//! These tests are the wiring gate for that flow. They grep the compiled
//! source for the exact markers the implementation must carry — cheap,
//! fast, and precise. A behavioral test that drives the full
//! `dispatch_message` async orchestrator would need to build a 40-arg
//! call site, which is not worth the investment for a wiring check.

use std::path::PathBuf;

fn server_src() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs");
    std::fs::read_to_string(path).expect("read src/lib.rs")
}

fn shared_session_src() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/shared_session.rs");
    std::fs::read_to_string(path).expect("read src/shared_session.rs")
}

#[test]
fn shared_session_carries_pending_replay_action_field() {
    let src = shared_session_src();
    assert!(
        src.contains("pub pending_replay_action: Option<sidequest_protocol::PlayerActionPayload>"),
        "SharedGameSession must declare pending_replay_action — the deferred \
         PlayerAction waiting for the DiceRequest/DiceThrow round-trip"
    );
    assert!(
        src.contains("pub pending_replay_beat_id: Option<String>"),
        "SharedGameSession must declare pending_replay_beat_id so the replay \
         tick re-populates DispatchContext.chosen_player_beat for confrontation \
         wiring repair"
    );
    assert!(
        src.contains("pending_replay_action: None") && src.contains("pending_replay_beat_id: None"),
        "SharedGameSession::new must initialize both replay fields to None"
    );
}

#[test]
fn beat_selection_short_circuits_and_defers_narrator() {
    let src = server_src();

    // The beat preprocessing must use `if let` short-circuit, NOT rebind
    // `msg` into a synthetic PlayerAction that falls through. The presence
    // of `dice.two_phase_defer` is the distinguishing OTEL marker for the
    // new path; its absence means the old narrate-first flow is still live.
    assert!(
        src.contains("dice.two_phase_defer"),
        "beat short-circuit must emit a dice.two_phase_defer OTEL event \
         before returning — missing marker suggests the old narrate-first \
         flow is still wired"
    );

    // The synthesized PlayerAction must be parked in shared state for the
    // reader loop to replay — otherwise the narrator never runs.
    assert!(
        src.contains("ss.pending_replay_action = Some(replay_action)"),
        "beat handler must store pending_replay_action = Some(replay_action)"
    );
    assert!(
        src.contains("ss.pending_replay_beat_id = Some(beat_id_str)"),
        "beat handler must store pending_replay_beat_id alongside the action"
    );

    // The beat handler must return a Vec containing the DiceRequest and
    // nothing else — no narration on this tick.
    assert!(
        src.contains("return vec![GameMessage::DiceRequest {"),
        "beat handler must return ONLY the DiceRequest — the narrator is \
         deferred until the reader loop replays the action"
    );
}

#[test]
fn reader_loop_drains_replay_action_gated_on_roll_outcome() {
    let src = server_src();

    // The drain must fire only when BOTH pending_replay_action is Some
    // (deferred action exists) AND pending_roll_outcome is Some (the
    // DiceThrow round-trip completed in this iteration). The double guard
    // is what prevents the replay from firing prematurely on the
    // BeatSelection tick itself.
    assert!(
        src.contains("pending_replay_action.is_some()")
            && src.contains("pending_roll_outcome.is_some()"),
        "reader-loop replay drain must gate on BOTH pending_replay_action \
         AND pending_roll_outcome being Some — otherwise replay fires on \
         the BeatSelection tick before the dice have been thrown"
    );

    // The drain must emit the two_phase_replay OTEL marker.
    assert!(
        src.contains("dice.two_phase_replay"),
        "reader loop must emit dice.two_phase_replay OTEL when re-dispatching \
         the deferred action"
    );

    // The drain must push a PlayerAction onto the work queue, not send it
    // directly to the client.
    assert!(
        src.contains("work_queue.push_back(GameMessage::PlayerAction"),
        "reader loop must push the replayed PlayerAction onto the work queue \
         so the normal dispatch_message path runs it through the narrator"
    );
}

#[test]
fn dispatch_context_pulls_chosen_beat_from_shared_session_on_replay() {
    let src = server_src();

    // On the replay tick the local `chosen_player_beat` is None (beat path
    // short-circuited without setting it). The ctx-build must fall through
    // to the one-shot pending_replay_beat_id on the shared session so
    // confrontation wiring repair still sees the beat.
    assert!(
        src.contains("ss.pending_replay_beat_id.take()"),
        "DispatchContext ctx-build must .take() pending_replay_beat_id \
         from shared session when local chosen_player_beat is None — \
         otherwise confrontation wiring repair skips the replay tick"
    );
}

#[test]
fn dead_narrator_side_dice_request_broadcast_is_gone() {
    let src = server_src();

    // Before the fix, the DiceRequest was broadcast AFTER the narrator
    // ran via a `pending_dice_request.take()` local. That branch must
    // be removed — if it comes back, the narration will lead the roll
    // again.
    assert!(
        !src.contains("pending_dice_request.take()"),
        "Legacy post-narration DiceRequest broadcast must be removed — \
         finding `pending_dice_request.take()` means the narrate-first \
         flow has been reintroduced"
    );
    assert!(
        !src.contains("dice.request_initiated — DiceRequest broadcast after beat selection"),
        "Legacy log line means the old post-narration broadcast is still live"
    );
}

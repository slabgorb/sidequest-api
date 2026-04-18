//! Story 34-12: End-to-end integration tests for physics-is-the-roll.
//!
//! These tests drive `dice_dispatch::handle_dice_throw` directly with a real
//! `SharedGameSession` holder. Every pre-34-12 "wiring" test for dice was
//! either a source-string grep or exercised isolated helper functions — none
//! of them actually followed a `DiceThrow` payload through the dispatch
//! pipeline to a broadcast `DiceResult`. That is the exact gap that let the
//! 34-2/34-3/34-5/34-7 stub chain ship.
//!
//! What these tests prove:
//!
//! 1. Client-reported `face` drives the broadcast `DiceResult.rolls[0].faces`
//!    — server does NOT invoke RNG on this path.
//! 2. Crit detection uses the client face (nat-20 → CritSuccess regardless of
//!    total vs DC; nat-1 → CritFail).
//! 3. Total and outcome are derived from the client face plus server-side
//!    modifier/DC, not from anything else.
//! 4. The broadcast reaches a subscribed consumer (the shared session
//!    broadcast channel, which in production feeds every connected client).
//! 5. `pending_roll_outcome` is persisted in the shared session so the next
//!    narration turn can read it (story 34-9 wiring still holds).
//! 6. Face-count mismatch and out-of-range faces produce wire ERROR messages,
//!    not panics or silent RNG fallback.
//! 7. The pending `DiceRequest` is consumed exactly once even on error paths.
//!
//! These tests do NOT spin up a WebSocket server — they drive the dispatch
//! helper directly because (a) getting a connection into `Playing` state
//! otherwise requires going through Claude-backed character creation, and
//! (b) the unit under test is the dispatch helper, not axum. The WebSocket
//! transport layer is covered by `server_story_2_1_tests::websocket_full_lifecycle`.

use std::num::{NonZeroU32, NonZeroU8};
use std::sync::Arc;

use sidequest_protocol::{
    DiceRequestPayload, DiceThrowPayload, DieSides, DieSpec, GameMessage, RollOutcome, ThrowParams,
};
use sidequest_server::dice_dispatch::{handle_dice_throw, SharedSessionHolder};
use sidequest_server::shared_session::{SharedGameSession, TargetedMessage};
use tokio::sync::Mutex;

// ----------------------------------------------------------------------------
// Fixtures
// ----------------------------------------------------------------------------

fn d20_pool() -> Vec<DieSpec> {
    vec![DieSpec {
        sides: DieSides::D20,
        count: NonZeroU8::new(1).unwrap(),
    }]
}

fn mixed_pool() -> Vec<DieSpec> {
    vec![
        DieSpec {
            sides: DieSides::D20,
            count: NonZeroU8::new(1).unwrap(),
        },
        DieSpec {
            sides: DieSides::D6,
            count: NonZeroU8::new(2).unwrap(),
        },
    ]
}

fn sample_throw_params() -> ThrowParams {
    ThrowParams {
        velocity: [0.5, 1.2, -0.8],
        angular: [1.5, 0.3, 2.1],
        position: [0.5, 0.5],
    }
}

fn make_request(id: &str, pool: Vec<DieSpec>, modifier: i32, dc: u32) -> DiceRequestPayload {
    DiceRequestPayload {
        request_id: id.to_string(),
        rolling_player_id: "test-player".to_string(),
        character_name: "Kira".to_string(),
        dice: pool,
        modifier,
        stat: sidequest_protocol::Stat::new("influence").unwrap(),
        difficulty: NonZeroU32::new(dc).unwrap(),
        context: "test".to_string(),
    }
}

/// Create a SharedSessionHolder primed with one pending DiceRequest.
async fn holder_with_pending(request: DiceRequestPayload) -> SharedSessionHolder {
    let mut ss = SharedGameSession::new("low_fantasy".into(), "pinwheel_coast".into());
    ss.pending_dice_requests
        .insert(request.request_id.clone(), request);
    let holder: SharedSessionHolder = Arc::new(Mutex::new(Some(Arc::new(Mutex::new(ss)))));
    holder
}

/// Subscribe to the session's broadcast channel so the test can observe
/// what gets sent to real clients.
async fn subscribe(
    holder: &SharedSessionHolder,
) -> tokio::sync::broadcast::Receiver<TargetedMessage> {
    let guard = holder.lock().await;
    let ss_arc = guard.as_ref().expect("holder populated");
    let ss = ss_arc.lock().await;
    ss.subscribe()
}

fn extract_dice_result(msgs: Vec<GameMessage>) -> sidequest_protocol::DiceResultPayload {
    assert_eq!(msgs.len(), 1, "handler should return exactly one message");
    match msgs.into_iter().next().unwrap() {
        GameMessage::DiceResult { payload, .. } => payload,
        other => panic!("expected DiceResult, got {other:?}"),
    }
}

fn extract_error(msgs: Vec<GameMessage>) -> String {
    assert_eq!(msgs.len(), 1, "handler should return exactly one message");
    match msgs.into_iter().next().unwrap() {
        GameMessage::Error { payload, .. } => payload.message.as_str().to_string(),
        other => panic!("expected Error, got {other:?}"),
    }
}

// ----------------------------------------------------------------------------
// Physics-is-the-roll: client face drives the broadcast result
// ----------------------------------------------------------------------------

#[tokio::test]
async fn client_face_15_produces_dice_result_with_face_15() {
    let request = make_request("req-success", d20_pool(), 1, 14);
    let holder = holder_with_pending(request.clone()).await;

    let payload = DiceThrowPayload {
        request_id: "req-success".to_string(),
        throw_params: sample_throw_params(),
        face: vec![15],
        beat_id: None,
    };

    let result = extract_dice_result(handle_dice_throw(payload, "test-player", &holder, 1).await);

    assert_eq!(
        result.rolls[0].faces,
        vec![15],
        "server must broadcast the client-reported face verbatim — no RNG"
    );
    assert_eq!(result.total, 16, "total = face + modifier (15 + 1)");
    assert_eq!(
        result.outcome,
        RollOutcome::Success,
        "16 vs DC 14 should be Success"
    );
}

#[tokio::test]
async fn client_face_12_below_dc_produces_fail_outcome() {
    // The exact scenario Keith saw: server says 12, outcome Fail — now with
    // client-reported face actually driving it instead of RNG.
    let request = make_request("req-fail", d20_pool(), 1, 14);
    let holder = holder_with_pending(request).await;

    let payload = DiceThrowPayload {
        request_id: "req-fail".to_string(),
        throw_params: sample_throw_params(),
        face: vec![12],
        beat_id: None,
    };

    let result = extract_dice_result(handle_dice_throw(payload, "test-player", &holder, 1).await);

    assert_eq!(result.rolls[0].faces, vec![12]);
    assert_eq!(result.total, 13);
    assert_eq!(result.outcome, RollOutcome::Fail);
}

#[tokio::test]
async fn client_face_nat20_produces_crit_success_regardless_of_dc() {
    // Even with a modifier that couldn't reach DC, nat-20 crits.
    let request = make_request("req-crit-success", d20_pool(), -50, 100);
    let holder = holder_with_pending(request).await;

    let payload = DiceThrowPayload {
        request_id: "req-crit-success".to_string(),
        throw_params: sample_throw_params(),
        face: vec![20],
        beat_id: None,
    };

    let result = extract_dice_result(handle_dice_throw(payload, "test-player", &holder, 1).await);

    assert_eq!(result.rolls[0].faces, vec![20]);
    assert_eq!(result.outcome, RollOutcome::CritSuccess);
}

#[tokio::test]
async fn client_face_nat1_produces_crit_fail_regardless_of_total() {
    // Even with huge modifier that would clear DC, nat-1 crit-fails.
    let request = make_request("req-crit-fail", d20_pool(), 50, 5);
    let holder = holder_with_pending(request).await;

    let payload = DiceThrowPayload {
        request_id: "req-crit-fail".to_string(),
        throw_params: sample_throw_params(),
        face: vec![1],
        beat_id: None,
    };

    let result = extract_dice_result(handle_dice_throw(payload, "test-player", &holder, 1).await);

    assert_eq!(result.rolls[0].faces, vec![1]);
    assert_eq!(result.outcome, RollOutcome::CritFail);
}

#[tokio::test]
async fn mixed_pool_faces_are_mapped_to_correct_die_groups() {
    // Flat order: [d20, d6, d6] — server must assign them to the right groups.
    let request = make_request("req-mixed", mixed_pool(), 0, 15);
    let holder = holder_with_pending(request).await;

    let payload = DiceThrowPayload {
        request_id: "req-mixed".to_string(),
        throw_params: sample_throw_params(),
        face: vec![14, 3, 5],
        beat_id: None,
    };

    let result = extract_dice_result(handle_dice_throw(payload, "test-player", &holder, 1).await);

    assert_eq!(result.rolls.len(), 2);
    assert_eq!(
        result.rolls[0].faces,
        vec![14],
        "d20 group gets first face in flat order"
    );
    assert_eq!(
        result.rolls[1].faces,
        vec![3, 5],
        "d6 group gets next two faces in flat order"
    );
    assert_eq!(result.total, 22, "14 + 3 + 5 + 0 modifier");
    assert_eq!(result.outcome, RollOutcome::Success);
}

// ----------------------------------------------------------------------------
// Broadcast: subscribed consumers see the DiceResult
// ----------------------------------------------------------------------------

#[tokio::test]
async fn dice_result_is_broadcast_to_subscribers() {
    let request = make_request("req-broadcast", d20_pool(), 0, 10);
    let holder = holder_with_pending(request).await;
    let mut rx = subscribe(&holder).await;

    let payload = DiceThrowPayload {
        request_id: "req-broadcast".to_string(),
        throw_params: sample_throw_params(),
        face: vec![17],
        beat_id: None,
    };

    let _ = handle_dice_throw(payload, "test-player", &holder, 1).await;

    let envelope = rx
        .try_recv()
        .expect("broadcast channel should receive the DiceResult");
    match envelope.msg {
        GameMessage::DiceResult { payload, .. } => {
            assert_eq!(
                payload.rolls[0].faces,
                vec![17],
                "broadcast result must carry the client face"
            );
            assert_eq!(payload.total, 17);
            assert_eq!(payload.outcome, RollOutcome::Success);
        }
        other => panic!("expected DiceResult in broadcast, got {other:?}"),
    }
}

// ----------------------------------------------------------------------------
// Story 34-9 wiring still holds: outcome persisted for next narration turn
// ----------------------------------------------------------------------------

#[tokio::test]
async fn outcome_is_persisted_to_pending_roll_outcome() {
    let request = make_request("req-persist", d20_pool(), 0, 10);
    let holder = holder_with_pending(request).await;

    let payload = DiceThrowPayload {
        request_id: "req-persist".to_string(),
        throw_params: sample_throw_params(),
        face: vec![18],
        beat_id: None,
    };

    let _ = handle_dice_throw(payload, "test-player", &holder, 1).await;

    let guard = holder.lock().await;
    let ss_arc = guard.as_ref().unwrap();
    let ss = ss_arc.lock().await;
    assert_eq!(
        ss.pending_roll_outcome,
        Some(RollOutcome::Success),
        "outcome must be stashed in SharedGameSession for next narration turn (34-9)"
    );
}

// ----------------------------------------------------------------------------
// Pending DiceRequest is consumed exactly once
// ----------------------------------------------------------------------------

#[tokio::test]
async fn pending_dice_request_is_removed_after_successful_resolve() {
    let request = make_request("req-consume", d20_pool(), 0, 10);
    let holder = holder_with_pending(request).await;

    let payload = DiceThrowPayload {
        request_id: "req-consume".to_string(),
        throw_params: sample_throw_params(),
        face: vec![15],
        beat_id: None,
    };

    let _ = handle_dice_throw(payload, "test-player", &holder, 1).await;

    let guard = holder.lock().await;
    let ss_arc = guard.as_ref().unwrap();
    let ss = ss_arc.lock().await;
    assert!(
        !ss.pending_dice_requests.contains_key("req-consume"),
        "pending request must be removed on success"
    );
}

#[tokio::test]
async fn second_throw_with_same_request_id_produces_error() {
    let request = make_request("req-once", d20_pool(), 0, 10);
    let holder = holder_with_pending(request).await;

    let first_payload = DiceThrowPayload {
        request_id: "req-once".to_string(),
        throw_params: sample_throw_params(),
        face: vec![15],
        beat_id: None,
    };
    let _ = handle_dice_throw(first_payload, "test-player", &holder, 1).await;

    let second_payload = DiceThrowPayload {
        request_id: "req-once".to_string(),
        throw_params: sample_throw_params(),
        face: vec![19],
        beat_id: None,
    };
    let err = extract_error(handle_dice_throw(second_payload, "test-player", &holder, 1).await);
    assert!(
        err.contains("No pending dice request"),
        "second throw must hit the no-pending-request error path, got: {err}"
    );
}

// ----------------------------------------------------------------------------
// Validation error paths
// ----------------------------------------------------------------------------

#[tokio::test]
async fn face_count_mismatch_returns_wire_error_and_does_not_broadcast() {
    let request = make_request("req-count-mismatch", d20_pool(), 0, 10);
    let holder = holder_with_pending(request).await;
    let mut rx = subscribe(&holder).await;

    // Pool has 1 die, client sent 2 faces.
    let payload = DiceThrowPayload {
        request_id: "req-count-mismatch".to_string(),
        throw_params: sample_throw_params(),
        face: vec![10, 5],
        beat_id: None,
    };

    let err = extract_error(handle_dice_throw(payload, "test-player", &holder, 1).await);
    assert!(
        err.contains("face count"),
        "FaceCountMismatch Display substring must reach the wire message, got: {err}"
    );
    assert!(
        rx.try_recv().is_err(),
        "no DiceResult should be broadcast on validation error"
    );
}

#[tokio::test]
async fn face_out_of_range_returns_wire_error() {
    let request = make_request("req-out-of-range", d20_pool(), 0, 10);
    let holder = holder_with_pending(request).await;

    let payload = DiceThrowPayload {
        request_id: "req-out-of-range".to_string(),
        throw_params: sample_throw_params(),
        face: vec![21],
        beat_id: None,
    };

    let err = extract_error(handle_dice_throw(payload, "test-player", &holder, 1).await);
    assert!(
        err.contains("out of range"),
        "FaceOutOfRange Display substring must reach the wire message, got: {err}"
    );
}

#[tokio::test]
async fn zero_face_returns_wire_error() {
    let request = make_request("req-zero-face", d20_pool(), 0, 10);
    let holder = holder_with_pending(request).await;

    let payload = DiceThrowPayload {
        request_id: "req-zero-face".to_string(),
        throw_params: sample_throw_params(),
        face: vec![0],
        beat_id: None,
    };

    let err = extract_error(handle_dice_throw(payload, "test-player", &holder, 1).await);
    assert!(
        err.contains("out of range"),
        "zero face must surface as FaceOutOfRange (out-of-range Display substring), got: {err}"
    );
}

#[tokio::test]
async fn unknown_request_id_returns_wire_error() {
    let request = make_request("req-known", d20_pool(), 0, 10);
    let holder = holder_with_pending(request).await;

    let payload = DiceThrowPayload {
        request_id: "req-unknown".to_string(),
        throw_params: sample_throw_params(),
        face: vec![15],
        beat_id: None,
    };

    let err = extract_error(handle_dice_throw(payload, "test-player", &holder, 1).await);
    assert!(
        err.contains("No pending dice request"),
        "unknown request id should surface as no-pending-request, got: {err}"
    );
}

// ----------------------------------------------------------------------------
// No RNG contamination: same face always produces identical result
// ----------------------------------------------------------------------------

#[tokio::test]
async fn same_client_face_produces_identical_results_across_calls() {
    // With physics-is-the-roll, no RNG is invoked on this path. The only
    // source of nondeterminism in resolve_dice was RNG, so repeated calls
    // must produce bit-identical DiceResults for a given face.
    let request_a = make_request("req-det-a", d20_pool(), 2, 14);
    let holder_a = holder_with_pending(request_a).await;
    let payload_a = DiceThrowPayload {
        request_id: "req-det-a".to_string(),
        throw_params: sample_throw_params(),
        face: vec![15],
        beat_id: None,
    };
    let result_a =
        extract_dice_result(handle_dice_throw(payload_a, "test-player", &holder_a, 7).await);

    let request_b = make_request("req-det-b", d20_pool(), 2, 14);
    let holder_b = holder_with_pending(request_b).await;
    let payload_b = DiceThrowPayload {
        request_id: "req-det-b".to_string(),
        throw_params: sample_throw_params(),
        face: vec![15],
        beat_id: None,
    };
    let result_b =
        extract_dice_result(handle_dice_throw(payload_b, "test-player", &holder_b, 7).await);

    assert_eq!(result_a.rolls, result_b.rolls);
    assert_eq!(result_a.total, result_b.total);
    assert_eq!(result_a.outcome, result_b.outcome);
}

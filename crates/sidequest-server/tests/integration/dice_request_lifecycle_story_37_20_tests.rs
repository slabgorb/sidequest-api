//! Story 37-20: Dice-request request_id lifecycle — server becomes the
//! single issuer, client echoes server-issued id, and a retry/recovery
//! path keeps a lost DiceRequest from wedging the session (the "lost
//! nat 20 on defend" bug from Playtest 2).
//!
//! Background (playtest 2026-04-12 family):
//!   1. A DiceRequest was broadcast to the rolling player's client.
//!   2. The frame never arrived (network blip / WebSocket reconnect
//!      between broadcast and receive).
//!   3. `pending_dice_requests` held the request forever. No timeout,
//!      no retry, no surfacing to the GM panel. The turn barrier sat
//!      waiting on a throw that would never come, and the only way out
//!      was kicking the session.
//!
//! Compounding issue (pre-fix): two sites inserted into
//! `SharedGameSession::pending_dice_requests` directly —
//!   * `src/lib.rs` (server-initiated two-phase beat path)
//!   * `src/lib.rs` (client-initiated beat+dice / physics-is-the-roll
//!     path, which trusted `payload.request_id` from the client)
//! The second site meant the *client* could mint a `request_id` the
//! server then adopted — a dual-issuer contract. Per 37-20 ACs the
//! server must be the single issuer, so both sites now funnel through a
//! chokepoint on `SharedGameSession` that timestamps the insertion for
//! retry detection.
//!
//! Fix contract under test:
//!
//!   A. `SharedGameSession::insert_pending_dice_request(req)` — the
//!      single insertion chokepoint. Records `issued_at: Instant` next
//!      to the stored `DiceRequestPayload`. Idempotent on re-insert of
//!      the same `request_id` (no duplicate, `issued_at` preserved).
//!
//!   B. `SharedGameSession::expired_pending_dice_requests(now, timeout)`
//!      — returns `DiceRequestPayload`s whose `issued_at + timeout <=
//!      now`. This is what the server ticks call to detect a wedged
//!      lost-request and re-emit the same `request_id` to the client.
//!
//!   C. `sidequest_server::emit_dice_request_recovery(&req)` — OTEL
//!      watcher span on the "dice" channel, event
//!      `"dice_request.recovery"`, `StateTransition` type, severity
//!      `Warn`. The GM panel's lie detector for retries firing.
//!
//!   D. Wiring: no call to `pending_dice_requests.insert` anywhere
//!      under `crates/sidequest-server/src/` except inside
//!      `shared_session.rs`. Every other site goes through
//!      `insert_pending_dice_request`.
//!
//! Each test below names the contract clause it exercises. Tests A.1,
//! A.2, B.1, B.2 exercise the insert / retry-detector behavior; C.1
//! asserts the OTEL span shape; D.1 is the source-grep wiring test that
//! enforces the single-issuer invariant structurally.

use std::time::{Duration, Instant};

use sidequest_protocol::{DiceRequestPayload, DieSides, DieSpec};
use sidequest_server::shared_session::SharedGameSession;
use sidequest_telemetry::{init_global_channel, subscribe_global, WatcherEventType};

fn fresh_session() -> SharedGameSession {
    SharedGameSession::new("caverns_and_claudes".to_string(), "testworld".to_string())
}

fn dice_request(request_id: &str) -> DiceRequestPayload {
    DiceRequestPayload {
        request_id: request_id.to_string(),
        rolling_player_id: "player-1".to_string(),
        character_name: "Rux".to_string(),
        dice: vec![DieSpec {
            sides: DieSides::D20,
            count: std::num::NonZeroU8::new(1).expect("1 is nonzero"),
        }],
        modifier: 2,
        stat: "dexterity".to_string(),
        difficulty: std::num::NonZeroU32::new(15).expect("15 is nonzero"),
        context: "Defend roll — nat 20 hunting grounds".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Contract A — chokepoint insertion records an issued_at timestamp.
// ---------------------------------------------------------------------------

/// A.1 — inserting a pending DiceRequest via the chokepoint records an
/// `issued_at` timestamp that `expired_pending_dice_requests` can see.
#[test]
fn insert_pending_dice_request_records_issued_at() {
    let mut ss = fresh_session();
    let req = dice_request("req-alpha");

    // Chokepoint insertion — this is the only sanctioned way to add a
    // pending request. Must not return an error on first insert.
    ss.insert_pending_dice_request(req.clone());

    assert!(
        ss.pending_dice_requests.contains_key("req-alpha"),
        "chokepoint insert must land the request in the pending map"
    );

    // Probing the retry API with a timeout well past the insert time
    // should surface exactly this request — proof that issued_at was
    // recorded and is reachable.
    let now_later = Instant::now() + Duration::from_secs(60);
    let expired = ss.expired_pending_dice_requests(now_later, Duration::from_secs(5));
    assert_eq!(
        expired.len(),
        1,
        "one pending request issued >5s ago (by simulated clock) must be flagged expired; \
         got {} entries — retry detector is not reading issued_at",
        expired.len()
    );
    assert_eq!(
        expired[0].request_id, "req-alpha",
        "expired request must carry the original request_id so retry re-emits with the same id"
    );
}

/// A.2 — re-inserting the same request_id is idempotent: no duplicate
/// entry, and `issued_at` is NOT reset (otherwise a retry that reuses
/// the id would keep resetting the timer and the wedge never resolves).
#[test]
fn insert_pending_dice_request_is_idempotent_on_same_request_id() {
    let mut ss = fresh_session();
    let req = dice_request("req-beta");
    ss.insert_pending_dice_request(req.clone());

    assert_eq!(
        ss.pending_dice_requests.len(),
        1,
        "first insert puts exactly one entry in the map"
    );

    // Sleep a hair so the second insert would appear "fresh" if
    // issued_at were naively overwritten.
    std::thread::sleep(Duration::from_millis(20));
    ss.insert_pending_dice_request(req.clone());

    assert_eq!(
        ss.pending_dice_requests.len(),
        1,
        "re-inserting same request_id must NOT create a duplicate entry"
    );

    // Probe with a tiny timeout (10ms) — the original insert is >10ms
    // old, so it must still be flagged expired. If issued_at got
    // bumped on re-insert, this assertion catches the regression.
    let expired = ss.expired_pending_dice_requests(Instant::now(), Duration::from_millis(10));
    assert_eq!(
        expired.len(),
        1,
        "re-insert must preserve original issued_at — retry must still fire for the original"
    );
}

// ---------------------------------------------------------------------------
// Contract B — expired_pending_dice_requests drives retry detection.
// ---------------------------------------------------------------------------

/// A.3 — duplicate `request_id` with a *different* payload surfaces a
/// `dice_request.duplicate_id_mismatch` WatcherEvent (No Silent
/// Fallbacks — reviewer pass 1, rule-checker finding #1). The stored
/// payload is NOT overwritten (idempotency wins), but the mismatch is
/// loudly observable on the GM panel so a bypass or stale-id replay
/// can be diagnosed.
#[test]
fn duplicate_request_id_with_different_payload_emits_warning_event() {
    let _tx = init_global_channel();
    let mut rx = subscribe_global().expect("global channel initialized above");
    while rx.try_recv().is_ok() {}

    let mut ss = fresh_session();
    let original = dice_request("req-dup");
    ss.insert_pending_dice_request(original.clone());

    // Build a conflicting payload: same id, different stat/context.
    let mut mutated = original.clone();
    mutated.stat = "wisdom".to_string();
    mutated.context = "Replayed from a stale session".to_string();
    ss.insert_pending_dice_request(mutated);

    // Stored payload must still be the original — idempotency is not
    // overridden by the warning.
    let stored = ss
        .pending_dice_requests
        .get("req-dup")
        .expect("original must still be present");
    assert_eq!(stored.stat, "dexterity", "duplicate must not overwrite");

    // And a loud signal must have hit the GM panel.
    let mut matched = None;
    for _ in 0..32 {
        match rx.try_recv() {
            Ok(ev) => {
                if ev.component == "dice"
                    && ev.fields.get("event").and_then(serde_json::Value::as_str)
                        == Some("dice_request.duplicate_id_mismatch")
                {
                    matched = Some(ev);
                    break;
                }
            }
            Err(_) => break,
        }
    }
    assert!(
        matched.is_some(),
        "duplicate-id mismatch must emit a dice.duplicate_id_mismatch WatcherEvent \
         — silent idempotency on payload drift is a silent fallback"
    );
}

/// B.1 — a freshly inserted request is NOT returned by
/// `expired_pending_dice_requests` when probed with a timeout larger
/// than the elapsed time. No false-positive retries.
#[test]
fn expired_pending_dice_requests_excludes_fresh_requests() {
    let mut ss = fresh_session();
    ss.insert_pending_dice_request(dice_request("req-gamma"));

    // Probe immediately with a generous timeout.
    let expired = ss.expired_pending_dice_requests(Instant::now(), Duration::from_secs(30));
    assert!(
        expired.is_empty(),
        "fresh request must not be flagged expired — got {:?}",
        expired.iter().map(|r| &r.request_id).collect::<Vec<_>>()
    );
}

/// B.2 — a resolved request (removed from the pending map) is not
/// re-surfaced by the retry detector. `remove` is the existing API on
/// the map; the retry detector must respect it.
#[test]
fn resolved_requests_are_not_retried() {
    let mut ss = fresh_session();
    ss.insert_pending_dice_request(dice_request("req-delta"));
    // Route removal through the chokepoint so the `issued_at` sidecar
    // stays in lockstep with the canonical map.
    let removed = ss.remove_pending_dice_request("req-delta");
    assert!(
        removed.is_some(),
        "chokepoint must return the removed payload"
    );

    let now_later = Instant::now() + Duration::from_secs(60);
    let expired = ss.expired_pending_dice_requests(now_later, Duration::from_secs(1));
    assert!(
        expired.is_empty(),
        "resolved request must not be re-surfaced by the retry detector"
    );
    // Issued_at must have been cleaned up alongside the canonical entry
    // — no orphan leak (rule-checker finding 13, reviewer pass 1).
    assert!(
        !ss.pending_dice_request_issued_at_contains("req-delta"),
        "remove_pending_dice_request must drop the issued_at sidecar too"
    );
}

/// B.3 — `clear_pending_dice_requests` drops both maps together.
/// Catches the reconnect path at `lib.rs:~2564` where a direct
/// `pending_dice_requests.clear()` would have left orphan issued_at
/// entries.
#[test]
fn clear_pending_dice_requests_drops_both_maps() {
    let mut ss = fresh_session();
    ss.insert_pending_dice_request(dice_request("req-zeta-1"));
    ss.insert_pending_dice_request(dice_request("req-zeta-2"));
    assert_eq!(ss.pending_dice_requests.len(), 2);
    assert_eq!(ss.pending_dice_request_issued_at_len(), 2);

    ss.clear_pending_dice_requests();

    assert!(ss.pending_dice_requests.is_empty());
    assert_eq!(
        ss.pending_dice_request_issued_at_len(),
        0,
        "clear chokepoint must drop issued_at sidecars to prevent unbounded growth \
         across reconnect cycles"
    );
}

// ---------------------------------------------------------------------------
// Contract C — OTEL helper for the GM panel's retry lie detector.
// ---------------------------------------------------------------------------

/// C.1 — `emit_dice_request_recovery` broadcasts a WatcherEvent on the
/// "dice" channel, event `"dice_request.recovery"`, type
/// `StateTransition`. The GM panel filters on these fields; the test
/// asserts the exact shape the panel relies on.
#[test]
fn emit_dice_request_recovery_sends_watcher_event() {
    let _tx = init_global_channel();
    let mut rx = subscribe_global().expect("global channel initialized above");

    // Drain any events left by a concurrent test on the shared global
    // channel. Without this, another dice-channel emitter running in
    // parallel can fill the broadcast buffer and push the event under
    // assertion past the fixed drain budget below, producing a
    // false-negative failure.
    while rx.try_recv().is_ok() {}

    let req = dice_request("req-epsilon");
    sidequest_server::emit_dice_request_recovery(&req);

    // Drain until we find our event or the channel empties.
    let mut matched = None;
    for _ in 0..16 {
        match rx.try_recv() {
            Ok(ev) => {
                let is_recovery = ev.component == "dice"
                    && ev.fields.get("event").and_then(serde_json::Value::as_str)
                        == Some("dice_request.recovery");
                if is_recovery {
                    matched = Some(ev);
                    break;
                }
            }
            Err(_) => break,
        }
    }

    let ev = matched.expect(
        "expected a WatcherEvent with component=\"dice\" and event=\"dice_request.recovery\" \
         — the GM panel's lie detector for dice-request retries",
    );
    assert!(
        matches!(ev.event_type, WatcherEventType::StateTransition),
        "recovery span must be StateTransition (visible in transition column); got {:?}",
        ev.event_type
    );
    assert_eq!(
        ev.fields
            .get("request_id")
            .and_then(serde_json::Value::as_str),
        Some("req-epsilon"),
        "request_id must ride the span so the GM panel can correlate retry -> original"
    );
    assert_eq!(
        ev.fields
            .get("rolling_player")
            .and_then(serde_json::Value::as_str),
        Some("player-1"),
        "rolling_player must ride the span for GM panel per-player filtering"
    );
}

// ---------------------------------------------------------------------------
// Contract D — wiring: no direct `pending_dice_requests.insert` outside
// the shared_session module. Enforces the "server is single issuer"
// invariant at the source level. This is the CLAUDE.md wiring test the
// project mandates for every new chokepoint.
// ---------------------------------------------------------------------------

/// D.1 — grep the server crate source tree: no file under `src/` other
/// than `shared_session.rs` may call `pending_dice_requests.insert`
/// directly. Today this fails because `lib.rs` has two such sites.
#[test]
fn only_shared_session_may_insert_into_pending_dice_requests() {
    use std::fs;
    use std::path::Path;

    fn walk(dir: &Path, offenders: &mut Vec<(String, usize, String)>) {
        let entries = fs::read_dir(dir).expect("server src tree readable");
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                walk(&p, offenders);
                continue;
            }
            if p.extension().and_then(|s| s.to_str()) != Some("rs") {
                continue;
            }
            // Tests in src/ (e.g. *_tests.rs) are allowed — they document
            // current state, not production wiring.
            let fname = p
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            if fname.ends_with("_tests.rs") || fname == "shared_session.rs" {
                continue;
            }
            let body = fs::read_to_string(&p).expect("source file readable");
            for (lineno, line) in body.lines().enumerate() {
                if line.contains("pending_dice_requests.insert") {
                    offenders.push((
                        p.to_string_lossy().into_owned(),
                        lineno + 1,
                        line.trim().to_string(),
                    ));
                }
            }
        }
    }

    let src_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();
    walk(&src_root, &mut offenders);

    assert!(
        offenders.is_empty(),
        "production code outside shared_session.rs must go through \
         SharedGameSession::insert_pending_dice_request — direct .insert calls \
         break the single-issuer contract (story 37-20). Offenders:\n{}",
        offenders
            .iter()
            .map(|(f, l, s)| format!("  {}:{}  {}", f, l, s))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

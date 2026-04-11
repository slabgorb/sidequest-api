//! Wiring guards against the 2026-04-11 "every turn processed multiple
//! times" regression.
//!
//! Background: the ADR-063 dispatch decomposition refactor (commit
//! d3896421, 2026-04-09) extracted `build_response_messages` into its own
//! module and accidentally both *sent* Narration/NarrationEnd via the fast
//! path (`ctx.tx.send`) AND *pushed* them into the `messages` Vec that the
//! caller flushes. Every player turn produced two NARRATION messages on
//! the acting player's socket, which the UI dutifully rendered twice —
//! Keith observed it as "we're processing every turn multiple times"
//! mid-playtest on 2026-04-11.
//!
//! The fix, mirrored in these assertions, is that `build_response_messages`
//! must only ever send Narration and NarrationEnd through `ctx.tx.send`,
//! never push them into the shared `messages` Vec, and
//! `sync_back_to_shared_session` must source the observer broadcast text
//! from an explicit `clean_narration` + `footnotes` parameter pair rather
//! than fishing it back out of the Vec.
//!
//! The solo-turn sequence diagram at `docs/sequences/solo-turn.md` shows
//! exactly one NARRATION arrow from dispatch to the writer — that's the
//! design invariant these tests enforce at the source level.

const RESPONSE_SRC: &str = include_str!("../src/dispatch/response.rs");
const SESSION_SYNC_SRC: &str = include_str!("../src/dispatch/session_sync.rs");
const DISPATCH_MOD_SRC: &str = include_str!("../src/dispatch/mod.rs");

fn prod(src: &str) -> &str {
    src.split("#[cfg(test)]").next().unwrap_or(src)
}

// ===========================================================================
// 1. build_response_messages must NOT push Narration/NarrationEnd into the
//    `messages` Vec that the caller flushes.
// ===========================================================================

#[test]
fn build_response_messages_does_not_push_narration_into_vec() {
    let prod = prod(RESPONSE_SRC);
    // The direct fast-path send must still exist
    assert!(
        prod.contains("ctx.tx.send(narration_msg)"),
        "build_response_messages must keep the ctx.tx.send(narration_msg) \
         fast-path so the acting player sees narration within a few ms \
         rather than waiting for the ~100-500ms RAG/embed work"
    );
    // The push into messages Vec must NOT exist
    assert!(
        !prod.contains("messages.push(narration_msg"),
        "build_response_messages must NOT push narration_msg into the \
         shared `messages` Vec. The caller's writer loop flushes that Vec \
         via tx.send(), so a push here duplicates every turn's narration \
         on the acting player's socket (the 2026-04-11 regression)"
    );
}

#[test]
fn build_response_messages_does_not_push_narration_end_into_vec() {
    let prod = prod(RESPONSE_SRC);
    assert!(
        prod.contains("ctx.tx.send(narration_end)"),
        "build_response_messages must keep the ctx.tx.send(narration_end) \
         fast-path"
    );
    assert!(
        !prod.contains("messages.push(narration_end"),
        "build_response_messages must NOT push narration_end into the \
         shared `messages` Vec — same regression as Narration. A duplicate \
         NARRATION_END double-flushes the narration buffer and fires the \
         canType unlock twice in the UI."
    );
}

// ===========================================================================
// 2. build_response_messages must return the merged footnotes so the
//    caller can forward them to session_sync for observer broadcasts.
// ===========================================================================

#[test]
fn build_response_messages_returns_merged_footnotes() {
    let prod = prod(RESPONSE_SRC);
    assert!(
        prod.contains("-> Vec<sidequest_protocol::Footnote>") || prod.contains("-> Vec<Footnote>"),
        "build_response_messages must return the merged footnotes Vec so \
         session_sync can rebroadcast them to observers. Without this, \
         observers see narration with empty footnotes in multiplayer \
         FreePlay mode."
    );
    assert!(
        prod.contains("merged_footnotes"),
        "build_response_messages body must build a merged_footnotes \
         snapshot (narrator footnotes + affinity tier-up events) to return"
    );
}

// ===========================================================================
// 3. sync_back_to_shared_session must take narration text + footnotes as
//    explicit parameters, not fish them out of the messages Vec.
// ===========================================================================

#[test]
fn sync_back_takes_explicit_narration_and_footnotes() {
    let prod = prod(SESSION_SYNC_SRC);
    assert!(
        prod.contains("clean_narration: &str"),
        "sync_back_to_shared_session must take `clean_narration: &str` as \
         an explicit parameter — the previous `_clean_narration` (unused) \
         was a silent wiring gap. Without it, observer broadcasts cannot \
         source the narration text now that Narration is no longer in the \
         messages Vec."
    );
    assert!(
        prod.contains("footnotes: &[sidequest_protocol::Footnote]")
            || prod.contains("footnotes: &[Footnote]"),
        "sync_back_to_shared_session must take `footnotes` as an explicit \
         parameter so observer narration broadcasts preserve the claimer's \
         merged footnotes."
    );
}

#[test]
fn sync_back_observer_broadcast_is_unconditional() {
    let prod = prod(SESSION_SYNC_SRC);
    // The new code builds observer Narration / NarrationEnd outside the
    // `for msg in messages` loop. The match arms for Narration /
    // NarrationEnd must be gone from that loop — otherwise we'd still be
    // relying on the Vec.
    let match_block_start = prod
        .find("for msg in messages")
        .expect("session_sync retains a messages loop for non-narration types");
    let match_block = &prod[match_block_start..];
    assert!(
        !match_block.contains("GameMessage::Narration { payload"),
        "session_sync must not match on GameMessage::Narration inside the \
         messages loop — that path relied on the removed push and now \
         always misses"
    );
    assert!(
        !match_block.contains("GameMessage::NarrationEnd {"),
        "session_sync must not match on GameMessage::NarrationEnd inside \
         the messages loop"
    );
}

#[test]
fn sync_back_still_guards_observer_broadcast_on_empty_other_players() {
    // Single-player mode has `other_players.is_empty()` — the unconditional
    // observer broadcast block must short-circuit so we don't send targeted
    // session messages to nobody (wasteful) or accidentally target self.
    let prod = prod(SESSION_SYNC_SRC);
    assert!(
        prod.contains("if !other_players.is_empty()"),
        "session_sync's observer broadcast block must guard on \
         `!other_players.is_empty()` so single-player mode stays a no-op. \
         Without this guard, we'd iterate an empty vec — harmless but \
         wasteful, and a sign that the single-player contract was lost."
    );
}

// ===========================================================================
// 4. The caller (dispatch/mod.rs) must capture the returned footnotes and
//    forward them to sync_back_to_shared_session.
// ===========================================================================

#[test]
fn dispatch_caller_forwards_merged_footnotes() {
    let prod = prod(DISPATCH_MOD_SRC);
    assert!(
        prod.contains("let merged_footnotes = response::build_response_messages"),
        "dispatch/mod.rs must capture the return value of \
         build_response_messages as `merged_footnotes`. Otherwise \
         sync_back_to_shared_session has no footnotes to forward and \
         observer narration loses affinity tier-up events."
    );
    assert!(
        prod.contains("&merged_footnotes"),
        "dispatch/mod.rs must pass `&merged_footnotes` to \
         sync_back_to_shared_session for observer broadcasts"
    );
}

// ===========================================================================
// 5. Non-claimer barrier path is UNCHANGED — it still sends its own
//    Narration + NarrationEnd via ctx.tx.send and returns an empty Vec.
//    (This is the sealed-letter invariant: non-claimers get narration
//    via their own polling path, not via session_sync.)
// ===========================================================================

#[test]
fn non_claimer_still_sends_narration_directly_and_returns_empty() {
    let prod = prod(DISPATCH_MOD_SRC);
    // The non-claimer block must still exist and still use ctx.tx.send +
    // `return vec![]` — my fix only touched the claimer (build_response_messages)
    // path, so the barrier sealed-letter contract must be preserved.
    assert!(
        prod.contains("barrier.non_claimer"),
        "non-claimer barrier path must still exist"
    );
    let nc_start = prod
        .find("barrier.non_claimer — retrieved shared narration")
        .expect("non_claimer narration retrieval log line exists");
    let nc_window = &prod[nc_start..nc_start + prod.len().min(2000).min(prod.len() - nc_start)];
    assert!(
        nc_window.contains("ctx.tx.send(msg)") && nc_window.contains("ctx.tx.send(end)"),
        "non-claimer must still fast-path Narration and NarrationEnd via \
         ctx.tx.send to avoid running the narrator twice"
    );
    assert!(
        nc_window.contains("return vec![]"),
        "non-claimer must still return vec![] so the caller's message \
         flush loop is a no-op (otherwise we'd duplicate the narration \
         it just sent)"
    );
}

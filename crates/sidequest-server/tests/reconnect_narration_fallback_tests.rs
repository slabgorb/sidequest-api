//! Wiring guard: the reconnect path in `dispatch/connect.rs` must push
//! a Narration message even when the snapshot has no recap and no
//! narrative_log rows.
//!
//! Background (sq-playtest 2026-04-09): OQ-1 reported that a returning
//! player Begin skips chargen, lands in the correct room with correct
//! stats, but the Narrative panel is BLANK — no "Welcome back", no
//! scene recap, no current situation. Root cause: `generate_recap()`
//! returns `None` when `narrative_log` is empty (e.g. a player who
//! completed chargen but closed the tab before taking any action), and
//! the connect handler only pushed a Narration message when recap was
//! Some. Silent blank panel = worst-case player experience.
//!
//! Fix: four-tier recap source — (1) saved.recap, (2) narrative_log.last,
//! (3) genre-pack room description fallback, (4) location-only fallback.
//! With the four tiers in place, the only way to ship a blank narrative
//! panel on reconnect is for `saved.snapshot.location` to be empty — in
//! which case the session is unrecoverable for other reasons.
//!
//! This file locks in the fallback chain with a source-level check.

use std::fs;
use std::path::PathBuf;

fn connect_source() -> String {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("src/dispatch/connect.rs");
    fs::read_to_string(&path).expect("failed to read src/dispatch/connect.rs")
}

#[test]
fn reconnect_narration_has_four_fallback_tiers() {
    let src = connect_source();

    // Each tier must be tagged with a distinct `recap_source` string so
    // the GM panel's `narration.reconnect.narration_source` telemetry
    // can distinguish which branch fired.
    for tier in [
        "\"recap\"",
        "\"narrative_log_last\"",
        "\"room_description_fallback\"",
        "\"location_only_fallback\"",
    ] {
        assert!(
            src.contains(tier),
            "reconnect narration fallback chain must include tier {} — see \
             src/dispatch/connect.rs reconnect path",
            tier
        );
    }
}

#[test]
fn reconnect_narration_emits_telemetry_with_source() {
    let src = connect_source();

    // The OTEL event must record which tier produced the text so the
    // GM panel can refute or confirm "the player reconnected with
    // narration X" vs "the player reconnected to an empty panel".
    assert!(
        src.contains("reconnect.narration_source"),
        "reconnect path must emit `narration.reconnect.narration_source` telemetry"
    );
    assert!(
        src.contains(".field(\"source\", recap_source)"),
        "telemetry event must record which fallback tier fired"
    );
    assert!(
        src.contains(".field(\"has_text\""),
        "telemetry event must record whether any narration was pushed"
    );
    assert!(
        src.contains(".field(\"narrative_log_rows\""),
        "telemetry event must record how many narrative_log rows were available"
    );
}

#[test]
fn reconnect_narration_fallback_consults_genre_pack_rooms() {
    let src = connect_source();
    // The room-description fallback must actually look up the current
    // room in the loaded genre pack. Without this call, it degenerates
    // into the location-only tier.
    assert!(
        src.contains("genre_cache()"),
        "room-description fallback must call state.genre_cache()"
    );
    assert!(
        src.contains("cartography.rooms"),
        "room-description fallback must read cartography.rooms from the world"
    );
    assert!(
        src.contains("r.id == saved.snapshot.location"),
        "room-description fallback must match RoomDef.id against saved location slug"
    );
}

#[test]
fn reconnect_narration_pushes_narration_and_narration_end() {
    let src = connect_source();
    // The push pattern must be Narration followed by NarrationEnd so
    // the UI's NarrationScroll correctly emits a separator after the
    // recap text. Dropping NarrationEnd would leave the recap stuck in
    // an open "current turn" that never closes.
    let narration_idx = src
        .find("responses.push(GameMessage::Narration {")
        .expect("reconnect path must push Narration");
    let narration_end_idx = src[narration_idx..]
        .find("GameMessage::NarrationEnd")
        .map(|i| i + narration_idx);
    assert!(
        narration_end_idx.is_some(),
        "reconnect path must push NarrationEnd after Narration"
    );
}

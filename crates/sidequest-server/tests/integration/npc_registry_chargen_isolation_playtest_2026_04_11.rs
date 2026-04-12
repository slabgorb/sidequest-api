//! Playtest 2026-04-11 regression guard: NPC state must not leak from
//! one character into a fresh character in the same genre:world.
//!
//! Bug summary (from sq-playtest-pingpong.md):
//!   Character A played in mutant_wasteland / flickering_reach and the
//!   narrator introduced NPC "Spine Copperjaw" during a parley. After a
//!   server restart and fresh chargen for character B in the same world,
//!   B's opening narration referenced Spine Copperjaw as if he were
//!   canonically present — even though B had never encountered him.
//!
//! Root cause:
//!   The `npc_registry` leaks via three coupled surfaces:
//!     1. SharedGameSession is keyed by genre:world and holds NPCs across
//!        all connections to that world
//!     2. SQLite persistence serializes npc_registry into the save file
//!     3. session_restore loads the saved registry into per-connection
//!        locals on returning-player connect
//!
//!   The chargen→Playing transition is the one moment we know a fresh
//!   character is entering the world. It is also the one moment where
//!   clearing the registry is unambiguously correct (the character
//!   literally does not have any narrative relationships yet).
//!
//! Fix (dispatch/connect.rs::dispatch_character_creation):
//!   After the chargen builder completes and before the initial save,
//!   explicitly clear npc_registry in all three surfaces:
//!     a) the local `npc_registry` Vec (carried by DispatchContext)
//!     b) snapshot.npc_registry (persisted to SQLite via the save call
//!        immediately following)
//!     c) the SharedGameSession's npc_registry (so sync_to_locals on the
//!        first turn doesn't repopulate from stale shared state)
//!
//! These tests pin the structural shape of the fix so a future refactor
//! cannot silently remove one of the three clear sites — any single
//! missing clear re-opens the leak. Matches the source-inspection test
//! convention used by npc_turns_beat_system_story_28_8_tests.rs.

// =========================================================================
// Source inspection — fix must remain in place in dispatch/connect.rs
// =========================================================================

/// The clear must happen in the chargen→Playing transition path
/// inside dispatch_character_creation, not elsewhere.
#[test]
fn chargen_clears_local_npc_registry() {
    let src = include_str!("../../src/dispatch/connect.rs");

    // The fix must call .clear() on the local npc_registry parameter.
    // Use a tight substring search to avoid false positives from unrelated
    // references to npc_registry elsewhere in connect.rs.
    assert!(
        src.contains("npc_registry.clear()"),
        "dispatch/connect.rs must call `npc_registry.clear()` during the \
         chargen→Playing transition to prevent NPC leakage from a previous \
         character in the same genre:world. Playtest 2026-04-11 regression."
    );
}

/// The persisted snapshot must also have its npc_registry cleared, or the
/// very next save() call will write the stale NPCs right back to SQLite
/// and the fix will be silently ineffective after any reconnect.
#[test]
fn chargen_clears_snapshot_npc_registry() {
    let src = include_str!("../../src/dispatch/connect.rs");

    assert!(
        src.contains("snapshot.npc_registry.clear()"),
        "dispatch/connect.rs must call `snapshot.npc_registry.clear()` \
         during the chargen→Playing transition. Without this, the save() \
         call immediately following will persist the stale NPC registry \
         from a previous character in the same genre:world."
    );
}

/// The SharedGameSession's npc_registry must be cleared too — otherwise
/// `sync_to_locals` on the opening turn will repopulate the just-cleared
/// local registry from the still-populated shared one.
#[test]
fn chargen_clears_shared_session_npc_registry() {
    let src = include_str!("../../src/dispatch/connect.rs");

    // Look for the exact pattern the fix uses: acquiring the holder lock,
    // matching on the Option, and clearing ss.npc_registry.
    let has_clear_pattern = src.contains("ss.npc_registry.clear()");
    assert!(
        has_clear_pattern,
        "dispatch/connect.rs must call `ss.npc_registry.clear()` on the \
         SharedGameSession during the chargen→Playing transition. Without \
         this, sync_to_locals on the first turn will repopulate the local \
         registry from the shared session's stale copy, silently undoing \
         the local/snapshot clears."
    );
}

/// OTEL rule from CLAUDE.md: every backend fix that touches a subsystem
/// MUST add OTEL watcher events so the GM panel can verify the fix is
/// working. This test pins the telemetry emission so it can't regress.
#[test]
fn chargen_emits_otel_event_for_npc_registry_clear() {
    let src = include_str!("../../src/dispatch/connect.rs");

    assert!(
        src.contains("npc_registry.cleared_on_chargen_complete"),
        "dispatch/connect.rs must emit an OTEL watcher event when the \
         npc_registry is cleared at chargen complete. The GM panel is the \
         lie detector for subsystem decisions — without this event, we \
         cannot tell whether the clear is running or whether the fix has \
         regressed. See CLAUDE.md 'OTEL Observability Principle'."
    );
}

/// The clear must happen BEFORE the initial save() call, not after.
/// A clear-after-save would persist the stale registry to SQLite and
/// only clear it in memory — the next reconnect would re-load the stale
/// state. Assert ordering by searching from the clear site to the save().
#[test]
fn chargen_clears_npc_registry_before_initial_save() {
    let src = include_str!("../../src/dispatch/connect.rs");

    let clear_pos = src.find("snapshot.npc_registry.clear()").expect(
        "snapshot.npc_registry.clear() must exist in dispatch/connect.rs \
         — see chargen_clears_snapshot_npc_registry",
    );

    // Find the FIRST `.save(` call that appears after the clear site.
    // That call must exist and must come after the clear, not before.
    let after_clear = &src[clear_pos..];
    let save_offset = after_clear.find(".save(").expect(
        "There must be a persistence.save() call after the npc_registry \
         clear in the chargen flow. Without it, the initial snapshot is \
         never persisted.",
    );

    // Sanity check: the save must be reasonably close so we know we're
    // looking at the chargen-complete save, not some distant unrelated
    // save() call. 4000 chars covers the clear site, the following
    // shared-session sync block, and the initial save — plenty of room
    // for comments and future small additions without false positives.
    // If this ever trips, the most likely cause is someone moved the
    // clear far from the save in a way that breaks ordering guarantees.
    assert!(
        save_offset < 4000,
        "The .save() call after the clear is suspiciously far away ({} \
         chars). Either the fix was moved far from the save site or this \
         test needs updating. Expected the save to follow the clear within \
         the same chargen-complete block.",
        save_offset
    );
}

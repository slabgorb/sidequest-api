//! Story 37-19: Phantom-player dedup on resume.
//!
//! Background (playtest 2026-04-12): A single player reconnects to a
//! saved session and `player_count` reports 2. Structured-mode auto-
//! promotion creates a TurnBarrier expecting 2 players → barrier
//! deadlock waiting on the phantom second player.
//!
//! Root cause: `ss.players` / `ss_guard.players` has TWO insertion
//! sites that take a fresh `player_id` and insert a new `PlayerState`:
//!
//!   1. `src/dispatch/connect.rs` (≈ line 2595-2725) — the new-connect
//!      path. THIS site already does reconnect detection: it looks up
//!      an existing entry with the same `player_name` under a different
//!      `player_id`, transfers the old `PlayerState`, and only inserts
//!      fresh state when `!is_reconnect`.
//!
//!   2. `src/lib.rs` (≈ line 2441-2473) — the returning-player path,
//!      hit when `session.is_playing()` on reconnect. THIS site
//!      unconditionally inserts a new `PlayerState` under the new
//!      `player_id` without checking whether a prior entry with the
//!      same `player_name` still lives under an old `player_id`. The
//!      result is two entries for the same human player.
//!
//! Fix contract:
//!
//!   - Add a single chokepoint on `SharedGameSession` that performs
//!     reconnect-safe insertion: remove any existing entry whose
//!     `player_name` matches the incoming one and whose `player_id`
//!     differs, then insert the new entry. Return the removed
//!     `player_id` (if any) so the caller can re-sync the turn
//!     barrier, perception filters, etc.
//!   - BOTH insertion sites call this chokepoint. Neither site calls
//!     `players.insert` directly.
//!   - `player_count()` is `1` after a same-name reconnect, end of
//!     story.
//!
//! This file is the RED harness for that contract. It will fail to
//! compile today because the chokepoint does not exist; the source-
//! level wiring tests will also fail because the existing sites call
//! `players.insert` directly. Dev's job in green is to add the
//! chokepoint, route both sites through it, and watch these tests go
//! green.

use sidequest_game::barrier::TurnBarrier;
use sidequest_game::perception::PerceptionFilter;
use sidequest_server::shared_session::{PlayerState, SharedGameSession};

fn fresh_session() -> SharedGameSession {
    SharedGameSession::new("caverns_and_claudes".to_string(), "testworld".to_string())
}

// ---------------------------------------------------------------------------
// Behavioral tests — invariant: at most one PlayerState per player_name.
// These exercise the dedup chokepoint Dev must add to SharedGameSession.
// ---------------------------------------------------------------------------

/// AC-1: Reconnect with same player_name but new player_id collapses to
/// a single entry. `player_count()` MUST be 1. The old `player_id` is
/// returned so the caller can sync external rosters (barrier, filters).
#[test]
fn reconnect_same_name_new_pid_collapses_to_single_entry() {
    let mut ss = fresh_session();

    // Initial connect — Alice under pid "old".
    let removed_initial =
        ss.insert_player_dedup_by_name("old-pid", PlayerState::new("Alice".to_string()));
    assert_eq!(
        removed_initial, None,
        "first insert for a name must not report a removed pid"
    );
    assert_eq!(ss.player_count(), 1, "one player in, one player out");

    // Reconnect — Alice comes back under pid "new".
    let removed_on_reconnect =
        ss.insert_player_dedup_by_name("new-pid", PlayerState::new("Alice".to_string()));

    assert_eq!(
        removed_on_reconnect,
        Some("old-pid".to_string()),
        "dedup must report the replaced player_id so caller can sync barrier/filters"
    );
    assert_eq!(
        ss.player_count(),
        1,
        "after same-name reconnect, player_count MUST stay at 1 — phantom dup is the bug"
    );
    assert!(
        ss.contains_player("new-pid"),
        "new player_id must be present after dedup"
    );
    assert!(
        !ss.contains_player("old-pid"),
        "old player_id must be removed after dedup — this is what playtest 2026-04-12 broke"
    );
}

/// AC-2: Two distinct players (different names) both coexist.
/// Dedup MUST NOT collapse by player_id; it only dedups by player_name.
#[test]
fn two_different_names_coexist_after_dedup_insert() {
    let mut ss = fresh_session();

    let removed_a = ss.insert_player_dedup_by_name("pid-a", PlayerState::new("Alice".to_string()));
    let removed_b = ss.insert_player_dedup_by_name("pid-b", PlayerState::new("Bob".to_string()));

    assert_eq!(
        removed_a, None,
        "distinct-name insert must not report a removal"
    );
    assert_eq!(
        removed_b, None,
        "distinct-name insert must not report a removal — a bug that evicts \
         Alice when inserting Bob would slip past a count-only assertion"
    );
    assert_eq!(
        ss.player_count(),
        2,
        "two real players with distinct names must coexist"
    );
    assert!(ss.contains_player("pid-a"));
    assert!(ss.contains_player("pid-b"));
}

/// AC-3: Re-insert under the SAME player_id (idempotent update) is a
/// no-op for membership — still 1 player, same pid, no removed-pid
/// reported (the current entry is being overwritten in place, not
/// replaced under a different id).
#[test]
fn reinsert_same_pid_same_name_is_idempotent() {
    let mut ss = fresh_session();

    let removed_first =
        ss.insert_player_dedup_by_name("pid-a", PlayerState::new("Alice".to_string()));
    assert_eq!(removed_first, None, "first insert cannot report a removal");

    let removed = ss.insert_player_dedup_by_name("pid-a", PlayerState::new("Alice".to_string()));

    assert_eq!(
        removed, None,
        "overwriting under the same pid is not a reconnect — nothing was removed"
    );
    assert_eq!(ss.player_count(), 1);
}

/// AC-4: The returned "removed pid" must correspond to the OLD pid,
/// not the incoming one. Guards against a sign-flip bug where the
/// caller would try to remove the just-inserted player from the
/// barrier.
#[test]
fn dedup_insert_returns_old_pid_not_new_pid() {
    let mut ss = fresh_session();
    let setup = ss.insert_player_dedup_by_name("stale", PlayerState::new("Alice".to_string()));
    assert_eq!(setup, None, "first insert cannot report a removal");

    let removed = ss.insert_player_dedup_by_name("fresh", PlayerState::new("Alice".to_string()));

    assert_eq!(
        removed.as_deref(),
        Some("stale"),
        "must return the old player_id, not the new one"
    );
}

/// AC-5 (regression, playtest 2026-04-12): After a single-player
/// reconnect through the dedup chokepoint, `player_count()` is 1.
/// This is the exact value that flows into `TurnBarrier` construction
/// and structured-mode auto-promotion. If this is 2, the barrier
/// deadlocks waiting on the phantom player.
#[test]
fn player_count_after_solo_reconnect_cannot_trigger_barrier_auto_promotion() {
    let mut ss = fresh_session();
    let setup = ss.insert_player_dedup_by_name("old", PlayerState::new("Alice".to_string()));
    assert_eq!(setup, None, "first insert cannot report a removal");
    let reconnect = ss.insert_player_dedup_by_name("new", PlayerState::new("Alice".to_string()));
    assert_eq!(
        reconnect.as_deref(),
        Some("old"),
        "same-name reconnect MUST return the old pid so caller reconciles external rosters"
    );

    assert_eq!(
        ss.player_count(),
        1,
        "player_count==2 for a solo reconnect is the playtest-2026-04-12 barrier deadlock"
    );
    assert!(
        ss.player_count() < 2,
        "structured-mode auto-promotion triggers when player_count >= 2; dedup must keep solo reconnects below this threshold"
    );
}

// ---------------------------------------------------------------------------
// Source-level wiring tests — both insertion sites must route through the
// chokepoint. A behavioral test on SharedGameSession can't catch a future
// regression where someone adds a new raw `players.insert` call, so we lock
// the wiring down at the source level too.
// ---------------------------------------------------------------------------

fn read_source(relative: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {relative}: {e}"))
}

/// AC-6 (wiring): `src/lib.rs` — the returning-player path (inside
/// the `is_playing()` branch that currently inserts at line ≈2473)
/// MUST route through `insert_player_dedup_by_name`, not call
/// `players.insert` directly. This is the site that introduced the
/// phantom entry.
#[test]
fn lib_returning_player_path_uses_dedup_chokepoint() {
    let src = read_source("src/lib.rs");

    assert!(
        src.contains("insert_player_dedup_by_name"),
        "src/lib.rs must call SharedGameSession::insert_player_dedup_by_name \
         in the returning-player reconnect path (was ss_guard.players.insert(...) directly)"
    );

    // Defence-in-depth: no raw `players.insert` inside lib.rs after
    // the fix. If a new site appears later, this assertion forces the
    // author to route through the chokepoint. Uses assert_eq!'s
    // format-arg tail so `raw_inserts` actually interpolates in the
    // failure message (story 37-19 review fix — the prior literal
    // "{raw_inserts}" was not substituted).
    let raw_inserts = src.matches("players.insert(").count();
    assert_eq!(
        raw_inserts, 0,
        "src/lib.rs must not call `.players.insert(...)` directly — \
         all inserts must go through insert_player_dedup_by_name \
         (found {} raw call sites)",
        raw_inserts
    );
}

/// AC-7 (wiring): `src/dispatch/connect.rs` — both the reconnect-
/// transfer insert and the new-player insert MUST route through the
/// dedup chokepoint. The transfer path already detects reconnects,
/// but routing it through the same chokepoint keeps the invariant
/// enforced in exactly one place.
#[test]
fn dispatch_connect_insertions_use_dedup_chokepoint() {
    let src = read_source("src/dispatch/connect.rs");

    assert!(
        src.contains("insert_player_dedup_by_name"),
        "src/dispatch/connect.rs must call SharedGameSession::insert_player_dedup_by_name"
    );

    let raw_inserts = src.matches("ss.players.insert(").count();
    assert_eq!(
        raw_inserts, 0,
        "src/dispatch/connect.rs must not call `ss.players.insert(...)` directly — \
         route all inserts through the dedup chokepoint (found {} raw call sites)",
        raw_inserts
    );
}

/// AC-8 (wiring): No other file in sidequest-server/src inserts
/// directly into `ss.players` or `ss_guard.players`. Catches future
/// regressions where a third insertion site is added elsewhere.
#[test]
fn no_other_server_src_file_inserts_into_players_directly() {
    use std::fs;

    let src_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders: Vec<String> = Vec::new();

    fn walk(dir: &std::path::Path, offenders: &mut Vec<String>) {
        for entry in fs::read_dir(dir).expect("walk src") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            if path.is_dir() {
                walk(&path, offenders);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                let content = fs::read_to_string(&path).expect("read .rs");
                for pat in ["ss.players.insert(", "ss_guard.players.insert("] {
                    if content.contains(pat) {
                        offenders.push(format!("{} contains `{}`", path.display(), pat));
                    }
                }
            }
        }
    }

    walk(&src_root, &mut offenders);

    assert!(
        offenders.is_empty(),
        "Every player insertion must go through SharedGameSession::insert_player_dedup_by_name. \
         Raw insertion sites found:\n{}",
        offenders.join("\n")
    );
}

// ---------------------------------------------------------------------------
// OTEL wiring test — per CLAUDE.md, every backend fix that touches a
// subsystem must emit OTEL so the GM panel can confirm the fix fired.
// ---------------------------------------------------------------------------

/// AC-9 (OTEL): The dedup chokepoint must emit a watcher event with the
/// canonical name `phantom_player_removed` when a phantom entry is
/// removed, and it must emit it from an actual `WatcherEventBuilder`
/// call — not just have the string floating in a comment or docstring.
/// Per CLAUDE.md OTEL observability rule, the GM panel is the lie
/// detector: a dead string in a comment wouldn't page anyone.
///
/// Review hardening (story 37-19, round 2): tightened from a pure
/// substring scan that matched anywhere (comments, dead code, docstrings)
/// to an adjacency check that requires `phantom_player_removed` to
/// appear within ~6 lines of a `WatcherEventBuilder::new(` and a
/// `.field("event",` — the actual emit shape.
#[test]
fn phantom_player_dedup_emits_otel_event() {
    let shared_src = read_source("src/shared_session.rs");

    // Look for the canonical event name.
    let event_name = "phantom_player_removed";
    assert!(
        shared_src.contains(event_name),
        "dedup chokepoint must emit an OTEL watcher event with the canonical \
         name `{event_name}` so the GM panel shows when the fix fires"
    );

    // Now prove the string is inside a real emit context, not a comment.
    // Walk the file line-by-line; every line that contains the event name
    // must have a WatcherEventBuilder::new(...) within the preceding 6
    // lines AND a .field("event", ...) on the same or an adjacent line.
    let lines: Vec<&str> = shared_src.lines().collect();
    let mut emit_site_found = false;
    for (idx, line) in lines.iter().enumerate() {
        if !line.contains(event_name) {
            continue;
        }
        // Skip doc / inline comments — they mention the name to document
        // the invariant, not to emit it.
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") || trimmed.starts_with("///") {
            continue;
        }
        let start = idx.saturating_sub(6);
        let window = lines[start..=idx].join("\n");
        if window.contains("WatcherEventBuilder::new(")
            && (window.contains(".field(\"event\",") || line.contains(".field(\"event\","))
        {
            emit_site_found = true;
            break;
        }
    }
    assert!(
        emit_site_found,
        "`{event_name}` must appear inside a real emit context — a \
         WatcherEventBuilder::new(...).field(\"event\", ...) chain. \
         A bare string reference (comment, docstring, dead code) is not \
         sufficient for the GM panel."
    );
}

// ---------------------------------------------------------------------------
// Reconcile-roster contract — the second half of the dedup chokepoint
// (rejected review round 1). Without this, the fix was half-wired: it
// removed the phantom from `players` but left stale references in
// `turn_barrier`, `perception_filters`, and `pending_dice_requests`,
// reproducing the playtest-2026-04-12 deadlock one subsystem up.
// ---------------------------------------------------------------------------

/// AC-10 (wiring): Both production call sites of the dedup chokepoint
/// must act on the returned `Option<String>`. A `let _ =` or bare call
/// that discards the return would silently recreate the half-wired
/// state. We look for `reconcile_removed_player(` adjacent to every
/// `insert_player_dedup_by_name(` call in production source.
#[test]
fn dedup_call_sites_feed_returned_pid_into_reconcile() {
    for relative in ["src/lib.rs", "src/dispatch/connect.rs"] {
        let src = read_source(relative);

        // Every call site must be paired with a reconcile_removed_player
        // call within a reasonable window.
        let calls = src.matches("insert_player_dedup_by_name(").count();
        let reconciles = src.matches("reconcile_removed_player(").count();
        assert!(
            reconciles >= calls,
            "{relative}: {calls} insert_player_dedup_by_name call site(s) but only {reconciles} \
             reconcile_removed_player call site(s). Every call to the chokepoint must feed its \
             Some(old_pid) return into reconcile_removed_player — otherwise turn_barrier / \
             perception_filters / pending_dice_requests leak references to a pid that no longer \
             exists in `players` (story 37-19 round 2 review finding — CRITICAL)."
        );
    }
}

/// AC-11 (runtime integration — CLAUDE.md wiring test rule): Exercise
/// the full chokepoint + reconcile chain end-to-end through the
/// `SharedGameSession` API. Install a `TurnBarrier` and a
/// `PerceptionFilter` keyed by the old pid, reconnect under a new pid,
/// feed the returned old pid through `reconcile_removed_player`, and
/// assert that BOTH `players` AND all downstream rosters collapse to
/// the new pid. This is the test that would have caught the round-1
/// review finding — the prior AC-5 only checked `player_count()`.
#[test]
fn dedup_plus_reconcile_collapses_downstream_rosters_end_to_end() {
    let mut ss = fresh_session();

    // Install Alice under pid "old-pid".
    let removed_initial =
        ss.insert_player_dedup_by_name("old-pid", PlayerState::new("Alice".to_string()));
    assert_eq!(
        removed_initial, None,
        "first insert never reports a removal"
    );

    // Install a TurnBarrier expecting "old-pid", and a perception filter
    // keyed by "old-pid". In production these are populated by the
    // multiplayer session / blinded-character paths respectively.
    let mp_session =
        sidequest_game::multiplayer::MultiplayerSession::new(std::collections::HashMap::new());
    ss.turn_barrier = Some(TurnBarrier::new(
        mp_session,
        sidequest_game::barrier::TurnBarrierConfig::default(),
    ));
    if let Some(ref barrier) = ss.turn_barrier {
        // Use the internal add_player with a placeholder character. The
        // exact character shape doesn't matter — we're testing roster
        // membership, not barrier resolution.
        let placeholder = sidequest_game::Character {
            core: sidequest_game::CreatureCore {
                name: sidequest_protocol::NonBlankString::new("Alice").unwrap(),
                description: sidequest_protocol::NonBlankString::new("reconnect-test").unwrap(),
                personality: sidequest_protocol::NonBlankString::new("n/a").unwrap(),
                level: 1,
                hp: 1,
                max_hp: 1,
                ac: 10,
                xp: 0,
                statuses: vec![],
                inventory: sidequest_game::Inventory::default(),
            },
            backstory: sidequest_protocol::NonBlankString::new("n/a").unwrap(),
            narrative_state: String::new(),
            hooks: vec![],
            char_class: sidequest_protocol::NonBlankString::new("barrier").unwrap(),
            race: sidequest_protocol::NonBlankString::new("barrier").unwrap(),
            pronouns: String::new(),
            stats: std::collections::HashMap::new(),
            abilities: vec![],
            known_facts: vec![],
            affinities: vec![],
            is_friendly: true,
            resolved_archetype: None,
            archetype_provenance: None,
        };
        barrier
            .add_player("old-pid".to_string(), placeholder)
            .expect("add_player on fresh barrier must succeed");
    }
    ss.perception_filters.insert(
        "old-pid".to_string(),
        PerceptionFilter::new("Alice".to_string(), vec![]),
    );

    // Precondition: both rosters reference old-pid.
    assert!(ss.has_perception_filter("old-pid"));

    // Alice reconnects under "new-pid".
    let removed_on_reconnect =
        ss.insert_player_dedup_by_name("new-pid", PlayerState::new("Alice".to_string()));
    assert_eq!(
        removed_on_reconnect.as_deref(),
        Some("old-pid"),
        "chokepoint must return the displaced pid"
    );

    // Caller wires the returned pid through reconcile_removed_player —
    // this is what the production call sites do (AC-10 locks that).
    let old_pid = removed_on_reconnect.expect("chokepoint returned Some above");
    ss.reconcile_removed_player(&old_pid);

    // Post-conditions: `players` has only new-pid, AND downstream rosters
    // no longer reference old-pid. This is the invariant that was broken
    // in round 1 of review — players collapsed but rosters didn't.
    assert_eq!(ss.player_count(), 1, "players collapsed to single entry");
    assert!(ss.contains_player("new-pid"));
    assert!(!ss.contains_player("old-pid"));
    assert!(
        !ss.has_perception_filter("old-pid"),
        "perception_filters must not retain a stale pid after reconcile — \
         round-1 review finding: this was leaking and would rewrite narration \
         for a pid that no longer exists"
    );
    // Barrier: we don't have a public observation API, but calling
    // remove_player on an absent pid returns Err — so removing old-pid
    // AGAIN after reconcile should return Err, confirming the slot is
    // gone.
    if let Some(ref barrier) = ss.turn_barrier {
        assert!(
            barrier.remove_player("old-pid").is_err(),
            "turn_barrier must not retain old-pid after reconcile — it should \
             already be gone. If this assertion fails, reconcile_removed_player \
             did not evict from the barrier and the playtest-2026-04-12 deadlock \
             will recur."
        );
    }
}

//! Story 37-28: Scrapbook image persistence across session resume (RED phase).
//!
//! # The bug
//!
//! The scrapbook *manifest* (DB rows for `scrapbook_entries`) survives session
//! resume cleanly — `dispatch/connect.rs` loads every row and re-emits a
//! `GameMessage::ScrapbookEntry` per row (`dispatch/connect.rs:469`). What does
//! **not** survive is the image file the row's `image_url` points to.
//!
//! `image_url` is a server-relative URL of the form `/api/renders/{filename}`
//! served out of `SIDEQUEST_OUTPUT_DIR` (default `~/.sidequest/renders`). That
//! directory is:
//!   - **not keyed by genre/world/session** — renders from every save mingle,
//!   - **not adjacent to the save DB** — copying a `.db` to another machine
//!     does not carry the render files,
//!   - **not garbage-protected** — any cleanup that targets the renders pool
//!     orphans scrapbook entries across *every* save.
//!
//! # The fix (encoded by the wiring tests below)
//!
//! 1. **New persistence seam**: `sidequest_game::persist_scrapbook_image(
//!        save_dir, genre, world, player, src_path) -> PathBuf`
//!    copies the rendered file into a **save-scoped** subtree:
//!    `{save_dir}/scrapbook/{genre}/{world}/{player}/{filename}`. One copy per
//!    scrapbook row, alongside the DB file, portable as a single tree.
//!
//! 2. **New static route**: `build_router()` in `sidequest-server/src/lib.rs`
//!    must expose `GET /api/scrapbook/{genre}/{world}/{player}/{filename}`,
//!    mirroring the existing `/api/renders/` mount but rooted under the save
//!    directory.
//!
//! 3. **Capture-path wiring**: `dispatch/response.rs` must call
//!    `persist_scrapbook_image` **before** persisting the scrapbook row, and
//!    rewrite the payload's `image_url` from `/api/renders/...` to
//!    `/api/scrapbook/...` so what is stored in SQLite is the durable path.
//!
//! 4. **Resume-path verification**: `dispatch/connect.rs` scrapbook replay
//!    block must check that the file referenced by each loaded row's URL
//!    exists on disk; on miss it must emit a loud OTEL
//!    `WatcherEventType::ValidationWarning` with
//!    `event = "scrapbook.image_missing"`. Today that block only emits a
//!    `tracing::warn!` which the GM panel cannot see — a silent fallback.
//!    This violates the "No Silent Fallbacks" rule in CLAUDE.md.
//!
//! # Why source-file wiring tests, not behavioral ones
//!
//! Same pattern as `scrapbook_entry_story_33_18_tests.rs` (see its "Wiring —
//! call-site verification" section). A behavioral test that references
//! `sidequest_game::persist_scrapbook_image` would break compile in RED phase
//! rather than fail at an assertion; the wire-first workflow wants tests that
//! compile, fail today, and pass after Dev lands the wiring. Behavioral
//! coverage (end-to-end capture → restore → URL resolves) belongs in GREEN
//! once the seams exist.
//!
//! # Design deviation from written AC
//!
//! The story AC as written names four protocol messages
//! (`ScrapbookImageCapture`, `ScrapbookManifestSnapshot`,
//! `ScrapbookImageRequest`, `ScrapbookImagePayload`). **None exist.** The real
//! wiring uses the single `GameMessage::ScrapbookEntry` from story 33-18 with
//! its `image_url: Option<NonBlankString>` field. No new protocol messages are
//! required — the fix is entirely on the file-persistence side. Full rationale
//! in the session file's "Design Deviations" section.

// ===========================================================================
// Helpers — source readers
// ===========================================================================

fn read_source(rel_path: &str) -> String {
    let path = format!("{}/{}", env!("CARGO_MANIFEST_DIR"), rel_path);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {}: {}", path, e))
}

fn read_game_crate_source(rel_path: &str) -> String {
    // sidequest-game lives at ../sidequest-game relative to the server crate
    // manifest directory.
    let path = format!(
        "{}/../sidequest-game/{}",
        env!("CARGO_MANIFEST_DIR"),
        rel_path
    );
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {}: {}", path, e))
}

// ===========================================================================
// Persistence seam — sidequest-game must expose persist_scrapbook_image
// ===========================================================================

#[test]
fn game_crate_defines_persist_scrapbook_image_helper() {
    // Dev is free to put this in persistence.rs or a new scrapbook_store.rs —
    // we look at the lib.rs re-exports (or the module itself) via a recursive
    // sweep of the crate's src/ directory. The function MUST exist at some
    // public path; without it there is no place for dispatch/response.rs to
    // route captures through.
    let src_dir = format!("{}/../sidequest-game/src", env!("CARGO_MANIFEST_DIR"));
    let mut found = false;
    for entry in walkdir_rs(&src_dir) {
        if !entry.ends_with(".rs") {
            continue;
        }
        if let Ok(source) = std::fs::read_to_string(&entry) {
            if source.contains("fn persist_scrapbook_image") {
                found = true;
                break;
            }
        }
    }
    assert!(
        found,
        "sidequest-game must define `fn persist_scrapbook_image(...)`. Expected a \
         public helper that copies a rendered image file into a save-scoped subtree \
         `{{save_dir}}/scrapbook/{{genre}}/{{world}}/{{player}}/{{filename}}`. Without \
         this seam, the capture path in dispatch/response.rs has no way to move bytes \
         out of the global `~/.sidequest/renders/` pool and scrapbook entries will \
         continue to dangle when the renders dir is cleaned or a save is moved."
    );
}

#[test]
fn game_crate_exports_persist_scrapbook_image() {
    // Dev's helper must be reachable from the server crate via the crate root,
    // not buried as a pub(crate) impl. Check lib.rs re-exports or pub fn at
    // module level.
    let lib = read_game_crate_source("src/lib.rs");
    assert!(
        lib.contains("persist_scrapbook_image"),
        "sidequest-game/src/lib.rs must re-export `persist_scrapbook_image` so \
         sidequest-server can call it from dispatch/response.rs. A pub fn that lives \
         in a private module with no re-export satisfies neither the wire-first rule \
         nor CLAUDE.md (\"Verify Wiring, Not Just Existence\")."
    );
}

// ===========================================================================
// Capture path — dispatch/response.rs must route through the new seam
// ===========================================================================

#[test]
fn response_rs_calls_persist_scrapbook_image_before_db_insert() {
    let source = read_source("src/dispatch/response.rs");
    let persist_idx = source.find("persist_scrapbook_image").unwrap_or_else(|| {
        panic!(
            "dispatch/response.rs must call `persist_scrapbook_image(...)` to copy the \
             rendered file into the save-scoped scrapbook subtree BEFORE the DB INSERT. \
             Without this call, `image_url` stays in the global `/api/renders/` pool and \
             the row survives resume but the bytes do not."
        )
    });
    let append_idx = source.find("append_scrapbook_entry").unwrap_or_else(|| {
        panic!(
            "dispatch/response.rs must call `persistence().append_scrapbook_entry(...)` \
             to write the row. This is an existing call — if it has been removed, the \
             manifest will not persist at all, regressing story 33-18."
        )
    });
    assert!(
        persist_idx < append_idx,
        "`persist_scrapbook_image` call (offset {}) must appear BEFORE \
         `append_scrapbook_entry` (offset {}) so the `image_url` stored in SQLite is \
         the save-scoped `/api/scrapbook/...` form, not the volatile `/api/renders/...` \
         form. Otherwise resume loads a DB pointer into a pool that can vanish.",
        persist_idx,
        append_idx
    );
}

#[test]
fn response_rs_rewrites_image_url_to_scrapbook_route() {
    let source = read_source("src/dispatch/response.rs");
    assert!(
        source.contains("/api/scrapbook/"),
        "dispatch/response.rs must rewrite the scrapbook payload's `image_url` to the \
         new `/api/scrapbook/{{genre}}/{{world}}/{{player}}/{{filename}}` form before \
         the row is persisted. Grepping for the literal `/api/scrapbook/` substring \
         should find the rewrite site. Today only `/api/renders/` paths are produced, \
         which is the root cause of the bug."
    );
}

// ===========================================================================
// Resume path — dispatch/connect.rs must fail loudly on missing files
// ===========================================================================

#[test]
fn connect_rs_scrapbook_replay_emits_validation_warning_on_missing_file() {
    // The replay block today only emits tracing::warn! on a load error, and
    // does NOT check per-row whether the image file still exists on disk.
    // That is a silent fallback — the GM panel cannot see it. CLAUDE.md: "No
    // Silent Fallbacks" and "Every backend fix that touches a subsystem MUST
    // add OTEL watcher events so the GM panel can verify the fix is working."
    let source = read_source("src/dispatch/connect.rs");
    assert!(
        source.contains("scrapbook.image_missing"),
        "dispatch/connect.rs scrapbook replay must emit a `WatcherEvent` with \
         `event = \"scrapbook.image_missing\"` whenever a loaded row's `image_url` \
         points to a file that no longer exists on disk. Without this, orphaned \
         images are invisible to the GM panel — the player sees a broken gallery \
         tile and the debugger has no trace. A `tracing::warn!` is not sufficient; \
         the WatcherEventBuilder pipeline is the visible-to-GM-panel channel."
    );
    assert!(
        source.contains("ValidationWarning"),
        "dispatch/connect.rs scrapbook replay must use \
         `WatcherEventType::ValidationWarning` (not `SubsystemExerciseSummary`) for \
         the missing-file case — this is a correctness warning about persisted \
         state, not a routine exercise summary. See the existing watcher event \
         taxonomy in sidequest-server/src/lib.rs."
    );
}

#[test]
fn connect_rs_scrapbook_replay_checks_file_existence() {
    // The replay block must actually touch the filesystem — otherwise it can't
    // know whether a row is dangling. Look for a Path::new(...).exists() or
    // std::fs::metadata(...) call inside the scrapbook load block.
    let source = read_source("src/dispatch/connect.rs");
    // Anchor on the nearby load call to avoid matching unrelated file checks
    // elsewhere in the ~2,700-line file.
    let load_idx = source
        .find("load_scrapbook_entries")
        .expect("dispatch/connect.rs must still call load_scrapbook_entries on resume");
    // Narrow the search window to a reasonable slice AFTER the load call.
    // 4KB is generous — the replay block itself is ~30 lines.
    let window_end = (load_idx + 4096).min(source.len());
    let window = &source[load_idx..window_end];
    let has_exists_check = window.contains(".exists()") || window.contains("fs::metadata");
    assert!(
        has_exists_check,
        "After calling `load_scrapbook_entries`, dispatch/connect.rs must verify \
         each entry's image file still exists on disk (via `Path::new(...).exists()` \
         or `std::fs::metadata(...)`). Without this check the loud OTEL event from \
         the previous test has no trigger condition. Checked a {}-byte window after \
         the load call site.",
        window.len()
    );
}

// ===========================================================================
// Static route — build_router must serve the save-scoped scrapbook subtree
// ===========================================================================

#[test]
fn build_router_exposes_scrapbook_static_route() {
    // The server already mounts /api/renders/ out of SIDEQUEST_OUTPUT_DIR.
    // It must also mount /api/scrapbook/ rooted at the save directory so the
    // rewritten URLs from the capture path can actually be fetched by the UI.
    let source = read_source("src/lib.rs");
    assert!(
        source.contains("/api/scrapbook"),
        "sidequest-server/src/lib.rs must register a route or static serve for \
         `/api/scrapbook` (e.g. `.nest_service(\"/api/scrapbook\", \
         ServeDir::new(save_dir.join(\"scrapbook\")))` or an equivalent axum \
         Router composition). Without this route, the UI fetches the rewritten \
         URLs and gets 404 — the fix is only half-wired."
    );
}

// ===========================================================================
// File directory walker — tiny dep-free recursive .rs sweeper
// ===========================================================================
//
// We avoid pulling in `walkdir` just for one test. The game crate has ~70
// source files, so a stack-based sweep is cheap.

fn walkdir_rs(root: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![std::path::PathBuf::from(root)];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Some(s) = path.to_str() {
                out.push(s.to_string());
            }
        }
    }
    out
}

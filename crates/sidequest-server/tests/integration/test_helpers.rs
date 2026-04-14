//! Shared helpers for the integration test suite.
//!
//! ## Why this exists
//!
//! Many wiring tests `include_str!("../../src/dispatch/mod.rs")` and grep
//! for substrings to verify a subsystem is wired into the dispatch pipeline.
//! This was correct when `dispatch` was a single monolithic module. After
//! ADR-063 ("dispatch handler splitting") moved most of the dispatch code
//! into 22 sibling files under `src/dispatch/`, those tests started
//! failing — the substrings they searched for had moved out of `mod.rs`
//! into `npc_registry.rs`, `persistence.rs`, `beat.rs`, etc.
//!
//! Rather than update each test to point at the right file (which would
//! break again on the next reshuffle), this helper concatenates the entire
//! dispatch directory at compile time. Wiring tests scan the combined
//! string instead of any individual file, so the assertion "this thing is
//! wired somewhere in the dispatch tree" is robust to file moves.
//!
//! When a new file is added under `src/dispatch/`, append it to the
//! `DISPATCH_FILES` slice below. The compiler will verify the path exists
//! at build time.

/// All `src/dispatch/*.rs` files, concatenated at compile time. Add new
/// files here when they're created — the compiler enforces the path.
const DISPATCH_FILES: &[&str] = &[
    include_str!("../../src/dispatch/mod.rs"),
    include_str!("../../src/dispatch/aside.rs"),
    include_str!("../../src/dispatch/audio.rs"),
    include_str!("../../src/dispatch/barrier.rs"),
    include_str!("../../src/dispatch/beat.rs"),
    include_str!("../../src/dispatch/catch_up.rs"),
    include_str!("../../src/dispatch/chargen_summary.rs"),
    include_str!("../../src/dispatch/connect.rs"),
    include_str!("../../src/dispatch/lore_embed_worker.rs"),
    include_str!("../../src/dispatch/lore_sync.rs"),
    include_str!("../../src/dispatch/npc_registry.rs"),
    include_str!("../../src/dispatch/patching.rs"),
    include_str!("../../src/dispatch/persistence.rs"),
    include_str!("../../src/dispatch/pregen.rs"),
    include_str!("../../src/dispatch/prompt.rs"),
    include_str!("../../src/dispatch/render.rs"),
    include_str!("../../src/dispatch/response.rs"),
    include_str!("../../src/dispatch/session_sync.rs"),
    include_str!("../../src/dispatch/slash.rs"),
    include_str!("../../src/dispatch/state_mutations.rs"),
    include_str!("../../src/dispatch/telemetry.rs"),
    include_str!("../../src/dispatch/tropes.rs"),
];

/// Server crate `lib.rs`, baked in at compile time. Some wiring tests
/// scan top-level handlers (DiceThrow, init_tracing, build_router) that
/// live here rather than in the dispatch tree.
pub const LIB_RS: &str = include_str!("../../src/lib.rs");

/// `src/dice_dispatch.rs` — separate from the dispatch tree because dice
/// resolution predates ADR-063 and lives at the crate root.
#[allow(dead_code)]
pub const DICE_DISPATCH_RS: &str = include_str!("../../src/dice_dispatch.rs");

/// `src/shared_session.rs` — multiplayer session state, also crate-root.
#[allow(dead_code)]
pub const SHARED_SESSION_RS: &str = include_str!("../../src/shared_session.rs");

use std::sync::OnceLock;

/// All dispatch files concatenated, with production-code-only filtering.
///
/// Each file's `#[cfg(test)]` block is stripped so wiring assertions
/// don't accidentally match test-internal helpers. Cached after first
/// call via `OnceLock` so the returned `&'static str` can be bound to
/// `let` without temporary-lifetime issues at the call site.
pub fn dispatch_source_combined() -> &'static str {
    static CACHED: OnceLock<String> = OnceLock::new();
    CACHED
        .get_or_init(|| {
            DISPATCH_FILES
                .iter()
                .map(|src| src.split("#[cfg(test)]").next().unwrap_or(src))
                .collect::<Vec<&str>>()
                .join("\n")
        })
        .as_str()
}

/// Combined dispatch + lib.rs + dice_dispatch + shared_session sources.
/// Use this when a wiring test needs to verify "this thing is wired
/// somewhere in the server crate," regardless of whether it landed in
/// dispatch/ or at the crate root.
///
/// Note: this does NOT strip `#[cfg(test)]` blocks. The naive
/// `.split("#[cfg(test)]").next()` approach is broken for lib.rs because
/// that file declares test sub-modules at the top with one-line
/// `#[cfg(test)] mod xxx_tests;` declarations — splitting at the first
/// occurrence would discard the entire production body. Wiring assertions
/// search for substrings unique enough that incidental matches in test
/// code are not a real risk.
#[allow(dead_code)]
pub fn server_source_combined() -> &'static str {
    static CACHED: OnceLock<String> = OnceLock::new();
    CACHED
        .get_or_init(|| {
            let mut combined = String::new();
            combined.push_str(dispatch_source_combined());
            combined.push('\n');
            combined.push_str(LIB_RS);
            combined.push('\n');
            combined.push_str(DICE_DISPATCH_RS);
            combined.push('\n');
            combined.push_str(SHARED_SESSION_RS);
            combined
        })
        .as_str()
}

/// Production code only from `lib.rs` (test modules stripped).
#[allow(dead_code)]
pub fn lib_rs_production() -> &'static str {
    LIB_RS.split("#[cfg(test)]").next().unwrap_or(LIB_RS)
}

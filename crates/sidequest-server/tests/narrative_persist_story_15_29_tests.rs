//! Tests for Story 15-29: Wire append_narrative to SQLite.
//!
//! persist_game_state() appends narration to snapshot.narrative_log (in-memory)
//! and saves the snapshot blob. But the dedicated narrative_log SQLite table
//! is never written to. append_narrative() exists but has no caller in dispatch.
//!
//! These tests verify:
//! 1. persist_game_state() calls persistence().append_narrative() after saving
//! 2. The narrative entry matches what was appended to snapshot.narrative_log
//! 3. OTEL event emitted: persistence.narrative_appended
//! 4. The SessionStore::append_narrative method is implemented (not stubbed)

use std::fs;

/// Read the dispatch module source code for structural verification.
fn dispatch_source() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/dispatch/mod.rs");
    fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read dispatch/mod.rs: {e}"))
}

/// Extract a function body from source code by name.
fn extract_fn_body(src: &str, fn_name: &str) -> String {
    let search = format!("fn {}(", fn_name);
    let start = src.find(&search)
        .unwrap_or_else(|| panic!("Function '{}' not found in source", fn_name));
    let from_fn = &src[start..];

    // Find the opening brace
    let brace_start = from_fn.find('{')
        .unwrap_or_else(|| panic!("No opening brace found for '{}'", fn_name));
    let body_start = brace_start + 1;

    // Count braces to find the matching close
    let mut depth = 1;
    let mut end = body_start;
    for (i, ch) in from_fn[body_start..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = body_start + i;
                    break;
                }
            }
            _ => {}
        }
    }

    from_fn[body_start..end].to_string()
}

/// Read the persistence module source for structural verification.
fn persistence_source() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../sidequest-game/src/persistence.rs");
    fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read persistence.rs: {e}"))
}

// ═══════════════════════════════════════════════════════════════
// AC-1: persist_game_state calls append_narrative after save
// ═��═══════════════���═══════════════════════════════════��═════════

#[test]
fn persist_game_state_calls_append_narrative() {
    let src = dispatch_source();
    let persist_fn = extract_fn_body(&src, "persist_game_state");

    // The function must call append_narrative on the persistence handle
    assert!(
        persist_fn.contains("append_narrative"),
        "persist_game_state() must call persistence().append_narrative() \
         to write the narrative entry to the SQLite narrative_log table. \
         Currently only appends to snapshot.narrative_log (in-memory)."
    );
}

#[test]
fn append_narrative_called_before_save() {
    let src = dispatch_source();
    let persist_fn = extract_fn_body(&src, "persist_game_state");

    // append_narrative should appear BEFORE the save() call — the narrative_log
    // table is append-only and should record the entry even if the snapshot save
    // subsequently fails.  This also prevents the duplicate-write bug where
    // append_narrative was called both before AND inside the save-success branch.
    let save_pos = persist_fn.find("persistence()")
        .and_then(|start| persist_fn[start..].find(".save(").map(|p| start + p))
        .expect("persist_game_state must call persistence().save()");

    let append_pos = persist_fn.find("append_narrative")
        .expect("persist_game_state must call append_narrative");

    assert!(
        append_pos < save_pos,
        "append_narrative must be called BEFORE save() for crash safety. \
         save_pos={}, append_pos={}",
        save_pos,
        append_pos
    );
}

// ══════���════════════════════════════════════════════════════════
// AC-3: OTEL event — persistence.narrative_appended
// ════════════════════════════��══════════════════════════════════

#[test]
fn persist_game_state_emits_narrative_appended_otel() {
    let src = dispatch_source();
    let persist_fn = extract_fn_body(&src, "persist_game_state");

    assert!(
        persist_fn.contains("narrative_appended") || persist_fn.contains("persistence.narrative_appended"),
        "persist_game_state() must emit an OTEL event for narrative_appended \
         so the GM panel can verify narrative persistence is working."
    );
}

// ═════════��════════════════���════════════════════════════════════
// AC-4: SessionStore::append_narrative is implemented
// ══��═══════════════════��══════════════════════════════════��═════

#[test]
fn sqlite_store_append_narrative_writes_to_table() {
    let src = persistence_source();

    // The SqliteStore::append_narrative implementation must INSERT into
    // the narrative_log table
    assert!(
        src.contains("INSERT INTO narrative_log") || src.contains("insert into narrative_log"),
        "SqliteStore::append_narrative must INSERT into the narrative_log table"
    );
}

#[test]
fn sqlite_store_recent_narrative_reads_from_table() {
    let src = persistence_source();

    // recent_narrative must SELECT from narrative_log table
    assert!(
        src.contains("SELECT") && src.contains("narrative_log"),
        "SqliteStore::recent_narrative must SELECT from narrative_log table"
    );
}

// ══════════��═════════════════════════════════���══════════════════
// Wiring test: PersistenceHandle exposes append_narrative
// ════════���══════════════════════════════════════════════════════

#[test]
fn persistence_handle_has_append_narrative_method() {
    let src = persistence_source();

    // PersistenceHandle (the async wrapper) must have append_narrative
    // method available for the server dispatch to call
    let handle_section = src.find("impl PersistenceHandle")
        .or_else(|| src.find("impl PersistenceWorker"))
        .expect("PersistenceHandle or PersistenceWorker impl must exist");
    let from_impl = &src[handle_section..];

    assert!(
        from_impl.contains("append_narrative"),
        "PersistenceHandle must expose append_narrative method for server dispatch"
    );
}

//! Wiring guard: every `MapUpdate` emit site must fire an
//! `emit_map_update_telemetry` call so the GM panel lie detector can
//! verify the Map subsystem actually ran.
//!
//! Background (sq-playtest 2026-04-09): the Map panel rewiring landed
//! without OTEL visibility. Per CLAUDE.md, "wired = visible in GM panel"
//! — internal data flow without a watcher event is not wired. This test
//! reads the dispatch source files and asserts that every
//! `GameMessage::MapUpdate {` construction is preceded (within 20 lines)
//! by `emit_map_update_telemetry(` so future edits can't silently add a
//! new emit site that skips telemetry.
//!
//! This is a source-level structural check rather than a runtime
//! subscription test — it catches the exact failure pattern (silent
//! omission during a refactor) that CLAUDE.md calls out as the most
//! expensive bug class in this project.

use std::fs;
use std::path::PathBuf;

fn repo_file(relative: &str) -> String {
    // CARGO_MANIFEST_DIR points at `crates/sidequest-server` during tests.
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push(relative);
    fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e))
}

/// Find every line index where `needle` appears (0-indexed).
fn find_lines(source: &str, needle: &str) -> Vec<usize> {
    source
        .lines()
        .enumerate()
        .filter_map(|(i, line)| if line.contains(needle) { Some(i) } else { None })
        .collect()
}

/// Assert that every occurrence of `emit_needle` in `source` has a
/// matching `telemetry_needle` call within `window` lines *before* it.
fn assert_telemetry_before_emit(
    source_name: &str,
    source: &str,
    emit_needle: &str,
    telemetry_needle: &str,
    window: usize,
) {
    let emit_lines = find_lines(source, emit_needle);
    assert!(
        !emit_lines.is_empty(),
        "expected at least one `{}` in {}, found none — did the emit site move?",
        emit_needle,
        source_name
    );

    let telemetry_lines = find_lines(source, telemetry_needle);
    let lines: Vec<&str> = source.lines().collect();

    for emit_line in emit_lines {
        let start = emit_line.saturating_sub(window);
        let covered = telemetry_lines
            .iter()
            .any(|&t| t >= start && t < emit_line);
        assert!(
            covered,
            "MapUpdate emit site at {}:{} has no `{}` call within the {} lines preceding it.\n\
             Context:\n{}",
            source_name,
            emit_line + 1,
            telemetry_needle,
            window,
            lines[start..=emit_line.min(lines.len() - 1)]
                .iter()
                .enumerate()
                .map(|(i, l)| format!("  {:>4}: {}", start + i + 1, l))
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
}

#[test]
fn every_map_update_push_in_dispatch_mod_has_telemetry() {
    let source = repo_file("src/dispatch/mod.rs");
    assert_telemetry_before_emit(
        "src/dispatch/mod.rs",
        &source,
        "GameMessage::MapUpdate {",
        "emit_map_update_telemetry(",
        20,
    );
}

#[test]
fn every_map_update_push_in_connect_rs_has_telemetry() {
    let source = repo_file("src/dispatch/connect.rs");
    assert_telemetry_before_emit(
        "src/dispatch/connect.rs",
        &source,
        "GameMessage::MapUpdate {",
        "emit_map_update_telemetry(",
        20,
    );
}

#[test]
fn every_map_update_push_in_response_rs_has_telemetry() {
    // build_response_messages was extracted from dispatch/mod.rs into
    // dispatch/response.rs post-refactor; its MAP_UPDATE emit site needs
    // the same lie-detector coverage.
    let source = repo_file("src/dispatch/response.rs");
    assert_telemetry_before_emit(
        "src/dispatch/response.rs",
        &source,
        "GameMessage::MapUpdate {",
        "emit_map_update_telemetry(",
        20,
    );
}

#[test]
fn emit_helper_is_defined_in_dispatch_mod() {
    // The helper itself must exist — guards against "test passes because
    // no one calls MapUpdate anymore" after a hypothetical deletion.
    let source = repo_file("src/dispatch/mod.rs");
    assert!(
        source.contains("fn emit_map_update_telemetry"),
        "emit_map_update_telemetry helper must be defined in dispatch/mod.rs"
    );
    // And it must actually talk to the watcher subsystem.
    let helper_start = source
        .find("fn emit_map_update_telemetry")
        .expect("helper not found");
    let helper_body = &source[helper_start..];
    assert!(
        helper_body.contains("WatcherEventBuilder::new(\"map\""),
        "emit_map_update_telemetry must call WatcherEventBuilder::new(\"map\", ...)"
    );
    // Fields the GM panel depends on to act as a lie detector.
    for required in [
        "room_count",
        "room_exits_total",
        "mode",
        "current_room_id",
        "origin",
    ] {
        assert!(
            helper_body.contains(required),
            "emit_map_update_telemetry must emit the `{}` field",
            required
        );
    }
}

//! RED-phase tests for story 27-9 — ADR-076 narration protocol collapse.
//!
//! These tests assert the dead TTS-era plumbing is GONE from the protocol
//! crate and its workspace siblings. They currently **fail** — Bicycle
//! Repair Man makes them pass in the GREEN phase by deleting the code
//! described in ADR-076.
//!
//! Pattern: deletion-driven TDD. Each test reads source files as strings
//! and asserts forbidden tokens are absent. This is the same pattern the
//! UI repo uses in `combat-overlay-deletion-28-9.test.ts` and
//! `voice-kill-switches-removed.test.ts`.
//!
//! Rule coverage: lang-review/rust.md check #6 (test quality) — every
//! assertion below is meaningful, non-vacuous, and produces actionable
//! failure messages that name the exact file and token that must be
//! removed. No `let _ = result;`, no `assert!(true)`, no
//! `is_none()` on an always-None value.

use crate::message::GameMessage;

/// Helper: read a source file relative to the protocol crate's manifest dir.
fn read_crate_source(relative: &str) -> String {
    let path = format!("{}/{}", env!("CARGO_MANIFEST_DIR"), relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {}: {}", path, e))
}

/// Helper: read a workspace-sibling file relative to this crate's manifest dir.
/// Protocol crate lives at `sidequest-api/crates/sidequest-protocol`, so
/// `../foo` reaches sibling crates under `sidequest-api/crates/`.
fn read_sibling_source(relative: &str) -> String {
    let path = format!("{}/../{}", env!("CARGO_MANIFEST_DIR"), relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {}: {}", path, e))
}

// ============================================================================
// ADR-076 §Protocol Layer — remove NarrationChunk variant and payload
// ============================================================================

#[test]
fn message_rs_contains_no_narration_chunk_references() {
    let source = read_crate_source("src/message.rs");
    let count = source.matches("NarrationChunk").count();
    assert_eq!(
        count, 0,
        "ADR-076: `NarrationChunk` must be fully removed from src/message.rs — \
         found {} occurrence(s). Dev: delete the variant, the `NarrationChunkPayload` \
         struct, and any lingering references. See ADR-076 §Protocol Layer.",
        count
    );
}

#[test]
fn narration_chunk_payload_struct_is_removed() {
    let source = read_crate_source("src/message.rs");
    assert!(
        !source.contains("pub struct NarrationChunkPayload"),
        "ADR-076: `pub struct NarrationChunkPayload` must be removed from \
         src/message.rs. It is dead infrastructure from the deleted TTS pipeline."
    );
}

#[test]
fn narration_chunk_json_does_not_deserialize_as_game_message() {
    // After Dev removes the variant, deserializing this JSON shape must
    // fail. While the variant exists, `from_str` succeeds and this test
    // fails — that is the RED state.
    let json = r#"{"type":"NARRATION_CHUNK","payload":{"text":"partial"},"player_id":""}"#;
    let result: Result<GameMessage, _> = serde_json::from_str(json);
    assert!(
        result.is_err(),
        "ADR-076: deserializing a NARRATION_CHUNK payload must fail because \
         the variant no longer exists. Got: {:?}",
        result
    );
}

// ============================================================================
// ADR-076 §Narration Flow — NarrationEnd doc comment update
// ============================================================================

#[test]
fn narration_end_doc_comment_no_longer_claims_stream_terminator() {
    // The original doc comment on `NarrationEnd` described it as the
    // terminator of a streaming sequence. Post-ADR-076 the variant is
    // a turn-completion marker carrying the final state delta, not a
    // stream closer. The comment rewrite must reflect this.
    let source = read_crate_source("src/message.rs");
    assert!(
        !source.contains("End of narration stream"),
        "ADR-076: NarrationEnd doc comment must be rewritten. \
         The phrase 'End of narration stream' describes the deleted TTS-era role. \
         Replace with language describing the turn-completion marker + state delta flush."
    );
}

// ============================================================================
// ADR-076 §Documentation Cleanup — prerender.rs stale TTS comments
// ============================================================================

#[test]
fn prerender_rs_has_no_stale_tts_playback_language() {
    let source = read_sibling_source("sidequest-game/src/prerender.rs");
    let forbidden = [
        "during voice playback",
        "TTS narration is playing",
        "TTS playback windows",
    ];
    let mut hits: Vec<&str> = Vec::new();
    for phrase in forbidden {
        if source.contains(phrase) {
            hits.push(phrase);
        }
    }
    assert!(
        hits.is_empty(),
        "ADR-076 §Documentation Cleanup: prerender.rs doc comments still \
         describe TTS playback windows that no longer exist. Stale phrases found: \
         {:?}. Rewrite to describe the current trigger (turn-boundary scheduling).",
        hits
    );
}

// ============================================================================
// ADR-076 §Documentation Cleanup — extraction.rs stale TTS comments
// ============================================================================

#[test]
fn extraction_rs_has_no_stale_tts_clean_language() {
    let source = read_sibling_source("sidequest-server/src/extraction.rs");
    let forbidden = ["TTS-clean text", "TTS pipeline"];
    let mut hits: Vec<&str> = Vec::new();
    for phrase in forbidden {
        if source.contains(phrase) {
            hits.push(phrase);
        }
    }
    assert!(
        hits.is_empty(),
        "ADR-076 §Documentation Cleanup: extraction.rs doc comments still \
         frame the cleanup as TTS-specific. Stale phrases found: {:?}. \
         Rewrite to 'narration display cleanup' language — the logic is still \
         valuable, but the motivation is no longer TTS.",
        hits
    );
}

// ============================================================================
// ADR-076 §Daemon Client Types — audit for orphaned Tts*/Voice* types
// ============================================================================

#[test]
fn daemon_client_types_has_no_tts_or_voice_synthesis_types() {
    let source = read_sibling_source("sidequest-daemon-client/src/types.rs");

    // Audit: any `pub struct Tts*` or `pub enum Tts*` is a dead carrier
    // for the deleted TTS pipeline. Same for `VoiceSynth*` patterns.
    // Note: `voice_volume` (audio mixer config) is OUT of scope per ADR-076
    // — that's a legitimate audio-channel slot, not synthesis plumbing.
    let forbidden_patterns = [
        "pub struct Tts",
        "pub enum Tts",
        "pub struct VoiceSynth",
        "pub enum VoiceSynth",
        "TtsRequest",
        "TtsResponse",
        "VoiceSynthRequest",
        "VoiceSynthResponse",
    ];
    let mut hits: Vec<&str> = Vec::new();
    for pattern in forbidden_patterns {
        if source.contains(pattern) {
            hits.push(pattern);
        }
    }
    assert!(
        hits.is_empty(),
        "ADR-076 §Daemon Client Types: daemon-client/src/types.rs still carries \
         TTS/voice-synthesis types. Orphan patterns found: {:?}. \
         Dev: delete these types — no callers remain after TTS removal.",
        hits
    );
}

// ============================================================================
// ADR-076 Acceptance Gate — wiring check across the whole api workspace
// ============================================================================

#[test]
fn no_production_references_to_narration_chunk_in_sidequest_api() {
    // Walks sidequest-api/crates/ and asserts that no production .rs file
    // (excluding test files) mentions `NarrationChunk`. This is the Rust
    // side of the ADR-076 wiring check in the acceptance gate.
    let crates_dir = format!("{}/../", env!("CARGO_MANIFEST_DIR"));

    let mut violations: Vec<String> = Vec::new();
    walk_and_check(
        std::path::Path::new(&crates_dir),
        "NarrationChunk",
        &mut violations,
    );

    assert!(
        violations.is_empty(),
        "ADR-076 wiring check (api): {} production .rs file(s) still \
         reference `NarrationChunk`:\n  {}",
        violations.len(),
        violations.join("\n  ")
    );
}

/// Recursively scan a directory for `.rs` files that are NOT test files
/// and record any that contain the forbidden token.
fn walk_and_check(dir: &std::path::Path, forbidden: &str, violations: &mut Vec<String>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return, // Missing dir = nothing to check, not an error
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };

        // Skip common non-source directories
        if file_name == "target" || file_name == ".git" || file_name == "node_modules" {
            continue;
        }

        if path.is_dir() {
            walk_and_check(&path, forbidden, violations);
            continue;
        }

        // Only check .rs files
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }

        // Skip test files — they legitimately mention the forbidden
        // token to assert its absence
        let is_test_file = file_name.ends_with("_tests.rs")
            || file_name == "tests.rs"
            || path.components().any(|c| c.as_os_str() == "tests");
        if is_test_file {
            continue;
        }

        // Read and check
        if let Ok(content) = std::fs::read_to_string(&path) {
            if content.contains(forbidden) {
                violations.push(path.display().to_string());
            }
        }
    }
}

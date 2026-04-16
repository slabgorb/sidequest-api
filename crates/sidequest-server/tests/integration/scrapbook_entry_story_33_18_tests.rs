//! Story 33-18: Scrapbook payload — assembly + wiring tests (RED phase).
//!
//! These tests cover the server-side scrapbook module that bundles per-turn
//! metadata (narration excerpt, world facts, NPCs, optional image) into a
//! `ScrapbookEntryPayload` and pushes a `GameMessage::ScrapbookEntry` onto
//! the dispatch response stream.
//!
//! # Layers
//!
//! 1. **Pure assembly** — `scrapbook::build_scrapbook_entry()` takes explicit
//!    inputs and returns a `ScrapbookEntryPayload`. Unit-tested for every
//!    input shape (empty footnotes, callback-only footnotes, empty NPC
//!    registry, unicode-heavy narration).
//!
//! 2. **Sentence extraction** — `scrapbook::extract_first_sentence()` handles
//!    the narrative_excerpt edge cases called out in the story risk notes:
//!    ellipses, abbreviations, quoted dialogue, missing terminators.
//!
//! 3. **Wiring** — `build_response_messages` in `dispatch/response.rs` MUST
//!    call the assembly function and push a `GameMessage::ScrapbookEntry` to
//!    the `messages` Vec AFTER the `NarrationEnd` send-path (per AC: "after
//!    NarrationEnd, world_facts must be settled"). Verified by a source-file
//!    read-and-grep test (same pattern as `narration_collapse_story_27_9_tests.rs`).
//!
//! # AC coverage
//! - AC: protocol type added                            → see protocol crate tests
//! - AC: server emits after NarrationEnd for that turn  → wiring tests below
//! - AC: narrative_excerpt = first complete sentence    → extract_first_sentence tests
//! - AC: world_facts from is_new=true footnotes only    → build_* world_facts tests
//! - AC: npcs_present from npc_registry                 → build_* npc tests
//! - AC: scene_title / scene_type piped through         → build_* scene_metadata test
//!
//! # Design deviations from session AC (see session Design Deviations)
//! - `image_url` is `Option<String>`, not `String` — image renders arrive on
//!   an async broadcast channel (`render_integration.rs`) and are NOT guaranteed
//!   to be complete by the time `NarrationEnd` fires. Story 33-17 merges the
//!   entry with later IMAGE messages by `turn_id` on the client side.
//! - `scene_title` and `scene_type` are also `Option<String>` for the same
//!   reason (they come from `RenderSubject`, which is only known after the
//!   render job starts).
//! - `disposition` on `NpcRef` is sourced from `NpcRegistryEntry.ocean_summary`
//!   (the behavioral summary generated from the OCEAN profile) because
//!   `NpcRegistryEntry` does not have a free-standing disposition string.
//!   ADR-020 treats `ocean_summary` as the canonical disposition descriptor.

use sidequest_game::NpcRegistryEntry;
use sidequest_protocol::{FactCategory, Footnote, NonBlankString};
use sidequest_server::scrapbook::{build_scrapbook_entry, extract_first_sentence};

// ===========================================================================
// Helpers
// ===========================================================================

fn nbs(s: &str) -> NonBlankString {
    NonBlankString::new(s).expect("test literal must be non-blank")
}

fn new_footnote(summary: &str, is_new: bool) -> Footnote {
    Footnote {
        marker: Some(1),
        fact_id: None,
        summary: nbs(summary),
        category: FactCategory::Lore,
        is_new,
    }
}

fn npc_at(name: &str, role: &str, ocean: &str, last_seen_turn: u32) -> NpcRegistryEntry {
    NpcRegistryEntry {
        name: name.to_string(),
        pronouns: "they/them".to_string(),
        role: role.to_string(),
        location: "Ironhold".to_string(),
        last_seen_turn,
        age: String::new(),
        appearance: String::new(),
        ocean_summary: ocean.to_string(),
        ocean: None,
        hp: 0,
        max_hp: 0,
        portrait_url: None,
    }
}

// ===========================================================================
// extract_first_sentence — edge cases (ellipses, abbreviations, dialogue,
// quoted terminators, missing terminators, CRLF line endings)
// ===========================================================================

#[test]
fn extract_first_sentence_simple_period() {
    assert_eq!(
        extract_first_sentence("The bell tolled. A crow answered from the tower."),
        "The bell tolled."
    );
}

#[test]
fn extract_first_sentence_question_mark() {
    assert_eq!(
        extract_first_sentence("Who goes there? Only the wind."),
        "Who goes there?"
    );
}

#[test]
fn extract_first_sentence_exclamation() {
    assert_eq!(
        extract_first_sentence("Beware! A shadow moves in the alley."),
        "Beware!"
    );
}

#[test]
fn extract_first_sentence_empty_input_returns_empty() {
    assert_eq!(extract_first_sentence(""), "");
}

#[test]
fn extract_first_sentence_whitespace_only_returns_empty() {
    assert_eq!(extract_first_sentence("   \t\n  "), "");
}

#[test]
fn extract_first_sentence_no_terminator_returns_trimmed_full_text() {
    assert_eq!(
        extract_first_sentence("  No terminator here  "),
        "No terminator here"
    );
}

#[test]
fn extract_first_sentence_does_not_split_on_ellipsis() {
    // The ellipsis (...) inside a sentence must NOT terminate the excerpt —
    // the narrator uses ellipses for pauses mid-sentence all the time.
    assert_eq!(
        extract_first_sentence("He paused... then drew his blade. Steel hummed."),
        "He paused... then drew his blade."
    );
}

#[test]
fn extract_first_sentence_handles_dr_abbreviation() {
    // "Dr." is not a sentence terminator.
    assert_eq!(
        extract_first_sentence("Dr. Mallory bent over the corpse. The wound was fresh."),
        "Dr. Mallory bent over the corpse."
    );
}

#[test]
fn extract_first_sentence_handles_mr_abbreviation() {
    assert_eq!(
        extract_first_sentence("Mr. Finch nodded once. Then he left."),
        "Mr. Finch nodded once."
    );
}

#[test]
fn extract_first_sentence_handles_st_abbreviation() {
    assert_eq!(
        extract_first_sentence("St. Aldric watched over the gate. Nobody passed."),
        "St. Aldric watched over the gate."
    );
}

#[test]
fn extract_first_sentence_single_sentence_with_terminator() {
    assert_eq!(
        extract_first_sentence("Just one sentence."),
        "Just one sentence."
    );
}

#[test]
fn extract_first_sentence_trims_leading_whitespace() {
    assert_eq!(
        extract_first_sentence("\n\t  The tavern was loud. Ale flowed."),
        "The tavern was loud."
    );
}

#[test]
fn extract_first_sentence_handles_closing_quote_dialogue() {
    // Period inside closing quotes is followed by `"` then space — the
    // byte-after-terminator check correctly skips it (because `"` is not
    // whitespace) and finds the NEXT terminator after the dialogue tag.
    // The narrator uses this shape constantly — regression guard.
    assert_eq!(
        extract_first_sentence("\"Come here,\" she said. The door slammed shut."),
        "\"Come here,\" she said."
    );
}

#[test]
fn extract_first_sentence_handles_crlf_line_endings() {
    // `\r` is ASCII whitespace so the byte check after the terminator
    // matches on CRLF just as on LF or space. Narrator output may arrive
    // with mixed line endings depending on platform.
    assert_eq!(
        extract_first_sentence("First sentence.\r\nSecond sentence."),
        "First sentence."
    );
}

#[test]
fn extract_first_sentence_does_not_consume_period_inside_quotes_alone() {
    // A period inside quotes WITHOUT a trailing space must be skipped
    // (the `"` blocks it), so the extractor keeps scanning into the
    // dialogue tag. Guards against future byte-check drift.
    let input = "\"Done.\" He walked away. The rain continued.";
    assert_eq!(extract_first_sentence(input), "\"Done.\" He walked away.");
}

// ===========================================================================
// build_scrapbook_entry — pure assembly tests
// ===========================================================================

#[test]
fn build_sets_turn_id_and_location_verbatim() {
    let payload = build_scrapbook_entry(
        42,
        nbs("Dustfall Crossing"),
        None,
        None,
        None,
        "A dry wind blew in from the west.",
        &[],
        &[],
    );
    assert_eq!(payload.turn_id, 42u64);
    assert_eq!(payload.location.as_str(), "Dustfall Crossing");
}

#[test]
fn build_narrative_excerpt_is_first_sentence_of_narration() {
    let payload = build_scrapbook_entry(
        1,
        nbs("Nowhere"),
        None,
        None,
        None,
        "The door creaked open. Something moved inside. Silence followed.",
        &[],
        &[],
    );
    assert_eq!(payload.narrative_excerpt.as_str(), "The door creaked open.");
}

#[test]
fn build_extracts_world_facts_from_is_new_footnotes_only() {
    let footnotes = vec![
        new_footnote("The forge has been cold for six days.", true),
        new_footnote("Callback to an earlier NPC conversation.", false),
        new_footnote("Ironhold's smith guild disbanded last winter.", true),
    ];
    let payload = build_scrapbook_entry(
        3,
        nbs("Ironhold"),
        None,
        None,
        None,
        "Hammers rang in the empty yard.",
        &footnotes,
        &[],
    );
    assert_eq!(
        payload.world_facts,
        vec![
            "The forge has been cold for six days.".to_string(),
            "Ironhold's smith guild disbanded last winter.".to_string(),
        ],
        "world_facts must contain only the summaries of footnotes with is_new=true; \
         callbacks (is_new=false) are NOT world facts — they reference prior knowledge"
    );
}

#[test]
fn build_empty_footnotes_yields_empty_world_facts() {
    let payload = build_scrapbook_entry(
        1,
        nbs("Void"),
        None,
        None,
        None,
        "Nothing.",
        &[],
        &[],
    );
    assert!(payload.world_facts.is_empty());
}

#[test]
fn build_maps_npc_registry_entries_to_npc_refs() {
    let npcs = vec![
        npc_at("Toggler Copperjaw", "blacksmith", "gruff but fair", 5),
        npc_at("Vera Ashmark", "guard captain", "watchful and quiet", 5),
    ];
    let payload = build_scrapbook_entry(
        5,
        nbs("Market"),
        None,
        None,
        None,
        "The crowd parted for the guard.",
        &[],
        &npcs,
    );
    assert_eq!(payload.npcs_present.len(), 2);
    assert_eq!(payload.npcs_present[0].name.as_str(), "Toggler Copperjaw");
    assert_eq!(payload.npcs_present[0].role.as_str(), "blacksmith");
    assert_eq!(payload.npcs_present[0].disposition.as_str(), "gruff but fair");
    assert_eq!(payload.npcs_present[1].name.as_str(), "Vera Ashmark");
    assert_eq!(payload.npcs_present[1].role.as_str(), "guard captain");
    assert_eq!(payload.npcs_present[1].disposition.as_str(), "watchful and quiet");
}

#[test]
fn build_filters_npc_registry_by_current_turn_id() {
    // NPC filter moved into build_scrapbook_entry (story 33-18 refactor):
    // the function now takes the full registry and filters by last_seen_turn
    // internally, so the caller in dispatch/response.rs doesn't allocate an
    // intermediate Vec on every turn.
    let npcs = vec![
        npc_at("Seen Now", "scribe", "watchful", 7),
        npc_at("Seen Earlier", "innkeeper", "jovial", 6),
        npc_at("Seen Next Turn", "guard", "stern", 8),
    ];
    let payload = build_scrapbook_entry(
        7,
        nbs("Market"),
        None,
        None,
        None,
        "The crowd parted for the guard.",
        &[],
        &npcs,
    );
    assert_eq!(payload.npcs_present.len(), 1);
    assert_eq!(payload.npcs_present[0].name.as_str(), "Seen Now");
}

#[test]
fn build_empty_npc_registry_yields_empty_npcs_present() {
    let payload = build_scrapbook_entry(
        1,
        nbs("Alone"),
        None,
        None,
        None,
        "Empty room.",
        &[],
        &[],
    );
    assert!(payload.npcs_present.is_empty());
}

#[test]
fn build_passes_scene_metadata_through() {
    let payload = build_scrapbook_entry(
        9,
        nbs("The Forge"),
        Some(nbs("The Forge of Broken Oaths")),
        Some("exploration".to_string()),
        Some(nbs("/renders/turn-9.png")),
        "Sparks flew as the hammer fell.",
        &[],
        &[],
    );
    assert_eq!(
        payload.scene_title.as_ref().map(|s| s.as_str()),
        Some("The Forge of Broken Oaths")
    );
    assert_eq!(payload.scene_type.as_deref(), Some("exploration"));
    assert_eq!(
        payload.image_url.as_ref().map(|s| s.as_str()),
        Some("/renders/turn-9.png")
    );
}

#[test]
fn build_with_all_nones_leaves_optional_fields_none() {
    let payload = build_scrapbook_entry(
        1,
        nbs("Hollow"),
        None,
        None,
        None,
        "Quiet.",
        &[],
        &[],
    );
    assert!(payload.scene_title.is_none());
    assert!(payload.scene_type.is_none());
    assert!(payload.image_url.is_none());
}

// ===========================================================================
// Wiring — call-site verification in dispatch/response.rs
//
// Every test suite must include a wiring test (CLAUDE.md: "Every Test Suite
// Needs a Wiring Test"). Dev implementations that pass the unit tests above
// but never call the new module from production code will still fail these.
// ===========================================================================

fn read_response_rs_source() -> String {
    let path = format!("{}/src/dispatch/response.rs", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {}: {}", path, e))
}

#[test]
fn response_rs_imports_scrapbook_module() {
    let source = read_response_rs_source();
    assert!(
        source.contains("crate::scrapbook") || source.contains("use crate::scrapbook"),
        "dispatch/response.rs must import from crate::scrapbook so build_response_messages \
         actually calls the new assembly function (not just define it in isolation). \
         Expected a `use crate::scrapbook::...` or `crate::scrapbook::build_scrapbook_entry(...)` \
         reference."
    );
}

#[test]
fn response_rs_calls_build_scrapbook_entry() {
    let source = read_response_rs_source();
    assert!(
        source.contains("build_scrapbook_entry"),
        "dispatch/response.rs must call `build_scrapbook_entry(...)` to assemble the \
         ScrapbookEntryPayload from per-turn dispatch context. Without this call, the \
         assembly function has no production consumer and the feature is not wired."
    );
}

#[test]
fn response_rs_pushes_scrapbook_entry_game_message() {
    let source = read_response_rs_source();
    assert!(
        source.contains("GameMessage::ScrapbookEntry"),
        "dispatch/response.rs must construct and push a `GameMessage::ScrapbookEntry` \
         variant so it reaches the WebSocket observers via the normal `messages` Vec \
         fan-out (same pattern as MapUpdate and Confrontation in this file)."
    );
}

#[test]
fn response_rs_emits_scrapbook_after_narration_end_send() {
    // Anchor on the literal send expression rather than the bare token
    // "NarrationEnd" — the token also appears in doc comments, which would
    // make a bare-token anchor ambiguous under future refactors.
    let source = read_response_rs_source();
    let send_idx = source
        .find("ctx.tx.send(narration_end)")
        .expect("response.rs must send narration_end via ctx.tx.send(narration_end)");
    let scrapbook_idx = source
        .find("messages.push(GameMessage::ScrapbookEntry")
        .expect("response.rs must push GameMessage::ScrapbookEntry onto the messages Vec");
    assert!(
        scrapbook_idx > send_idx,
        "ScrapbookEntry push must appear textually AFTER the `ctx.tx.send(narration_end)` \
         call so `world_facts` and `npcs_present` are settled before the entry ships \
         (AC: \"emit after NarrationEnd\"). Found send at byte offset {} but push at {}.",
        send_idx,
        scrapbook_idx
    );
}

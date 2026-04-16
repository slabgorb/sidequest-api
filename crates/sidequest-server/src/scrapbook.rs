//! Scrapbook entry assembly — story 33-18.
//!
//! Bundles per-turn metadata (turn id, location, narration excerpt, world
//! facts, NPCs present, optional image) into a `ScrapbookEntryPayload` that
//! the Scrapbook widget (story 33-17) renders as a single gallery card.
//!
//! Pure functions only — no global state, no OTEL, no I/O. The caller in
//! `dispatch/response.rs` is responsible for emitting telemetry and for
//! filtering the NPC registry to the current turn before calling in.

use sidequest_game::NpcRegistryEntry;
use sidequest_protocol::{Footnote, NonBlankString, NpcRef, ScrapbookEntryPayload};

/// Known sentence-boundary abbreviations that must NOT terminate the
/// first-sentence extraction. Tokens are compared case-insensitively and
/// the trailing period is part of the match.
///
/// Intentionally short — this list exists only to cover the abbreviations
/// the narrator actually produces in practice. Extend as new cases surface
/// during playtest, not speculatively.
const SENTENCE_ABBREVIATIONS: &[&str] = &[
    "dr.", "mr.", "mrs.", "ms.", "st.", "jr.", "sr.", "vs.", "etc.", "e.g.", "i.e.",
];

/// Returns the first complete sentence of `text`.
///
/// A "sentence" ends at the first `.`, `?`, or `!` that:
///   1. Is not part of an ellipsis (`...`)
///   2. Is not the terminator of a known abbreviation (`Dr.`, `Mr.`, …)
///   3. Is followed by whitespace or end of input
///
/// Leading and trailing whitespace is trimmed. If no terminator matches the
/// rules above, the entire trimmed input is returned — better to surface a
/// full paragraph than a silently empty excerpt.
pub fn extract_first_sentence(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let bytes = trimmed.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        let c = bytes[i];
        if c != b'.' && c != b'?' && c != b'!' {
            i += 1;
            continue;
        }

        // Skip ellipsis — treat "..." as a single non-terminating token.
        if c == b'.' && i + 2 < len && bytes[i + 1] == b'.' && bytes[i + 2] == b'.' {
            i += 3;
            continue;
        }
        // Handle accidental 2-dot ellipsis defensively.
        if c == b'.' && i + 1 < len && bytes[i + 1] == b'.' {
            i += 2;
            continue;
        }

        // Must be followed by whitespace or end-of-input to count as a terminator.
        let at_end = i + 1 >= len;
        let followed_by_space = !at_end && bytes[i + 1].is_ascii_whitespace();
        if !at_end && !followed_by_space {
            i += 1;
            continue;
        }

        // Check abbreviation — look back at the preceding word including the period.
        if c == b'.' && is_abbreviation_terminator(trimmed, i) {
            i += 1;
            continue;
        }

        // Found the end of the first sentence. Include the terminator.
        return trimmed[..=i].to_string();
    }

    // No terminator matched — return the whole trimmed input.
    trimmed.to_string()
}

/// Returns true if the period at byte index `period_idx` is the terminator
/// of a known abbreviation (e.g. the `.` in "Dr."). Matches against
/// `SENTENCE_ABBREVIATIONS` case-insensitively without allocating.
fn is_abbreviation_terminator(text: &str, period_idx: usize) -> bool {
    // Walk backward from the period to the previous whitespace or start.
    let bytes = text.as_bytes();
    let mut start = period_idx;
    while start > 0 && !bytes[start - 1].is_ascii_whitespace() {
        start -= 1;
    }
    let token = &bytes[start..=period_idx];
    SENTENCE_ABBREVIATIONS
        .iter()
        .any(|abbr| abbr.as_bytes().eq_ignore_ascii_case(token))
}

/// Assembles a `ScrapbookEntryPayload` from pure per-turn inputs.
///
/// - `world_facts` contains the `summary` of each footnote with `is_new=true`
///   (callbacks are excluded — they reference prior knowledge).
/// - `npcs_present` maps each `NpcRegistryEntry` to a `NpcRef` using the
///   behavioral summary as the disposition, with `role` as a non-empty
///   fallback so the UI never shows a blank disposition string.
/// - `narrative_excerpt` is the first complete sentence of `narration`.
/// - `scene_title`, `scene_type`, and `image_url` are passed through
///   as-is. Callers pass `None` when the render pipeline has not yet
///   completed for this turn — images arrive on an async broadcast
///   channel (see `render_integration::spawn_image_broadcaster_with_throttle`)
///   and the UI merges a later `GameMessage::Image` by `turn_id`. Threading
///   the latest completed `RenderSubject` through `DispatchContext` is a
///   follow-up; for now this story emits `None` and accepts the deferred
///   image join on the client side.
#[allow(clippy::too_many_arguments)]
pub fn build_scrapbook_entry(
    turn_id: u64,
    location: NonBlankString,
    scene_title: Option<NonBlankString>,
    scene_type: Option<String>,
    image_url: Option<NonBlankString>,
    narration: &str,
    footnotes: &[Footnote],
    npc_registry: &[NpcRegistryEntry],
) -> ScrapbookEntryPayload {
    let world_facts: Vec<String> = footnotes
        .iter()
        .filter(|f| f.is_new)
        .map(|f| f.summary.as_str().to_string())
        .collect();

    // Filter NPCs to just those seen this turn — keeps the scrapbook
    // entry focused and avoids an intermediate Vec allocation at the
    // call site. The u64→u32 cast is safe because last_seen_turn is a
    // u32 counter bounded by interaction count (see ADR-051).
    let turn_id_u32 = u32::try_from(turn_id).unwrap_or(u32::MAX);
    let npcs_present: Vec<NpcRef> = npc_registry
        .iter()
        .filter(|e| e.last_seen_turn == turn_id_u32)
        .filter_map(|entry| {
            let name = NonBlankString::new(&entry.name).ok()?;
            let role = NonBlankString::new(&entry.role).ok()?;
            let disposition = NonBlankString::new(&entry.ocean_summary)
                .unwrap_or_else(|_| role.clone());
            Some(NpcRef {
                name,
                role,
                disposition,
            })
        })
        .collect();

    let excerpt = extract_first_sentence(narration);
    let narrative_excerpt = NonBlankString::new(&excerpt)
        .expect("narration is non-empty when build_scrapbook_entry is called after NarrationEnd");

    ScrapbookEntryPayload {
        turn_id,
        scene_title,
        scene_type,
        location,
        image_url,
        narrative_excerpt,
        world_facts,
        npcs_present,
    }
}

//! Story 37-12: Narrator never re-declares confrontation after first emission
//!
//! # Background
//!
//! Story 37-13 built the encounter creation gate (`dispatch/encounter_gate.rs`),
//! which covers six observable cases for how to route a narrator-emitted
//! `"confrontation": <type>` signal against the current `snapshot.encounter`
//! state (Created, Redeclared, ReplacedPreBeat, RejectedMidEncounter,
//! UnknownType). The dispatch side is READY to receive re-emits from the
//! narrator.
//!
//! 37-12 is the prompt-side half: the narrator is never ASKED to re-emit.
//! Three concrete regressions in `dispatch/prompt.rs`:
//!
//! 1. Line 421 (current) contains the active misdirection
//!    `"Only emit confrontation on the turn the encounter STARTS."` — the
//!    exact inverse of the 37-13 gate's contract. The narrator reads the
//!    prompt, sees this instruction, and dutifully stays silent through
//!    every subsequent turn.
//!
//! 2. The `"AVAILABLE ENCOUNTER TYPES"` block (lines 412–434) is gated on
//!    `ctx.snapshot.encounter.is_none()`. Once an encounter is active, the
//!    narrator has zero visibility into what OTHER confrontation types exist
//!    in the genre pack, so even if it wanted to signal a transition, it
//!    couldn't name the destination type.
//!
//! 3. No OTEL event records whether the prompt included transition guidance.
//!    The GM panel (CLAUDE.md § OTEL Observability) cannot verify that the
//!    narrator was ever told about re-emit, so a prompt-template regression
//!    would be invisible.
//!
//! # Acceptance Criteria
//!
//! | AC                   | What it proves                                             |
//! |----------------------|------------------------------------------------------------|
//! | AC-NoOnlyOnStart     | The "Only emit ... on the turn the encounter STARTS"      |
//! |                      | misdirection is removed.                                   |
//! | AC-TransitionMarker  | prompt.rs contains a `TRANSITION CONFRONTATION` section    |
//! |                      | header for the active-encounter branch.                    |
//! | AC-ReemitGuidance    | prompt.rs tells the narrator to re-emit `confrontation`   |
//! |                      | when the scene shifts to a new type.                       |
//! | AC-AltTypesListed    | The active-encounter branch iterates `confrontation_defs`  |
//! |                      | so the narrator sees what other types it can transition   |
//! |                      | to.                                                         |
//! | AC-OTEL              | prompt.rs emits `encounter.transition_guidance_injected`   |
//! |                      | with an `alternative_count` field.                         |
//! | AC-Wiring            | Guidance block lives in `build_prompt_context`, the sole   |
//! |                      | production entry point to narrator prompt assembly.        |
//!
//! # Test strategy
//!
//! Source-scan tests against `prompt.rs` via `include_str!`, matching the
//! convention established by Story 28-4
//! (`encounter_context_wiring_story_28_4_tests.rs`). `DispatchContext` carries
//! 50+ fields including `AppState`, shared async session handles, render
//! queues, music directors, and an unbounded mpsc sender — building one in an
//! integration test would be larger than the fix itself.
//!
//! Source scanning is acceptable here because `build_prompt_context` is the
//! SOLE production entry point to narrator prompt assembly. Strings present
//! in that function body are reachable from the live dispatch path; the
//! CLAUDE.md wiring rule is satisfied by the fact that the scanned file IS
//! production code, not a helper nobody calls.
//!
//! The OTEL event name and marker strings (`TRANSITION CONFRONTATION`,
//! `encounter.transition_guidance_injected`, `alternative_count`) are a
//! contract between this test file and the Dev implementing the fix. If Dev
//! prefers different phrasing, update both sides in the same commit.

/// Live snapshot of `dispatch/prompt.rs`. One copy, many assertions.
const PROMPT_SRC: &str = include_str!("../../src/dispatch/prompt.rs");

// ---------------------------------------------------------------------------
// AC-NoOnlyOnStart — the misdirection is removed
// ---------------------------------------------------------------------------

/// The narrator prompt currently instructs:
///   "Only emit confrontation on the turn the encounter STARTS."
/// That line is the proximate cause of 37-12. The dispatch gate (37-13) will
/// never see a re-emit while this line is in the prompt, because the narrator
/// is explicitly told not to send one. This test fails today and must pass
/// after the fix.
#[test]
fn prompt_no_longer_tells_narrator_to_only_emit_on_start() {
    assert!(
        !PROMPT_SRC.contains("Only emit confrontation on the turn the encounter STARTS"),
        "dispatch/prompt.rs still contains the Story 37-12 misdirection. \
         The narrator must be allowed to re-emit confrontation on scene \
         transitions — the 37-13 gate is built to route re-emits, but the \
         narrator never sends them while this instruction is in the prompt."
    );
}

// ---------------------------------------------------------------------------
// AC-TransitionMarker — active-encounter branch carries the guidance header
// ---------------------------------------------------------------------------

/// The active-encounter branch of `build_prompt_context` must inject a
/// dedicated `TRANSITION CONFRONTATION` section so the narrator can locate
/// re-emit guidance at a stable position in the prompt. Using an explicit
/// section marker (matching the style of other sections like
/// `=== AVAILABLE CONFRONTATIONS ===`) makes the guidance visible to prompt
/// analysis tools and to the GM panel's prompt inspector.
#[test]
fn prompt_includes_transition_confrontation_section_marker() {
    assert!(
        PROMPT_SRC.contains("TRANSITION CONFRONTATION"),
        "dispatch/prompt.rs must inject a 'TRANSITION CONFRONTATION' section \
         when an encounter is active. This is the section where the narrator \
         learns it may re-emit a `confrontation` field when the scene shifts \
         to a different encounter type."
    );
}

// ---------------------------------------------------------------------------
// AC-ReemitGuidance — narrator is told how, not just that
// ---------------------------------------------------------------------------

/// Section headers are not enough; the guidance must name the action.
/// Accept any of several phrasings so Dev has room to pick wording that
/// reads naturally alongside the rest of the prompt, but at least one must
/// be present.
#[test]
fn prompt_instructs_narrator_to_reemit_on_scene_shift() {
    let candidates = [
        "re-emit",
        "re-declare",
        "emit a new confrontation",
        "emit the new confrontation",
        "emit a different confrontation",
        "emit the transition",
    ];
    let matched: Vec<&&str> = candidates
        .iter()
        .filter(|c| PROMPT_SRC.contains(**c))
        .collect();
    assert!(
        !matched.is_empty(),
        "dispatch/prompt.rs must instruct the narrator to re-emit `confrontation` \
         when the scene transitions to a different type. None of the accepted \
         phrasings were found: {:?}",
        candidates
    );
}

// ---------------------------------------------------------------------------
// AC-AltTypesListed — narrator sees other types it can shift to
// ---------------------------------------------------------------------------

/// Telling the narrator it MAY transition is useless if the prompt also
/// hides every other confrontation type. The `TRANSITION CONFRONTATION`
/// block must iterate `ctx.confrontation_defs` so the narrator sees the
/// alternatives by name. We verify by locating the marker and checking for
/// a `confrontation_defs` reference within a reasonable window below it.
#[test]
fn transition_block_iterates_confrontation_defs() {
    let trans_idx = PROMPT_SRC
        .find("TRANSITION CONFRONTATION")
        .expect("TRANSITION CONFRONTATION marker missing — see AC-TransitionMarker test");

    let window_end = (trans_idx + 2000).min(PROMPT_SRC.len());
    let window = &PROMPT_SRC[trans_idx..window_end];

    assert!(
        window.contains("confrontation_defs"),
        "The TRANSITION CONFRONTATION block must iterate `ctx.confrontation_defs` \
         so the narrator sees the list of other types it could transition to. \
         Without this, the narrator knows it MAY transition but not WHAT IT CAN \
         transition TO."
    );
}

// ---------------------------------------------------------------------------
// AC-OTEL — GM panel can verify the guidance was injected
// ---------------------------------------------------------------------------

/// Per CLAUDE.md § "OTEL Observability", every subsystem decision must emit
/// a watcher event the GM panel can observe. If the narrator prompt fails
/// to include transition guidance (template regression, branch skipped,
/// feature flag flipped), the only way to detect it is an OTEL event that
/// fires on every successful injection.
#[test]
fn prompt_emits_transition_guidance_otel_event() {
    assert!(
        PROMPT_SRC.contains("encounter.transition_guidance_injected"),
        "dispatch/prompt.rs must emit a watcher event \
         `encounter.transition_guidance_injected` whenever the transition \
         guidance section is added to the narrator prompt. Without this \
         event, the GM panel cannot distinguish 'no transition happened' \
         from 'narrator was never told about transitions'."
    );
}

/// The OTEL event must carry `alternative_count` — the number of other
/// confrontation types shown to the narrator. A count of zero means the
/// guidance was injected but empty (genre pack has one type), and a count
/// equal to `confrontation_defs.len() - 1` is the happy path. Either way
/// the field is the mechanical fingerprint of the alternatives list.
#[test]
fn otel_transition_event_carries_alternative_count_field() {
    assert!(
        PROMPT_SRC.contains("alternative_count"),
        "The `encounter.transition_guidance_injected` event must include \
         an `alternative_count` field so the GM panel can verify the \
         narrator was shown N alternative confrontation types. Without \
         this field the event is decorative, not diagnostic."
    );
}

// ---------------------------------------------------------------------------
// AC-Wiring — guidance lives in the production prompt builder, not a helper
// ---------------------------------------------------------------------------

/// CLAUDE.md § "Verify Wiring, Not Just Existence" — the fix must live in
/// `build_prompt_context` (or a helper it calls on every turn), not in a
/// dead module nobody imports. Because this test file scans the source of
/// `dispatch/prompt.rs`, and `build_prompt_context` is the only production
/// narrator prompt entry point in that file, the marker strings passing
/// above implicitly prove production reachability. This test makes the
/// coupling explicit by checking that the TRANSITION CONFRONTATION marker
/// appears somewhere after the `fn build_prompt_context` declaration.
#[test]
fn transition_guidance_is_below_build_prompt_context_declaration() {
    let build_fn_start = PROMPT_SRC
        .find("fn build_prompt_context")
        .expect("build_prompt_context declaration not found — narrator prompt \
                 entry point has moved; update this test");

    let tail = &PROMPT_SRC[build_fn_start..];
    assert!(
        tail.contains("TRANSITION CONFRONTATION"),
        "TRANSITION CONFRONTATION marker must appear below the \
         `fn build_prompt_context` declaration. If the marker is ONLY above \
         it, the guidance is in a dead helper that `build_prompt_context` \
         does not call — a CLAUDE.md wiring violation."
    );
}

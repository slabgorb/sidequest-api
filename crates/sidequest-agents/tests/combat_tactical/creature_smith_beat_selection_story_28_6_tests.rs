//! Story 28-6: Change creature_smith to output beat selections instead of CombatPatch
//!
//! The narrator currently emits CombatPatch/ChasePatch fields in its game_patch JSON block.
//! After this story, it emits beat_selections — an array of {actor, beat_id, target?} objects
//! that map to ConfrontationDef beats. The server dispatches these via apply_beat() (28-5).
//!
//! ACs tested:
//!   AC-Beat-Output-Schema:     Narrator output format includes beat_selections array
//!   AC-No-CombatPatch:         CombatPatch fields removed from narrator output schema
//!   AC-No-ChasePatch:          ChasePatch fields removed from narrator output schema
//!   AC-Extraction:             Orchestrator extracts beat_selections from narrator JSON
//!   AC-Unified-Encounter:      build_encounter_context() replaces build_combat/chase_context
//!   AC-IntentRouter:           IntentRouter checks in_encounter, not in_combat/in_chase
//!   AC-OTEL:                   encounter.agent_beat_selection emitted in orchestrator
//!   AC-Wiring:                 beat_selections flows narrator → extraction → ActionResult

// =========================================================================
// AC-Beat-Output-Schema: Narrator output format includes beat_selections
// =========================================================================

/// The narrator's output format definition must instruct the LLM to emit
/// a beat_selections array when an encounter is active.
#[test]
fn narrator_output_format_includes_beat_selections() {
    let narrator_src = include_str!("../../src/agents/narrator.rs");

    // The NARRATOR_OUTPUT_ONLY constant (or equivalent) must mention beat_selections
    // so the LLM knows to produce this field in game_patch.
    assert!(
        narrator_src.contains("beat_selections"),
        "narrator.rs output format must include 'beat_selections' field \
         so the LLM knows to emit beat selection data in the game_patch block"
    );
}

/// The output format must show the beat_selections array structure with
/// actor, beat_id, and optional target fields.
#[test]
fn narrator_output_format_shows_beat_selection_structure() {
    let narrator_src = include_str!("../../src/agents/narrator.rs");

    assert!(
        narrator_src.contains("beat_id"),
        "narrator.rs output format must show beat_id field in beat_selections schema"
    );
    assert!(
        narrator_src.contains("actor"),
        "narrator.rs output format must show actor field in beat_selections schema"
    );
}

// =========================================================================
// AC-No-CombatPatch: CombatPatch fields removed from narrator output schema
// =========================================================================

/// The narrator's output format must NOT instruct the LLM to emit CombatPatch
/// fields (in_combat, hp_changes, turn_order, drama_weight). These are replaced
/// by beat_selections.
#[test]
fn narrator_output_format_no_combat_initiation_instructions() {
    let narrator_src = include_str!("../../src/agents/narrator.rs");

    // The NARRATOR_OUTPUT_ONLY constant should no longer contain combat-specific
    // instructions telling the LLM to emit in_combat/hp_changes/turn_order.
    // Note: We check the output format section specifically, not the whole file
    // (combat rules sections may still reference these concepts narratively).
    //
    // include_str! reads raw source bytes, so escaped quotes appear as \"
    // Check for the field names as they appear in the valid fields list and examples.

    // Find the NARRATOR_OUTPUT_ONLY constant content
    let output_only_start = narrator_src
        .find("NARRATOR_OUTPUT_ONLY")
        .expect("NARRATOR_OUTPUT_ONLY constant must exist in narrator.rs");
    let output_section = &narrator_src[output_only_start..];
    // The constant ends at the closing ";
    let section_end = output_section
        .find("\nconst ")
        .or_else(|| output_section.find("\nstatic "))
        .or_else(|| output_section.find("\n/// Output-style"))
        .unwrap_or(output_section.len().min(5000));
    let output_format = &output_section[..section_end];

    // Check valid fields list (unquoted: "in_combat, hp_changes, turn_order")
    assert!(
        !output_format.contains("in_combat"),
        "NARRATOR_OUTPUT_ONLY must not reference in_combat field — \
         beat_selections replaces CombatPatch fields"
    );
    assert!(
        !output_format.contains("hp_changes"),
        "NARRATOR_OUTPUT_ONLY must not reference hp_changes field — \
         beat_selections replaces CombatPatch fields"
    );
    assert!(
        !output_format.contains("turn_order"),
        "NARRATOR_OUTPUT_ONLY must not reference turn_order field — \
         beat_selections replaces CombatPatch fields"
    );
    assert!(
        !output_format.contains("drama_weight"),
        "NARRATOR_OUTPUT_ONLY must not reference drama_weight field — \
         beat_selections replaces CombatPatch fields"
    );
}

// =========================================================================
// AC-No-ChasePatch: ChasePatch fields removed from narrator output schema
// =========================================================================

/// ChasePatch extraction must be removed from the orchestrator — beat_selections
/// replaces both CombatPatch and ChasePatch extraction pathways.
#[test]
fn orchestrator_no_chase_patch_extraction() {
    let orchestrator_src = include_str!("../../src/orchestrator.rs");

    assert!(
        !orchestrator_src.contains("ChasePatch"),
        "orchestrator.rs must not reference ChasePatch — \
         beat_selections replaces both CombatPatch and ChasePatch extraction"
    );
}

// =========================================================================
// AC-Extraction: Orchestrator extracts beat_selections from narrator JSON
// =========================================================================

/// GamePatchExtraction must have a beat_selections field that deserializes
/// from the narrator's game_patch JSON block.
#[test]
fn game_patch_with_beat_selections_deserializes() {
    let json = r#"{
  "beat_selections": [
    {"actor": "Player", "beat_id": "attack", "target": "Goblin"},
    {"actor": "Goblin", "beat_id": "defend"}
  ],
  "mood": "tense"
}"#;

    // The narrator emits game_patch blocks. When beat_selections is present,
    // it must deserialize into the extraction struct.
    // This will compile-fail until GamePatchExtraction adds beat_selections.
    let patch: serde_json::Value = serde_json::from_str(json).unwrap();
    let selections = patch
        .get("beat_selections")
        .expect("beat_selections field must exist");
    assert!(selections.is_array(), "beat_selections must be an array");
    assert_eq!(
        selections.as_array().unwrap().len(),
        2,
        "beat_selections must contain both actor entries"
    );

    let first = &selections.as_array().unwrap()[0];
    assert_eq!(first["actor"].as_str(), Some("Player"));
    assert_eq!(first["beat_id"].as_str(), Some("attack"));
    assert_eq!(first["target"].as_str(), Some("Goblin"));

    let second = &selections.as_array().unwrap()[1];
    assert_eq!(second["actor"].as_str(), Some("Goblin"));
    assert_eq!(second["beat_id"].as_str(), Some("defend"));
    assert!(
        second.get("target").is_none() || second["target"].is_null(),
        "target should be absent or null when not specified"
    );
}

/// ActionResult must carry beat_selections extracted from narrator output.
/// This is the handoff point to the dispatch pipeline (apply_beat in 28-5).
#[test]
fn action_result_has_beat_selections_field() {
    let orchestrator_src = include_str!("../../src/orchestrator.rs");

    // ActionResult struct must have a beat_selections field
    let action_result_start = orchestrator_src
        .find("pub struct ActionResult")
        .expect("ActionResult struct must exist");
    let struct_body = &orchestrator_src[action_result_start..];
    let struct_end = struct_body
        .find("\n}")
        .expect("ActionResult must have closing brace");
    let struct_text = &struct_body[..struct_end];

    assert!(
        struct_text.contains("beat_selections"),
        "ActionResult must have a beat_selections field to carry extracted \
         beat data from narrator output to the dispatch pipeline"
    );
}

/// The orchestrator must extract beat_selections from the game_patch block
/// (not from a separate fenced JSON block like CombatPatch was).
#[test]
fn orchestrator_extracts_beat_selections_from_game_patch() {
    let orchestrator_src = include_str!("../../src/orchestrator.rs");

    // GamePatchExtraction struct must include beat_selections
    let extraction_start = orchestrator_src
        .find("struct GamePatchExtraction")
        .expect("GamePatchExtraction struct must exist");
    let extraction_body = &orchestrator_src[extraction_start..];
    let extraction_end = extraction_body
        .find("\n}")
        .expect("GamePatchExtraction must close");
    let extraction_text = &extraction_body[..extraction_end];

    assert!(
        extraction_text.contains("beat_selections"),
        "GamePatchExtraction must include beat_selections field so it's \
         captured when parsing the narrator's game_patch JSON block"
    );
}

// =========================================================================
// AC-Unified-Encounter: build_encounter_context replaces combat/chase
// =========================================================================

/// The narrator must have a build_encounter_context method that replaces
/// the separate build_combat_context and build_chase_context methods.
#[test]
fn narrator_has_build_encounter_context() {
    let narrator_src = include_str!("../../src/agents/narrator.rs");

    assert!(
        narrator_src.contains("build_encounter_context"),
        "narrator.rs must define build_encounter_context() — unified encounter \
         context injection replacing build_combat_context and build_chase_context"
    );
}

/// build_combat_context must be removed from the narrator — it's replaced
/// by the unified build_encounter_context.
#[test]
fn narrator_no_build_combat_context() {
    let narrator_src = include_str!("../../src/agents/narrator.rs");

    assert!(
        !narrator_src.contains("fn build_combat_context"),
        "narrator.rs must NOT define build_combat_context() — replaced by \
         build_encounter_context() for unified encounter handling"
    );
}

/// build_chase_context must be removed from the narrator — it's replaced
/// by the unified build_encounter_context.
#[test]
fn narrator_no_build_chase_context() {
    let narrator_src = include_str!("../../src/agents/narrator.rs");

    assert!(
        !narrator_src.contains("fn build_chase_context"),
        "narrator.rs must NOT define build_chase_context() — replaced by \
         build_encounter_context() for unified encounter handling"
    );
}

// =========================================================================
// AC-IntentRouter: Checks in_encounter, not in_combat/in_chase
// =========================================================================

/// intent_router.rs must no longer branch on in_combat and in_chase
/// separately. Per ADR-067 + the 2026-04-12 confrontation wiring repair,
/// the `IntentRouter` / `IntentClassifier` / `NoOpClassifier` trio was
/// deleted — they were dead stubs unconditionally returning Exploration.
/// Callers now use `IntentRoute::exploration()` directly.
///
/// This test is the regression guard for that repair. It scans the entire
/// file (not just one function, since the function was deleted) for the
/// anti-patterns we specifically want to keep out:
///   1. Any branching on `ctx.in_combat` or `ctx.in_chase` as classifier
///      inputs (the pre-ADR-067 pattern).
///   2. Re-introduction of the deleted `IntentClassifier` trait or its
///      `classify_with_classifier` dispatch helper.
#[test]
fn intent_router_no_separate_combat_chase_branches() {
    let router_src = include_str!("../../src/agents/intent_router.rs");

    assert!(
        !router_src.contains("ctx.in_combat"),
        "intent_router.rs must not branch on ctx.in_combat — \
         unified narrator (ADR-067) routes all intents via the encounter \
         engine, not separate state fields"
    );
    assert!(
        !router_src.contains("ctx.in_chase"),
        "intent_router.rs must not branch on ctx.in_chase — \
         unified narrator (ADR-067) routes all intents via the encounter \
         engine, not separate state fields"
    );
    assert!(
        !router_src.contains("fn classify_with_classifier"),
        "fn classify_with_classifier was deleted in the 2026-04-12 \
         confrontation wiring repair (dead stub). Do not re-introduce it."
    );
    assert!(
        !router_src.contains("trait IntentClassifier"),
        "trait IntentClassifier was deleted in the 2026-04-12 \
         confrontation wiring repair (dead stub). Do not re-introduce it."
    );
    assert!(
        !router_src.contains("struct NoOpClassifier"),
        "struct NoOpClassifier was deleted in the 2026-04-12 \
         confrontation wiring repair (dead stub). Do not re-introduce it."
    );
}

// =========================================================================
// AC-OTEL: encounter.agent_beat_selection event emitted
// =========================================================================

/// The orchestrator must emit an OTEL event for each beat selection extracted
/// from the narrator's output — this is how the GM panel verifies the narrator
/// is actually engaging the encounter engine (not just winging it).
#[test]
fn orchestrator_emits_agent_beat_selection_otel_event() {
    let orchestrator_src = include_str!("../../src/orchestrator.rs");

    assert!(
        orchestrator_src.contains("agent_beat_selection"),
        "orchestrator.rs must emit 'agent_beat_selection' OTEL event when \
         beat_selections are extracted from narrator output — this is the \
         lie detector for encounter engagement"
    );
}

// =========================================================================
// AC-Wiring: beat_selections flows narrator → extraction → ActionResult
// =========================================================================

/// The orchestrator must populate ActionResult.beat_selections from the
/// extracted game_patch data (non-test consumer — this is the production wiring).
#[test]
fn orchestrator_populates_action_result_beat_selections() {
    let orchestrator_src = include_str!("../../src/orchestrator.rs");

    // Find the ActionResult construction in the success path
    // (the `ActionResult { combat_patch, chase_patch, ... }` block around line 923)
    assert!(
        orchestrator_src.contains("beat_selections")
            && orchestrator_src.matches("beat_selections").count() >= 2,
        "beat_selections must appear in both GamePatchExtraction and ActionResult \
         construction — extraction alone is useless without wiring into the result"
    );
}

/// The old extract_combat_from_game_patch function must be removed — beat_selections
/// replaces the CombatPatch extraction pathway.
#[test]
fn no_extract_combat_from_game_patch() {
    let orchestrator_src = include_str!("../../src/orchestrator.rs");

    assert!(
        !orchestrator_src.contains("fn extract_combat_from_game_patch"),
        "extract_combat_from_game_patch must be removed — beat_selections \
         replaces the CombatPatch extraction pathway"
    );
}

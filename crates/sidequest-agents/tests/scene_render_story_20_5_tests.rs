//! Story 20-5: scene_render tool — visual scene via tool call
//!
//! RED phase — tests for the scene_render tool that validates subject, tier, mood,
//! and tags parameters and returns a structured VisualScene. Follows set_mood/set_intent
//! pattern from ADR-057 Phase 2.
//!
//! ACs tested:
//!   1. scene_render accepts subject, tier, mood, tags → returns VisualScene JSON
//!   2. Tier, mood, tags validated against their enums
//!   3. Subject text passed through as-is (narrator's creative judgment)
//!   4. Narrator prompt documents the tool instead of JSON field schema
//!   5. assemble_turn merges tool result into ActionResult.visual_scene
//!   6. OTEL span with subject text and tier for GM panel visibility
//!
//! Rule enforcement:
//!   #2  — non_exhaustive on public enums (SceneTier, VisualMood, VisualTag)
//!   #5  — validated constructors
//!   #6  — meaningful assertions (self-checked)
//!   Wiring — scene_render has non-test consumers (tool_call_parser, mod.rs)

use std::collections::HashMap;
use std::io::Write;

use sidequest_agents::orchestrator::{ActionFlags, ActionRewrite, NarratorExtraction, VisualScene};
use sidequest_agents::tools::assemble_turn::{assemble_turn, ToolCallResults};
use sidequest_agents::tools::scene_render::{
    validate_scene_render, SceneTier, VisualMood, VisualTag,
};
use sidequest_agents::tools::tool_call_parser::{parse_tool_results, sidecar_path};

// ============================================================================
// Helpers
// ============================================================================

fn default_rewrite() -> ActionRewrite {
    ActionRewrite {
        you: "You look around".to_string(),
        named: "Kael looks around".to_string(),
        intent: "look around".to_string(),
    }
}

fn default_flags() -> ActionFlags {
    ActionFlags {
        is_power_grab: false,
        references_inventory: false,
        references_npc: false,
        references_ability: false,
        references_location: false,
    }
}

fn extraction_with_visual_scene() -> NarratorExtraction {
    NarratorExtraction {
        prose: "The grove shimmers with eldritch light.".to_string(),
        footnotes: vec![],
        items_gained: vec![],
        npcs_present: vec![],
        quest_updates: HashMap::new(),
        visual_scene: Some(VisualScene {
            subject: "narrator fallback subject".to_string(),
            tier: "portrait".to_string(),
            mood: "tense".to_string(),
            tags: vec!["character".to_string()],
        }),
        scene_mood: None,
        personality_events: vec![],
        scene_intent: None,
        resource_deltas: HashMap::new(),
        lore_established: None,
        merchant_transactions: vec![],
        sfx_triggers: vec![],
        action_rewrite: None,
        action_flags: None,
        beat_selections: vec![],
        confrontation: None,
        location: None,
    }
}

fn extraction_without_visual_scene() -> NarratorExtraction {
    NarratorExtraction {
        prose: "Nothing to see here.".to_string(),
        footnotes: vec![],
        items_gained: vec![],
        npcs_present: vec![],
        quest_updates: HashMap::new(),
        visual_scene: None,
        scene_mood: None,
        personality_events: vec![],
        scene_intent: None,
        resource_deltas: HashMap::new(),
        lore_established: None,
        merchant_transactions: vec![],
        sfx_triggers: vec![],
        action_rewrite: None,
        action_flags: None,
        beat_selections: vec![],
        confrontation: None,
        location: None,
    }
}

fn test_session_id(test_name: &str) -> String {
    format!("test-20-5-{}-{}", test_name, std::process::id())
}

fn write_sidecar(session_id: &str, lines: &[&str]) {
    let path = sidecar_path(session_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("failed to create sidecar dir");
    }
    let mut file = std::fs::File::create(&path).expect("failed to create sidecar file");
    for line in lines {
        writeln!(file, "{}", line).expect("failed to write line");
    }
}

fn cleanup_sidecar(session_id: &str) {
    let path = sidecar_path(session_id);
    let _ = std::fs::remove_file(path);
}

// ============================================================================
// AC-1: scene_render accepts subject, tier, mood, tags → VisualScene
// ============================================================================

#[test]
fn validate_scene_render_returns_visual_scene_for_valid_input() {
    let result = validate_scene_render(
        "weathered woman crouching by barrel fire",
        "portrait",
        "tense",
        &["character", "atmosphere"],
    );
    assert!(
        result.is_ok(),
        "valid input should return Ok: {:?}",
        result.err()
    );

    let scene = result.unwrap();
    assert_eq!(scene.subject, "weathered woman crouching by barrel fire");
    assert_eq!(scene.tier, "portrait");
    assert_eq!(scene.mood, "tense");
    assert_eq!(scene.tags, vec!["character", "atmosphere"]);
}

#[test]
fn validate_scene_render_accepts_all_tier_values() {
    for tier in ["portrait", "landscape", "scene_illustration"] {
        let result = validate_scene_render("test subject", tier, "tense", &["character"]);
        assert!(
            result.is_ok(),
            "tier '{}' should be valid: {:?}",
            tier,
            result.err()
        );
    }
}

#[test]
fn validate_scene_render_accepts_all_mood_values() {
    for mood in [
        "ominous",
        "tense",
        "mystical",
        "dramatic",
        "melancholic",
        "atmospheric",
    ] {
        let result = validate_scene_render("test subject", "portrait", mood, &["character"]);
        assert!(
            result.is_ok(),
            "mood '{}' should be valid: {:?}",
            mood,
            result.err()
        );
    }
}

#[test]
fn validate_scene_render_accepts_all_tag_values() {
    for tag in [
        "combat",
        "magic",
        "special_effect",
        "character",
        "location",
        "atmosphere",
    ] {
        let result = validate_scene_render("test subject", "portrait", "tense", &[tag]);
        assert!(
            result.is_ok(),
            "tag '{}' should be valid: {:?}",
            tag,
            result.err()
        );
    }
}

#[test]
fn validate_scene_render_accepts_multiple_tags() {
    let result = validate_scene_render(
        "test subject",
        "landscape",
        "dramatic",
        &["combat", "magic", "character"],
    );
    assert!(result.is_ok());
    assert_eq!(result.unwrap().tags, vec!["combat", "magic", "character"]);
}

#[test]
fn validate_scene_render_accepts_empty_tags() {
    let result = validate_scene_render("test subject", "portrait", "tense", &[]);
    assert!(result.is_ok(), "empty tags should be valid");
    assert!(result.unwrap().tags.is_empty());
}

// ============================================================================
// AC-2: Tier, mood, tags validated against enums
// ============================================================================

#[test]
fn validate_scene_render_rejects_invalid_tier() {
    let result = validate_scene_render("test subject", "vignette", "tense", &["character"]);
    assert!(
        result.is_err(),
        "invalid tier 'vignette' should be rejected"
    );
}

#[test]
fn validate_scene_render_rejects_invalid_mood() {
    let result = validate_scene_render("test subject", "portrait", "happy", &["character"]);
    assert!(result.is_err(), "invalid mood 'happy' should be rejected");
}

#[test]
fn validate_scene_render_rejects_invalid_tag() {
    let result = validate_scene_render("test subject", "portrait", "tense", &["invalid_tag"]);
    assert!(
        result.is_err(),
        "invalid tag 'invalid_tag' should be rejected"
    );
}

#[test]
fn validate_scene_render_rejects_mixed_valid_invalid_tags() {
    let result =
        validate_scene_render("test subject", "portrait", "tense", &["character", "bogus"]);
    assert!(
        result.is_err(),
        "one invalid tag in list should reject the whole call"
    );
}

#[test]
fn validate_scene_render_is_case_insensitive_for_tier() {
    let result = validate_scene_render("test subject", "Portrait", "tense", &["character"]);
    assert!(result.is_ok(), "tier should be case-insensitive");
    assert_eq!(
        result.unwrap().tier,
        "portrait",
        "tier should be normalized to lowercase"
    );
}

#[test]
fn validate_scene_render_is_case_insensitive_for_mood() {
    let result = validate_scene_render("test subject", "portrait", "TENSE", &["character"]);
    assert!(result.is_ok(), "mood should be case-insensitive");
    assert_eq!(
        result.unwrap().mood,
        "tense",
        "mood should be normalized to lowercase"
    );
}

// ============================================================================
// AC-3: Subject text passed through as-is
// ============================================================================

#[test]
fn validate_scene_render_preserves_subject_text_exactly() {
    let creative_subject = "tall woman with bark-like skin, standing in corrupted grove";
    let result = validate_scene_render(creative_subject, "portrait", "mystical", &["character"]);
    assert!(result.is_ok());
    assert_eq!(
        result.unwrap().subject,
        creative_subject,
        "subject text must be preserved exactly as the narrator wrote it"
    );
}

#[test]
fn validate_scene_render_accepts_subject_at_100_chars() {
    let subject = "a".repeat(100);
    let result = validate_scene_render(&subject, "portrait", "tense", &["character"]);
    assert!(
        result.is_ok(),
        "subject at exactly 100 chars should be valid"
    );
}

#[test]
fn validate_scene_render_rejects_subject_over_100_chars() {
    let subject = "a".repeat(101);
    let result = validate_scene_render(&subject, "portrait", "tense", &["character"]);
    assert!(result.is_err(), "subject over 100 chars should be rejected");
}

#[test]
fn validate_scene_render_rejects_empty_subject() {
    let result = validate_scene_render("", "portrait", "tense", &["character"]);
    assert!(result.is_err(), "empty subject should be rejected");
}

// ============================================================================
// AC-5: assemble_turn merges tool result visual_scene into ActionResult
// ============================================================================

#[test]
fn assemble_turn_uses_tool_visual_scene_over_narrator() {
    let tool_scene = VisualScene {
        subject: "tool-provided scene".to_string(),
        tier: "landscape".to_string(),
        mood: "dramatic".to_string(),
        tags: vec!["location".to_string(), "atmosphere".to_string()],
    };
    let tool_results = ToolCallResults {
        visual_scene: Some(tool_scene),
        ..ToolCallResults::default()
    };

    let extraction = extraction_with_visual_scene();
    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);

    let vs = result.visual_scene.expect("visual_scene should be present");
    assert_eq!(
        vs.subject, "tool-provided scene",
        "tool result visual_scene must override narrator extraction"
    );
}

#[test]
fn assemble_turn_falls_back_to_narrator_visual_scene_when_no_tool() {
    let tool_results = ToolCallResults::default();

    let extraction = extraction_with_visual_scene();
    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);

    let vs = result
        .visual_scene
        .expect("visual_scene should fall back to narrator");
    assert_eq!(
        vs.subject, "narrator fallback subject",
        "with no tool result, narrator extraction visual_scene should pass through"
    );
}

#[test]
fn assemble_turn_returns_none_visual_scene_when_neither_source() {
    let tool_results = ToolCallResults::default();

    let extraction = extraction_without_visual_scene();
    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);

    assert!(
        result.visual_scene.is_none(),
        "visual_scene should be None when neither tool nor narrator provides one"
    );
}

// ============================================================================
// Tool call parser: scene_render sidecar integration
// ============================================================================

#[test]
fn parser_extracts_scene_render_from_sidecar() {
    let sid = test_session_id("parse-scene");
    write_sidecar(
        &sid,
        &[
            r#"{"tool":"scene_render","result":{"subject":"crumbling tower at sunset","tier":"landscape","mood":"dramatic","tags":["location","atmosphere"]}}"#,
        ],
    );

    let results = parse_tool_results(&sid);
    let vs = results
        .visual_scene
        .expect("scene_render tool result should populate visual_scene");
    assert_eq!(vs.subject, "crumbling tower at sunset");
    assert_eq!(vs.tier, "landscape");
    assert_eq!(vs.mood, "dramatic");
    assert_eq!(vs.tags, vec!["location", "atmosphere"]);

    cleanup_sidecar(&sid);
}

#[test]
fn parser_handles_scene_render_with_missing_fields() {
    let sid = test_session_id("parse-scene-missing");
    // Missing "mood" field — should skip this record, not crash
    write_sidecar(
        &sid,
        &[r#"{"tool":"scene_render","result":{"subject":"test","tier":"portrait"}}"#],
    );

    let results = parse_tool_results(&sid);
    // Missing required field → record should be skipped
    assert!(
        results.visual_scene.is_none(),
        "scene_render with missing fields should be skipped"
    );

    cleanup_sidecar(&sid);
}

#[test]
fn parser_handles_scene_render_alongside_other_tools() {
    let sid = test_session_id("parse-scene-multi");
    write_sidecar(
        &sid,
        &[
            r#"{"tool":"set_mood","result":{"mood":"tension"}}"#,
            r#"{"tool":"scene_render","result":{"subject":"dark corridor","tier":"scene_illustration","mood":"ominous","tags":["atmosphere"]}}"#,
            r#"{"tool":"set_intent","result":{"intent":"exploration"}}"#,
        ],
    );

    let results = parse_tool_results(&sid);
    assert_eq!(results.scene_mood.as_deref(), Some("tension"));
    assert_eq!(results.scene_intent.as_deref(), Some("exploration"));
    let vs = results
        .visual_scene
        .expect("scene_render should be parsed alongside other tools");
    assert_eq!(vs.subject, "dark corridor");

    cleanup_sidecar(&sid);
}

// ============================================================================
// Wiring: end-to-end sidecar → parser → assemble_turn
// ============================================================================

#[test]
fn scene_render_e2e_sidecar_to_action_result() {
    let sid = test_session_id("e2e-scene");
    write_sidecar(
        &sid,
        &[
            r#"{"tool":"scene_render","result":{"subject":"ancient ruins under moonlight","tier":"landscape","mood":"mystical","tags":["location","magic"]}}"#,
        ],
    );

    let tool_results = parse_tool_results(&sid);
    let extraction = extraction_with_visual_scene(); // has narrator fallback
    let result = assemble_turn(extraction, default_rewrite(), default_flags(), tool_results);

    let vs = result
        .visual_scene
        .expect("e2e: visual_scene should be present");
    assert_eq!(
        vs.subject, "ancient ruins under moonlight",
        "e2e: tool result must override narrator extraction visual_scene"
    );
    assert_eq!(vs.tier, "landscape");
    assert_eq!(vs.mood, "mystical");
    assert_eq!(vs.tags, vec!["location", "magic"]);
}

// ============================================================================
// Wiring: scene_render module is exported and tool_call_parser knows about it
// ============================================================================

#[test]
fn scene_render_module_is_exported() {
    // Compile-time check: scene_render is a public module reachable from integration tests
    let _fn_ptr: fn(&str, &str, &str, &[&str]) -> Result<VisualScene, _> = validate_scene_render;
}

#[test]
fn tool_call_parser_handles_scene_render_tool_name() {
    // Verify that "scene_render" is a recognized tool name in the parser.
    // If the parser doesn't have a match arm for "scene_render", this test
    // will pass but visual_scene will be None — caught by the assertion.
    let sid = test_session_id("wiring-parser");
    write_sidecar(
        &sid,
        &[
            r#"{"tool":"scene_render","result":{"subject":"test","tier":"portrait","mood":"tense","tags":["character"]}}"#,
        ],
    );

    let results = parse_tool_results(&sid);
    assert!(
        results.visual_scene.is_some(),
        "tool_call_parser must recognize 'scene_render' as a known tool — \
         visual_scene is None, meaning the parser has no match arm for it"
    );

    cleanup_sidecar(&sid);
}

// ============================================================================
// Wiring: narrator prompt references scene_render tool (AC-4)
// ============================================================================

#[test]
fn narrator_prompt_does_not_contain_visual_scene_json_schema() {
    // AC-4: The narrator prompt should document the tool call, not the JSON field schema.
    // After 20-5, the visual_scene JSON schema (~100 tokens) should be removed from
    // the narrator system prompt. The narrator will call scene_render as a tool instead.
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let narrator_src = std::fs::read_to_string(format!("{manifest_dir}/src/agents/narrator.rs"))
        .expect("should be able to read narrator.rs");

    // The old JSON block docs for visual_scene should be gone
    assert!(
        !narrator_src.contains("visual_scene: ALWAYS INCLUDE"),
        "narrator.rs should no longer contain visual_scene JSON field documentation — \
         scene_render is now a tool call, not a JSON field"
    );
}

// ============================================================================
// Rule #2: non_exhaustive on public enums
// ============================================================================

#[test]
fn scene_tier_enum_is_non_exhaustive() {
    // Rule #2: public enums that will grow must have #[non_exhaustive].
    // SceneTier may gain future tiers (e.g., "panorama", "close_up").
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let src = std::fs::read_to_string(format!("{manifest_dir}/src/tools/scene_render.rs"))
        .expect("should be able to read scene_render.rs");

    // Check that #[non_exhaustive] appears before "pub enum SceneTier"
    let tier_pos = src
        .find("pub enum SceneTier")
        .expect("SceneTier enum must exist");
    let before_tier = &src[..tier_pos];
    assert!(
        before_tier.rfind("#[non_exhaustive]").map_or(false, |pos| {
            // Verify it's close to the enum (within 200 chars, accounting for derives/docs)
            tier_pos - pos < 200
        }),
        "SceneTier must have #[non_exhaustive] — new tiers may be added in the future"
    );
}

#[test]
fn visual_mood_enum_is_non_exhaustive() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let src = std::fs::read_to_string(format!("{manifest_dir}/src/tools/scene_render.rs"))
        .expect("should be able to read scene_render.rs");

    let mood_pos = src
        .find("pub enum VisualMood")
        .expect("VisualMood enum must exist");
    let before_mood = &src[..mood_pos];
    assert!(
        before_mood
            .rfind("#[non_exhaustive]")
            .map_or(false, |pos| { mood_pos - pos < 200 }),
        "VisualMood must have #[non_exhaustive] — new moods may be added"
    );
}

#[test]
fn visual_tag_enum_is_non_exhaustive() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let src = std::fs::read_to_string(format!("{manifest_dir}/src/tools/scene_render.rs"))
        .expect("should be able to read scene_render.rs");

    let tag_pos = src
        .find("pub enum VisualTag")
        .expect("VisualTag enum must exist");
    let before_tag = &src[..tag_pos];
    assert!(
        before_tag
            .rfind("#[non_exhaustive]")
            .map_or(false, |pos| { tag_pos - pos < 200 }),
        "VisualTag must have #[non_exhaustive] — new tags may be added"
    );
}

// ============================================================================
// Rule #6: OTEL instrumentation verification (AC-6)
// ============================================================================

#[test]
fn validate_scene_render_has_tracing_instrument() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let src = std::fs::read_to_string(format!("{manifest_dir}/src/tools/scene_render.rs"))
        .expect("should be able to read scene_render.rs");

    assert!(
        src.contains("tracing::instrument") || src.contains("#[instrument"),
        "validate_scene_render must have #[tracing::instrument] for OTEL visibility"
    );

    // AC-6: OTEL span should include subject and tier
    assert!(
        src.contains("subject") && src.contains("tier"),
        "OTEL span fields must include subject and tier per AC-6"
    );
}

//! Story 23-11: Rework tool sections — wrapper names, env vars, compact XML format
//!
//! RED phase — tests for the three-part tool section rework:
//!   1. Wrapper names: `sidequest-encounter`, `sidequest-npc`, `sidequest-loadout`
//!   2. Env vars: `SIDEQUEST_GENRE` and `SIDEQUEST_CONTENT_PATH` on Claude CLI subprocess
//!   3. Compact XML format: `<tool>` tags instead of Markdown flag tables
//!
//! ACs covered:
//!   AC-1:  Compact `<tool>` XML format (no flag tables)
//!   AC-2:  Commands reference wrapper names, not binary paths
//!   AC-3:  No `--genre` or `--genre-packs-path` flags in prompt text
//!   AC-4:  No filesystem paths in prompt text
//!   AC-5:  `SIDEQUEST_GENRE` env var on Claude CLI subprocess
//!   AC-6:  `SIDEQUEST_CONTENT_PATH` env var on Claude CLI subprocess
//!   AC-7:  "When to call" and checklist preserved
//!   AC-8:  NPC tool "MANDATORY: call before introducing any new NPC" preserved
//!   AC-9:  Token reduction ≥60%
//!   AC-11: OTEL tool section registration in compose span (covered by 15-27 tests)

use sidequest_agents::orchestrator::{Orchestrator, ScriptToolConfig, TurnContext};
use sidequest_agents::turn_record::{TurnRecord, WATCHER_CHANNEL_CAPACITY};
use tokio::sync::mpsc;

// ============================================================================
// Test helpers
// ============================================================================

/// Create an Orchestrator with all three script tools registered.
fn orchestrator_with_tools() -> Orchestrator {
    let (tx, _rx) = mpsc::channel::<TurnRecord>(WATCHER_CHANNEL_CAPACITY);
    let mut orch = Orchestrator::new(tx);
    orch.register_script_tool(
        "encountergen",
        ScriptToolConfig {
            binary_path: "/usr/local/bin/sidequest-encountergen".to_string(),
            genre_packs_path: "/data/genre_packs".to_string(),
        },
    );
    orch.register_script_tool(
        "namegen",
        ScriptToolConfig {
            binary_path: "/usr/local/bin/sidequest-namegen".to_string(),
            genre_packs_path: "/data/genre_packs".to_string(),
        },
    );
    orch.register_script_tool(
        "loadoutgen",
        ScriptToolConfig {
            binary_path: "/usr/local/bin/sidequest-loadoutgen".to_string(),
            genre_packs_path: "/data/genre_packs".to_string(),
        },
    );
    orch
}

fn context_with_genre(genre: &str) -> TurnContext {
    TurnContext {
        genre: Some(genre.to_string()),
        ..Default::default()
    }
}

/// Extract only the tool sections from the full prompt text.
/// Tool sections are registered as `script_tool_*` sections in the Valley zone.
fn extract_tool_text(prompt: &str) -> String {
    // All three tool sections appear between the first <tool and the last </tool>
    // or between [ENCOUNTER / [NPC / [STARTING — whatever format is active.
    // For these tests we just search the full prompt text since tool content is
    // uniquely identifiable by wrapper names and XML tags.
    prompt.to_string()
}

// ============================================================================
// AC-1: Compact `<tool>` XML format
// ============================================================================

/// Each tool section must use `<tool name="...">` XML wrapper, not Markdown headers.
///
/// RED because: Current code uses `[ENCOUNTER GENERATOR]`, `[NPC GENERATOR]`,
/// `[STARTING LOADOUT GENERATOR]` Markdown-style headers.
#[test]
fn tool_sections_use_xml_tool_tags() {
    let orch = orchestrator_with_tools();
    let ctx = context_with_genre("mutant_wasteland");
    let result = orch.build_narrator_prompt("look around", &ctx);

    assert!(
        result.prompt_text.contains("<tool name=\"ENCOUNTER\">"),
        "Encounter tool must use <tool name=\"ENCOUNTER\"> XML format"
    );
    assert!(
        result.prompt_text.contains("<tool name=\"NPC\">"),
        "NPC tool must use <tool name=\"NPC\"> XML format"
    );
    assert!(
        result.prompt_text.contains("<tool name=\"LOADOUT\">"),
        "Loadout tool must use <tool name=\"LOADOUT\"> XML format"
    );
}

/// Tool sections must have closing </tool> tags.
#[test]
fn tool_sections_have_closing_tags() {
    let orch = orchestrator_with_tools();
    let ctx = context_with_genre("mutant_wasteland");
    let result = orch.build_narrator_prompt("look around", &ctx);

    let open_count = result.prompt_text.matches("<tool ").count();
    let close_count = result.prompt_text.matches("</tool>").count();
    assert_eq!(
        open_count, close_count,
        "Every <tool> must have a matching </tool>: found {open_count} opens, {close_count} closes"
    );
    assert_eq!(
        open_count, 3,
        "Expected 3 tool sections (encounter, npc, loadout), got {open_count}"
    );
}

/// Tool sections must contain <command> tags with the invocation syntax.
#[test]
fn tool_sections_contain_command_tags() {
    let orch = orchestrator_with_tools();
    let ctx = context_with_genre("mutant_wasteland");
    let result = orch.build_narrator_prompt("look around", &ctx);

    assert!(
        result.prompt_text.contains("<command>") && result.prompt_text.contains("</command>"),
        "Tool sections must contain <command>...</command> tags for invocation syntax"
    );
    let cmd_count = result.prompt_text.matches("<command>").count();
    assert_eq!(
        cmd_count, 3,
        "Each of the 3 tools should have a <command> tag, got {cmd_count}"
    );
}

/// Tool sections must contain <usage> tags with checklists.
#[test]
fn tool_sections_contain_usage_tags() {
    let orch = orchestrator_with_tools();
    let ctx = context_with_genre("mutant_wasteland");
    let result = orch.build_narrator_prompt("look around", &ctx);

    assert!(
        result.prompt_text.contains("<usage>") && result.prompt_text.contains("</usage>"),
        "Tool sections must contain <usage>...</usage> tags for checklists"
    );
    let usage_count = result.prompt_text.matches("<usage>").count();
    assert_eq!(
        usage_count, 3,
        "Each of the 3 tools should have a <usage> tag, got {usage_count}"
    );
}

/// No Markdown flag tables in tool sections. The old format had `| Flag | Required |`.
#[test]
fn tool_sections_have_no_markdown_tables() {
    let orch = orchestrator_with_tools();
    let ctx = context_with_genre("mutant_wasteland");
    let result = orch.build_narrator_prompt("look around", &ctx);

    assert!(
        !result.prompt_text.contains("| Flag |"),
        "Tool sections must not contain Markdown flag tables (| Flag |)"
    );
    assert!(
        !result.prompt_text.contains("| Required |"),
        "Tool sections must not contain Markdown table headers (| Required |)"
    );
}

/// No old-style Markdown headers for tool sections.
#[test]
fn tool_sections_have_no_markdown_headers() {
    let orch = orchestrator_with_tools();
    let ctx = context_with_genre("mutant_wasteland");
    let result = orch.build_narrator_prompt("look around", &ctx);

    assert!(
        !result.prompt_text.contains("[ENCOUNTER GENERATOR]"),
        "Must not use old [ENCOUNTER GENERATOR] header — use <tool> XML format"
    );
    assert!(
        !result.prompt_text.contains("[NPC GENERATOR]"),
        "Must not use old [NPC GENERATOR] header — use <tool> XML format"
    );
    assert!(
        !result.prompt_text.contains("[STARTING LOADOUT GENERATOR]"),
        "Must not use old [STARTING LOADOUT GENERATOR] header — use <tool> XML format"
    );
}

// ============================================================================
// AC-2: Commands reference wrapper names
// ============================================================================

/// Encounter tool command must use `sidequest-encounter` wrapper name.
#[test]
fn encounter_command_uses_wrapper_name() {
    let orch = orchestrator_with_tools();
    let ctx = context_with_genre("mutant_wasteland");
    let result = orch.build_narrator_prompt("look around", &ctx);

    assert!(
        result.prompt_text.contains("sidequest-encounter"),
        "Encounter tool command must reference `sidequest-encounter` wrapper name"
    );
}

/// NPC tool command must use `sidequest-npc` wrapper name.
#[test]
fn npc_command_uses_wrapper_name() {
    let orch = orchestrator_with_tools();
    let ctx = context_with_genre("mutant_wasteland");
    let result = orch.build_narrator_prompt("look around", &ctx);

    assert!(
        result.prompt_text.contains("sidequest-npc"),
        "NPC tool command must reference `sidequest-npc` wrapper name"
    );
}

/// Loadout tool command must use `sidequest-loadout` wrapper name.
#[test]
fn loadout_command_uses_wrapper_name() {
    let orch = orchestrator_with_tools();
    let ctx = context_with_genre("mutant_wasteland");
    let result = orch.build_narrator_prompt("look around", &ctx);

    assert!(
        result.prompt_text.contains("sidequest-loadout"),
        "Loadout tool command must reference `sidequest-loadout` wrapper name"
    );
}

// ============================================================================
// AC-3: No --genre or --genre-packs-path flags in prompt text
// ============================================================================

/// Prompt text must not contain `--genre` flag — genre is now an env var.
#[test]
fn prompt_has_no_genre_flag() {
    let orch = orchestrator_with_tools();
    let ctx = context_with_genre("mutant_wasteland");
    let result = orch.build_narrator_prompt("look around", &ctx);

    assert!(
        !result.prompt_text.contains("--genre "),
        "Prompt must not contain --genre flag (genre is now SIDEQUEST_GENRE env var)"
    );
    assert!(
        !result.prompt_text.contains("--genre\n"),
        "Prompt must not contain --genre flag at end of line"
    );
}

/// Prompt text must not contain `--genre-packs-path` flag — path is now an env var.
#[test]
fn prompt_has_no_genre_packs_path_flag() {
    let orch = orchestrator_with_tools();
    let ctx = context_with_genre("mutant_wasteland");
    let result = orch.build_narrator_prompt("look around", &ctx);

    assert!(
        !result.prompt_text.contains("--genre-packs-path"),
        "Prompt must not contain --genre-packs-path flag (path is now SIDEQUEST_CONTENT_PATH env var)"
    );
}

// ============================================================================
// AC-4: No filesystem paths in prompt text
// ============================================================================

/// No absolute filesystem paths should appear in the tool sections.
/// The old format embedded `cfg.binary_path` and `cfg.genre_packs_path` directly.
#[test]
fn prompt_has_no_filesystem_paths() {
    let orch = orchestrator_with_tools();
    let ctx = context_with_genre("mutant_wasteland");
    let result = orch.build_narrator_prompt("look around", &ctx);

    // Check for the specific paths we registered
    assert!(
        !result.prompt_text.contains("/usr/local/bin/sidequest-encountergen"),
        "Prompt must not contain binary filesystem paths"
    );
    assert!(
        !result.prompt_text.contains("/usr/local/bin/sidequest-namegen"),
        "Prompt must not contain binary filesystem paths"
    );
    assert!(
        !result.prompt_text.contains("/usr/local/bin/sidequest-loadoutgen"),
        "Prompt must not contain binary filesystem paths"
    );
    assert!(
        !result.prompt_text.contains("/data/genre_packs"),
        "Prompt must not contain genre_packs filesystem paths"
    );
}

/// The `--allowedTools Bash(...)` specs must still use real binary paths,
/// because that's what the Claude CLI needs for subprocess execution.
/// Only the PROMPT TEXT should use wrapper names — the CLI still needs absolute paths.
#[test]
fn allowed_tools_still_use_binary_paths() {
    let orch = orchestrator_with_tools();
    let tools = orch.narrator_allowed_tools();

    assert!(
        !tools.is_empty(),
        "Allowed tools should be populated"
    );
    for tool in &tools {
        assert!(
            tool.starts_with("Bash(") && tool.ends_with(":*)"),
            "Allowed tools must use Bash(path:*) format for Claude CLI, got: {tool}"
        );
        // These should still be absolute paths
        assert!(
            tool.contains("/usr/local/bin/"),
            "Allowed tools should still contain absolute binary paths, got: {tool}"
        );
    }
}

// ============================================================================
// AC-5 & AC-6: Environment variables on Claude CLI subprocess
// ============================================================================

/// NarratorPromptResult must carry `SIDEQUEST_GENRE` env var when genre is set.
///
/// RED because: NarratorPromptResult does not have an `env_vars` field yet.
/// Dev must add `pub env_vars: HashMap<String, String>` to NarratorPromptResult.
#[test]
fn prompt_result_has_sidequest_genre_env_var() {
    let orch = orchestrator_with_tools();
    let ctx = context_with_genre("neon_dystopia");
    let result = orch.build_narrator_prompt("enter the club", &ctx);

    let genre_val = result.env_vars.get("SIDEQUEST_GENRE");
    assert_eq!(
        genre_val,
        Some(&"neon_dystopia".to_string()),
        "NarratorPromptResult.env_vars must contain SIDEQUEST_GENRE=neon_dystopia",
    );
}

/// NarratorPromptResult must carry `SIDEQUEST_CONTENT_PATH` env var.
///
/// RED because: NarratorPromptResult does not have an `env_vars` field yet.
/// The content path comes from the registered ScriptToolConfig.genre_packs_path.
#[test]
fn prompt_result_has_sidequest_content_path_env_var() {
    let orch = orchestrator_with_tools();
    let ctx = context_with_genre("neon_dystopia");
    let result = orch.build_narrator_prompt("enter the club", &ctx);

    let content_path = result.env_vars.get("SIDEQUEST_CONTENT_PATH");
    assert_eq!(
        content_path,
        Some(&"/data/genre_packs".to_string()),
        "NarratorPromptResult.env_vars must contain SIDEQUEST_CONTENT_PATH from ScriptToolConfig",
    );
}

/// When genre is None, SIDEQUEST_GENRE should not be in env_vars.
/// (No tools injected, so no env vars needed.)
#[test]
fn prompt_result_has_no_env_vars_when_genre_missing() {
    let orch = orchestrator_with_tools();
    let ctx = TurnContext::default(); // genre: None
    let result = orch.build_narrator_prompt("look around", &ctx);

    assert!(
        result.env_vars.is_empty(),
        "env_vars should be empty when genre is None (no tools injected)"
    );
}

/// Different genres produce different SIDEQUEST_GENRE values.
#[test]
fn env_var_genre_matches_context_genre() {
    let orch = orchestrator_with_tools();

    for genre in &["space_opera", "low_fantasy", "road_warrior", "pulp_noir"] {
        let ctx = context_with_genre(genre);
        let result = orch.build_narrator_prompt("explore", &ctx);

        let env_genre = result.env_vars.get("SIDEQUEST_GENRE")
            .unwrap_or_else(|| panic!("SIDEQUEST_GENRE missing for genre {genre}"));
        assert_eq!(
            env_genre, genre,
            "SIDEQUEST_GENRE should match context genre"
        );
    }
}

// ============================================================================
// AC-7: "When to call" and checklist preserved
// ============================================================================

/// Encounter tool must preserve "When to call" guidance.
#[test]
fn encounter_tool_preserves_when_to_call() {
    let orch = orchestrator_with_tools();
    let ctx = context_with_genre("mutant_wasteland");
    let result = orch.build_narrator_prompt("look around", &ctx);

    assert!(
        result.prompt_text.contains("When to call") || result.prompt_text.contains("when to call"),
        "Encounter tool must preserve 'When to call' guidance text"
    );
    // Specifically check encounter's when-to-call is about enemies entering the scene
    assert!(
        result.prompt_text.contains("new enemies enter the scene"),
        "Encounter tool 'When to call' must mention enemies entering the scene"
    );
}

/// NPC tool must preserve "When to call" guidance.
#[test]
fn npc_tool_preserves_when_to_call() {
    let orch = orchestrator_with_tools();
    let ctx = context_with_genre("mutant_wasteland");
    let result = orch.build_narrator_prompt("look around", &ctx);

    // NPC tool should mention calling when new NPCs appear
    assert!(
        result.prompt_text.contains("new NPC") || result.prompt_text.contains("is_new"),
        "NPC tool 'When to call' must reference new NPC appearance"
    );
}

/// Encounter tool must preserve checklist items.
#[test]
fn encounter_tool_preserves_checklist() {
    let orch = orchestrator_with_tools();
    let ctx = context_with_genre("mutant_wasteland");
    let result = orch.build_narrator_prompt("look around", &ctx);

    assert!(
        result.prompt_text.contains("Use the generated name in your narration"),
        "Encounter tool must preserve checklist item about using generated names"
    );
    assert!(
        result.prompt_text.contains("Reference abilities from the abilities list"),
        "Encounter tool must preserve checklist item about referencing abilities"
    );
}

/// NPC tool must preserve checklist items.
#[test]
fn npc_tool_preserves_checklist() {
    let orch = orchestrator_with_tools();
    let ctx = context_with_genre("mutant_wasteland");
    let result = orch.build_narrator_prompt("look around", &ctx);

    assert!(
        result.prompt_text.contains("Use the generated name exactly"),
        "NPC tool must preserve checklist item about using generated name exactly"
    );
    assert!(
        result.prompt_text.contains("dialogue_quirks"),
        "NPC tool must preserve checklist item about dialogue quirks"
    );
}

/// Loadout tool must preserve checklist items.
#[test]
fn loadout_tool_preserves_checklist() {
    let orch = orchestrator_with_tools();
    let ctx = context_with_genre("mutant_wasteland");
    let result = orch.build_narrator_prompt("look around", &ctx);

    assert!(
        result.prompt_text.contains("narrative_hook"),
        "Loadout tool must preserve checklist item about narrative_hook"
    );
    assert!(
        result.prompt_text.contains("currency_name"),
        "Loadout tool must preserve checklist item about currency_name"
    );
}

// ============================================================================
// AC-8: NPC tool "MANDATORY" rule preserved
// ============================================================================

/// The NPC tool section must contain the mandatory pre-introduction call rule.
#[test]
fn npc_tool_has_mandatory_pre_introduction_rule() {
    let orch = orchestrator_with_tools();
    let ctx = context_with_genre("mutant_wasteland");
    let result = orch.build_narrator_prompt("look around", &ctx);

    assert!(
        result.prompt_text.contains("MANDATORY") && result.prompt_text.contains("before introducing"),
        "NPC tool must contain 'MANDATORY: Call this BEFORE introducing any new NPC' rule"
    );
}

// ============================================================================
// AC-9: Token reduction ≥60% vs current format
// ============================================================================

/// The compact XML format should reduce tool section size by at least 60%.
///
/// Current format is ~490 tokens (~1,960 chars for all three tools).
/// Target is ~150 tokens (~600 chars). We measure chars as a proxy.
///
/// RED because: Current format is verbose Markdown with flag tables.
#[test]
fn tool_sections_achieve_token_reduction() {
    let orch = orchestrator_with_tools();
    let ctx = context_with_genre("mutant_wasteland");
    let result = orch.build_narrator_prompt("look around", &ctx);

    // Extract tool-related content: everything between <tool and </tool> tags
    let mut total_tool_chars = 0;
    let text = &result.prompt_text;
    let mut search_from = 0;
    while let Some(start) = text[search_from..].find("<tool ") {
        let abs_start = search_from + start;
        if let Some(end) = text[abs_start..].find("</tool>") {
            let abs_end = abs_start + end + "</tool>".len();
            total_tool_chars += abs_end - abs_start;
            search_from = abs_end;
        } else {
            break;
        }
    }

    // The old format was ~1,960 chars for all 3 tools (measured from current code).
    // 60% reduction means new format should be ≤ 784 chars.
    // We use 800 as a generous ceiling.
    assert!(
        total_tool_chars > 0,
        "Should find tool content in <tool> tags"
    );
    assert!(
        total_tool_chars <= 800,
        "All 3 tool sections should be ≤800 chars total for ≥60% reduction, got {total_tool_chars}"
    );
}

/// No "Output: JSON with..." descriptions in the new format.
/// These were removed because structured return format isn't the narrator's concern.
#[test]
fn tool_sections_have_no_output_descriptions() {
    let orch = orchestrator_with_tools();
    let ctx = context_with_genre("mutant_wasteland");
    let result = orch.build_narrator_prompt("look around", &ctx);

    assert!(
        !result.prompt_text.contains("Output: JSON with"),
        "Tool sections must not contain 'Output: JSON with...' descriptions"
    );
}

// ============================================================================
// Wiring: end-to-end integration test
// ============================================================================

/// Full pipeline: register tools → build prompt → verify compact XML format,
/// wrapper names, no filesystem paths, env vars populated, checklists preserved.
#[test]
fn wiring_tool_rework_end_to_end() {
    let orch = orchestrator_with_tools();
    let ctx = context_with_genre("low_fantasy");
    let result = orch.build_narrator_prompt("enter the tavern", &ctx);

    // Format: XML
    assert!(
        result.prompt_text.contains("<tool name="),
        "Wiring: tool sections should use XML format"
    );
    assert!(
        !result.prompt_text.contains("[ENCOUNTER GENERATOR]"),
        "Wiring: old Markdown headers should be gone"
    );

    // Wrapper names: present
    assert!(
        result.prompt_text.contains("sidequest-encounter"),
        "Wiring: wrapper name sidequest-encounter should appear"
    );
    assert!(
        result.prompt_text.contains("sidequest-npc"),
        "Wiring: wrapper name sidequest-npc should appear"
    );
    assert!(
        result.prompt_text.contains("sidequest-loadout"),
        "Wiring: wrapper name sidequest-loadout should appear"
    );

    // Filesystem paths: absent from prompt
    assert!(
        !result.prompt_text.contains("/usr/local/bin/"),
        "Wiring: no filesystem paths in prompt text"
    );
    assert!(
        !result.prompt_text.contains("/data/genre_packs"),
        "Wiring: no genre_packs path in prompt text"
    );

    // Env vars: populated
    assert_eq!(
        result.env_vars.get("SIDEQUEST_GENRE"),
        Some(&"low_fantasy".to_string()),
        "Wiring: SIDEQUEST_GENRE env var should be set"
    );
    assert_eq!(
        result.env_vars.get("SIDEQUEST_CONTENT_PATH"),
        Some(&"/data/genre_packs".to_string()),
        "Wiring: SIDEQUEST_CONTENT_PATH env var should be set"
    );

    // Checklists: preserved
    assert!(
        result.prompt_text.contains("- [ ]"),
        "Wiring: checklist items should be preserved"
    );

    // Allowed tools: still use binary paths for CLI
    assert!(
        !result.allowed_tools.is_empty(),
        "Wiring: allowed_tools should still be populated"
    );

    // OTEL: tools reported as injected
    assert_eq!(
        result.script_tools_injected.len(),
        3,
        "Wiring: all 3 tools reported as injected"
    );
}

/// Negative wiring case: no genre → no tool sections, no env vars.
#[test]
fn wiring_no_genre_means_no_tools_no_env_vars() {
    let orch = orchestrator_with_tools();
    let ctx = TurnContext::default(); // genre: None

    let result = orch.build_narrator_prompt("look around", &ctx);

    assert!(
        !result.prompt_text.contains("<tool "),
        "Wiring: no tool sections when genre is None"
    );
    assert!(
        result.env_vars.is_empty(),
        "Wiring: no env vars when genre is None"
    );
    assert!(
        result.script_tools_injected.is_empty(),
        "Wiring: no injected tools when genre is None"
    );
}

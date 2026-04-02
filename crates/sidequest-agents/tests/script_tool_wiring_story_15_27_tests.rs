//! Story 15-27: Script tool invocation wiring tests
//!
//! RED phase — tests that verify script tools (encountergen, loadoutgen, namegen)
//! are registered, injected into narrator prompts, and observable via OTEL.
//!
//! The gap: Script tools are registered at startup and prompt sections are built,
//! but the LLM never invokes them. Root causes under test:
//!   1. Tool definitions may not be formatted as Claude tool_use specs
//!   2. Narrator prompt sections are injected but only when genre is Some
//!   3. No OTEL spans exist for script_tool.invoked / script_tool.result
//!
//! ACs covered:
//!   AC-1: Script tools registered as Claude tool_use definitions
//!   AC-2: Narrator prompt explicitly instructs LLM to use tools
//!   AC-3: OTEL events script_tool.invoked and script_tool.result in traces
//!   AC-4: Wiring test — full pipeline from registration to prompt inclusion
//!
//! Design note: process_action() currently builds prompts inline and calls the LLM
//! in the same function. These tests require a public `build_narrator_prompt()` method
//! extracted from process_action() so we can verify prompt content without LLM side effects.

use sidequest_agents::orchestrator::{Orchestrator, ScriptToolConfig, TurnContext};
use sidequest_agents::turn_record::{TurnRecord, WATCHER_CHANNEL_CAPACITY};
use tokio::sync::mpsc;

// ============================================================================
// Test helpers
// ============================================================================

/// Create an Orchestrator with all three script tools registered.
fn orchestrator_with_script_tools() -> Orchestrator {
    let (tx, _rx) = mpsc::channel::<TurnRecord>(WATCHER_CHANNEL_CAPACITY);
    let mut orch = Orchestrator::new(tx);
    orch.register_script_tool(
        "encountergen",
        ScriptToolConfig {
            binary_path: "/usr/local/bin/sidequest-encountergen".to_string(),
            genre_packs_path: "/tmp/genre_packs".to_string(),
        },
    );
    orch.register_script_tool(
        "loadoutgen",
        ScriptToolConfig {
            binary_path: "/usr/local/bin/sidequest-loadoutgen".to_string(),
            genre_packs_path: "/tmp/genre_packs".to_string(),
        },
    );
    orch.register_script_tool(
        "namegen",
        ScriptToolConfig {
            binary_path: "/usr/local/bin/sidequest-namegen".to_string(),
            genre_packs_path: "/tmp/genre_packs".to_string(),
        },
    );
    orch
}

/// Create a TurnContext with genre set (required for script tool injection).
fn context_with_genre(genre: &str) -> TurnContext {
    TurnContext {
        genre: Some(genre.to_string()),
        ..Default::default()
    }
}

// ============================================================================
// AC-1: Script tools registered as Claude tool_use definitions
// ============================================================================

/// Verify that `narrator_allowed_tools()` returns Bash(...) specs for all
/// registered script tools. This is what gets passed to `--allowedTools`.
///
/// RED because: `narrator_allowed_tools()` is currently private.
/// Dev must make it `pub(crate)` or `pub` for this test to compile.
#[test]
fn allowed_tools_include_all_registered_script_tools() {
    let orch = orchestrator_with_script_tools();
    let tools = orch.narrator_allowed_tools();

    assert_eq!(
        tools.len(),
        3,
        "Expected 3 allowed tools (encountergen, loadoutgen, namegen), got {}",
        tools.len()
    );

    // Each tool should be formatted as Bash(/path/to/binary:*)
    let tools_str = tools.join(" ");
    assert!(
        tools_str.contains("sidequest-encountergen"),
        "Allowed tools should include encountergen binary path, got: {tools_str}"
    );
    assert!(
        tools_str.contains("sidequest-loadoutgen"),
        "Allowed tools should include loadoutgen binary path, got: {tools_str}"
    );
    assert!(
        tools_str.contains("sidequest-namegen"),
        "Allowed tools should include namegen binary path, got: {tools_str}"
    );
}

/// With no tools registered, narrator_allowed_tools should return empty vec.
#[test]
fn allowed_tools_empty_when_no_script_tools_registered() {
    let (tx, _rx) = mpsc::channel::<TurnRecord>(WATCHER_CHANNEL_CAPACITY);
    let orch = Orchestrator::new(tx);
    let tools = orch.narrator_allowed_tools();

    assert!(
        tools.is_empty(),
        "Expected empty allowed tools when none registered, got {} tools",
        tools.len()
    );
}

// ============================================================================
// AC-2: Narrator prompt includes script tool instructions per tool
// ============================================================================

/// Verify that build_narrator_prompt() includes [ENCOUNTER GENERATOR] section
/// when encountergen is registered and genre is set.
///
/// RED because: `build_narrator_prompt()` does not exist yet.
/// Dev must extract prompt building from process_action() into this method.
#[test]
fn prompt_includes_encountergen_section_when_registered_with_genre() {
    let orch = orchestrator_with_script_tools();
    let context = context_with_genre("mutant_wasteland");

    let prompt_result = orch.build_narrator_prompt("look around", &context);

    assert!(
        prompt_result.prompt_text.contains("[ENCOUNTER GENERATOR]"),
        "Narrator prompt should contain [ENCOUNTER GENERATOR] section when tool is registered"
    );
    assert!(
        prompt_result.prompt_text.contains("--genre mutant_wasteland"),
        "Encountergen section should include the genre slug from context"
    );
}

/// Verify that build_narrator_prompt() includes [NPC GENERATOR] section.
#[test]
fn prompt_includes_namegen_section_when_registered_with_genre() {
    let orch = orchestrator_with_script_tools();
    let context = context_with_genre("mutant_wasteland");

    let prompt_result = orch.build_narrator_prompt("look around", &context);

    assert!(
        prompt_result.prompt_text.contains("[NPC GENERATOR]"),
        "Narrator prompt should contain [NPC GENERATOR] section when namegen is registered"
    );
}

/// Verify that build_narrator_prompt() includes [STARTING LOADOUT GENERATOR] section.
#[test]
fn prompt_includes_loadoutgen_section_when_registered_with_genre() {
    let orch = orchestrator_with_script_tools();
    let context = context_with_genre("mutant_wasteland");

    let prompt_result = orch.build_narrator_prompt("look around", &context);

    assert!(
        prompt_result.prompt_text.contains("[STARTING LOADOUT GENERATOR]"),
        "Narrator prompt should contain [STARTING LOADOUT GENERATOR] section when tool is registered"
    );
}

/// Script tool sections should NOT be injected when genre is None.
/// This is the silent failure mode — tools exist but genre slug is missing.
#[test]
fn prompt_omits_script_tools_when_genre_is_none() {
    let orch = orchestrator_with_script_tools();
    let context = TurnContext::default(); // genre: None

    let prompt_result = orch.build_narrator_prompt("look around", &context);

    assert!(
        !prompt_result.prompt_text.contains("[ENCOUNTER GENERATOR]"),
        "Script tool sections should NOT appear when genre is None"
    );
    assert!(
        !prompt_result.prompt_text.contains("[NPC GENERATOR]"),
        "Script tool sections should NOT appear when genre is None"
    );
    assert!(
        !prompt_result.prompt_text.contains("[STARTING LOADOUT GENERATOR]"),
        "Script tool sections should NOT appear when genre is None"
    );
}

/// The narrator system prompt should reference namegen in the NPC protocol.
/// This is already partially there (narrator.rs:72) but needs to reference
/// all three tools for completeness.
#[test]
fn narrator_system_prompt_references_script_tools() {
    let orch = orchestrator_with_script_tools();
    let context = context_with_genre("mutant_wasteland");

    let prompt_result = orch.build_narrator_prompt("look around", &context);

    // The system prompt (Primacy zone) should mention namegen for NPC creation
    assert!(
        prompt_result.prompt_text.contains("sidequest-namegen"),
        "Narrator system prompt should reference namegen tool for NPC creation"
    );
}

// ============================================================================
// AC-2 (cont.): Tool sections contain correct binary paths and genre
// ============================================================================

/// Each script tool section should embed the correct binary path so the LLM
/// can invoke it via Bash.
#[test]
fn script_tool_sections_contain_correct_binary_paths() {
    let orch = orchestrator_with_script_tools();
    let context = context_with_genre("neon_dystopia");

    let prompt_result = orch.build_narrator_prompt("enter the club", &context);

    assert!(
        prompt_result
            .prompt_text
            .contains("/usr/local/bin/sidequest-encountergen"),
        "Encountergen section should contain its binary path"
    );
    assert!(
        prompt_result
            .prompt_text
            .contains("/usr/local/bin/sidequest-loadoutgen"),
        "Loadoutgen section should contain its binary path"
    );
    assert!(
        prompt_result
            .prompt_text
            .contains("/usr/local/bin/sidequest-namegen"),
        "Namegen section should contain its binary path"
    );
}

/// Tool sections should use the genre from context, not a hardcoded value.
#[test]
fn script_tool_sections_use_genre_from_context() {
    let orch = orchestrator_with_script_tools();
    let context = context_with_genre("space_opera");

    let prompt_result = orch.build_narrator_prompt("hail the ship", &context);

    assert!(
        prompt_result.prompt_text.contains("--genre space_opera"),
        "Script tool sections should use the genre from TurnContext"
    );
}

// ============================================================================
// AC-3: OTEL spans for script tool invocation
// ============================================================================

/// The prompt result should report which script tools were injected,
/// enabling OTEL consumers to verify tool availability per turn.
///
/// RED because: build_narrator_prompt() doesn't exist yet, and the
/// NarratorPromptResult.script_tools_injected field needs to be added.
#[test]
fn prompt_result_reports_injected_script_tools() {
    let orch = orchestrator_with_script_tools();
    let context = context_with_genre("mutant_wasteland");

    let prompt_result = orch.build_narrator_prompt("look around", &context);

    assert_eq!(
        prompt_result.script_tools_injected.len(),
        3,
        "Should report 3 injected script tools"
    );
    assert!(
        prompt_result
            .script_tools_injected
            .contains(&"encountergen".to_string()),
        "Should list encountergen as injected"
    );
    assert!(
        prompt_result
            .script_tools_injected
            .contains(&"loadoutgen".to_string()),
        "Should list loadoutgen as injected"
    );
    assert!(
        prompt_result
            .script_tools_injected
            .contains(&"namegen".to_string()),
        "Should list namegen as injected"
    );
}

/// When genre is None, no tools should be reported as injected.
#[test]
fn prompt_result_reports_no_tools_when_genre_missing() {
    let orch = orchestrator_with_script_tools();
    let context = TurnContext::default();

    let prompt_result = orch.build_narrator_prompt("look around", &context);

    assert!(
        prompt_result.script_tools_injected.is_empty(),
        "Should report 0 injected tools when genre is None"
    );
}

// ============================================================================
// AC-4: WIRING TEST — full pipeline from registration to prompt
// ============================================================================

/// End-to-end wiring test: register tools → build prompt → verify sections
/// are in the correct attention zone (Valley) and tool specs are in allowed_tools.
///
/// This is the integration test that proves the pipeline is connected.
#[test]
fn wiring_script_tools_registered_injected_and_allowed() {
    let orch = orchestrator_with_script_tools();
    let context = context_with_genre("low_fantasy");

    // Step 1: Verify tools are registered (allowed_tools populated)
    let allowed_tools = orch.narrator_allowed_tools();
    assert_eq!(
        allowed_tools.len(),
        3,
        "Wiring check: 3 tools should be registered"
    );

    // Step 2: Verify tools are injected into prompt
    let prompt_result = orch.build_narrator_prompt("enter the tavern", &context);
    assert!(
        prompt_result.prompt_text.contains("[ENCOUNTER GENERATOR]"),
        "Wiring check: encountergen section should be in prompt"
    );
    assert!(
        prompt_result.prompt_text.contains("[NPC GENERATOR]"),
        "Wiring check: namegen section should be in prompt"
    );
    assert!(
        prompt_result.prompt_text.contains("[STARTING LOADOUT GENERATOR]"),
        "Wiring check: loadoutgen section should be in prompt"
    );

    // Step 3: Verify genre is threaded through
    assert!(
        prompt_result.prompt_text.contains("--genre low_fantasy"),
        "Wiring check: genre slug from TurnContext should appear in tool sections"
    );

    // Step 4: Verify tools are in allowed_tools (what gets passed to --allowedTools)
    for tool_spec in &allowed_tools {
        assert!(
            tool_spec.contains("Bash("),
            "Wiring check: each allowed tool should be a Bash(...) spec, got: {tool_spec}"
        );
    }

    // Step 5: Verify script_tools_injected for OTEL reporting
    assert_eq!(
        prompt_result.script_tools_injected.len(),
        3,
        "Wiring check: all 3 tools reported as injected for OTEL"
    );
}

/// Wiring negative case: no tools registered → no sections, no allowed_tools.
#[test]
fn wiring_no_tools_means_clean_prompt_and_empty_allowed() {
    let (tx, _rx) = mpsc::channel::<TurnRecord>(WATCHER_CHANNEL_CAPACITY);
    let orch = Orchestrator::new(tx);
    let context = context_with_genre("mutant_wasteland");

    let allowed_tools = orch.narrator_allowed_tools();
    assert!(
        allowed_tools.is_empty(),
        "Wiring check: no tools registered → empty allowed_tools"
    );

    let prompt_result = orch.build_narrator_prompt("look around", &context);
    assert!(
        !prompt_result.prompt_text.contains("[ENCOUNTER GENERATOR]"),
        "Wiring check: no tools → no encountergen section"
    );
    assert!(
        prompt_result.script_tools_injected.is_empty(),
        "Wiring check: no tools → no injected tools reported"
    );
}

// ============================================================================
// AC-1 (cont.): Allowed tools format validation
// ============================================================================

/// Each allowed tool spec should follow the Bash(binary_path:*) format
/// that the Claude CLI expects for --allowedTools.
#[test]
fn allowed_tools_use_bash_wildcard_format() {
    let orch = orchestrator_with_script_tools();
    let tools = orch.narrator_allowed_tools();

    for tool in &tools {
        assert!(
            tool.starts_with("Bash(") && tool.ends_with(":*)"),
            "Tool spec should be Bash(path:*) format, got: {tool}"
        );
    }
}

/// Registering a tool with the same name twice should overwrite, not duplicate.
#[test]
fn registering_same_tool_twice_overwrites() {
    let (tx, _rx) = mpsc::channel::<TurnRecord>(WATCHER_CHANNEL_CAPACITY);
    let mut orch = Orchestrator::new(tx);

    orch.register_script_tool(
        "encountergen",
        ScriptToolConfig {
            binary_path: "/old/path".to_string(),
            genre_packs_path: "/tmp/gp".to_string(),
        },
    );
    orch.register_script_tool(
        "encountergen",
        ScriptToolConfig {
            binary_path: "/new/path".to_string(),
            genre_packs_path: "/tmp/gp".to_string(),
        },
    );

    let tools = orch.narrator_allowed_tools();
    assert_eq!(
        tools.len(),
        1,
        "Re-registering the same tool name should overwrite, not duplicate"
    );
    assert!(
        tools[0].contains("/new/path"),
        "Overwritten tool should use the new path"
    );
}

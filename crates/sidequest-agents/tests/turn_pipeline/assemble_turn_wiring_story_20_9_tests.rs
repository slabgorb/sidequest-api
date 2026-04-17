//! Story 20-9: Wire assemble_turn into dispatch pipeline
//!
//! RED phase — source-level wiring tests that verify orchestrator.rs
//! actually CALLS assemble_turn() instead of building ActionResult directly.
//!
//! These 3 tests fail until Dev wires the import and call. Behavioral
//! tests for assemble_turn itself live in assemble_turn_story_20_1_tests.rs.

// ============================================================================
// AC-2 + AC-5: orchestrator.rs calls assemble_turn (source-level wiring)
// ============================================================================

/// The orchestrator's process_action() MUST call assemble_turn().
/// Without this, assemble_turn remains dead code and story 20-10
/// (tool call parsing) can't reach the game.
#[test]
fn orchestrator_calls_assemble_turn() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let orchestrator_source =
        std::fs::read_to_string(format!("{manifest_dir}/src/orchestrator.rs"))
            .expect("orchestrator.rs must exist");

    assert!(
        orchestrator_source.contains("assemble_turn("),
        "orchestrator.rs must call assemble_turn() — currently builds ActionResult directly \
         at lines 704-730. Story 20-9 wires assemble_turn into the dispatch pipeline."
    );
}

/// The orchestrator must use tool call results (either directly or via parse_tool_results).
/// Story 20-10 replaced direct ToolCallResults::default() with parse_tool_results().
#[test]
fn orchestrator_uses_tool_call_results() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let orchestrator_source =
        std::fs::read_to_string(format!("{manifest_dir}/src/orchestrator.rs"))
            .expect("orchestrator.rs must exist");

    assert!(
        orchestrator_source.contains("ToolCallResults")
            || orchestrator_source.contains("parse_tool_results"),
        "orchestrator.rs must reference ToolCallResults or parse_tool_results — \
         needed to pass tool results to assemble_turn()."
    );
}

/// The orchestrator must import from the tools module, not re-implement assembly.
#[test]
fn orchestrator_imports_from_tools_module() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let orchestrator_source =
        std::fs::read_to_string(format!("{manifest_dir}/src/orchestrator.rs"))
            .expect("orchestrator.rs must exist");

    assert!(
        orchestrator_source.contains("tools::assemble_turn")
            || orchestrator_source.contains("use crate::tools"),
        "orchestrator.rs must import from the tools module — assemble_turn lives in \
         crate::tools::assemble_turn, not in orchestrator.rs itself."
    );
}

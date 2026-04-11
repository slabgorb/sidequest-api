//! Story 18-1 RED: Sub-span instrumentation for system_tick, prompt_build, and barrier.
//!
//! Tests that the dispatch pipeline emits child spans inside the three opaque phases
//! (preprocess, agent_llm, system_tick) and that prompt_build and barrier spans
//! capture real durations.
//!
//! Tests use the `assert_span_emitted_by_source!` macro to verify span
//! definitions exist in the source code — a structural test that fails until
//! spans are added to dispatch/preprocess/orchestrator.

// ---------------------------------------------------------------------------
// Structural verification: span names must exist in dispatch source code
// ---------------------------------------------------------------------------

/// Read the dispatch module source and verify a span name is defined in it.
/// This is a structural test — it fails until the span definition exists.
fn dispatch_source() -> String {
    let dispatch_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/dispatch/mod.rs");
    std::fs::read_to_string(&dispatch_path)
        .unwrap_or_else(|e| panic!("Failed to read dispatch/mod.rs: {e}"))
}

fn preprocessor_source() -> String {
    // The preprocessor lives in the agents crate
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("sidequest-agents/src/preprocessor.rs");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("Failed to read preprocessor.rs: {e}"))
}

fn orchestrator_source() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("sidequest-agents/src/orchestrator.rs");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("Failed to read orchestrator.rs: {e}"))
}

// ===========================================================================
// AC1: system_tick sub-spans — combat, tropes, beat_context
// ===========================================================================

/// dispatch/mod.rs must define a "turn.system_tick.combat" span wrapping
/// the process_combat_and_chase() call inside the system_tick phase.
#[test]
fn system_tick_has_combat_sub_span() {
    let src = dispatch_source();
    assert!(
        src.contains("turn.system_tick.combat"),
        "dispatch/mod.rs must define a 'turn.system_tick.combat' sub-span \
         wrapping the combat::process_combat_and_chase() call"
    );
}

/// dispatch/mod.rs must define a "turn.system_tick.tropes" span wrapping
/// the tropes::process_tropes() call inside the system_tick phase.
#[test]
fn system_tick_has_tropes_sub_span() {
    let src = dispatch_source();
    assert!(
        src.contains("turn.system_tick.tropes"),
        "dispatch/mod.rs must define a 'turn.system_tick.tropes' sub-span \
         wrapping the tropes::process_tropes() call"
    );
}

/// dispatch/mod.rs must define a "turn.system_tick.beat_context" span
/// wrapping the trope beat context formatting for next turn.
#[test]
fn system_tick_has_beat_context_sub_span() {
    let src = dispatch_source();
    assert!(
        src.contains("turn.system_tick.beat_context"),
        "dispatch/mod.rs must define a 'turn.system_tick.beat_context' sub-span \
         wrapping the beat context formatting block"
    );
}

/// AC5: turn.system_tick.combat sub-span must record an in_combat diagnostic field.
#[test]
fn system_tick_combat_span_has_diagnostic_field() {
    let src = dispatch_source();
    // The span definition must include a field like in_combat
    assert!(
        src.contains("turn.system_tick.combat"),
        "turn.system_tick.combat span must exist first"
    );
    // Find the span definition and check it has a field
    let combat_idx = src.find("turn.system_tick.combat").unwrap();
    // Look at the next ~200 chars for field definitions
    let span_context = &src[combat_idx..src.len().min(combat_idx + 300)];
    assert!(
        span_context.contains("in_combat"),
        "turn.system_tick.combat span must record 'in_combat' field, \
         context around span: {}",
        &span_context[..span_context.len().min(200)]
    );
}

/// AC5: turn.system_tick.tropes sub-span must record active_count or similar.
#[test]
fn system_tick_tropes_span_has_diagnostic_field() {
    let src = dispatch_source();
    assert!(
        src.contains("turn.system_tick.tropes"),
        "turn.system_tick.tropes span must exist first"
    );
    let tropes_idx = src.find("turn.system_tick.tropes").unwrap();
    let span_context = &src[tropes_idx..src.len().min(tropes_idx + 300)];
    assert!(
        span_context.contains("active_count") || span_context.contains("trope_count"),
        "turn.system_tick.tropes span must record active_count or trope_count field, \
         context: {}",
        &span_context[..span_context.len().min(200)]
    );
}

// ===========================================================================
// AC1: preprocess sub-spans — structural verification in preprocessor.rs
// ===========================================================================

/// preprocessor.rs must define a "turn.preprocess.llm" span wrapping the
/// ClaudeClient::send_with_model() call.
#[test]
fn preprocessor_source_has_llm_sub_span() {
    let src = preprocessor_source();
    assert!(
        src.contains("turn.preprocess.llm"),
        "preprocessor.rs must define a 'turn.preprocess.llm' sub-span \
         wrapping the ClaudeClient::send_with_model() call"
    );
}

/// preprocessor.rs must define a "turn.preprocess.parse" span wrapping
/// the parse_response() + validation logic.
#[test]
fn preprocessor_source_has_parse_sub_span() {
    let src = preprocessor_source();
    assert!(
        src.contains("turn.preprocess.parse"),
        "preprocessor.rs must define a 'turn.preprocess.parse' sub-span \
         wrapping parse_response() and validation"
    );
}

// ===========================================================================
// AC1: preprocess wish_check sub-span in dispatch/mod.rs
// ===========================================================================

/// dispatch/mod.rs must define a "turn.preprocess.wish_check" span wrapping
/// the WishConsequenceEngine::evaluate() call inside the preprocess phase.
#[test]
fn preprocess_has_wish_check_sub_span() {
    let src = dispatch_source();
    assert!(
        src.contains("turn.preprocess.wish_check"),
        "dispatch/mod.rs must define a 'turn.preprocess.wish_check' sub-span \
         wrapping the WishConsequenceEngine::evaluate() call"
    );
}

// ===========================================================================
// AC1: agent_llm sub-spans — structural verification in orchestrator.rs
// ===========================================================================

/// orchestrator.rs must define a "turn.agent_llm.prompt_build" span wrapping
/// ContextBuilder zone assembly.
#[test]
fn orchestrator_source_has_prompt_build_sub_span() {
    let src = orchestrator_source();
    assert!(
        src.contains("turn.agent_llm.prompt_build"),
        "orchestrator.rs must define a 'turn.agent_llm.prompt_build' sub-span \
         wrapping ContextBuilder zone assembly"
    );
}

/// orchestrator.rs must define a "turn.agent_llm.inference" span wrapping
/// the Claude subprocess call.
#[test]
fn orchestrator_source_has_inference_sub_span() {
    let src = orchestrator_source();
    assert!(
        src.contains("turn.agent_llm.inference"),
        "orchestrator.rs must define a 'turn.agent_llm.inference' sub-span \
         wrapping the Claude subprocess call"
    );
}

/// orchestrator.rs must define a "turn.agent_llm.parse_response" span wrapping
/// response parsing and patch extraction.
#[test]
fn orchestrator_source_has_extraction_sub_span() {
    let src = orchestrator_source();
    assert!(
        src.contains("turn.agent_llm.parse_response"),
        "orchestrator.rs must define a 'turn.agent_llm.parse_response' sub-span \
         wrapping response parsing and patch extraction"
    );
}

// ===========================================================================
// AC2: prompt_build shows real duration — must wrap build_prompt_context()
// ===========================================================================

/// dispatch/mod.rs must wrap the build_prompt_context() call in a span.
/// Currently line 222 calls `prompt::build_prompt_context(ctx).await` but
/// there is no span wrapping this specific call. The existing
/// `turn.build_prompt_context` #[instrument] on the function is fine, but
/// the 0ms reading suggests the async work isn't captured — verify the span
/// uses .instrument() or an async-aware guard.
#[test]
fn prompt_build_has_async_aware_span() {
    let src = dispatch_source();
    // The build_prompt_context call must be instrumented with .instrument()
    // for async span capture, not just a sync .enter() guard
    let has_instrument_call = src.contains("build_prompt_context")
        && (src.contains(".instrument(") || src.contains("turn.prompt_build"));
    assert!(
        has_instrument_call,
        "build_prompt_context() must be wrapped with an async-aware span \
         (either .instrument() or a properly scoped turn.prompt_build span) \
         to capture real duration instead of 0ms"
    );
}

// ===========================================================================
// AC3: barrier shows real duration — must wrap handle_barrier()
// ===========================================================================

/// dispatch/mod.rs must wrap handle_barrier() in a "turn.barrier" span.
/// Currently there is no span wrapping this call — just inline tracing::info!()
/// events. The span must capture real duration for barrier turns and show 0ms
/// for FreePlay (no-op).
#[test]
fn barrier_has_dedicated_span() {
    let src = dispatch_source();
    assert!(
        src.contains("\"turn.barrier\"") || src.contains("turn.barrier"),
        "dispatch/mod.rs must define a 'turn.barrier' span wrapping the \
         handle_barrier() call to capture real duration"
    );
    // Verify it's an actual span definition, not just a log event name
    let has_span_def = src.contains("info_span!(\"turn.barrier\"")
        || src.contains("info_span!(\"turn.barrier\",")
        || src.contains("instrument(name = \"turn.barrier\"");
    assert!(
        has_span_def,
        "turn.barrier must be a tracing span (info_span! or #[instrument]), \
         not just a log event string"
    );
}

// ===========================================================================
// AC4: Existing tests must still pass (verified by test runner, not here)
// ===========================================================================

// AC4 is verified by running the full test suite. These tests themselves
// must compile and the pre-existing tests must still pass after adding
// sub-span instrumentation.

// ===========================================================================
// Wiring test: all 9 sub-spans + 2 fixed spans are defined
// ===========================================================================

/// Integration check: verify ALL required sub-spans from the AC list exist
/// across the relevant source files.
#[test]
fn all_required_sub_spans_are_defined() {
    let dispatch = dispatch_source();
    let preprocessor = preprocessor_source();
    let orchestrator = orchestrator_source();

    let missing: Vec<&str> = vec![
        // Preprocess sub-spans
        ("turn.preprocess.llm", &preprocessor),
        ("turn.preprocess.parse", &preprocessor),
        ("turn.preprocess.wish_check", &dispatch),
        // Agent LLM sub-spans
        ("turn.agent_llm.prompt_build", &orchestrator),
        ("turn.agent_llm.inference", &orchestrator),
        ("turn.agent_llm.parse_response", &orchestrator),
        // System tick sub-spans
        ("turn.system_tick.combat", &dispatch),
        ("turn.system_tick.tropes", &dispatch),
        ("turn.system_tick.beat_context", &dispatch),
    ]
    .into_iter()
    .filter(|(name, src)| !src.contains(name))
    .map(|(name, _)| name)
    .collect();

    assert!(
        missing.is_empty(),
        "Missing sub-span definitions: {:?}\n\
         All 9 sub-spans must be defined for flame chart granularity.",
        missing
    );
}

/// Integration check: prompt_build and barrier spans must exist for
/// non-zero duration capture (AC2 + AC3).
#[test]
fn prompt_build_and_barrier_spans_exist() {
    let dispatch = dispatch_source();

    let mut missing = Vec::new();
    if !dispatch.contains("info_span!(\"turn.barrier\"")
        && !dispatch.contains("info_span!(\"turn.barrier\",")
    {
        missing.push("turn.barrier (span definition in dispatch/mod.rs)");
    }
    // prompt_build can be either in dispatch or in the prompt module — check for
    // the async-aware instrumentation
    if !dispatch.contains(".instrument(") || !dispatch.contains("build_prompt_context") {
        // Also acceptable: the existing #[instrument] on the function is correct
        // and just needs the 0ms fix
        let prompt_src_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/dispatch/prompt.rs");
        let prompt_src = std::fs::read_to_string(&prompt_src_path).unwrap_or_default();
        if !prompt_src.contains("turn.build_prompt_context")
            && !prompt_src.contains("turn.prompt_build")
        {
            missing.push("turn.prompt_build (async-aware span in dispatch)");
        }
    }

    assert!(
        missing.is_empty(),
        "Missing span definitions for duration capture: {:?}",
        missing
    );
}

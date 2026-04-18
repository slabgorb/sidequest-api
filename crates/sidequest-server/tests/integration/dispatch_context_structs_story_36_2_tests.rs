//! Story 36-2: Refactor dispatch functions with too many arguments into context structs
//!
//! RED phase — these tests verify that functions in dispatch/ that had
//! `#[allow(clippy::too_many_arguments)]` are refactored to accept context structs.
//!
//! Five functions are targeted:
//!   1. response::build_response_messages — extra args folded into ResponseContext
//!   2. telemetry::emit_telemetry — extra args folded into TelemetryContext
//!   3. connect::dispatch_connect — 29 args folded into ConnectContext
//!   4. connect::start_character_creation — 15 args folded into ChargenInitContext
//!   5. connect::dispatch_character_creation — 34+ args folded into ChargenDispatchContext
//!
//! Test categories:
//!   - Structural: source-level verification that context structs exist and are used
//!   - Lint: verify #[allow(clippy::too_many_arguments)] annotations are removed
//!   - Wiring: verify call sites construct and pass context structs

use std::fs;
use std::path::Path;

// ============================================================================
// Helpers
// ============================================================================

fn read_dispatch_source(module: &str) -> String {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let path = Path::new(manifest)
        .join("src")
        .join("dispatch")
        .join(format!("{module}.rs"));
    fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read dispatch/{module}.rs: {e}"))
}

fn read_lib_source() -> String {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let path = Path::new(manifest).join("src").join("lib.rs");
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("Failed to read lib.rs: {e}"))
}

/// Extract a struct body from source text by name.
fn extract_struct_body<'a>(source: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!("struct {name}");
    let start = source.find(&needle)?;
    let body = &source[start..];
    let mut brace_depth = 0;
    let mut struct_end = body.len();
    for (i, ch) in body.char_indices() {
        match ch {
            '{' => brace_depth += 1,
            '}' => {
                brace_depth -= 1;
                if brace_depth == 0 {
                    struct_end = i + 1;
                    break;
                }
            }
            _ => {}
        }
    }
    Some(&body[..struct_end])
}

/// Extract a function signature (from `fn name` up to the opening `{`).
fn extract_fn_signature<'a>(source: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!("fn {name}(");
    let start = source.find(&needle)?;
    let body = &source[start..];
    let brace = body.find('{')?;
    Some(&body[..brace])
}

// ============================================================================
// LINT: #[allow(clippy::too_many_arguments)] must be removed
// ============================================================================

#[test]
fn response_rs_has_no_too_many_arguments_allow() {
    let src = read_dispatch_source("response");
    assert!(
        !src.contains("too_many_arguments"),
        "response.rs must not contain #[allow(clippy::too_many_arguments)] after refactoring. \
         The extra arguments to build_response_messages should be folded into a ResponseContext struct."
    );
}

#[test]
fn telemetry_rs_has_no_too_many_arguments_allow() {
    let src = read_dispatch_source("telemetry");
    assert!(
        !src.contains("too_many_arguments"),
        "telemetry.rs must not contain #[allow(clippy::too_many_arguments)] after refactoring. \
         The extra arguments to emit_telemetry should be folded into a TelemetryContext struct."
    );
}

#[test]
fn connect_rs_has_no_too_many_arguments_allow() {
    let src = read_dispatch_source("connect");
    assert!(
        !src.contains("too_many_arguments"),
        "connect.rs must not contain #[allow(clippy::too_many_arguments)] after refactoring. \
         dispatch_connect, start_character_creation, and dispatch_character_creation should \
         accept context structs instead of 15-29 individual parameters."
    );
}

// ============================================================================
// STRUCTURAL: ResponseContext exists and build_response_messages uses it
// ============================================================================

#[test]
fn response_context_struct_exists() {
    let src = read_dispatch_source("response");
    let body = extract_struct_body(&src, "ResponseContext");
    assert!(
        body.is_some(),
        "dispatch/response.rs must define a ResponseContext struct to bundle the per-turn \
         arguments that build_response_messages currently takes individually \
         (clean_narration, narration_text, result, tier_events, effective_action, etc.)"
    );
}

#[test]
fn response_context_has_required_fields() {
    let src = read_dispatch_source("response");
    let body = extract_struct_body(&src, "ResponseContext")
        .expect("ResponseContext struct must exist");

    // These fields correspond to the current extra args of build_response_messages
    let required_fields = [
        ("clean_narration", "cleaned narration text"),
        ("result", "ActionResult from the agent"),
        ("tier_events", "affinity tier-up events"),
        ("narration_state_delta", "state delta for narration message"),
    ];

    for (field, purpose) in &required_fields {
        assert!(
            body.contains(field),
            "ResponseContext must include a '{field}' field ({purpose}). \
             This was previously a separate parameter to build_response_messages."
        );
    }
}

#[test]
fn build_response_messages_accepts_response_context() {
    let src = read_dispatch_source("response");
    let sig = extract_fn_signature(&src, "build_response_messages")
        .expect("build_response_messages function must exist");

    assert!(
        sig.contains("ResponseContext"),
        "build_response_messages must accept a ResponseContext parameter. \
         Current signature has 8 args (DispatchContext + 7 extras). \
         After refactoring: (ctx: &mut DispatchContext, rctx: &ResponseContext, messages: &mut Vec<GameMessage>)"
    );
}

// ============================================================================
// STRUCTURAL: TelemetryContext exists and emit_telemetry uses it
// ============================================================================

#[test]
fn telemetry_context_struct_exists() {
    let src = read_dispatch_source("telemetry");
    let body = extract_struct_body(&src, "TelemetryContext");
    assert!(
        body.is_some(),
        "dispatch/telemetry.rs must define a TelemetryContext struct to bundle the per-turn \
         arguments that emit_telemetry currently takes individually \
         (turn_number, result, timing info, game_delta, patches, beats)."
    );
}

#[test]
fn telemetry_context_has_required_fields() {
    let src = read_dispatch_source("telemetry");
    let body = extract_struct_body(&src, "TelemetryContext")
        .expect("TelemetryContext struct must exist");

    let required_fields = [
        ("turn_number", "which turn this telemetry is for"),
        ("result", "ActionResult from the agent"),
        ("turn_start", "timing: when the turn started"),
        ("game_delta", "state changes this turn"),
        ("patches_applied", "patch summaries for GM panel"),
        ("beats_fired", "beat activations this turn"),
    ];

    for (field, purpose) in &required_fields {
        assert!(
            body.contains(field),
            "TelemetryContext must include a '{field}' field ({purpose}). \
             This was previously a separate parameter to emit_telemetry."
        );
    }
}

#[test]
fn emit_telemetry_accepts_telemetry_context() {
    let src = read_dispatch_source("telemetry");
    let sig = extract_fn_signature(&src, "emit_telemetry")
        .expect("emit_telemetry function must exist");

    assert!(
        sig.contains("TelemetryContext"),
        "emit_telemetry must accept a TelemetryContext parameter. \
         Current signature has 9 args (DispatchContext + 8 extras). \
         After refactoring: (ctx: &mut DispatchContext, tctx: &TelemetryContext)"
    );
}

// ============================================================================
// STRUCTURAL: ConnectContext exists and dispatch_connect uses it
// ============================================================================

#[test]
fn connect_context_struct_exists() {
    let src = read_dispatch_source("connect");
    let body = extract_struct_body(&src, "ConnectContext");
    assert!(
        body.is_some(),
        "dispatch/connect.rs must define a ConnectContext struct to bundle the 29 \
         individual mutable references that dispatch_connect currently takes. \
         This struct holds per-session state during the connection handshake."
    );
}

#[test]
fn connect_context_has_core_session_fields() {
    let src = read_dispatch_source("connect");
    let body = extract_struct_body(&src, "ConnectContext")
        .expect("ConnectContext struct must exist");

    // Key fields from the current 29-arg signature
    let required_fields = [
        ("session", "session state"),
        ("builder", "character builder"),
        ("trope_defs", "trope definitions from genre pack"),
        ("world_context", "world context string"),
        ("turn_manager", "turn manager"),
        ("npc_registry", "NPC registry"),
        ("inventory", "player inventory"),
        ("snapshot", "game snapshot"),
    ];

    for (field, purpose) in &required_fields {
        assert!(
            body.contains(field),
            "ConnectContext must include a '{field}' field ({purpose}). \
             This was previously a separate parameter to dispatch_connect."
        );
    }
}

#[test]
fn dispatch_connect_accepts_connect_context() {
    let src = read_dispatch_source("connect");
    let sig = extract_fn_signature(&src, "dispatch_connect")
        .expect("dispatch_connect function must exist");

    assert!(
        sig.contains("ConnectContext"),
        "dispatch_connect must accept a ConnectContext parameter instead of 29 individual args. \
         The function signature should be: (payload, ctx: &mut ConnectContext, state, player_id)"
    );
}

// ============================================================================
// STRUCTURAL: ChargenInitContext and start_character_creation
// ============================================================================

#[test]
fn chargen_init_context_struct_exists() {
    let src = read_dispatch_source("connect");
    let body = extract_struct_body(&src, "ChargenInitContext");
    assert!(
        body.is_some(),
        "dispatch/connect.rs must define a ChargenInitContext struct to bundle the 15 \
         arguments that start_character_creation currently takes. \
         This struct holds output slots and shared state for genre pack loading."
    );
}

#[test]
fn start_character_creation_accepts_chargen_init_context() {
    let src = read_dispatch_source("connect");
    let sig = extract_fn_signature(&src, "start_character_creation")
        .expect("start_character_creation function must exist");

    assert!(
        sig.contains("ChargenInitContext"),
        "start_character_creation must accept a ChargenInitContext parameter instead of \
         15 individual args. The init context bundles output slots (builder, trope_defs_out, \
         world_context_out, etc.) and shared state (lore_store, audio_mixer, etc.)."
    );
}

// ============================================================================
// STRUCTURAL: ChargenDispatchContext and dispatch_character_creation
// ============================================================================

#[test]
fn chargen_dispatch_context_struct_exists() {
    let src = read_dispatch_source("connect");
    let body = extract_struct_body(&src, "ChargenDispatchContext");
    assert!(
        body.is_some(),
        "dispatch/connect.rs must define a ChargenDispatchContext struct to bundle the 34+ \
         arguments that dispatch_character_creation currently takes. \
         This struct is the largest offender — it bundles character state, session state, \
         and shared infrastructure references."
    );
}

#[test]
fn dispatch_character_creation_accepts_chargen_dispatch_context() {
    let src = read_dispatch_source("connect");
    let sig = extract_fn_signature(&src, "dispatch_character_creation")
        .expect("dispatch_character_creation function must exist");

    assert!(
        sig.contains("ChargenDispatchContext"),
        "dispatch_character_creation must accept a ChargenDispatchContext parameter instead of \
         34+ individual args."
    );
}

// ============================================================================
// WIRING: Call sites in mod.rs construct context structs
// ============================================================================

#[test]
fn mod_rs_constructs_response_context() {
    let src = read_dispatch_source("mod");
    assert!(
        src.contains("ResponseContext"),
        "dispatch/mod.rs must construct a ResponseContext when calling build_response_messages. \
         The call site around line 2060 currently passes 7 individual args — it should \
         construct a ResponseContext struct instead."
    );
}

#[test]
fn mod_rs_constructs_telemetry_context() {
    let src = read_dispatch_source("mod");
    assert!(
        src.contains("TelemetryContext"),
        "dispatch/mod.rs must construct a TelemetryContext when calling emit_telemetry. \
         The call site around line 2337 currently passes 8 individual args — it should \
         construct a TelemetryContext struct instead."
    );
}

// ============================================================================
// WIRING: Call sites in lib.rs construct connect context structs
// ============================================================================

#[test]
fn lib_rs_constructs_connect_context() {
    let src = read_lib_source();
    assert!(
        src.contains("ConnectContext"),
        "lib.rs must construct a ConnectContext when calling dispatch_connect. \
         The call site around line 2374 currently passes 29 individual args — it should \
         construct a ConnectContext struct instead."
    );
}

#[test]
fn lib_rs_constructs_chargen_dispatch_context() {
    let src = read_lib_source();
    assert!(
        src.contains("ChargenDispatchContext"),
        "lib.rs must construct a ChargenDispatchContext when calling dispatch_character_creation. \
         The call site around line 2634 currently passes 34+ individual args — it should \
         construct a ChargenDispatchContext struct instead."
    );
}

// ============================================================================
// RULE COVERAGE: Rust lang-review checklist
// ============================================================================

// Rule #2: #[non_exhaustive] — not directly applicable (no new public enums)
// Rule #5: Validated constructors — context structs are internal, no trust boundary
// Rule #8: Deserialize bypass — context structs are not deserialized
// Rule #9: Public fields — context structs are pub(crate) or pub(super)

/// Rule #6: Test quality self-check — verify all tests in this file have
/// meaningful assertions (not vacuous).
#[test]
fn self_check_no_vacuous_assertions() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let test_path = std::path::Path::new(manifest)
        .join("tests")
        .join("integration")
        .join("dispatch_context_structs_story_36_2_tests.rs");
    let src = fs::read_to_string(&test_path)
        .expect("Should be able to read own test file");

    // Count vacuous `let _ = ...` patterns (discarding a Result without assertion).
    // We look for lines where the code has this pattern, excluding the self-check
    // function itself by skipping lines inside this test.
    let vacuous_pattern = ["let", " _", " ="].concat();
    let vacuous_count = src
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            // Skip comments and string-only lines
            !trimmed.starts_with("//") && !trimmed.starts_with("*") && !trimmed.starts_with('"')
        })
        // Skip lines that reference the pattern as a search target (this very function)
        .filter(|line| !line.contains("vacuous_pattern") && !line.contains(".contains(") && !line.contains(".concat()"))
        .filter(|line| line.contains(&vacuous_pattern))
        .count();
    assert_eq!(
        vacuous_count, 0,
        "Test file contains {vacuous_count} vacuous `let _ =` patterns in code lines. \
         Every test must assert something meaningful."
    );

    // Every #[test] function should contain at least one assert
    let test_count = src.matches("#[test]").count();
    // Count assert macros, excluding those inside string literals (assertion messages)
    let assert_count = src
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.starts_with("//")
                && !trimmed.starts_with('"')
                && (trimmed.starts_with("assert!")
                    || trimmed.starts_with("assert_eq!")
                    || trimmed.starts_with("assert_ne!"))
        })
        .count();
    assert!(
        assert_count >= test_count,
        "Found {test_count} tests but only {assert_count} top-level assertions. \
         Every test function must contain at least one assert."
    );
}

/// Rule #4: Tracing — verify dispatch modules have observability instrumentation.
/// response.rs and connect.rs use tracing::; telemetry.rs uses WatcherEventBuilder.
#[test]
fn dispatch_modules_have_observability() {
    // response.rs and connect.rs use tracing directly
    for module in &["response", "connect"] {
        let src = read_dispatch_source(module);
        assert!(
            src.contains("tracing::") || src.contains("use tracing"),
            "dispatch/{module}.rs must use tracing for error paths (Rule #4). \
             Refactored functions must preserve existing tracing instrumentation."
        );
    }
    // telemetry.rs uses WatcherEventBuilder for OTEL emission, not tracing:: directly
    let tel_src = read_dispatch_source("telemetry");
    assert!(
        tel_src.contains("WatcherEventBuilder") || tel_src.contains("tracing::"),
        "dispatch/telemetry.rs must use WatcherEventBuilder or tracing for observability. \
         Refactored emit_telemetry must preserve OTEL event emission."
    );
}

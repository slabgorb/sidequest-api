//! Story 7-9: ScenarioEngine integration — wire scenario lifecycle into
//! orchestrator turn loop.
//!
//! Capstone story for Epic 7 (Scenario System). All subsystems (belief state,
//! gossip, clues, accusations, NPC actions, pacing, archiver, scoring) are
//! implemented in stories 7-1 through 7-8. This story wires them into the
//! dispatch pipeline.
//!
//! ACs tested:
//!   AC1: ScenarioState initialization from genre pack at session start
//!   AC2: Between-turn processing (gossip, NPC actions, clue activation) + narrator injection
//!   AC3: Accusation handling through dispatcher (/accuse command)
//!   AC4: OTEL span emission for scenario decisions
//!   AC5: Full lifecycle integration test
//!
//! Wiring tests use source-level assertions where DispatchContext is too
//! complex to construct directly. This is the established pattern in this
//! codebase (see combat_wiring_story_15_6_tests.rs).

// =========================================================================
// AC1: ScenarioState initialization from genre pack at session start
// =========================================================================

/// ScenarioState::from_genre_pack is importable and functional from the
/// server crate. Compile-time wiring proof.
#[test]
fn scenario_state_importable_from_server_crate() {
    use sidequest_game::ScenarioState;
    use sidequest_game::ScenarioEvent;
    use sidequest_game::ScenarioEventType;

    // Verify types exist and are constructable
    let clue_graph = sidequest_game::ClueGraph::new(vec![]);
    let state = ScenarioState::new(
        clue_graph,
        std::collections::HashMap::new(),
        "test_guilty".to_string(),
        std::collections::HashMap::new(),
    );
    assert!(!state.is_resolved(), "New scenario should not be resolved");
    assert_eq!(state.guilty_npc(), "test_guilty");
    assert!(state.discovered_clues().is_empty());
}

/// Connect handler must load ScenarioPack from genre cache and bind to
/// GameSnapshot::scenario_state. Source-level verification that the
/// initialization path exists.
#[test]
fn connect_handler_initializes_scenario_state() {
    let source = std::fs::read_to_string(
        concat!(env!("CARGO_MANIFEST_DIR"), "/src/dispatch/connect.rs")
    ).expect("connect.rs must exist");

    assert!(
        source.contains("ScenarioState::from_genre_pack"),
        "connect.rs must call ScenarioState::from_genre_pack() to initialize \
         scenario state from genre pack data — AC1"
    );
    assert!(
        source.contains("snap.scenario_state = Some("),
        "connect.rs must assign scenario_state to the GameSnapshot — AC1"
    );
}

/// ScenarioState must be deserialized from saved state on session resume.
/// The GameSnapshot already has `scenario_state: Option<ScenarioState>` with
/// serde support, but we verify the roundtrip works.
#[test]
fn scenario_state_serde_roundtrip() {
    use sidequest_game::ScenarioState;

    let clue_graph = sidequest_game::ClueGraph::new(vec![]);
    let mut roles = std::collections::HashMap::new();
    roles.insert(
        "Mayor".to_string(),
        sidequest_game::npc_actions::ScenarioRole::Guilty,
    );
    let state = ScenarioState::new(
        clue_graph,
        roles,
        "Mayor".to_string(),
        std::collections::HashMap::new(),
    );

    let json = serde_json::to_string(&state).expect("ScenarioState must serialize");
    let restored: ScenarioState =
        serde_json::from_str(&json).expect("ScenarioState must deserialize");
    assert_eq!(restored.guilty_npc(), "Mayor");
    assert!(!restored.is_resolved());
}

// =========================================================================
// AC2: Between-turn processing + narrator context injection
// =========================================================================

/// Dispatch pipeline must call scenario_state.process_between_turns() during
/// the turn loop. Source-level check.
#[test]
fn dispatch_calls_process_between_turns() {
    let source = std::fs::read_to_string(
        concat!(env!("CARGO_MANIFEST_DIR"), "/src/dispatch/mod.rs")
    ).expect("dispatch/mod.rs must exist");

    assert!(
        source.contains("process_between_turns("),
        "dispatch must call scenario_state.process_between_turns() to run \
         gossip, NPC actions, and clue checks each turn — AC2"
    );
}

/// format_narrator_context() must be called in the prompt builder so the
/// narrator receives scenario state (tension, clues, NPC suspicions).
/// This is currently UNWIRED — prompt.rs has zero scenario references.
#[test]
fn prompt_builder_injects_scenario_context() {
    let source = std::fs::read_to_string(
        concat!(env!("CARGO_MANIFEST_DIR"), "/src/dispatch/prompt.rs")
    ).expect("dispatch/prompt.rs must exist");

    assert!(
        source.contains("format_narrator_context")
            || source.contains("scenario_state"),
        "prompt.rs must call format_narrator_context() or reference scenario_state \
         to inject scenario context into the narrator prompt — AC2. \
         Without this, the narrator has zero awareness of scenario tension, \
         discovered clues, or NPC suspicions."
    );
}

/// Between-turn ScenarioEvents must be formatted and injected into the
/// state_summary for the narrator. The events (gossip, NPC actions, clue
/// discovery) need to flow from process_between_turns() into prompt context.
#[test]
fn scenario_events_injected_into_prompt_context() {
    let source = std::fs::read_to_string(
        concat!(env!("CARGO_MANIFEST_DIR"), "/src/dispatch/mod.rs")
    ).expect("dispatch/mod.rs must exist");

    // The dispatch loop must format scenario events into the state_summary
    // or an equivalent narrator-visible string.
    assert!(
        source.contains("ScenarioEvent") || source.contains("scenario_events"),
        "dispatch/mod.rs must surface ScenarioEvents from between-turn processing \
         into the narrator prompt context — AC2"
    );
}

// =========================================================================
// AC3: Accusation handling through dispatcher
// =========================================================================

/// The dispatch pipeline must handle /accuse commands by routing them to
/// ScenarioState::handle_accusation(). Currently NOT wired.
#[test]
fn accuse_command_routed_to_scenario() {
    let slash_source = std::fs::read_to_string(
        concat!(env!("CARGO_MANIFEST_DIR"), "/src/dispatch/slash.rs")
    ).expect("dispatch/slash.rs must exist");

    let mod_source = std::fs::read_to_string(
        concat!(env!("CARGO_MANIFEST_DIR"), "/src/dispatch/mod.rs")
    ).expect("dispatch/mod.rs must exist");

    let has_accuse = slash_source.contains("accuse")
        || mod_source.contains("accuse")
        || mod_source.contains("handle_accusation");

    assert!(
        has_accuse,
        "dispatch must handle /accuse commands by routing to \
         ScenarioState::handle_accusation() — AC3. Currently no accusation \
         handling exists in the dispatch pipeline."
    );
}

/// handle_accusation() must evaluate evidence via evaluate_accusation() and
/// resolve the scenario. Source-level check that the accusation result is
/// processed (not just called and dropped).
#[test]
fn accusation_result_triggers_resolution() {
    let mod_source = std::fs::read_to_string(
        concat!(env!("CARGO_MANIFEST_DIR"), "/src/dispatch/mod.rs")
    ).expect("dispatch/mod.rs must exist");

    let slash_source = std::fs::read_to_string(
        concat!(env!("CARGO_MANIFEST_DIR"), "/src/dispatch/slash.rs")
    ).expect("dispatch/slash.rs must exist");

    let combined = format!("{}\n{}", mod_source, slash_source);

    assert!(
        combined.contains("AccusationResult") || combined.contains("handle_accusation"),
        "dispatch must process AccusationResult from handle_accusation() to \
         determine scenario resolution — AC3"
    );
}

/// The accusation system types must be importable from the server crate.
#[test]
fn accusation_types_importable() {
    use sidequest_game::Accusation;
    use sidequest_game::AccusationResult;
    use sidequest_game::EvidenceQuality;

    // Verify types are constructable
    let accusation = Accusation::new(
        "Detective".to_string(),
        "Mayor".to_string(),
        "I accuse the Mayor of the murder!".to_string(),
    );
    assert_eq!(accusation.accused_npc_name, "Mayor");

    // EvidenceQuality and AccusationResult enums must be pattern-matchable
    let quality = EvidenceQuality::Circumstantial;
    assert!(matches!(quality, EvidenceQuality::Circumstantial));
}

// =========================================================================
// AC4: OTEL span emission for scenario decisions
// =========================================================================

/// Between-turn processing must be wrapped in a `scenario:advance` span.
#[test]
fn otel_scenario_advance_span_exists() {
    let source = std::fs::read_to_string(
        concat!(env!("CARGO_MANIFEST_DIR"), "/src/dispatch/mod.rs")
    ).expect("dispatch/mod.rs must exist");

    assert!(
        source.contains("scenario:advance")
            || source.contains("scenario.advance")
            || source.contains("\"scenario\", \"advance\""),
        "dispatch must emit a scenario:advance (or scenario.advance) OTEL span \
         wrapping between-turn processing — AC4"
    );
}

/// Clue discovery events must emit OTEL events.
#[test]
fn otel_clue_discovered_event_exists() {
    let source = std::fs::read_to_string(
        concat!(env!("CARGO_MANIFEST_DIR"), "/src/dispatch/mod.rs")
    ).expect("dispatch/mod.rs must exist");

    assert!(
        source.contains("clue_discovered")
            || source.contains("clue.discovered")
            || source.contains("scenario:clue_discovered")
            || source.contains("scenario.clue_discovered"),
        "dispatch must emit a clue_discovered OTEL event when ClueActivation \
         fires during between-turn processing — AC4"
    );
}

/// Gossip propagation must emit OTEL events.
#[test]
fn otel_gossip_spread_event_exists() {
    let source = std::fs::read_to_string(
        concat!(env!("CARGO_MANIFEST_DIR"), "/src/dispatch/mod.rs")
    ).expect("dispatch/mod.rs must exist");

    assert!(
        source.contains("gossip_spread")
            || source.contains("gossip.spread")
            || source.contains("scenario:gossip_spread")
            || source.contains("scenario.gossip_spread"),
        "dispatch must emit a gossip_spread OTEL event when claims propagate \
         during between-turn processing — AC4"
    );
}

/// NPC autonomous actions must emit OTEL events with the scenario namespace.
#[test]
fn otel_npc_action_event_uses_scenario_namespace() {
    let source = std::fs::read_to_string(
        concat!(env!("CARGO_MANIFEST_DIR"), "/src/dispatch/mod.rs")
    ).expect("dispatch/mod.rs must exist");

    // Existing code uses "npc_actions" prefix. AC4 requires "scenario:npc_action"
    // to group all scenario OTEL under a unified namespace.
    assert!(
        source.contains("scenario:npc_action")
            || source.contains("scenario.npc_action")
            || (source.contains("\"scenario\"") && source.contains("npc_action")),
        "dispatch must emit scenario:npc_action OTEL events (under the scenario \
         namespace, not the generic npc_actions prefix) — AC4"
    );
}

/// Accusation resolution must emit OTEL events.
#[test]
fn otel_accusation_resolved_event_exists() {
    let source = std::fs::read_to_string(
        concat!(env!("CARGO_MANIFEST_DIR"), "/src/dispatch/mod.rs")
    ).expect("dispatch/mod.rs must exist");

    let slash_source = std::fs::read_to_string(
        concat!(env!("CARGO_MANIFEST_DIR"), "/src/dispatch/slash.rs")
    ).expect("dispatch/slash.rs must exist");

    let combined = format!("{}\n{}", source, slash_source);

    assert!(
        combined.contains("accusation_resolved")
            || combined.contains("accusation.resolved")
            || combined.contains("scenario:accusation_resolved")
            || combined.contains("scenario.accusation_resolved"),
        "dispatch must emit an accusation_resolved OTEL event when a player \
         accusation is resolved — AC4"
    );
}

// =========================================================================
// AC5: Full lifecycle wiring test — all pieces connected end-to-end
// =========================================================================

/// Comprehensive wiring check: the dispatch pipeline must have all five
/// scenario wiring points connected. This is the "lie detector" test —
/// if any piece is missing, the scenario system is half-wired.
#[test]
fn scenario_lifecycle_fully_wired() {
    let connect_source = std::fs::read_to_string(
        concat!(env!("CARGO_MANIFEST_DIR"), "/src/dispatch/connect.rs")
    ).expect("connect.rs must exist");

    let mod_source = std::fs::read_to_string(
        concat!(env!("CARGO_MANIFEST_DIR"), "/src/dispatch/mod.rs")
    ).expect("dispatch/mod.rs must exist");

    let prompt_source = std::fs::read_to_string(
        concat!(env!("CARGO_MANIFEST_DIR"), "/src/dispatch/prompt.rs")
    ).expect("dispatch/prompt.rs must exist");

    let slash_source = std::fs::read_to_string(
        concat!(env!("CARGO_MANIFEST_DIR"), "/src/dispatch/slash.rs")
    ).expect("dispatch/slash.rs must exist");

    // 1. Initialization
    assert!(
        connect_source.contains("ScenarioState::from_genre_pack"),
        "LIFECYCLE GAP: ScenarioState not initialized from genre pack in connect.rs"
    );

    // 2. Between-turn processing
    assert!(
        mod_source.contains("process_between_turns("),
        "LIFECYCLE GAP: process_between_turns() not called in dispatch pipeline"
    );

    // 3. Narrator context injection
    assert!(
        prompt_source.contains("scenario_state")
            || prompt_source.contains("format_narrator_context"),
        "LIFECYCLE GAP: Scenario context not injected into narrator prompt. \
         The narrator cannot see tension, clues, or NPC suspicions."
    );

    // 4. Accusation handling
    let all_dispatch = format!("{}\n{}", mod_source, slash_source);
    assert!(
        all_dispatch.contains("handle_accusation")
            || all_dispatch.contains("accuse"),
        "LIFECYCLE GAP: No accusation handling in dispatch pipeline. \
         Players cannot resolve scenarios."
    );

    // 5. OTEL observability (at least the advance span)
    assert!(
        mod_source.contains("scenario:advance")
            || mod_source.contains("scenario.advance"),
        "LIFECYCLE GAP: No scenario:advance OTEL span. GM panel cannot \
         verify scenario processing is active."
    );
}

/// ScenarioState integration with GameSnapshot — verify the field exists
/// and round-trips through the state snapshot.
#[test]
fn game_snapshot_scenario_state_roundtrip() {
    use sidequest_game::state::GameSnapshot;

    let snap = GameSnapshot::default();
    assert!(
        snap.scenario_state.is_none(),
        "Default GameSnapshot should have no scenario_state"
    );

    let json = serde_json::to_string(&snap).expect("GameSnapshot must serialize");
    let restored: GameSnapshot =
        serde_json::from_str(&json).expect("GameSnapshot must deserialize");
    assert!(
        restored.scenario_state.is_none(),
        "Deserialized default GameSnapshot should have no scenario_state"
    );
}

// =========================================================================
// Wiring: Module reachability from server crate
// =========================================================================

/// All scenario subsystem types must be reachable from the server crate.
/// This is a compile-time wiring test — if any re-export is missing, it
/// fails at compile time, not at runtime.
#[test]
fn all_scenario_types_reachable_from_server() {
    // Core scenario types
    use sidequest_game::ScenarioState;
    use sidequest_game::ScenarioEvent;
    use sidequest_game::ScenarioEventType;

    // Accusation types
    use sidequest_game::Accusation;
    use sidequest_game::AccusationResult;
    use sidequest_game::EvidenceQuality;
    use sidequest_game::EvidenceSummary;

    // Scoring types
    use sidequest_game::ScenarioScore;
    use sidequest_game::ScenarioGrade;
    use sidequest_game::DeductionQuality;
    use sidequest_game::ScenarioScoreInput;

    // Belief state types (used by scenario engine)
    use sidequest_game::BeliefState;
    use sidequest_game::Belief;
    use sidequest_game::BeliefSource;

    // Gossip types
    use sidequest_game::GossipEngine;
    use sidequest_game::PropagationResult;

    // NPC action types
    use sidequest_game::NpcAction;
    use sidequest_game::ScenarioRole;

    // Clue types
    use sidequest_game::ClueActivation;
    use sidequest_game::ClueGraph;

    // Suppress unused warnings while proving importability
    let _ = std::any::type_name::<ScenarioState>();
    let _ = std::any::type_name::<ScenarioEvent>();
    let _ = std::any::type_name::<ScenarioEventType>();
    let _ = std::any::type_name::<Accusation>();
    let _ = std::any::type_name::<AccusationResult>();
    let _ = std::any::type_name::<EvidenceQuality>();
    let _ = std::any::type_name::<EvidenceSummary>();
    let _ = std::any::type_name::<ScenarioScore>();
    let _ = std::any::type_name::<ScenarioGrade>();
    let _ = std::any::type_name::<DeductionQuality>();
    let _ = std::any::type_name::<ScenarioScoreInput>();
    let _ = std::any::type_name::<BeliefState>();
    let _ = std::any::type_name::<Belief>();
    let _ = std::any::type_name::<BeliefSource>();
    let _ = std::any::type_name::<GossipEngine>();
    let _ = std::any::type_name::<PropagationResult>();
    let _ = std::any::type_name::<NpcAction>();
    let _ = std::any::type_name::<ScenarioRole>();
    let _ = std::any::type_name::<ClueActivation>();
    let _ = std::any::type_name::<ClueGraph>();

    // This test passes at compile time — all types are reachable.
    // The assertions below verify the types are non-ZST (real types, not stubs).
    assert!(
        std::mem::size_of::<ScenarioState>() > 0,
        "ScenarioState must be a real type, not a zero-sized stub"
    );
    assert!(
        std::mem::size_of::<Accusation>() > 0,
        "Accusation must be a real type, not a zero-sized stub"
    );
}

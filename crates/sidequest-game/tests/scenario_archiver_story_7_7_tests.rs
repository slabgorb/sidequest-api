//! Story 7-7: Scenario archiver — save/resume mid-scenario state, session boundary handling
//!
//! RED phase — these tests reference types and methods that don't exist yet.
//!
//! `clippy::arc_with_non_send_sync` is allowed at the module level because
//! these tests are intentionally single-threaded — `SqliteStore` is `!Sync`
//! by design (the production code wraps it in an actor pattern), and for
//! test scaffolding an `Arc<SqliteStore>` is the right shape for calling
//! `ScenarioArchiver::new`.
#![allow(clippy::arc_with_non_send_sync)]
//! They will fail to compile until Dev implements:
//!   - ScenarioArchiver — persistence wrapper with versioned save/load
//!   - VersionedScenario — version-tagged wrapper around ScenarioState
//!   - ArchiveError — error type with VersionMismatch, Store, Serialization variants
//!   - SCENARIO_FORMAT_VERSION — current format version constant
//!   - SessionStore::save_scenario() — scenario-specific persistence method
//!   - SessionStore::load_scenario() — scenario-specific load method
//!
//! ACs tested: Round-trip, All state preserved, Version check, No scenario,
//!             Store integration, Session resume

use std::collections::HashMap;
use std::sync::Arc;

use sidequest_game::clue_activation::{ClueNode, ClueType, ClueVisibility, DiscoveryMethod};
use sidequest_game::npc_actions::ScenarioRole;
use sidequest_game::persistence::{SessionStore, SqliteStore};
use sidequest_game::scenario_archiver::{
    ArchiveError, ScenarioArchiver, VersionedScenario, SCENARIO_FORMAT_VERSION,
};
use sidequest_game::scenario_state::ScenarioState;
use sidequest_game::state::GameSnapshot;

// ============================================================================
// Helpers — build realistic scenario state for tests
// ============================================================================

/// Build a non-trivial ScenarioState with all field types populated.
fn build_rich_scenario_state() -> ScenarioState {
    let mut npc_roles = HashMap::new();
    npc_roles.insert("barkeep".to_string(), ScenarioRole::Guilty);
    npc_roles.insert("guard".to_string(), ScenarioRole::Witness);
    npc_roles.insert("smith".to_string(), ScenarioRole::Innocent);
    npc_roles.insert("merchant_wife".to_string(), ScenarioRole::Accomplice);

    let mut adjacency = HashMap::new();
    adjacency.insert(
        "barkeep".to_string(),
        vec!["guard".to_string(), "smith".to_string()],
    );
    adjacency.insert("guard".to_string(), vec!["barkeep".to_string()]);
    adjacency.insert(
        "smith".to_string(),
        vec!["barkeep".to_string(), "merchant_wife".to_string()],
    );
    adjacency.insert("merchant_wife".to_string(), vec!["smith".to_string()]);

    // Build a clue graph with dependencies
    let murder_weapon = ClueNode::new(
        "murder_weapon".to_string(),
        "A bloody dagger found behind the bar".to_string(),
        ClueType::Physical,
        DiscoveryMethod::Search,
        ClueVisibility::Hidden,
    );
    let witness_testimony = ClueNode::new(
        "witness_testimony".to_string(),
        "The guard saw someone near the docks".to_string(),
        ClueType::Testimonial,
        DiscoveryMethod::Interrogate,
        ClueVisibility::Obvious,
    );

    // Use ClueGraph constructor (clue_activation module)
    let clue_graph =
        sidequest_game::clue_activation::ClueGraph::new(vec![murder_weapon, witness_testimony]);

    let mut state = ScenarioState::new(clue_graph, npc_roles, "barkeep".to_string(), adjacency);

    // Set tension to a non-default value
    state.set_tension(0.65);

    // Discover a clue
    state.discover_clue("witness_testimony".to_string());

    state
}

// ============================================================================
// AC: Round-trip — save then load returns identical ScenarioState
// ============================================================================

#[test]
fn archiver_save_then_load_round_trips() {
    let store = SqliteStore::open_in_memory().expect("in-memory store");
    store
        .init_session("mutant_wasteland", "flickering_reach")
        .expect("init session");
    let archiver = ScenarioArchiver::new(Arc::new(store));

    let original = build_rich_scenario_state();
    archiver
        .save("test-session", &original)
        .expect("save should succeed");

    let loaded = archiver
        .load("test-session")
        .expect("load should succeed")
        .expect("should find saved scenario");

    // Verify key fields survived the round-trip
    assert_eq!(
        loaded.tension(),
        original.tension(),
        "tension must survive round-trip"
    );
    assert_eq!(
        loaded.guilty_npc(),
        original.guilty_npc(),
        "guilty_npc must survive round-trip"
    );
    assert_eq!(
        loaded.is_resolved(),
        original.is_resolved(),
        "resolved flag must survive round-trip"
    );
    assert_eq!(
        loaded.discovered_clues(),
        original.discovered_clues(),
        "discovered_clues must survive round-trip"
    );
    assert_eq!(
        loaded.npc_roles(),
        original.npc_roles(),
        "npc_roles must survive round-trip"
    );
}

// ============================================================================
// AC: All state preserved — BeliefStates, clues, tension, turn count survive
// ============================================================================

#[test]
fn archiver_preserves_tension_at_boundary_values() {
    let store = SqliteStore::open_in_memory().expect("in-memory store");
    store
        .init_session("mutant_wasteland", "flickering_reach")
        .expect("init session");
    let archiver = ScenarioArchiver::new(Arc::new(store));

    // Test with tension at maximum
    let mut state = build_rich_scenario_state();
    state.set_tension(1.0);

    archiver.save("max-tension", &state).expect("save");
    let loaded = archiver.load("max-tension").expect("load").expect("found");
    assert_eq!(loaded.tension(), 1.0, "max tension must survive");

    // Test with tension at zero
    state.set_tension(0.0);
    archiver.save("zero-tension", &state).expect("save");
    let loaded = archiver.load("zero-tension").expect("load").expect("found");
    assert_eq!(loaded.tension(), 0.0, "zero tension must survive");
}

#[test]
fn archiver_preserves_discovered_clues() {
    let store = SqliteStore::open_in_memory().expect("in-memory store");
    store
        .init_session("mutant_wasteland", "flickering_reach")
        .expect("init session");
    let archiver = ScenarioArchiver::new(Arc::new(store));

    let mut state = build_rich_scenario_state();
    state.discover_clue("murder_weapon".to_string());
    state.discover_clue("witness_testimony".to_string());

    archiver.save("clues-test", &state).expect("save");
    let loaded = archiver.load("clues-test").expect("load").expect("found");

    assert!(
        loaded.discovered_clues().contains("murder_weapon"),
        "murder_weapon clue must persist"
    );
    assert!(
        loaded.discovered_clues().contains("witness_testimony"),
        "witness_testimony clue must persist"
    );
    assert_eq!(
        loaded.discovered_clues().len(),
        2,
        "exactly 2 clues should be discovered"
    );
}

#[test]
fn archiver_preserves_all_npc_roles() {
    let store = SqliteStore::open_in_memory().expect("in-memory store");
    store
        .init_session("mutant_wasteland", "flickering_reach")
        .expect("init session");
    let archiver = ScenarioArchiver::new(Arc::new(store));

    let state = build_rich_scenario_state();
    archiver.save("roles-test", &state).expect("save");
    let loaded = archiver.load("roles-test").expect("load").expect("found");

    let roles = loaded.npc_roles();
    assert_eq!(
        roles.get("barkeep"),
        Some(&ScenarioRole::Guilty),
        "barkeep should be Guilty"
    );
    assert_eq!(
        roles.get("guard"),
        Some(&ScenarioRole::Witness),
        "guard should be Witness"
    );
    assert_eq!(
        roles.get("smith"),
        Some(&ScenarioRole::Innocent),
        "smith should be Innocent"
    );
    assert_eq!(
        roles.get("merchant_wife"),
        Some(&ScenarioRole::Accomplice),
        "merchant_wife should be Accomplice"
    );
}

#[test]
fn archiver_preserves_clue_graph_structure() {
    let store = SqliteStore::open_in_memory().expect("in-memory store");
    store
        .init_session("mutant_wasteland", "flickering_reach")
        .expect("init session");
    let archiver = ScenarioArchiver::new(Arc::new(store));

    let state = build_rich_scenario_state();
    let original_node_count = state.clue_graph().nodes().len();

    archiver.save("graph-test", &state).expect("save");
    let loaded = archiver.load("graph-test").expect("load").expect("found");

    assert_eq!(
        loaded.clue_graph().nodes().len(),
        original_node_count,
        "clue graph node count must survive round-trip"
    );
}

// ============================================================================
// AC: Version check — loading different version returns VersionMismatch error
// ============================================================================

#[test]
fn archiver_rejects_version_mismatch() {
    let store = SqliteStore::open_in_memory().expect("in-memory store");
    store
        .init_session("mutant_wasteland", "flickering_reach")
        .expect("init session");

    // Manually insert a VersionedScenario with a wrong version
    let state = build_rich_scenario_state();
    let wrong_version = VersionedScenario {
        version: SCENARIO_FORMAT_VERSION + 99,
        state,
    };
    let json = serde_json::to_string(&wrong_version).expect("serialize");

    // Store it directly via the store's scenario method
    store
        .save_scenario("version-test", &json)
        .expect("raw save");

    // Now try to load via the archiver — should get VersionMismatch
    let archiver = ScenarioArchiver::new(Arc::new(store));
    let result = archiver.load("version-test");

    assert!(result.is_err(), "loading wrong version should error");
    match result.unwrap_err() {
        ArchiveError::VersionMismatch { expected, found } => {
            assert_eq!(
                expected, SCENARIO_FORMAT_VERSION,
                "expected version should match current"
            );
            assert_eq!(
                found,
                SCENARIO_FORMAT_VERSION + 99,
                "found version should match what was stored"
            );
        }
        other => panic!("expected VersionMismatch error, got: {:?}", other),
    }
}

#[test]
fn versioned_scenario_wraps_state_with_current_version() {
    let state = build_rich_scenario_state();
    let versioned = VersionedScenario {
        version: SCENARIO_FORMAT_VERSION,
        state: state.clone(),
    };

    assert_eq!(
        versioned.version, SCENARIO_FORMAT_VERSION,
        "version should match current format version"
    );
    assert_eq!(
        versioned.state.guilty_npc(),
        state.guilty_npc(),
        "wrapped state should be identical"
    );
}

// ============================================================================
// AC: No scenario — loading when no scenario saved returns Ok(None)
// ============================================================================

#[test]
fn archiver_load_returns_none_when_no_scenario_saved() {
    let store = SqliteStore::open_in_memory().expect("in-memory store");
    store
        .init_session("mutant_wasteland", "flickering_reach")
        .expect("init session");
    let archiver = ScenarioArchiver::new(Arc::new(store));

    let result = archiver
        .load("nonexistent-session")
        .expect("load should not error for missing scenario");
    assert!(
        result.is_none(),
        "should return None when no scenario has been saved"
    );
}

// ============================================================================
// AC: Store integration — uses existing SessionStore trait
// ============================================================================

#[test]
fn archiver_constructed_from_session_store() {
    let store = SqliteStore::open_in_memory().expect("in-memory store");
    // ScenarioArchiver must accept Arc<dyn SessionStore>
    let _archiver = ScenarioArchiver::new(Arc::new(store));
}

#[test]
fn archiver_save_overwrites_previous() {
    let store = SqliteStore::open_in_memory().expect("in-memory store");
    store
        .init_session("mutant_wasteland", "flickering_reach")
        .expect("init session");
    let archiver = ScenarioArchiver::new(Arc::new(store));

    // Save initial state
    let mut state = build_rich_scenario_state();
    state.set_tension(0.3);
    archiver.save("overwrite-test", &state).expect("save 1");

    // Save updated state to same session
    state.set_tension(0.9);
    state.discover_clue("murder_weapon".to_string());
    archiver.save("overwrite-test", &state).expect("save 2");

    // Load should return the latest
    let loaded = archiver
        .load("overwrite-test")
        .expect("load")
        .expect("found");
    assert_eq!(
        loaded.tension(),
        0.9,
        "should load the most recently saved state"
    );
    assert!(
        loaded.discovered_clues().contains("murder_weapon"),
        "latest clue discovery should persist"
    );
}

// ============================================================================
// AC: Session resume — GameSnapshot preserves scenario_state through persistence
// ============================================================================

#[test]
fn game_snapshot_round_trips_scenario_state_through_sqlite() {
    // This is the wiring test: scenario_state survives the full
    // GameSnapshot → SqliteStore → GameSnapshot round-trip.
    let store = SqliteStore::open_in_memory().expect("in-memory store");
    store
        .init_session("mutant_wasteland", "flickering_reach")
        .expect("init session");

    let scenario = build_rich_scenario_state();
    let snapshot = GameSnapshot {
        genre_slug: "mutant_wasteland".to_string(),
        world_slug: "flickering_reach".to_string(),
        scenario_state: Some(scenario),
        ..Default::default()
    };

    use sidequest_game::persistence::SessionStore;
    store.save(&snapshot).expect("save snapshot");

    let loaded = store
        .load()
        .expect("load should succeed")
        .expect("should find saved session");

    let loaded_scenario = loaded
        .snapshot
        .scenario_state
        .expect("scenario_state must survive GameSnapshot persistence");

    assert_eq!(
        loaded_scenario.tension(),
        0.65,
        "tension must survive full GameSnapshot round-trip"
    );
    assert_eq!(
        loaded_scenario.guilty_npc(),
        "barkeep",
        "guilty_npc must survive full GameSnapshot round-trip"
    );
    assert!(
        loaded_scenario
            .discovered_clues()
            .contains("witness_testimony"),
        "discovered clues must survive full GameSnapshot round-trip"
    );
    assert_eq!(
        loaded_scenario.npc_roles().len(),
        4,
        "all NPC roles must survive full GameSnapshot round-trip"
    );
}

#[test]
fn game_snapshot_without_scenario_loads_as_none() {
    // Backward compatibility: old saves without scenario_state
    // should deserialize with scenario_state = None.
    let store = SqliteStore::open_in_memory().expect("in-memory store");
    store
        .init_session("mutant_wasteland", "flickering_reach")
        .expect("init session");

    let snapshot = GameSnapshot::default();
    use sidequest_game::persistence::SessionStore;
    store.save(&snapshot).expect("save");

    let loaded = store.load().expect("load").expect("found");
    assert!(
        loaded.snapshot.scenario_state.is_none(),
        "default snapshot should have no scenario_state"
    );
}

// ============================================================================
// Rule enforcement: #2 — ArchiveError must be #[non_exhaustive]
// ============================================================================

#[test]
fn archive_error_version_mismatch_variant_exists() {
    // Verify the VersionMismatch variant exists with expected/found fields.
    let err = ArchiveError::VersionMismatch {
        expected: 1,
        found: 2,
    };
    assert!(
        matches!(
            err,
            ArchiveError::VersionMismatch {
                expected: 1,
                found: 2
            }
        ),
        "VersionMismatch must carry expected and found version numbers"
    );
}

#[test]
fn archive_error_store_variant_exists() {
    // ArchiveError must have a Store variant wrapping PersistError.
    let persist_err = sidequest_game::persistence::PersistError::NotFound;
    let err = ArchiveError::Store(persist_err);
    assert!(
        matches!(err, ArchiveError::Store(_)),
        "Store variant must wrap PersistError"
    );
}

#[test]
fn archive_error_serialization_variant_exists() {
    // ArchiveError must have a Serialization variant for serde failures.
    let err = ArchiveError::Serialization("bad json".to_string());
    assert!(
        matches!(err, ArchiveError::Serialization(_)),
        "Serialization variant must exist"
    );
}

// ============================================================================
// Rule enforcement: #6 — VersionedScenario serde round-trip
// ============================================================================

#[test]
fn versioned_scenario_serde_round_trip() {
    let state = build_rich_scenario_state();
    let versioned = VersionedScenario {
        version: SCENARIO_FORMAT_VERSION,
        state,
    };

    let json = serde_json::to_string(&versioned).expect("serialize VersionedScenario");
    let deserialized: VersionedScenario =
        serde_json::from_str(&json).expect("deserialize VersionedScenario");

    assert_eq!(
        deserialized.version, SCENARIO_FORMAT_VERSION,
        "version must round-trip through serde"
    );
    assert_eq!(
        deserialized.state.guilty_npc(),
        versioned.state.guilty_npc(),
        "state must round-trip through serde"
    );
}

#[test]
fn versioned_scenario_format_version_is_positive() {
    // Compile-time assertion on a constant — evaluate in a const block so
    // clippy sees a static check, not a runtime check of a variable.
    const _: () = assert!(
        SCENARIO_FORMAT_VERSION > 0,
        "format version must be a positive integer"
    );
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn archiver_handles_empty_scenario_state() {
    // A scenario with no clues, no NPC roles, zero tension.
    let store = SqliteStore::open_in_memory().expect("in-memory store");
    store
        .init_session("mutant_wasteland", "flickering_reach")
        .expect("init session");
    let archiver = ScenarioArchiver::new(Arc::new(store));

    let clue_graph = sidequest_game::clue_activation::ClueGraph::new(vec![]);
    let state = ScenarioState::new(
        clue_graph,
        HashMap::new(),
        "nobody".to_string(),
        HashMap::new(),
    );

    archiver.save("empty-test", &state).expect("save");
    let loaded = archiver.load("empty-test").expect("load").expect("found");

    assert_eq!(loaded.tension(), 0.0, "empty state tension should be 0");
    assert_eq!(
        loaded.guilty_npc(),
        "nobody",
        "guilty_npc should persist even for empty scenario"
    );
    assert!(
        loaded.discovered_clues().is_empty(),
        "no clues should be discovered"
    );
    assert!(loaded.npc_roles().is_empty(), "no NPC roles should exist");
}

#[test]
fn archiver_independent_sessions_dont_interfere() {
    // Two different session IDs should have independent scenario states.
    let store = SqliteStore::open_in_memory().expect("in-memory store");
    store
        .init_session("mutant_wasteland", "flickering_reach")
        .expect("init session");
    let archiver = ScenarioArchiver::new(Arc::new(store));

    let mut state_a = build_rich_scenario_state();
    state_a.set_tension(0.2);

    let mut state_b = build_rich_scenario_state();
    state_b.set_tension(0.8);

    archiver.save("session-a", &state_a).expect("save a");
    archiver.save("session-b", &state_b).expect("save b");

    let loaded_a = archiver
        .load("session-a")
        .expect("load a")
        .expect("found a");
    let loaded_b = archiver
        .load("session-b")
        .expect("load b")
        .expect("found b");

    assert_eq!(
        loaded_a.tension(),
        0.2,
        "session A tension should be independent"
    );
    assert_eq!(
        loaded_b.tension(),
        0.8,
        "session B tension should be independent"
    );
}

//! Wiring tests for Story 35-4: Wire treasure_xp into state_mutations.
//!
//! Verifies that:
//! 1. apply_treasure_xp is called from state_mutations.rs (source-level wiring)
//! 2. WatcherEventBuilder("treasure_xp", StateTransition) is emitted
//! 3. Gold delta is captured before/after mutations
//! 4. TreasureXpConfig is built from genre pack rules.xp_affinity
//! 5. treasure_xp has a non-test consumer

#[test]
fn wiring_state_mutations_calls_apply_treasure_xp() {
    let source = include_str!("../../src/dispatch/state_mutations.rs");
    let production_code = source.split("#[cfg(test)]").next().unwrap_or(source);
    assert!(
        production_code.contains("apply_treasure_xp"),
        "state_mutations.rs must call apply_treasure_xp() — story 35-4"
    );
}

#[test]
fn wiring_state_mutations_emits_treasure_xp_otel() {
    let source = include_str!("../../src/dispatch/state_mutations.rs");
    assert!(
        source.contains("treasure.extracted"),
        "state_mutations.rs must emit treasure.extracted OTEL event — story 35-4"
    );
}

#[test]
fn wiring_state_mutations_captures_gold_before() {
    let source = include_str!("../../src/dispatch/state_mutations.rs");
    assert!(
        source.contains("gold_before"),
        "state_mutations.rs must capture gold_before for delta computation — story 35-4"
    );
}

#[test]
fn wiring_state_mutations_builds_treasure_xp_config() {
    let source = include_str!("../../src/dispatch/state_mutations.rs");
    assert!(
        source.contains("TreasureXpConfig"),
        "state_mutations.rs must build TreasureXpConfig from genre pack — story 35-4"
    );
}

#[test]
fn wiring_treasure_xp_uses_xp_affinity_from_rules() {
    let source = include_str!("../../src/dispatch/state_mutations.rs");
    assert!(
        source.contains("xp_affinity"),
        "state_mutations.rs must read xp_affinity from pack.rules — story 35-4"
    );
}

#[test]
fn wiring_treasure_xp_passes_rooms_for_surface_detection() {
    let source = include_str!("../../src/dispatch/state_mutations.rs");
    assert!(
        source.contains("ctx.rooms"),
        "state_mutations.rs must pass ctx.rooms for surface detection — story 35-4"
    );
}

// Note: apply_treasure_xp unit tests already exist in
// sidequest-game/tests/treasure_xp_story_19_9_tests.rs (15 tests).
// These wiring tests verify the dispatch integration only.

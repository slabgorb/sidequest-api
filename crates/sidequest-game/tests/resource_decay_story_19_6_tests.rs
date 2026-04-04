//! Story 19-6: Wire ResourceDeclaration.decay_per_turn — apply resource decay on trope tick
//!
//! Tests that decay_per_turn from ResourceDeclaration is applied to resource_state
//! after trope ticks, resources are clamped to min/max, and resources hitting min
//! are reported for GameMessage emission.
//!
//! AC coverage:
//! - AC1: decay_per_turn applied to resource_state after trope tick
//! - AC2: Resources clamped to declared min/max
//! - AC3: Detection of resources at min (for GameMessage)
//! - AC4: Resource with decay -0.1 reaches 0 after 10 ticks

use std::collections::HashMap;
use sidequest_game::state::GameSnapshot;
use sidequest_genre::ResourceDeclaration;

// ═══════════════════════════════════════════════════════════
// Test helpers
// ═══════════════════════════════════════════════════════════

/// Create a ResourceDeclaration via YAML deserialization (validated constructor).
fn make_resource(name: &str, min: f64, max: f64, starting: f64, decay: f64) -> ResourceDeclaration {
    let yaml = format!(
        "name: {name}\nlabel: {label}\nmin: {min}\nmax: {max}\nstarting: {starting}\nvoluntary: false\ndecay_per_turn: {decay}",
        name = name,
        label = name.to_uppercase(),
        min = min,
        max = max,
        starting = starting,
        decay = decay,
    );
    serde_yaml::from_str(&yaml).unwrap()
}

fn setup_snapshot_with_resources(resources: Vec<ResourceDeclaration>) -> GameSnapshot {
    let mut snap = GameSnapshot::default();
    for r in &resources {
        snap.resource_state.insert(r.name.clone(), r.starting);
    }
    snap.resource_declarations = resources;
    snap
}

// ═══════════════════════════════════════════════════════════
// AC1: decay_per_turn applied to resource_state
// ═══════════════════════════════════════════════════════════

/// Single resource with decay -0.1: after one tick, value drops from 1.0 to 0.9.
#[test]
fn single_decay_applied_once() {
    let heat = make_resource("heat", 0.0, 1.0, 1.0, -0.1);
    let mut snap = setup_snapshot_with_resources(vec![heat]);

    let at_min = snap.apply_decay_per_turn();

    let val = snap.resource_state["heat"];
    assert!(
        (val - 0.9).abs() < 1e-9,
        "heat should be 0.9 after one decay tick, got {val}"
    );
    assert!(at_min.is_empty(), "No resource should be at min after 1 tick");
}

/// Multiple resources decay independently in the same tick.
#[test]
fn multiple_resources_decay_independently() {
    let heat = make_resource("heat", 0.0, 1.0, 1.0, -0.1);
    let luck = make_resource("luck", 0.0, 10.0, 5.0, -0.5);
    let mut snap = setup_snapshot_with_resources(vec![heat, luck]);

    snap.apply_decay_per_turn();

    let heat_val = snap.resource_state["heat"];
    let luck_val = snap.resource_state["luck"];
    assert!(
        (heat_val - 0.9).abs() < 1e-9,
        "heat should be 0.9, got {heat_val}"
    );
    assert!(
        (luck_val - 4.5).abs() < 1e-9,
        "luck should be 4.5, got {luck_val}"
    );
}

/// Resource with zero decay (decay_per_turn = 0.0) is unchanged.
#[test]
fn zero_decay_no_change() {
    let stable = make_resource("stable", 0.0, 10.0, 5.0, 0.0);
    let mut snap = setup_snapshot_with_resources(vec![stable]);

    snap.apply_decay_per_turn();

    let val = snap.resource_state["stable"];
    assert!(
        (val - 5.0).abs() < 1e-9,
        "stable resource should remain 5.0, got {val}"
    );
}

/// Positive decay (resource increases per turn, e.g., regeneration).
#[test]
fn positive_decay_increases_resource() {
    let regen = make_resource("mana", 0.0, 100.0, 50.0, 2.0);
    let mut snap = setup_snapshot_with_resources(vec![regen]);

    snap.apply_decay_per_turn();

    let val = snap.resource_state["mana"];
    assert!(
        (val - 52.0).abs() < 1e-9,
        "mana should be 52.0 after regen tick, got {val}"
    );
}

// ═══════════════════════════════════════════════════════════
// AC2: Resources clamped to declared min/max
// ═══════════════════════════════════════════════════════════

/// Resource at min doesn't go below min even with decay.
#[test]
fn clamp_to_min() {
    let heat = make_resource("heat", 0.0, 1.0, 0.05, -0.1);
    let mut snap = setup_snapshot_with_resources(vec![heat]);

    snap.apply_decay_per_turn();

    let val = snap.resource_state["heat"];
    assert!(
        (val - 0.0).abs() < 1e-9,
        "heat should be clamped to min 0.0, got {val}"
    );
}

/// Resource at max doesn't exceed max even with positive decay.
#[test]
fn clamp_to_max() {
    let mana = make_resource("mana", 0.0, 100.0, 99.5, 2.0);
    let mut snap = setup_snapshot_with_resources(vec![mana]);

    snap.apply_decay_per_turn();

    let val = snap.resource_state["mana"];
    assert!(
        (val - 100.0).abs() < 1e-9,
        "mana should be clamped to max 100.0, got {val}"
    );
}

// ═══════════════════════════════════════════════════════════
// AC3: Detection of resources at min (for GameMessage)
// ═══════════════════════════════════════════════════════════

/// When resource hits min, apply_decay_per_turn returns it in the at_min list.
#[test]
fn returns_resource_at_min() {
    let heat = make_resource("heat", 0.0, 1.0, 0.1, -0.1);
    let mut snap = setup_snapshot_with_resources(vec![heat]);

    let at_min = snap.apply_decay_per_turn();

    assert_eq!(at_min.len(), 1, "One resource should be at min");
    assert_eq!(at_min[0].0, "heat", "heat should be the resource at min");
    assert!(
        (at_min[0].1 - 0.0).abs() < 1e-9,
        "min value should be 0.0, got {}",
        at_min[0].1
    );
}

/// Resource already at min stays at min and is NOT re-reported.
#[test]
fn already_at_min_not_re_reported() {
    let heat = make_resource("heat", 0.0, 1.0, 0.0, -0.1);
    let mut snap = setup_snapshot_with_resources(vec![heat]);

    let at_min = snap.apply_decay_per_turn();

    // Value was already at min before decay — should not be in at_min
    // (it didn't *reach* min this tick, it was already there)
    assert!(
        at_min.is_empty(),
        "Resource already at min should not be re-reported, got {:?}",
        at_min
    );
}

/// Multiple resources can hit min in the same tick.
#[test]
fn multiple_resources_hit_min_same_tick() {
    let heat = make_resource("heat", 0.0, 1.0, 0.1, -0.1);
    let luck = make_resource("luck", 0.0, 10.0, 0.5, -0.5);
    let mut snap = setup_snapshot_with_resources(vec![heat, luck]);

    let at_min = snap.apply_decay_per_turn();

    assert_eq!(at_min.len(), 2, "Both resources should hit min");
    let names: Vec<&str> = at_min.iter().map(|(n, _)| n.as_str()).collect();
    assert!(names.contains(&"heat"), "heat should be at min");
    assert!(names.contains(&"luck"), "luck should be at min");
}

// ═══════════════════════════════════════════════════════════
// AC4: Canonical test — resource with decay -0.1 reaches 0 after 10 ticks
// ═══════════════════════════════════════════════════════════

/// The acceptance criteria's canonical scenario: heat starts at 1.0, decays -0.1
/// per tick, reaches min (0.0) on tick 10.
#[test]
fn canonical_ten_tick_decay_to_zero() {
    let heat = make_resource("heat", 0.0, 1.0, 1.0, -0.1);
    let mut snap = setup_snapshot_with_resources(vec![heat]);

    // Ticks 1-9: heat should decrease but not hit 0
    for tick in 1..=9 {
        let at_min = snap.apply_decay_per_turn();
        let val = snap.resource_state["heat"];
        let expected = 1.0 - (tick as f64) * 0.1;
        assert!(
            (val - expected).abs() < 1e-9,
            "Tick {tick}: expected {expected}, got {val}"
        );
        assert!(
            at_min.is_empty(),
            "Tick {tick}: heat should not be at min yet"
        );
    }

    // Tick 10: heat reaches 0.0 — at_min fires
    let at_min = snap.apply_decay_per_turn();
    let val = snap.resource_state["heat"];
    assert!(
        (val - 0.0).abs() < 1e-9,
        "Tick 10: heat should be 0.0, got {val}"
    );
    assert_eq!(at_min.len(), 1, "Tick 10: heat should be reported at min");
    assert_eq!(at_min[0].0, "heat");

    // Tick 11: heat stays at 0.0, NOT re-reported
    let at_min = snap.apply_decay_per_turn();
    let val = snap.resource_state["heat"];
    assert!(
        (val - 0.0).abs() < 1e-9,
        "Tick 11: heat should still be 0.0"
    );
    assert!(
        at_min.is_empty(),
        "Tick 11: already at min, should not re-report"
    );
}

// ═══════════════════════════════════════════════════════════
// Edge cases
// ═══════════════════════════════════════════════════════════

/// No resource declarations → no decay, no error.
#[test]
fn no_declarations_no_decay() {
    let mut snap = GameSnapshot::default();
    let at_min = snap.apply_decay_per_turn();
    assert!(at_min.is_empty(), "Empty declarations = no decay");
}

/// Resource in declarations but missing from resource_state → no crash.
#[test]
fn declaration_without_state_entry_no_crash() {
    let heat = make_resource("heat", 0.0, 1.0, 1.0, -0.1);
    let mut snap = GameSnapshot::default();
    snap.resource_declarations = vec![heat];
    // Deliberately NOT inserting into resource_state

    let at_min = snap.apply_decay_per_turn();
    assert!(at_min.is_empty(), "Missing state entry should be harmless");
}

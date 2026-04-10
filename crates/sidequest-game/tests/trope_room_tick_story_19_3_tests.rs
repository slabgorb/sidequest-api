//! Story 19-3: Trope tick on room transition — fire trope engine per room move
//!
//! Tests that TropeEngine ticks with the room's keeper_awareness_modifier when
//! navigation_mode is RoomGraph and the player transitions between rooms.
//!
//! AC coverage:
//! - AC1: Trope tick fires on room transition in room_graph mode
//! - AC2: keeper_awareness_modifier from room data scales the multiplier
//! - AC3: Trope escalation events fire at thresholds
//! - AC4: Existing per-turn tick behavior unchanged in region mode
//! - AC5: 5 room transitions advance trope by 5 × rate_per_turn × modifier

use sidequest_game::trope::{TropeEngine, TropeState};
use sidequest_genre::{PassiveProgression, RoomDef, TropeDefinition, TropeEscalation};
use sidequest_protocol::NonBlankString;

/// Helper: create a trope definition with passive progression and optional escalation beats.
fn make_trope_def(id: &str, rate_per_turn: f64, escalation: Vec<(f64, &str)>) -> TropeDefinition {
    TropeDefinition {
        id: Some(id.to_string()),
        name: NonBlankString::new(id).unwrap(),
        description: None,
        category: "test".into(),
        triggers: vec![],
        narrative_hints: vec![],
        tension_level: None,
        resolution_hints: None,
        resolution_patterns: None,
        tags: vec![],
        passive_progression: Some(PassiveProgression {
            rate_per_turn,
            rate_per_day: 0.0,
            accelerators: vec![],
            decelerators: vec![],
            accelerator_bonus: 0.0,
            decelerator_penalty: 0.0,
        }),
        escalation: escalation
            .into_iter()
            .map(|(at, event)| TropeEscalation {
                at,
                event: event.to_string(),
                npcs_involved: vec![],
                stakes: String::new(),
            })
            .collect(),
        is_abstract: false,
        extends: None,
    }
}

/// Helper: create a RoomDef with a given keeper_awareness_modifier.
fn make_room(id: &str, modifier: f64) -> RoomDef {
    RoomDef {
        id: id.to_string(),
        name: id.to_string(),
        room_type: "normal".into(),
        size: (2, 2),
        keeper_awareness_modifier: modifier,
        exits: vec![],
        description: None, grid: None, legend: None, tactical_scale: None,
    }
}

// ═══════════════════════════════════════════════════════════
// AC1: Trope tick fires on room transition in room_graph mode
// ═══════════════════════════════════════════════════════════

/// When a player moves between rooms in RoomGraph mode, the trope engine
/// should tick once using the destination room's keeper_awareness_modifier.
#[test]
fn room_transition_ticks_trope_with_room_modifier() {
    let defs = vec![make_trope_def("keeper_rising", 0.1, vec![])];
    let mut tropes = vec![TropeState::new("keeper_rising")];
    let rooms = vec![make_room("entrance", 1.0), make_room("corridor", 1.5)];

    // Simulate room transition to "corridor" (modifier 1.5)
    let fired = TropeEngine::tick_room_transition(
        &mut tropes,
        &defs,
        &rooms,
        "corridor",
    );

    // Progression should be 0.1 × 1.5 = 0.15
    let prog = tropes[0].progression();
    assert!(
        (prog - 0.15).abs() < f64::EPSILON,
        "Expected progression 0.15 after room transition with modifier 1.5, got {prog}"
    );
    assert!(fired.is_empty(), "No escalation beats should fire at 0.15");
}

// ═══════════════════════════════════════════════════════════
// AC2: keeper_awareness_modifier from room data scales the multiplier
// ═══════════════════════════════════════════════════════════

/// A room with modifier 0.8 (low awareness) should advance tropes slower.
#[test]
fn low_modifier_room_slows_trope_progression() {
    let defs = vec![make_trope_def("keeper_rising", 0.1, vec![])];
    let mut tropes = vec![TropeState::new("keeper_rising")];
    let rooms = vec![make_room("safe_room", 0.8)];

    TropeEngine::tick_room_transition(&mut tropes, &defs, &rooms, "safe_room");

    let prog = tropes[0].progression();
    assert!(
        (prog - 0.08).abs() < f64::EPSILON,
        "Expected 0.1 × 0.8 = 0.08, got {prog}"
    );
}

/// A room with modifier 1.5 (high awareness) advances tropes faster.
#[test]
fn high_modifier_room_accelerates_trope_progression() {
    let defs = vec![make_trope_def("keeper_rising", 0.1, vec![])];
    let mut tropes = vec![TropeState::new("keeper_rising")];
    let rooms = vec![make_room("boss_chamber", 1.5)];

    TropeEngine::tick_room_transition(&mut tropes, &defs, &rooms, "boss_chamber");

    let prog = tropes[0].progression();
    assert!(
        (prog - 0.15).abs() < f64::EPSILON,
        "Expected 0.1 × 1.5 = 0.15, got {prog}"
    );
}

/// Default modifier (1.0) should behave identically to a plain tick.
#[test]
fn default_modifier_matches_plain_tick() {
    let defs = vec![make_trope_def("keeper_rising", 0.1, vec![])];

    let mut tropes_room = vec![TropeState::new("keeper_rising")];
    let rooms = vec![make_room("normal_room", 1.0)];
    TropeEngine::tick_room_transition(&mut tropes_room, &defs, &rooms, "normal_room");

    let mut tropes_plain = vec![TropeState::new("keeper_rising")];
    TropeEngine::tick(&mut tropes_plain, &defs);

    assert!(
        (tropes_room[0].progression() - tropes_plain[0].progression()).abs() < f64::EPSILON,
        "Room tick with modifier 1.0 should match plain tick"
    );
}

// ═══════════════════════════════════════════════════════════
// AC3: Trope escalation events fire at thresholds
// ═══════════════════════════════════════════════════════════

/// Escalation beat fires when room-modified progression crosses threshold.
#[test]
fn escalation_fires_at_threshold_with_room_modifier() {
    let defs = vec![make_trope_def(
        "keeper_rising",
        0.2,
        vec![(0.25, "The shadows grow deeper")],
    )];
    let mut tropes = vec![TropeState::new("keeper_rising")];
    let rooms = vec![make_room("danger_room", 1.5)];

    // First tick: 0.2 × 1.5 = 0.30, which crosses the 0.25 threshold
    let fired = TropeEngine::tick_room_transition(
        &mut tropes,
        &defs,
        &rooms,
        "danger_room",
    );

    assert_eq!(fired.len(), 1, "One escalation beat should fire");
    assert_eq!(fired[0].trope_id, "keeper_rising");
    assert_eq!(fired[0].beat.event, "The shadows grow deeper");
}

/// Escalation does NOT fire when modifier keeps progression below threshold.
#[test]
fn escalation_does_not_fire_below_threshold_with_low_modifier() {
    let defs = vec![make_trope_def(
        "keeper_rising",
        0.2,
        vec![(0.25, "The shadows grow deeper")],
    )];
    let mut tropes = vec![TropeState::new("keeper_rising")];
    let rooms = vec![make_room("safe_room", 0.8)];

    // First tick: 0.2 × 0.8 = 0.16, below 0.25
    let fired = TropeEngine::tick_room_transition(
        &mut tropes,
        &defs,
        &rooms,
        "safe_room",
    );

    assert!(
        fired.is_empty(),
        "Escalation should not fire at progression 0.16 (threshold 0.25)"
    );
}

// ═══════════════════════════════════════════════════════════
// AC4: Existing per-turn tick behavior unchanged in region mode
// ═══════════════════════════════════════════════════════════

/// The plain tick() method continues to work without room data — region mode
/// should not regress.
#[test]
fn plain_tick_unchanged_for_region_mode() {
    let defs = vec![make_trope_def("keeper_rising", 0.1, vec![])];
    let mut tropes = vec![TropeState::new("keeper_rising")];

    // Three plain ticks (region mode behavior)
    TropeEngine::tick(&mut tropes, &defs);
    TropeEngine::tick(&mut tropes, &defs);
    TropeEngine::tick(&mut tropes, &defs);

    let prog = tropes[0].progression();
    assert!(
        (prog - 0.3).abs() < f64::EPSILON,
        "3 plain ticks × 0.1 = 0.3, got {prog}"
    );
}

/// tick_room_transition with an unknown room_id should NOT tick (fail loud,
/// not silently advance with default multiplier).
#[test]
fn unknown_room_id_does_not_tick() {
    let defs = vec![make_trope_def("keeper_rising", 0.1, vec![])];
    let mut tropes = vec![TropeState::new("keeper_rising")];
    let rooms = vec![make_room("entrance", 1.0)];

    // "nonexistent" is not in the rooms list
    let fired = TropeEngine::tick_room_transition(
        &mut tropes,
        &defs,
        &rooms,
        "nonexistent",
    );

    assert!(
        tropes[0].progression() == 0.0,
        "Unknown room should NOT advance trope progression (no silent fallback)"
    );
    assert!(fired.is_empty());
}

// ═══════════════════════════════════════════════════════════
// AC5: 5 room transitions advance trope by 5 × rate × modifier
// ═══════════════════════════════════════════════════════════

/// Five sequential room transitions each apply the room's modifier.
#[test]
fn five_transitions_accumulate_correctly() {
    let defs = vec![make_trope_def("keeper_rising", 0.05, vec![])];
    let mut tropes = vec![TropeState::new("keeper_rising")];
    let rooms = vec![make_room("corridor", 1.5)];

    for _ in 0..5 {
        TropeEngine::tick_room_transition(&mut tropes, &defs, &rooms, "corridor");
    }

    // 5 × 0.05 × 1.5 = 0.375
    let expected = 5.0 * 0.05 * 1.5;
    let prog = tropes[0].progression();
    assert!(
        (prog - expected).abs() < 1e-10,
        "5 transitions × 0.05 × 1.5 = {expected}, got {prog}"
    );
}

/// Five transitions through rooms with different modifiers.
#[test]
fn five_transitions_different_rooms() {
    let defs = vec![make_trope_def("keeper_rising", 0.1, vec![])];
    let mut tropes = vec![TropeState::new("keeper_rising")];
    let rooms = vec![
        make_room("entrance", 1.0),
        make_room("corridor", 1.2),
        make_room("trap_room", 1.5),
        make_room("safe_room", 0.8),
        make_room("boss", 1.5),
    ];

    let room_sequence = ["entrance", "corridor", "trap_room", "safe_room", "boss"];
    for room_id in &room_sequence {
        TropeEngine::tick_room_transition(&mut tropes, &defs, &rooms, room_id);
    }

    // 0.1 × (1.0 + 1.2 + 1.5 + 0.8 + 1.5) = 0.1 × 6.0 = 0.6
    let expected = 0.1 * (1.0 + 1.2 + 1.5 + 0.8 + 1.5);
    let prog = tropes[0].progression();
    assert!(
        (prog - expected).abs() < 1e-10,
        "Expected {expected}, got {prog}"
    );
}

// ═══════════════════════════════════════════════════════════
// Edge cases — the Brute Squad stress tests
// ═══════════════════════════════════════════════════════════

/// Multiple tropes all tick on a single room transition.
#[test]
fn multiple_tropes_all_tick_on_room_transition() {
    let defs = vec![
        make_trope_def("keeper_rising", 0.1, vec![]),
        make_trope_def("corruption_spread", 0.05, vec![]),
    ];
    let mut tropes = vec![
        TropeState::new("keeper_rising"),
        TropeState::new("corruption_spread"),
    ];
    let rooms = vec![make_room("corridor", 1.3)];

    TropeEngine::tick_room_transition(&mut tropes, &defs, &rooms, "corridor");

    assert!(
        (tropes[0].progression() - 0.13).abs() < f64::EPSILON,
        "keeper_rising: 0.1 × 1.3 = 0.13"
    );
    assert!(
        (tropes[1].progression() - 0.065).abs() < f64::EPSILON,
        "corruption_spread: 0.05 × 1.3 = 0.065"
    );
}

/// Progression should clamp at 1.0 even with high modifier.
#[test]
fn progression_clamps_at_one() {
    let defs = vec![make_trope_def("fast_trope", 0.5, vec![])];
    let mut tropes = vec![TropeState::new("fast_trope")];
    tropes[0].set_progression(0.9);
    let rooms = vec![make_room("boss", 1.5)];

    // 0.9 + (0.5 × 1.5) = 0.9 + 0.75 = 1.65, should clamp to 1.0
    TropeEngine::tick_room_transition(&mut tropes, &defs, &rooms, "boss");

    assert!(
        (tropes[0].progression() - 1.0).abs() < f64::EPSILON,
        "Progression should clamp at 1.0"
    );
}

/// Empty rooms list means no room can be found — no tick.
#[test]
fn empty_rooms_list_no_tick() {
    let defs = vec![make_trope_def("keeper_rising", 0.1, vec![])];
    let mut tropes = vec![TropeState::new("keeper_rising")];
    let rooms: Vec<RoomDef> = vec![];

    TropeEngine::tick_room_transition(&mut tropes, &defs, &rooms, "anywhere");

    assert_eq!(
        tropes[0].progression(),
        0.0,
        "Empty rooms should not advance tropes"
    );
}

/// Resolved/dormant tropes should NOT tick even during room transition.
#[test]
fn resolved_tropes_skip_room_tick() {
    use sidequest_game::trope::TropeStatus;

    let defs = vec![make_trope_def("keeper_rising", 0.1, vec![])];
    let mut tropes = vec![TropeState::new("keeper_rising")];
    tropes[0].set_status(TropeStatus::Resolved);
    let rooms = vec![make_room("corridor", 1.5)];

    TropeEngine::tick_room_transition(&mut tropes, &defs, &rooms, "corridor");

    assert_eq!(
        tropes[0].progression(),
        0.0,
        "Resolved tropes must not advance"
    );
}

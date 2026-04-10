//! Story 16-11: Resource threshold → KnownFact pipeline — permanent narrator memory
//!
//! RED phase tests. When a ResourcePool crosses a threshold, a LoreFragment
//! (the system's "KnownFact" for world-level knowledge) is minted in LoreStore.
//!
//!   AC1: apply_resource_patch crossing → LoreFragment minted in LoreStore
//!   AC2: LoreFragment has threshold's event_id (as id) and narrator_hint (as content)
//!   AC3: High relevance — Event category + recent turn ensures budget selection picks it
//!   AC4: apply_pool_decay crossings also mint LoreFragments
//!   AC5: Already-crossed thresholds → no duplicate LoreFragments
//!   AC6: Multiple thresholds crossed in one patch → multiple LoreFragments
//!   AC7: Integration — LoreFragment appears in select_lore_for_prompt output

use sidequest_game::lore::{LoreCategory, LoreSource, LoreStore, select_lore_for_prompt};
use sidequest_game::resource_pool::{
    mint_threshold_lore, ResourcePatch, ResourcePatchOp, ResourcePool, ResourceThreshold,
};
use sidequest_game::state::GameSnapshot;

// ═══════════════════════════════════════════════════════════
// Test helpers
// ═══════════════════════════════════════════════════════════

fn make_threshold(at: f64, event_id: &str, hint: &str) -> ResourceThreshold {
    ResourceThreshold {
        at,
        event_id: event_id.to_string(),
        narrator_hint: hint.to_string(),
    }
}

fn make_pool_with_thresholds(
    name: &str,
    current: f64,
    min: f64,
    max: f64,
    thresholds: Vec<ResourceThreshold>,
) -> ResourcePool {
    ResourcePool {
        name: name.to_string(),
        label: name.to_string(),
        current,
        min,
        max,
        voluntary: true,
        decay_per_turn: 0.0,
        thresholds,
    }
}

fn snapshot_with_pools(pools: Vec<ResourcePool>) -> GameSnapshot {
    let mut snap = GameSnapshot::default();
    for pool in pools {
        snap.resources.insert(pool.name.clone(), pool);
    }
    snap
}

// ═══════════════════════════════════════════════════════════
// AC1: apply_resource_patch crossing → LoreFragment minted
// ═══════════════════════════════════════════════════════════

#[test]
fn patch_crossing_threshold_mints_lore_fragment() {
    let pool = make_pool_with_thresholds(
        "humanity",
        50.0,
        0.0,
        100.0,
        vec![make_threshold(25.0, "humanity_low", "Humanity has dropped dangerously low.")],
    );
    let mut snap = snapshot_with_pools(vec![pool]);
    let mut lore = LoreStore::new();

    let patch = ResourcePatch {
        resource_name: "humanity".to_string(),
        operation: ResourcePatchOp::Set,
        value: 20.0,
    };

    let result = snap.apply_resource_patch(&patch).unwrap();
    assert_eq!(result.crossed_thresholds.len(), 1);

    // This is the new function under test — mints LoreFragments from crossings
    mint_threshold_lore(&result.crossed_thresholds, &mut lore, 5);

    assert_eq!(lore.len(), 1);
}

// ═══════════════════════════════════════════════════════════
// AC2: LoreFragment has event_id and narrator_hint
// ═══════════════════════════════════════════════════════════

#[test]
fn minted_fragment_carries_event_id_and_narrator_hint() {
    let threshold = make_threshold(
        25.0,
        "humanity_low",
        "Humanity has dropped dangerously low.",
    );
    let mut lore = LoreStore::new();

    mint_threshold_lore(&[threshold], &mut lore, 10);

    // Fragment id should be the event_id
    let results = lore.query_by_category(&LoreCategory::Event);
    assert_eq!(results.len(), 1);

    let frag = results[0];
    assert_eq!(frag.id(), "humanity_low");
    assert_eq!(frag.content(), "Humanity has dropped dangerously low.");
}

#[test]
fn minted_fragment_source_is_game_event() {
    let threshold = make_threshold(10.0, "heat_critical", "Heat is at critical levels.");
    let mut lore = LoreStore::new();

    mint_threshold_lore(&[threshold], &mut lore, 3);

    let results = lore.query_by_category(&LoreCategory::Event);
    assert_eq!(results.len(), 1);
    assert_eq!(*results[0].source(), LoreSource::GameEvent);
}

// ═══════════════════════════════════════════════════════════
// AC3: High relevance — Event category + recent turn
// ═══════════════════════════════════════════════════════════

#[test]
fn minted_fragment_has_event_category_for_high_relevance() {
    let threshold = make_threshold(50.0, "morale_half", "Morale has fallen to half.");
    let mut lore = LoreStore::new();

    mint_threshold_lore(&[threshold], &mut lore, 7);

    let results = lore.query_by_category(&LoreCategory::Event);
    assert_eq!(results.len(), 1, "Fragment must be in Event category for high relevance");
}

#[test]
fn minted_fragment_has_turn_created_for_recency_sorting() {
    let threshold = make_threshold(50.0, "morale_half", "Morale has fallen to half.");
    let mut lore = LoreStore::new();

    mint_threshold_lore(&[threshold], &mut lore, 42);

    let results = lore.query_by_category(&LoreCategory::Event);
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].turn_created(),
        Some(42),
        "turn_created must be set for recency-based budget selection"
    );
}

// ═══════════════════════════════════════════════════════════
// AC4: apply_pool_decay crossings also mint LoreFragments
// ═══════════════════════════════════════════════════════════

#[test]
fn decay_crossing_threshold_mints_lore_fragment() {
    let mut pool = make_pool_with_thresholds(
        "fuel",
        12.0,
        0.0,
        100.0,
        vec![make_threshold(10.0, "fuel_low", "Fuel reserves are running low.")],
    );
    pool.decay_per_turn = -5.0;

    let mut snap = snapshot_with_pools(vec![pool]);
    let mut lore = LoreStore::new();

    let crossings = snap.apply_pool_decay();
    assert_eq!(crossings.len(), 1, "decay should cross the fuel_low threshold");

    mint_threshold_lore(&crossings, &mut lore, 15);

    assert_eq!(lore.len(), 1);
    let results = lore.query_by_category(&LoreCategory::Event);
    assert_eq!(results[0].id(), "fuel_low");
    assert_eq!(results[0].content(), "Fuel reserves are running low.");
}

// ═══════════════════════════════════════════════════════════
// AC5: No duplicate LoreFragments for already-crossed thresholds
// ═══════════════════════════════════════════════════════════

#[test]
fn duplicate_threshold_crossing_does_not_mint_second_fragment() {
    let threshold = make_threshold(25.0, "humanity_low", "Humanity has dropped dangerously low.");
    let mut lore = LoreStore::new();

    // First crossing — should succeed
    mint_threshold_lore(&[threshold.clone()], &mut lore, 5);
    assert_eq!(lore.len(), 1);

    // Second crossing with same event_id — should not add duplicate
    mint_threshold_lore(&[threshold], &mut lore, 10);
    assert_eq!(lore.len(), 1, "duplicate event_id must not create a second fragment");
}

// ═══════════════════════════════════════════════════════════
// AC6: Multiple thresholds crossed → multiple LoreFragments
// ═══════════════════════════════════════════════════════════

#[test]
fn multiple_thresholds_crossed_mints_multiple_fragments() {
    let pool = make_pool_with_thresholds(
        "humanity",
        80.0,
        0.0,
        100.0,
        vec![
            make_threshold(75.0, "humanity_warning", "Humanity is declining."),
            make_threshold(50.0, "humanity_half", "Humanity has fallen to half."),
            make_threshold(25.0, "humanity_low", "Humanity is dangerously low."),
        ],
    );
    let mut snap = snapshot_with_pools(vec![pool]);
    let mut lore = LoreStore::new();

    // Drop from 80 to 20 — crosses all three thresholds
    let patch = ResourcePatch {
        resource_name: "humanity".to_string(),
        operation: ResourcePatchOp::Set,
        value: 20.0,
    };
    let result = snap.apply_resource_patch(&patch).unwrap();
    assert_eq!(result.crossed_thresholds.len(), 3);

    mint_threshold_lore(&result.crossed_thresholds, &mut lore, 8);

    assert_eq!(lore.len(), 3, "each crossed threshold should mint one fragment");

    // Verify each event_id is present
    let events = lore.query_by_category(&LoreCategory::Event);
    let ids: Vec<&str> = events.iter().map(|f| f.id()).collect();
    assert!(ids.contains(&"humanity_warning"));
    assert!(ids.contains(&"humanity_half"));
    assert!(ids.contains(&"humanity_low"));
}

// ═══════════════════════════════════════════════════════════
// AC7: Integration — appears in select_lore_for_prompt
// ═══════════════════════════════════════════════════════════

#[test]
fn threshold_lore_appears_in_narrator_context_selection() {
    let threshold = make_threshold(
        25.0,
        "humanity_critical",
        "Humanity has reached a critical threshold — the world teeters on the edge.",
    );
    let mut lore = LoreStore::new();

    mint_threshold_lore(&[threshold], &mut lore, 20);

    // Budget of 1000 tokens — generous enough to include our fragment
    let selected = select_lore_for_prompt(&lore, 1000, None, None);

    assert!(
        !selected.is_empty(),
        "threshold lore must appear in budget-aware narrator context"
    );
    assert!(
        selected.iter().any(|f| f.id() == "humanity_critical"),
        "humanity_critical fragment must be selected for narrator prompt"
    );
}

#[test]
fn threshold_lore_prioritized_when_event_category_requested() {
    let threshold = make_threshold(
        50.0,
        "heat_spike",
        "Heat levels have spiked — attention from authorities is imminent.",
    );
    let mut lore = LoreStore::new();

    mint_threshold_lore(&[threshold], &mut lore, 30);

    // Specifically request Event category — threshold facts should appear
    let selected = select_lore_for_prompt(
        &lore,
        1000,
        Some(&[LoreCategory::Event]),
        None,
    );

    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].id(), "heat_spike");
}

// ═══════════════════════════════════════════════════════════
// Wiring test: end-to-end patch → lore → selection
// ═══════════════════════════════════════════════════════════

#[test]
fn end_to_end_patch_to_narrator_context() {
    // Full pipeline: create snapshot → patch resource → cross threshold →
    // mint lore → verify it appears in narrator context selection
    let pool = make_pool_with_thresholds(
        "reputation",
        40.0,
        0.0,
        100.0,
        vec![make_threshold(
            30.0,
            "reputation_shaky",
            "Your reputation in this district is becoming unreliable.",
        )],
    );
    let mut snap = snapshot_with_pools(vec![pool]);
    let mut lore = LoreStore::new();

    // Patch drops reputation below threshold
    let patch = ResourcePatch {
        resource_name: "reputation".to_string(),
        operation: ResourcePatchOp::Subtract,
        value: 15.0,
    };
    let result = snap.apply_resource_patch(&patch).unwrap();
    assert_eq!(result.crossed_thresholds.len(), 1);

    // Mint the lore
    mint_threshold_lore(&result.crossed_thresholds, &mut lore, 12);

    // Verify it's in the narrator context
    let selected = select_lore_for_prompt(&lore, 2000, None, None);
    assert!(selected.iter().any(|f| f.id() == "reputation_shaky"));
    assert!(selected.iter().any(|f| {
        f.content() == "Your reputation in this district is becoming unreliable."
    }));
}

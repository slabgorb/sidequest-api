//! Story 16-11: Resource threshold → KnownFact pipeline — permanent narrator memory
//!
//! RED phase tests. When a ResourcePool crosses a threshold, mint a LoreFragment
//! in LoreStore so the narrator remembers it forever. Tests cover:
//!   AC1: Threshold crossing creates a LoreFragment (KnownFact) in LoreStore
//!   AC2: Fact indexed with category "resource_event" and high relevance
//!   AC3: Fact appears in narrator prompt context on subsequent turns
//!   AC4: Idempotent — save/load doesn't duplicate facts
//!   AC5: Multiple resources can fire thresholds independently
//!   AC6: KnownFact content matches the narrator_hint from YAML

use sidequest_game::lore::{
    select_lore_for_prompt, LoreCategory, LoreFragment, LoreSource, LoreStore,
};
use sidequest_game::state::{
    mint_threshold_fact, GameSnapshot, ResourcePatchOp, ResourcePool, ResourceThreshold,
    ThresholdEvent,
};
use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════
// Test helpers
// ═══════════════════════════════════════════════════════════

fn make_pool_with_threshold(
    name: &str,
    current: f64,
    min: f64,
    max: f64,
    thresholds: Vec<ResourceThreshold>,
) -> ResourcePool {
    ResourcePool {
        name: name.to_string(),
        current,
        min,
        max,
        voluntary: true,
        decay_per_turn: 0.0,
        thresholds,
        fired_thresholds: Default::default(),
    }
}

fn luck_pool_with_thresholds() -> ResourcePool {
    make_pool_with_threshold(
        "luck",
        5.0,
        0.0,
        6.0,
        vec![
            ResourceThreshold {
                at: 3.0,
                event_id: "luck_low".to_string(),
                narrator_hint: "Luck is running thin. The odds are turning.".to_string(),
            },
            ResourceThreshold {
                at: 1.0,
                event_id: "luck_critical".to_string(),
                narrator_hint: "Nearly out of luck. Desperate times ahead.".to_string(),
            },
            ResourceThreshold {
                at: 0.0,
                event_id: "luck_depleted".to_string(),
                narrator_hint: "Out of luck entirely. Everything depends on skill.".to_string(),
            },
        ],
    )
}

fn humanity_pool_with_thresholds() -> ResourcePool {
    make_pool_with_threshold(
        "humanity",
        80.0,
        0.0,
        100.0,
        vec![
            ResourceThreshold {
                at: 50.0,
                event_id: "humanity_cold".to_string(),
                narrator_hint: "Your chrome is showing. NPCs notice the detachment.".to_string(),
            },
            ResourceThreshold {
                at: 25.0,
                event_id: "humanity_critical".to_string(),
                narrator_hint: "More machine than human. People recoil instinctively.".to_string(),
            },
            ResourceThreshold {
                at: 0.0,
                event_id: "humanity_lost".to_string(),
                narrator_hint: "No trace of humanity remains. You are a machine.".to_string(),
            },
        ],
    )
}

fn snapshot_with_pools(pools: Vec<ResourcePool>) -> GameSnapshot {
    let mut snap = GameSnapshot::default();
    for pool in pools {
        snap.resources.insert(pool.name.clone(), pool);
    }
    snap
}

// ═══════════════════════════════════════════════════════════
// AC1: Threshold crossing creates a LoreFragment in LoreStore
// ═══════════════════════════════════════════════════════════

#[test]
fn threshold_crossing_mints_lore_fragment() {
    let event = ThresholdEvent {
        resource_name: "luck".to_string(),
        event_id: "luck_critical".to_string(),
        narrator_hint: "Nearly out of luck. Desperate times ahead.".to_string(),
        value_at_crossing: 0.5,
    };

    let fragment = mint_threshold_fact(&event, 14);

    assert_eq!(fragment.content(), event.narrator_hint);
    assert!(
        fragment.id().contains("luck_critical"),
        "fragment id should contain event_id, got: {}",
        fragment.id()
    );
}

#[test]
fn threshold_crossing_adds_to_lore_store() {
    let event = ThresholdEvent {
        resource_name: "luck".to_string(),
        event_id: "luck_critical".to_string(),
        narrator_hint: "Nearly out of luck.".to_string(),
        value_at_crossing: 0.5,
    };

    let mut store = LoreStore::new();
    let fragment = mint_threshold_fact(&event, 14);
    store.add(fragment).expect("should add threshold fact");

    assert_eq!(store.len(), 1);
    let results = store.query_by_category(&LoreCategory::Custom("resource_event".to_string()));
    assert_eq!(results.len(), 1, "should find fact by resource_event category");
}

#[test]
fn process_threshold_crossings_integrates_with_snapshot() {
    let pool = luck_pool_with_thresholds();
    let mut snap = snapshot_with_pools(vec![pool]);
    let mut store = LoreStore::new();

    // Subtract enough to cross the luck_low threshold at 3.0
    // Current is 5.0, subtract 3.0 → lands at 2.0, crosses 3.0
    snap.process_resource_patch_with_lore(
        "luck",
        ResourcePatchOp::Subtract,
        3.0,
        &mut store,
        14,
    )
    .expect("valid patch");

    assert!(
        !store.is_empty(),
        "crossing a threshold should mint a fact in lore store"
    );
    let results = store.query_by_category(&LoreCategory::Custom("resource_event".to_string()));
    assert_eq!(results.len(), 1);
    assert!(
        results[0].content().contains("Luck is running thin"),
        "fact content should match narrator_hint"
    );
}

// ═══════════════════════════════════════════════════════════
// AC2: Fact indexed with category "resource_event" and high relevance
// ═══════════════════════════════════════════════════════════

#[test]
fn threshold_fact_has_resource_event_category() {
    let event = ThresholdEvent {
        resource_name: "humanity".to_string(),
        event_id: "humanity_cold".to_string(),
        narrator_hint: "Your chrome is showing.".to_string(),
        value_at_crossing: 48.0,
    };

    let fragment = mint_threshold_fact(&event, 10);

    assert_eq!(
        *fragment.category(),
        LoreCategory::Custom("resource_event".to_string()),
        "threshold facts should use resource_event category"
    );
}

#[test]
fn threshold_fact_has_game_event_source() {
    let event = ThresholdEvent {
        resource_name: "luck".to_string(),
        event_id: "luck_critical".to_string(),
        narrator_hint: "Nearly out of luck.".to_string(),
        value_at_crossing: 0.5,
    };

    let fragment = mint_threshold_fact(&event, 14);

    assert_eq!(
        *fragment.source(),
        LoreSource::GameEvent,
        "threshold facts should have GameEvent source"
    );
}

#[test]
fn threshold_fact_has_turn_created() {
    let event = ThresholdEvent {
        resource_name: "luck".to_string(),
        event_id: "luck_critical".to_string(),
        narrator_hint: "Nearly out of luck.".to_string(),
        value_at_crossing: 0.5,
    };

    let fragment = mint_threshold_fact(&event, 14);

    assert_eq!(
        fragment.turn_created(),
        Some(14),
        "threshold fact should record the turn it was created"
    );
}

#[test]
fn threshold_fact_metadata_contains_resource_name() {
    let event = ThresholdEvent {
        resource_name: "humanity".to_string(),
        event_id: "humanity_cold".to_string(),
        narrator_hint: "Your chrome is showing.".to_string(),
        value_at_crossing: 48.0,
    };

    let fragment = mint_threshold_fact(&event, 10);

    assert_eq!(
        fragment.metadata().get("resource_name").map(|s| s.as_str()),
        Some("humanity"),
        "metadata should include the resource name for debugging"
    );
}

// ═══════════════════════════════════════════════════════════
// AC3: Fact appears in narrator prompt context on subsequent turns
// ═══════════════════════════════════════════════════════════

#[test]
fn threshold_fact_appears_in_prompt_selection() {
    let event = ThresholdEvent {
        resource_name: "humanity".to_string(),
        event_id: "humanity_cold".to_string(),
        narrator_hint: "Your chrome is showing. NPCs notice the detachment.".to_string(),
        value_at_crossing: 48.0,
    };

    let mut store = LoreStore::new();
    let fragment = mint_threshold_fact(&event, 10);
    store.add(fragment).unwrap();

    // Select with a generous budget — the fact should appear
    let selected = select_lore_for_prompt(&store, 1000, None, None);

    assert!(
        !selected.is_empty(),
        "threshold fact should be selectable for prompt"
    );
    assert!(
        selected.iter().any(|f| f.content().contains("chrome is showing")),
        "the humanity_cold fact should appear in prompt selection"
    );
}

#[test]
fn threshold_fact_survives_among_other_lore() {
    let mut store = LoreStore::new();

    // Add some regular lore
    store
        .add(LoreFragment::new(
            "history_1".to_string(),
            LoreCategory::History,
            "The city was founded 200 years ago.".to_string(),
            LoreSource::GenrePack,
            None,
            HashMap::new(),
        ))
        .unwrap();

    store
        .add(LoreFragment::new(
            "geo_1".to_string(),
            LoreCategory::Geography,
            "The northern district is industrial.".to_string(),
            LoreSource::GenrePack,
            None,
            HashMap::new(),
        ))
        .unwrap();

    // Add the threshold fact
    let event = ThresholdEvent {
        resource_name: "humanity".to_string(),
        event_id: "humanity_cold".to_string(),
        narrator_hint: "Your chrome is showing.".to_string(),
        value_at_crossing: 48.0,
    };
    store.add(mint_threshold_fact(&event, 10)).unwrap();

    let selected = select_lore_for_prompt(&store, 1000, None, None);
    assert!(
        selected.iter().any(|f| f.content().contains("chrome is showing")),
        "threshold fact should appear alongside regular lore"
    );
}

#[test]
fn threshold_fact_appears_with_priority_boost() {
    let mut store = LoreStore::new();

    // Fill store with lots of geography lore
    for i in 0..20 {
        store
            .add(LoreFragment::new(
                format!("geo_{i}"),
                LoreCategory::Geography,
                format!("Geography fact number {i} with some filler text for token estimate."),
                LoreSource::GenrePack,
                None,
                HashMap::new(),
            ))
            .unwrap();
    }

    // Add a threshold fact
    let event = ThresholdEvent {
        resource_name: "humanity".to_string(),
        event_id: "humanity_cold".to_string(),
        narrator_hint: "Your chrome is showing.".to_string(),
        value_at_crossing: 48.0,
    };
    store.add(mint_threshold_fact(&event, 10)).unwrap();

    // With resource_event priority, threshold fact should be selected even with tight budget
    let priority = vec![LoreCategory::Custom("resource_event".to_string())];
    let selected = select_lore_for_prompt(&store, 50, Some(&priority), None);
    assert!(
        selected.iter().any(|f| f.content().contains("chrome is showing")),
        "threshold fact should be prioritized when resource_event is in priority categories"
    );
}

// ═══════════════════════════════════════════════════════════
// AC4: Idempotent — save/load doesn't duplicate facts
// ═══════════════════════════════════════════════════════════

#[test]
fn fired_thresholds_tracked_on_resource_pool() {
    let pool = luck_pool_with_thresholds();
    let mut snap = snapshot_with_pools(vec![pool]);
    let mut store = LoreStore::new();

    // Cross the luck_low threshold
    snap.process_resource_patch_with_lore(
        "luck",
        ResourcePatchOp::Subtract,
        3.0,
        &mut store,
        14,
    )
    .unwrap();

    // fired_thresholds should now contain "luck_low"
    let pool = &snap.resources["luck"];
    assert!(
        pool.fired_thresholds.contains("luck_low"),
        "fired_thresholds should track which thresholds have been crossed"
    );
}

#[test]
fn second_crossing_does_not_duplicate_fact() {
    let pool = luck_pool_with_thresholds();
    let mut snap = snapshot_with_pools(vec![pool]);
    let mut store = LoreStore::new();

    // Cross luck_low (5.0 → 2.0)
    snap.process_resource_patch_with_lore(
        "luck",
        ResourcePatchOp::Subtract,
        3.0,
        &mut store,
        14,
    )
    .unwrap();
    assert_eq!(store.len(), 1, "first crossing should mint one fact");

    // Add some luck back (2.0 → 4.0)
    snap.process_resource_patch_with_lore(
        "luck",
        ResourcePatchOp::Add,
        2.0,
        &mut store,
        15,
    )
    .unwrap();

    // Cross luck_low again (4.0 → 2.0)
    snap.process_resource_patch_with_lore(
        "luck",
        ResourcePatchOp::Subtract,
        2.0,
        &mut store,
        16,
    )
    .unwrap();

    assert_eq!(
        store.len(),
        1,
        "re-crossing the same threshold should NOT mint another fact"
    );
}

#[test]
fn fired_thresholds_survive_serde_roundtrip() {
    let mut pool = luck_pool_with_thresholds();
    pool.fired_thresholds.insert("luck_low".to_string());

    let json = serde_json::to_string(&pool).unwrap();
    let restored: ResourcePool = serde_json::from_str(&json).unwrap();

    assert!(
        restored.fired_thresholds.contains("luck_low"),
        "fired_thresholds should survive save/load"
    );
}

#[test]
fn snapshot_with_fired_thresholds_survives_roundtrip() {
    let mut pool = luck_pool_with_thresholds();
    pool.fired_thresholds.insert("luck_low".to_string());
    pool.fired_thresholds.insert("luck_critical".to_string());
    let snap = snapshot_with_pools(vec![pool]);

    let json = serde_json::to_string(&snap).unwrap();
    let restored: GameSnapshot = serde_json::from_str(&json).unwrap();

    let pool = &restored.resources["luck"];
    assert_eq!(pool.fired_thresholds.len(), 2);
    assert!(pool.fired_thresholds.contains("luck_low"));
    assert!(pool.fired_thresholds.contains("luck_critical"));
}

// ═══════════════════════════════════════════════════════════
// AC5: Multiple resources can fire thresholds independently
// ═══════════════════════════════════════════════════════════

#[test]
fn multiple_resources_fire_thresholds_independently() {
    let luck = luck_pool_with_thresholds();
    let humanity = humanity_pool_with_thresholds();
    let mut snap = snapshot_with_pools(vec![luck, humanity]);
    let mut store = LoreStore::new();

    // Cross luck_low (5.0 → 2.0)
    snap.process_resource_patch_with_lore(
        "luck",
        ResourcePatchOp::Subtract,
        3.0,
        &mut store,
        14,
    )
    .unwrap();

    // Cross humanity_cold (80.0 → 45.0)
    snap.process_resource_patch_with_lore(
        "humanity",
        ResourcePatchOp::Subtract,
        35.0,
        &mut store,
        15,
    )
    .unwrap();

    assert_eq!(
        store.len(),
        2,
        "each resource should mint its own threshold fact"
    );

    let results = store.query_by_category(&LoreCategory::Custom("resource_event".to_string()));
    assert_eq!(results.len(), 2);

    let contents: Vec<&str> = results.iter().map(|f| f.content()).collect();
    assert!(
        contents.iter().any(|c| c.contains("Luck is running thin")),
        "luck threshold fact should exist"
    );
    assert!(
        contents.iter().any(|c| c.contains("chrome is showing")),
        "humanity threshold fact should exist"
    );
}

#[test]
fn crossing_multiple_thresholds_on_same_resource_mints_multiple_facts() {
    let luck = luck_pool_with_thresholds();
    let mut snap = snapshot_with_pools(vec![luck]);
    let mut store = LoreStore::new();

    // Big subtract: 5.0 → 0.0 — crosses luck_low (3.0), luck_critical (1.0), luck_depleted (0.0)
    snap.process_resource_patch_with_lore(
        "luck",
        ResourcePatchOp::Subtract,
        5.0,
        &mut store,
        14,
    )
    .unwrap();

    assert_eq!(
        store.len(),
        3,
        "crossing 3 thresholds should mint 3 facts"
    );
}

// ═══════════════════════════════════════════════════════════
// AC6: KnownFact content matches the narrator_hint from YAML
// ═══════════════════════════════════════════════════════════

#[test]
fn fact_content_matches_narrator_hint_exactly() {
    let event = ThresholdEvent {
        resource_name: "humanity".to_string(),
        event_id: "humanity_cold".to_string(),
        narrator_hint: "Your chrome is showing. NPCs notice the detachment.".to_string(),
        value_at_crossing: 48.0,
    };

    let fragment = mint_threshold_fact(&event, 10);
    assert_eq!(
        fragment.content(),
        "Your chrome is showing. NPCs notice the detachment.",
        "fact content must match narrator_hint verbatim"
    );
}

#[test]
fn fact_content_from_integration_matches_yaml_hint() {
    let pool = luck_pool_with_thresholds();
    let mut snap = snapshot_with_pools(vec![pool]);
    let mut store = LoreStore::new();

    // Cross luck_low at 3.0
    snap.process_resource_patch_with_lore(
        "luck",
        ResourcePatchOp::Subtract,
        3.0,
        &mut store,
        14,
    )
    .unwrap();

    let results = store.query_by_category(&LoreCategory::Custom("resource_event".to_string()));
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].content(),
        "Luck is running thin. The odds are turning.",
        "minted fact should use the exact narrator_hint from the threshold definition"
    );
}

// ═══════════════════════════════════════════════════════════
// Edge cases
// ═══════════════════════════════════════════════════════════

#[test]
fn no_threshold_crossing_no_fact_minted() {
    let pool = luck_pool_with_thresholds();
    let mut snap = snapshot_with_pools(vec![pool]);
    let mut store = LoreStore::new();

    // Small subtract: 5.0 → 4.0 — no threshold crossed
    snap.process_resource_patch_with_lore(
        "luck",
        ResourcePatchOp::Subtract,
        1.0,
        &mut store,
        14,
    )
    .unwrap();

    assert!(
        store.is_empty(),
        "no threshold crossing should mean no fact minted"
    );
}

#[test]
fn threshold_event_struct_has_all_fields() {
    let event = ThresholdEvent {
        resource_name: "luck".to_string(),
        event_id: "luck_critical".to_string(),
        narrator_hint: "Nearly out of luck.".to_string(),
        value_at_crossing: 0.5,
    };

    assert_eq!(event.resource_name, "luck");
    assert_eq!(event.event_id, "luck_critical");
    assert_eq!(event.narrator_hint, "Nearly out of luck.");
    assert!((event.value_at_crossing - 0.5).abs() < f64::EPSILON);
}

#[test]
fn threshold_event_serde_roundtrip() {
    let event = ThresholdEvent {
        resource_name: "luck".to_string(),
        event_id: "luck_critical".to_string(),
        narrator_hint: "Nearly out of luck.".to_string(),
        value_at_crossing: 0.5,
    };

    let json = serde_json::to_string(&event).unwrap();
    let restored: ThresholdEvent = serde_json::from_str(&json).unwrap();

    assert_eq!(restored.event_id, "luck_critical");
    assert_eq!(restored.narrator_hint, "Nearly out of luck.");
}

#[test]
fn resource_pool_default_fired_thresholds_empty() {
    let pool = make_pool_with_threshold("luck", 3.0, 0.0, 6.0, vec![]);
    assert!(
        pool.fired_thresholds.is_empty(),
        "new pool should have no fired thresholds"
    );
}

#[test]
fn add_operation_does_not_trigger_downward_threshold() {
    let pool = luck_pool_with_thresholds();
    let mut snap = snapshot_with_pools(vec![pool]);
    let mut store = LoreStore::new();

    // First cross luck_low going down (5.0 → 2.0)
    snap.process_resource_patch_with_lore(
        "luck",
        ResourcePatchOp::Subtract,
        3.0,
        &mut store,
        14,
    )
    .unwrap();
    assert_eq!(store.len(), 1, "should mint luck_low fact");

    // Now add luck back past 3.0 (2.0 → 5.0) — upward crossing should NOT re-trigger
    snap.process_resource_patch_with_lore(
        "luck",
        ResourcePatchOp::Add,
        3.0,
        &mut store,
        15,
    )
    .unwrap();

    assert_eq!(
        store.len(),
        1,
        "adding resources back past threshold should not mint new facts"
    );
}

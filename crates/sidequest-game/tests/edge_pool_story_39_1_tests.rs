//! Story 39-1: Extract Threshold Helper + EdgePool Type (RED phase).
//!
//! ACs (paraphrased):
//!   AC1: `EdgePool` compiles with required fields + serde round-trip
//!   AC2: `apply_delta` debits correctly (cap at max, floor at 0)
//!   AC3: Threshold crossings fire on direction-correct transitions
//!   AC4: `thresholds.rs` helpers are the single source of truth —
//!        ResourcePool keeps minting lore identically (wiring regression)
//!   AC5: No wiring leakage (verified by grep during review, plus:
//!        this test file does NOT import EdgePool from dispatch/server)

use sidequest_game::creature_core::{DeltaResult, EdgePool, EdgeThreshold, RecoveryTrigger};
use sidequest_game::lore::{LoreCategory, LoreStore};
use sidequest_game::resource_pool::{
    mint_threshold_lore, ResourcePatch, ResourcePatchOp, ResourcePool, ResourceThreshold,
};
use sidequest_game::state::GameSnapshot;

// ═══════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════

fn strained_at(n: i32) -> EdgeThreshold {
    EdgeThreshold {
        at: n,
        event_id: "edge_strained".into(),
        narrator_hint: "Their composure is fraying.".into(),
    }
}

fn break_at(n: i32) -> EdgeThreshold {
    EdgeThreshold {
        at: n,
        event_id: "composure_break".into(),
        narrator_hint: "They break.".into(),
    }
}

fn edge_pool(current: i32, max: i32, thresholds: Vec<EdgeThreshold>) -> EdgePool {
    EdgePool {
        current,
        max,
        base_max: max,
        recovery_triggers: Vec::new(),
        thresholds,
    }
}

// ═══════════════════════════════════════════════════════════
// AC1: EdgePool compiles + serde round-trip
// ═══════════════════════════════════════════════════════════

#[test]
fn edge_pool_has_required_fields() {
    // Field presence is enforced by the constructor — this proves the struct
    // exposes `current`, `max`, `base_max`, `recovery_triggers`, `thresholds`.
    let pool = EdgePool {
        current: 3,
        max: 5,
        base_max: 5,
        recovery_triggers: vec![RecoveryTrigger::OnResolution],
        thresholds: vec![strained_at(1), break_at(0)],
    };
    assert_eq!(pool.current, 3);
    assert_eq!(pool.max, 5);
    assert_eq!(pool.base_max, 5);
    assert_eq!(pool.recovery_triggers.len(), 1);
    assert_eq!(pool.thresholds.len(), 2);
}

#[test]
fn edge_pool_serde_round_trip() {
    let pool = EdgePool {
        current: 4,
        max: 6,
        base_max: 6,
        recovery_triggers: vec![
            RecoveryTrigger::OnResolution,
            RecoveryTrigger::OnAllyRescue,
            RecoveryTrigger::OnBeatSuccess {
                beat_id: "rally".into(),
                amount: 2,
                while_strained: true,
            },
        ],
        thresholds: vec![strained_at(2), break_at(0)],
    };
    let json = serde_json::to_string(&pool).expect("serialize");
    let back: EdgePool = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, pool, "EdgePool must round-trip through JSON");
}

#[test]
fn recovery_trigger_on_beat_success_carries_all_fields() {
    let trig = RecoveryTrigger::OnBeatSuccess {
        beat_id: "finisher".into(),
        amount: 3,
        while_strained: false,
    };
    let json = serde_json::to_string(&trig).expect("serialize");
    let back: RecoveryTrigger = serde_json::from_str(&json).expect("deserialize");
    match back {
        RecoveryTrigger::OnBeatSuccess {
            beat_id,
            amount,
            while_strained,
        } => {
            assert_eq!(beat_id, "finisher");
            assert_eq!(amount, 3);
            assert!(!while_strained);
        }
        other => panic!("expected OnBeatSuccess, got {:?}", other),
    }
}

// ═══════════════════════════════════════════════════════════
// AC2: apply_delta debits correctly
// ═══════════════════════════════════════════════════════════

#[test]
fn apply_delta_positive_caps_at_max() {
    let mut pool = edge_pool(3, 5, vec![]);
    let res = pool.apply_delta(10);
    assert_eq!(pool.current, 5, "current must cap at max");
    assert_eq!(res.new_current, 5);
    assert!(res.crossed.is_empty());
}

#[test]
fn apply_delta_negative_floors_at_zero() {
    let mut pool = edge_pool(2, 5, vec![]);
    let res = pool.apply_delta(-10);
    assert_eq!(pool.current, 0, "current must floor at 0 (never negative)");
    assert_eq!(res.new_current, 0);
}

#[test]
fn apply_delta_zero_is_noop() {
    let mut pool = edge_pool(3, 5, vec![strained_at(1)]);
    let res = pool.apply_delta(0);
    assert_eq!(pool.current, 3);
    assert_eq!(res.new_current, 3);
    assert!(res.crossed.is_empty());
}

#[test]
fn apply_delta_returns_delta_result_shape() {
    // AC2: the returned type is `DeltaResult { new_current, crossed }`.
    // This test pins the field names at compile time.
    let mut pool = edge_pool(3, 5, vec![]);
    let res: DeltaResult = pool.apply_delta(-1);
    let _: i32 = res.new_current;
    let _: Vec<EdgeThreshold> = res.crossed;
}

// ═══════════════════════════════════════════════════════════
// AC3: Threshold crossings fire on direction-correct transitions
// ═══════════════════════════════════════════════════════════

#[test]
fn crossing_at_one_while_decreasing_fires_edge_strained() {
    let mut pool = edge_pool(2, 5, vec![strained_at(1)]);
    let res = pool.apply_delta(-1);
    assert_eq!(pool.current, 1);
    assert_eq!(
        res.crossed.len(),
        1,
        "crossing at=1 downward must fire exactly one threshold"
    );
    assert_eq!(res.crossed[0].event_id, "edge_strained");
}

#[test]
fn crossing_at_zero_while_decreasing_fires_composure_break() {
    let mut pool = edge_pool(1, 5, vec![break_at(0)]);
    let res = pool.apply_delta(-1);
    assert_eq!(pool.current, 0);
    assert_eq!(res.crossed.len(), 1);
    assert_eq!(res.crossed[0].event_id, "composure_break");
}

#[test]
fn non_crossing_delta_fires_nothing() {
    // current=5 → 4, thresholds at 1 and 0 — neither is crossed.
    let mut pool = edge_pool(5, 10, vec![strained_at(1), break_at(0)]);
    let res = pool.apply_delta(-1);
    assert_eq!(pool.current, 4);
    assert!(
        res.crossed.is_empty(),
        "deltas that do not span a threshold must not fire"
    );
}

#[test]
fn ascending_delta_does_not_fire_downward_threshold() {
    // current=0 → 5, threshold at 1 — crossing is downward-only.
    let mut pool = edge_pool(0, 5, vec![strained_at(1), break_at(0)]);
    let res = pool.apply_delta(5);
    assert_eq!(pool.current, 5);
    assert!(
        res.crossed.is_empty(),
        "thresholds must only fire on downward crossings"
    );
}

#[test]
fn single_delta_crossing_multiple_thresholds_fires_all() {
    // current=2 → 0, thresholds at 1 and 0 — both crossed in one delta.
    let mut pool = edge_pool(2, 5, vec![strained_at(1), break_at(0)]);
    let res = pool.apply_delta(-2);
    assert_eq!(pool.current, 0);
    let ids: Vec<&str> = res.crossed.iter().map(|t| t.event_id.as_str()).collect();
    assert!(ids.contains(&"edge_strained"));
    assert!(ids.contains(&"composure_break"));
    assert_eq!(res.crossed.len(), 2);
}

#[test]
fn threshold_already_below_does_not_re_fire() {
    // current=0 with threshold at=0 — already at the threshold, delta 0 must not fire.
    let mut pool = edge_pool(0, 5, vec![break_at(0)]);
    let res = pool.apply_delta(0);
    assert!(res.crossed.is_empty());
}

// ═══════════════════════════════════════════════════════════
// AC4: Shared helper is the single source of truth.
// Wiring/regression test: ResourcePool still mints lore identically.
// ═══════════════════════════════════════════════════════════

#[test]
fn resource_pool_still_mints_lore_after_extraction() {
    // This test guards the refactor: before 39-1, ResourcePool had private
    // `detect_crossings`/`mint_threshold_lore`. After 39-1 they live in
    // `thresholds.rs` and ResourcePool calls the shared helpers. Behavior
    // must be bit-for-bit identical — a fragment is minted with the
    // threshold's event_id and narrator_hint.
    let mut snap = GameSnapshot::default();
    snap.resources.insert(
        "luck".into(),
        ResourcePool {
            name: "luck".into(),
            label: "Luck".into(),
            current: 2.0,
            min: 0.0,
            max: 10.0,
            voluntary: true,
            decay_per_turn: 0.0,
            thresholds: vec![ResourceThreshold {
                at: 1.0,
                event_id: "luck_waning".into(),
                narrator_hint: "Luck thins.".into(),
            }],
        },
    );

    let patch = ResourcePatch {
        resource_name: "luck".into(),
        operation: ResourcePatchOp::Subtract,
        value: 2.0,
    };
    let result = snap
        .apply_resource_patch(&patch)
        .expect("patch must apply cleanly");
    assert_eq!(result.crossed_thresholds.len(), 1);
    assert_eq!(result.crossed_thresholds[0].event_id, "luck_waning");

    // And the minting helper must still produce a LoreFragment.
    let mut store = LoreStore::new();
    mint_threshold_lore(&result.crossed_thresholds, &mut store, 7);
    let events = store.query_by_category(&LoreCategory::Event);
    let fragment = events
        .iter()
        .find(|f| f.id() == "luck_waning")
        .expect("threshold crossing must mint a LoreFragment with id=event_id");
    assert_eq!(fragment.content(), "Luck thins.");
    assert!(matches!(fragment.category(), LoreCategory::Event));
}

#[test]
fn resource_pool_decay_path_still_works_after_extraction() {
    // The apply_pool_decay path also uses the (now-shared) detect_crossings
    // helper. Regression-proofs the second call site.
    let mut snap = GameSnapshot::default();
    snap.resources.insert(
        "heat".into(),
        ResourcePool {
            name: "heat".into(),
            label: "Heat".into(),
            current: 2.0,
            min: 0.0,
            max: 10.0,
            voluntary: false,
            decay_per_turn: -2.0,
            thresholds: vec![ResourceThreshold {
                at: 1.0,
                event_id: "heat_cooled".into(),
                narrator_hint: "The heat dies down.".into(),
            }],
        },
    );
    let crossings = snap.apply_pool_decay();
    assert_eq!(crossings.len(), 1);
    assert_eq!(crossings[0].event_id, "heat_cooled");
}

// ═══════════════════════════════════════════════════════════
// AC4 (continued): thresholds module is reachable as a public module.
// Compile-level proof that `pub mod thresholds;` was added to lib.rs.
// ═══════════════════════════════════════════════════════════

#[test]
fn thresholds_module_is_publicly_accessible() {
    // Forces `sidequest_game::thresholds` to resolve. If the module is
    // missing, the build fails — this guards `pub mod thresholds;` in lib.rs.
    // The reference is a no-op but must compile.
    #[allow(unused_imports)]
    use sidequest_game::thresholds as _th;
}

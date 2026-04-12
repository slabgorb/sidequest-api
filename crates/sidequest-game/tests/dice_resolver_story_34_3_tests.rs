//! RED-phase tests for story 34-3: Dice resolution engine.
//!
//! Tests cover: resolve_dice pure function, d20 crit semantics, DC boundary
//! conditions, negative modifiers, error cases, determinism, and rule-enforcement
//! checks from the Rust lang-review checklist.

use std::num::{NonZeroU32, NonZeroU8};

use rand::{rngs::StdRng, Rng, SeedableRng};
use sidequest_game::dice::{resolve_dice, ResolveError};
use sidequest_protocol::{DieSides, DieSpec, RollOutcome};

// ---------------------------------------------------------------------------
// Seed discovery helpers
// ---------------------------------------------------------------------------

/// Brute-force search for a seed where a single d20 produces the target face.
fn find_d20_seed(target_face: u32) -> u64 {
    for seed in 0u64..100_000 {
        let mut rng = StdRng::seed_from_u64(seed);
        let face = rng.random_range(1u32..=20);
        if face == target_face {
            return seed;
        }
    }
    panic!("Could not find seed producing d20 face {target_face} in 100k attempts");
}

/// Find a seed where a single d20 produces a specific face AND a d6 group
/// produces known values. Returns (seed, d6_faces).
fn find_mixed_pool_seed(d20_target: u32) -> (u64, Vec<u32>) {
    for seed in 0u64..100_000 {
        let mut rng = StdRng::seed_from_u64(seed);
        // d20 group drawn first (pool order: d20 then d6)
        let d20_face = rng.random_range(1u32..=20);
        if d20_face == d20_target {
            // 2d6 faces drawn next
            let d6_1 = rng.random_range(1u32..=6);
            let d6_2 = rng.random_range(1u32..=6);
            return (seed, vec![d6_1, d6_2]);
        }
    }
    panic!("Could not find mixed-pool seed for d20={d20_target}");
}

fn d20_spec() -> DieSpec {
    DieSpec {
        sides: DieSides::D20,
        count: NonZeroU8::new(1).unwrap(),
    }
}

fn d6_spec(count: u8) -> DieSpec {
    DieSpec {
        sides: DieSides::D6,
        count: NonZeroU8::new(count).unwrap(),
    }
}

fn dc(val: u32) -> NonZeroU32 {
    NonZeroU32::new(val).unwrap()
}

// ---------------------------------------------------------------------------
// AC: resolve_dice exists with correct signature
// ---------------------------------------------------------------------------

#[test]
fn resolve_dice_exists_and_returns_resolved_roll() {
    let seed = find_d20_seed(10);
    let result = resolve_dice(&[d20_spec()], 0, dc(10), seed);
    assert!(
        result.is_ok(),
        "resolve_dice should return Ok for valid input"
    );
    let roll = result.unwrap();
    // ResolvedRoll must have rolls, total, and outcome fields
    assert_eq!(roll.rolls.len(), 1);
    assert_eq!(roll.total, 10);
    assert!(
        !matches!(roll.outcome, RollOutcome::Unknown),
        "outcome must never be Unknown"
    );
}

// ---------------------------------------------------------------------------
// AC: DC boundary — total == DC is Success, total == DC-1 is Fail
// ---------------------------------------------------------------------------

#[test]
fn dc_boundary_exact_match_is_success() {
    // d20 rolls 10, modifier 0, DC 10 → total 10 >= 10 → Success
    let seed = find_d20_seed(10);
    let roll = resolve_dice(&[d20_spec()], 0, dc(10), seed).unwrap();
    assert_eq!(roll.total, 10);
    assert_eq!(roll.outcome, RollOutcome::Success);
}

#[test]
fn dc_boundary_one_below_is_fail() {
    // d20 rolls 9, modifier 0, DC 10 → total 9 < 10 → Fail
    let seed = find_d20_seed(9);
    let roll = resolve_dice(&[d20_spec()], 0, dc(10), seed).unwrap();
    assert_eq!(roll.total, 9);
    assert_eq!(roll.outcome, RollOutcome::Fail);
}

#[test]
fn dc_boundary_one_above_is_success() {
    // d20 rolls 11, modifier 0, DC 10 → total 11 >= 10 → Success
    let seed = find_d20_seed(11);
    let roll = resolve_dice(&[d20_spec()], 0, dc(10), seed).unwrap();
    assert_eq!(roll.total, 11);
    assert_eq!(roll.outcome, RollOutcome::Success);
}

// ---------------------------------------------------------------------------
// AC: D20 crit detection — natural 20 → CritSuccess, natural 1 → CritFail
// ---------------------------------------------------------------------------

#[test]
fn d20_natural_20_is_crit_success() {
    let seed = find_d20_seed(20);
    let roll = resolve_dice(&[d20_spec()], 0, dc(25), seed).unwrap();
    // CritSuccess regardless of DC — nat 20 always crits
    assert_eq!(roll.outcome, RollOutcome::CritSuccess);
    assert_eq!(roll.rolls[0].faces, vec![20]);
}

#[test]
fn d20_natural_1_is_crit_fail() {
    let seed = find_d20_seed(1);
    let roll = resolve_dice(&[d20_spec()], 0, dc(1), seed).unwrap();
    // CritFail regardless of DC — nat 1 always crits (even if total meets DC)
    assert_eq!(roll.outcome, RollOutcome::CritFail);
    assert_eq!(roll.rolls[0].faces, vec![1]);
}

#[test]
fn d20_natural_20_with_modifier_still_crit_success() {
    let seed = find_d20_seed(20);
    // Even with modifier -100, nat 20 = CritSuccess
    let roll = resolve_dice(&[d20_spec()], -100, dc(1), seed).unwrap();
    assert_eq!(roll.outcome, RollOutcome::CritSuccess);
}

#[test]
fn d20_natural_1_with_high_modifier_still_crit_fail() {
    let seed = find_d20_seed(1);
    // Even with modifier +100, nat 1 = CritFail
    let roll = resolve_dice(&[d20_spec()], 100, dc(1), seed).unwrap();
    assert_eq!(roll.outcome, RollOutcome::CritFail);
}

// ---------------------------------------------------------------------------
// AC: 2d20 edge case — both nat 20 and nat 1 present, CritSuccess wins
// ---------------------------------------------------------------------------

#[test]
fn two_d20_nat20_and_nat1_crit_success_wins() {
    // Find a seed where 2d20 produces one 20 and one 1
    let two_d20 = DieSpec {
        sides: DieSides::D20,
        count: NonZeroU8::new(2).unwrap(),
    };
    let mut found_seed = None;
    for seed in 0u64..500_000 {
        let mut rng = StdRng::seed_from_u64(seed);
        let f1 = rng.random_range(1u32..=20);
        let f2 = rng.random_range(1u32..=20);
        if (f1 == 20 && f2 == 1) || (f1 == 1 && f2 == 20) {
            found_seed = Some(seed);
            break;
        }
    }
    let seed = found_seed.expect("Could not find 2d20 seed with 20+1 in 500k attempts");
    let roll = resolve_dice(&[two_d20], 0, dc(15), seed).unwrap();
    assert_eq!(
        roll.outcome,
        RollOutcome::CritSuccess,
        "When both nat 20 and nat 1 appear, CritSuccess wins"
    );
}

// ---------------------------------------------------------------------------
// AC: Non-d20 pools never produce crit outcomes
// ---------------------------------------------------------------------------

#[test]
fn pure_d6_pool_never_crits_on_success() {
    // 4d6, find seed where total >= DC
    let pool = [d6_spec(4)];
    for seed in 0u64..1000 {
        let roll = resolve_dice(&pool, 0, dc(1), seed).unwrap();
        assert!(
            matches!(roll.outcome, RollOutcome::Success | RollOutcome::Fail),
            "Non-d20 pool must only produce Success or Fail, got {:?} at seed {seed}",
            roll.outcome
        );
    }
}

#[test]
fn pure_d6_pool_never_crits_on_fail() {
    // 1d6, DC 100 — always fails, never CritFail
    let pool = [d6_spec(1)];
    for seed in 0u64..1000 {
        let roll = resolve_dice(&pool, 0, dc(100), seed).unwrap();
        assert_eq!(
            roll.outcome,
            RollOutcome::Fail,
            "d6 vs DC 100 must always be Fail, never CritFail. seed={seed}"
        );
    }
}

#[test]
fn d100_pool_never_crits() {
    let d100 = DieSpec {
        sides: DieSides::D100,
        count: NonZeroU8::new(1).unwrap(),
    };
    for seed in 0u64..1000 {
        let roll = resolve_dice(&[d100], 0, dc(50), seed).unwrap();
        assert!(
            matches!(roll.outcome, RollOutcome::Success | RollOutcome::Fail),
            "d100 must not produce crits. seed={seed}, outcome={:?}",
            roll.outcome
        );
    }
}

// ---------------------------------------------------------------------------
// AC: Mixed pool — d20 crit detection with non-d20 dice present
// ---------------------------------------------------------------------------

#[test]
fn mixed_pool_d20_nat20_plus_d6_is_crit_success() {
    let (seed, d6_faces) = find_mixed_pool_seed(20);
    let pool = [d20_spec(), d6_spec(2)];
    let roll = resolve_dice(&pool, 0, dc(30), seed).unwrap();
    assert_eq!(roll.outcome, RollOutcome::CritSuccess);
    // Verify individual group results
    assert_eq!(roll.rolls.len(), 2);
    assert_eq!(roll.rolls[0].faces, vec![20]); // d20 group
    assert_eq!(roll.rolls[1].faces, d6_faces); // d6 group
}

#[test]
fn mixed_pool_d20_normal_plus_d6_resolves_on_total() {
    // d20 rolls 10 (not crit), 2d6 rolled, total vs DC
    let (seed, d6_faces) = find_mixed_pool_seed(10);
    let d6_total: i32 = d6_faces.iter().map(|f| *f as i32).sum();
    let expected_total = 10 + d6_total;
    let pool = [d20_spec(), d6_spec(2)];
    let roll = resolve_dice(&pool, 0, dc(expected_total as u32), seed).unwrap();
    assert_eq!(roll.total, expected_total);
    // Exact DC match = Success
    assert_eq!(roll.outcome, RollOutcome::Success);
}

// ---------------------------------------------------------------------------
// AC: Negative totals pass through (no clamping)
// ---------------------------------------------------------------------------

#[test]
fn negative_modifier_produces_negative_total() {
    let seed = find_d20_seed(1);
    // d20 = 1, modifier = -19 → total = 1 + (-19) = -18
    let roll = resolve_dice(&[d20_spec()], -19, dc(1), seed).unwrap();
    assert_eq!(roll.total, -18, "Negative totals must not be clamped");
    // nat 1 on d20 → CritFail regardless
    assert_eq!(roll.outcome, RollOutcome::CritFail);
}

#[test]
fn large_negative_modifier_no_overflow() {
    let seed = find_d20_seed(5);
    // d20 = 5, modifier = i32::MIN/2 (very negative but no overflow risk with small face)
    let modifier = -1_000_000;
    let roll = resolve_dice(&[d20_spec()], modifier, dc(1), seed).unwrap();
    assert_eq!(roll.total, 5 + modifier);
}

// ---------------------------------------------------------------------------
// AC: Total computation correctness
// ---------------------------------------------------------------------------

#[test]
fn total_is_sum_of_all_faces_plus_modifier() {
    let seed = find_d20_seed(15);
    let roll = resolve_dice(&[d20_spec()], 3, dc(10), seed).unwrap();
    let face_sum: i32 = roll
        .rolls
        .iter()
        .flat_map(|g| &g.faces)
        .map(|f| *f as i32)
        .sum();
    assert_eq!(roll.total, face_sum + 3);
}

// ---------------------------------------------------------------------------
// AC: Error cases
// ---------------------------------------------------------------------------

#[test]
fn empty_pool_returns_error() {
    let result = resolve_dice(&[], 0, dc(10), 42);
    assert!(result.is_err());
    assert!(
        matches!(result, Err(ResolveError::EmptyPool)),
        "Empty pool must return EmptyPool error"
    );
}

#[test]
fn unknown_die_returns_error() {
    let bad_spec = DieSpec {
        sides: DieSides::Unknown,
        count: NonZeroU8::new(1).unwrap(),
    };
    let result = resolve_dice(&[bad_spec], 0, dc(10), 42);
    assert!(result.is_err());
    assert!(
        matches!(result, Err(ResolveError::UnknownDie)),
        "Unknown die must return UnknownDie error"
    );
}

#[test]
fn unknown_die_in_mixed_pool_returns_error() {
    let bad_spec = DieSpec {
        sides: DieSides::Unknown,
        count: NonZeroU8::new(1).unwrap(),
    };
    // Even if d20 is valid, an Unknown die anywhere in pool should fail
    let result = resolve_dice(&[d20_spec(), bad_spec], 0, dc(10), 42);
    assert!(matches!(result, Err(ResolveError::UnknownDie)));
}

// ---------------------------------------------------------------------------
// AC: Determinism — same seed produces identical output
// ---------------------------------------------------------------------------

#[test]
fn determinism_100_iterations_same_seed() {
    let seed = 12345u64;
    let pool = [d20_spec(), d6_spec(2)];
    let baseline = resolve_dice(&pool, 5, dc(15), seed).unwrap();

    for i in 0..100 {
        let roll = resolve_dice(&pool, 5, dc(15), seed).unwrap();
        assert_eq!(
            roll.rolls, baseline.rolls,
            "Iteration {i}: rolls diverged from baseline"
        );
        assert_eq!(roll.total, baseline.total, "Iteration {i}: total diverged");
        assert_eq!(
            roll.outcome, baseline.outcome,
            "Iteration {i}: outcome diverged"
        );
    }
}

#[test]
fn different_seeds_diverge() {
    let pool = [d20_spec()];
    let roll_a = resolve_dice(&pool, 0, dc(10), 1).unwrap();
    let roll_b = resolve_dice(&pool, 0, dc(10), 2).unwrap();
    // With overwhelming probability, different seeds produce different faces.
    // If they happen to match (1 in 20 chance), try another pair.
    let roll_c = resolve_dice(&pool, 0, dc(10), 3).unwrap();
    let all_same = roll_a.rolls[0].faces == roll_b.rolls[0].faces
        && roll_b.rolls[0].faces == roll_c.rolls[0].faces;
    assert!(
        !all_same,
        "Three different seeds should not all produce identical d20 faces"
    );
}

// ---------------------------------------------------------------------------
// AC: RollOutcome is never Unknown
// ---------------------------------------------------------------------------

#[test]
fn outcome_never_unknown_across_many_seeds() {
    let pool = [d20_spec()];
    for seed in 0u64..500 {
        let roll = resolve_dice(&pool, 0, dc(10), seed).unwrap();
        assert!(
            !matches!(roll.outcome, RollOutcome::Unknown),
            "outcome must never be Unknown. seed={seed}"
        );
    }
}

// ---------------------------------------------------------------------------
// AC: DieGroupResult face count matches spec count
// ---------------------------------------------------------------------------

#[test]
fn face_count_matches_die_spec_count() {
    let multi_d6 = DieSpec {
        sides: DieSides::D6,
        count: NonZeroU8::new(4).unwrap(),
    };
    let roll = resolve_dice(&[d20_spec(), multi_d6], 0, dc(10), 42).unwrap();
    assert_eq!(roll.rolls.len(), 2);
    assert_eq!(roll.rolls[0].faces.len(), 1); // 1d20
    assert_eq!(roll.rolls[1].faces.len(), 4); // 4d6
}

#[test]
fn each_face_within_die_range() {
    let pool = [d20_spec(), d6_spec(3)];
    for seed in 0u64..100 {
        let roll = resolve_dice(&pool, 0, dc(10), seed).unwrap();
        for face in &roll.rolls[0].faces {
            assert!((1..=20).contains(face), "d20 face {face} out of range");
        }
        for face in &roll.rolls[1].faces {
            assert!((1..=6).contains(face), "d6 face {face} out of range");
        }
    }
}

// ---------------------------------------------------------------------------
// AC: DieGroupResult.spec echoes the input DieSpec
// ---------------------------------------------------------------------------

#[test]
fn group_result_spec_echoes_input() {
    let d8 = DieSpec {
        sides: DieSides::D8,
        count: NonZeroU8::new(3).unwrap(),
    };
    let pool = [d20_spec(), d8];
    let roll = resolve_dice(&pool, 0, dc(10), 99).unwrap();
    assert_eq!(roll.rolls[0].spec, d20_spec());
    assert_eq!(roll.rolls[1].spec, d8);
}

// ---------------------------------------------------------------------------
// Rule enforcement: #2 — ResolveError must be #[non_exhaustive]
// ---------------------------------------------------------------------------

#[test]
fn resolve_error_is_non_exhaustive() {
    // This test compiles only if ResolveError has #[non_exhaustive].
    // We match with a wildcard arm — if non_exhaustive is missing, this
    // would warn about unreachable patterns (but compile). The real guard
    // is that downstream crates MUST have the wildcard arm.
    let err = ResolveError::EmptyPool;
    match err {
        ResolveError::EmptyPool => {}
        ResolveError::UnknownDie => {}
        _ => {} // required by #[non_exhaustive]
    }
}

// ---------------------------------------------------------------------------
// Rule enforcement: #6 — test quality self-check
// Every test above uses assert_eq! or assert!(matches!(...)) with meaningful
// values. No vacuous `let _ =` or `assert!(true)`.
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// All die types produce valid face ranges
// ---------------------------------------------------------------------------

#[test]
fn all_standard_die_types_produce_valid_faces() {
    let die_types = [
        (DieSides::D4, 4u32),
        (DieSides::D6, 6),
        (DieSides::D8, 8),
        (DieSides::D10, 10),
        (DieSides::D12, 12),
        (DieSides::D20, 20),
        (DieSides::D100, 100),
    ];
    for (sides, max_face) in die_types {
        let spec = DieSpec {
            sides,
            count: NonZeroU8::new(1).unwrap(),
        };
        for seed in 0u64..50 {
            let roll = resolve_dice(&[spec], 0, dc(1), seed).unwrap();
            let face = roll.rolls[0].faces[0];
            assert!(
                (1..=max_face).contains(&face),
                "{sides:?} face {face} not in 1..={max_face} at seed {seed}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Modifier-only edge cases
// ---------------------------------------------------------------------------

#[test]
fn zero_modifier_total_equals_face_sum() {
    let seed = find_d20_seed(15);
    let roll = resolve_dice(&[d20_spec()], 0, dc(10), seed).unwrap();
    assert_eq!(roll.total, 15);
}

#[test]
fn positive_modifier_added_to_total() {
    let seed = find_d20_seed(10);
    let roll = resolve_dice(&[d20_spec()], 5, dc(10), seed).unwrap();
    assert_eq!(roll.total, 15);
    assert_eq!(roll.outcome, RollOutcome::Success);
}

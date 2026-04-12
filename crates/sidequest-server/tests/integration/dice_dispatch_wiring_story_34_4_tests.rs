//! RED-phase tests for story 34-4: Dispatch integration for dice rolling.
//!
//! Tests cover: dispatch boundary validation, seed generation, DiceResult
//! composition, DiceThrow handling wiring, and the full beat→request→throw→result
//! integration path.

use std::num::{NonZeroU32, NonZeroU8};

use sidequest_game::dice::resolve_dice;
use sidequest_protocol::{
    DiceRequestPayload, DiceResultPayload, DieSides, DieSpec, RollOutcome, ThrowParams,
};

// ---- Imports for dispatch-layer functions (will fail until implemented) ----
use sidequest_server::dice_dispatch::{
    compose_dice_result, generate_dice_seed, validate_dice_inputs, DiceInputError,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

fn sample_throw_params() -> ThrowParams {
    ThrowParams {
        velocity: [1.0, 2.0, 0.5],
        angular: [0.1, 0.3, 0.2],
        position: [0.5, 0.5],
    }
}

// ===========================================================================
// AC: Dispatch boundary validates DC in range (1..=100)
// ===========================================================================

#[test]
fn validate_dc_in_range_accepts_1() {
    let result = validate_dice_inputs(&[d20_spec()], 0, dc(1));
    assert!(result.is_ok(), "DC 1 should be valid");
}

#[test]
fn validate_dc_in_range_accepts_100() {
    let result = validate_dice_inputs(&[d20_spec()], 0, dc(100));
    assert!(result.is_ok(), "DC 100 should be valid");
}

#[test]
fn validate_dc_rejects_101() {
    let result = validate_dice_inputs(&[d20_spec()], 0, dc(101));
    assert!(
        matches!(result, Err(DiceInputError::DcOutOfRange { .. })),
        "DC 101 should be rejected"
    );
}

#[test]
fn validate_dc_rejects_large_value() {
    let result = validate_dice_inputs(&[d20_spec()], 0, dc(u32::MAX));
    assert!(
        matches!(result, Err(DiceInputError::DcOutOfRange { .. })),
        "DC u32::MAX should be rejected"
    );
}

// ===========================================================================
// AC: Dispatch boundary validates modifier in range (-100..=100)
// ===========================================================================

#[test]
fn validate_modifier_accepts_zero() {
    let result = validate_dice_inputs(&[d20_spec()], 0, dc(10));
    assert!(result.is_ok());
}

#[test]
fn validate_modifier_accepts_negative_100() {
    let result = validate_dice_inputs(&[d20_spec()], -100, dc(10));
    assert!(result.is_ok());
}

#[test]
fn validate_modifier_accepts_positive_100() {
    let result = validate_dice_inputs(&[d20_spec()], 100, dc(10));
    assert!(result.is_ok());
}

#[test]
fn validate_modifier_rejects_101() {
    let result = validate_dice_inputs(&[d20_spec()], 101, dc(10));
    assert!(
        matches!(result, Err(DiceInputError::ModifierOutOfRange { .. })),
        "Modifier 101 should be rejected"
    );
}

#[test]
fn validate_modifier_rejects_negative_101() {
    let result = validate_dice_inputs(&[d20_spec()], -101, dc(10));
    assert!(
        matches!(result, Err(DiceInputError::ModifierOutOfRange { .. })),
        "Modifier -101 should be rejected"
    );
}

// ===========================================================================
// AC: Dispatch boundary validates pool group count capped (≤10)
// ===========================================================================

#[test]
fn validate_pool_accepts_10_groups() {
    let pool: Vec<DieSpec> = (0..10).map(|_| d20_spec()).collect();
    let result = validate_dice_inputs(&pool, 0, dc(10));
    assert!(result.is_ok(), "10 groups should be valid");
}

#[test]
fn validate_pool_rejects_11_groups() {
    let pool: Vec<DieSpec> = (0..11).map(|_| d20_spec()).collect();
    let result = validate_dice_inputs(&pool, 0, dc(10));
    assert!(
        matches!(result, Err(DiceInputError::PoolTooLarge { .. })),
        "11 groups should be rejected"
    );
}

#[test]
fn validate_pool_rejects_empty() {
    let result = validate_dice_inputs(&[], 0, dc(10));
    assert!(
        matches!(result, Err(DiceInputError::EmptyPool)),
        "Empty pool should be rejected at dispatch boundary"
    );
}

#[test]
fn validate_pool_rejects_unknown_die() {
    let unknown = DieSpec {
        sides: DieSides::Unknown,
        count: NonZeroU8::new(1).unwrap(),
    };
    let result = validate_dice_inputs(&[unknown], 0, dc(10));
    assert!(
        matches!(result, Err(DiceInputError::UnknownDie)),
        "Unknown die should be rejected at dispatch boundary"
    );
}

// ===========================================================================
// AC: DiceInputError is #[non_exhaustive]
// ===========================================================================

#[test]
fn dice_input_error_is_non_exhaustive() {
    let err = DiceInputError::EmptyPool;
    match err {
        DiceInputError::EmptyPool => {}
        DiceInputError::UnknownDie => {}
        DiceInputError::DcOutOfRange { .. } => {}
        DiceInputError::ModifierOutOfRange { .. } => {}
        DiceInputError::PoolTooLarge { .. } => {}
        _ => {} // required by #[non_exhaustive]
    }
}

// ===========================================================================
// AC: Seed generation — deterministic from session state, independent of client
// ===========================================================================

#[test]
fn seed_generation_produces_nonzero_values() {
    let seed = generate_dice_seed("session-123", 5);
    assert_ne!(seed, 0, "Generated seed should not be zero");
}

#[test]
fn seed_generation_is_deterministic() {
    let seed_a = generate_dice_seed("session-abc", 10);
    let seed_b = generate_dice_seed("session-abc", 10);
    assert_eq!(
        seed_a, seed_b,
        "Same session + turn should produce same seed"
    );
}

#[test]
fn seed_generation_varies_by_session() {
    let seed_a = generate_dice_seed("session-1", 5);
    let seed_b = generate_dice_seed("session-2", 5);
    assert_ne!(
        seed_a, seed_b,
        "Different sessions should produce different seeds"
    );
}

#[test]
fn seed_generation_varies_by_turn() {
    let seed_a = generate_dice_seed("session-x", 1);
    let seed_b = generate_dice_seed("session-x", 2);
    assert_ne!(
        seed_a, seed_b,
        "Different turns in same session should produce different seeds"
    );
}

// ===========================================================================
// AC: compose_dice_result — maps ResolvedRoll + echo fields → DiceResultPayload
// ===========================================================================

#[test]
fn compose_dice_result_maps_all_fields() {
    let seed = 42u64;
    let resolved = resolve_dice(&[d20_spec()], 3, dc(15), seed).unwrap();
    let throw = sample_throw_params();

    let result = compose_dice_result(
        "req-001",
        "player-1",
        "Grimjaw",
        &resolved,
        3,
        dc(15),
        seed,
        &throw,
    );

    assert_eq!(result.request_id, "req-001");
    assert_eq!(result.rolling_player_id, "player-1");
    assert_eq!(result.character_name, "Grimjaw");
    assert_eq!(result.rolls, resolved.rolls);
    assert_eq!(result.modifier, 3);
    assert_eq!(result.total, resolved.total);
    assert_eq!(result.difficulty, dc(15));
    assert_eq!(result.outcome, resolved.outcome);
    assert_eq!(result.seed, 42);
    assert_eq!(result.throw_params, throw);
}

#[test]
fn compose_dice_result_outcome_never_unknown() {
    for seed in 0u64..100 {
        let resolved = resolve_dice(&[d20_spec()], 0, dc(10), seed).unwrap();
        let result = compose_dice_result(
            "req",
            "player",
            "char",
            &resolved,
            0,
            dc(10),
            seed,
            &sample_throw_params(),
        );
        assert!(
            !matches!(result.outcome, RollOutcome::Unknown),
            "Composed result must never have Unknown outcome. seed={seed}"
        );
    }
}

#[test]
fn compose_dice_result_serde_round_trip() {
    let seed = 99u64;
    let resolved = resolve_dice(&[d20_spec(), d6_spec(2)], 5, dc(18), seed).unwrap();
    let result = compose_dice_result(
        "req-rt",
        "p1",
        "Tormund",
        &resolved,
        5,
        dc(18),
        seed,
        &sample_throw_params(),
    );

    let json = serde_json::to_string(&result).expect("should serialize");
    let deserialized: DiceResultPayload = serde_json::from_str(&json).expect("should round-trip");

    assert_eq!(deserialized.request_id, "req-rt");
    assert_eq!(deserialized.total, result.total);
    assert_eq!(deserialized.outcome, result.outcome);
    assert_eq!(deserialized.seed, 99);
}

// ===========================================================================
// AC: Wiring — production dispatch code handles DiceThrow messages
// ===========================================================================

// Wiring tests below are #[ignore] until the DiceThrow handler is fully
// implemented (34-4 orchestration completion). The include_str! pattern
// matches text presence, not call sites — these must be replaced with
// behavioral integration tests when the full flow is wired.

#[test]
#[ignore = "34-4: DiceThrow handler is a stub — wiring test matches comments, not call sites"]
fn dispatch_has_dice_throw_handler() {
    let dispatch_src = include_str!("../../src/lib.rs");
    let production_code = dispatch_src
        .split("#[cfg(test)]")
        .next()
        .unwrap_or(dispatch_src);

    assert!(
        production_code.contains("DiceThrow"),
        "dispatch must handle GameMessage::DiceThrow"
    );
}

#[test]
#[ignore = "34-4: resolve_dice not yet called from dispatch — import only"]
fn dispatch_calls_resolve_dice() {
    let dispatch_src = include_str!("../../src/lib.rs");
    let beat_src = include_str!("../../src/dispatch/beat.rs");
    let combined = format!("{}\n{}", dispatch_src, beat_src);
    let production_code = combined.split("#[cfg(test)]").next().unwrap_or(&combined);

    assert!(
        production_code.contains("resolve_dice"),
        "dispatch or beat module must call resolve_dice"
    );
}

#[test]
#[ignore = "34-4: validate_dice_inputs not yet called from dispatch"]
fn dispatch_calls_validate_dice_inputs() {
    let dispatch_src = include_str!("../../src/lib.rs");
    let beat_src = include_str!("../../src/dispatch/beat.rs");
    let combined = format!("{}\n{}", dispatch_src, beat_src);
    let production_code = combined.split("#[cfg(test)]").next().unwrap_or(&combined);

    assert!(
        production_code.contains("validate_dice_inputs"),
        "dispatch must validate inputs before calling resolve_dice"
    );
}

#[test]
#[ignore = "34-4: DiceResult not yet broadcast — handler returns error"]
fn dispatch_broadcasts_dice_result() {
    let dispatch_src = include_str!("../../src/lib.rs");
    let production_code = dispatch_src
        .split("#[cfg(test)]")
        .next()
        .unwrap_or(dispatch_src);

    assert!(
        production_code.contains("DiceResult"),
        "dispatch must broadcast GameMessage::DiceResult"
    );
}

// ===========================================================================
// AC: Determinism — same seed + params = same result across calls
// ===========================================================================

#[test]
fn full_composition_deterministic() {
    let seed = 12345u64;
    let pool = [d20_spec(), d6_spec(2)];
    let throw = sample_throw_params();

    let resolved_a = resolve_dice(&pool, 5, dc(15), seed).unwrap();
    let result_a = compose_dice_result("r1", "p1", "c1", &resolved_a, 5, dc(15), seed, &throw);

    let resolved_b = resolve_dice(&pool, 5, dc(15), seed).unwrap();
    let result_b = compose_dice_result("r1", "p1", "c1", &resolved_b, 5, dc(15), seed, &throw);

    assert_eq!(result_a.total, result_b.total);
    assert_eq!(result_a.outcome, result_b.outcome);
    assert_eq!(result_a.rolls, result_b.rolls);
}

// ===========================================================================
// AC: No unhandled panics — all Result types matched
// ===========================================================================

#[test]
fn validate_then_resolve_error_propagation() {
    // After validation passes, resolve_dice should never fail for validated inputs
    let pool = [d20_spec()];
    validate_dice_inputs(&pool, 0, dc(10)).expect("validation should pass");
    let result = resolve_dice(&pool, 0, dc(10), 42);
    assert!(
        result.is_ok(),
        "resolve_dice must succeed when inputs pass dispatch validation"
    );
}

#[test]
fn validate_catches_what_resolve_would_reject() {
    // Empty pool: dispatch catches before resolver
    assert!(validate_dice_inputs(&[], 0, dc(10)).is_err());

    // Unknown die: dispatch catches before resolver
    let unknown = DieSpec {
        sides: DieSides::Unknown,
        count: NonZeroU8::new(1).unwrap(),
    };
    assert!(validate_dice_inputs(&[unknown], 0, dc(10)).is_err());
}

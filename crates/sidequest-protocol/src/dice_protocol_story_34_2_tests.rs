//! Story 34-2: Dice resolution protocol — DiceRequest / DiceThrow / DiceResult
//!
//! Types defined (per ADR-074 and the review pass on PR #34-2):
//!   - `GameMessage::DiceRequest` variant      (server -> client broadcast)
//!   - `GameMessage::DiceThrow` variant        (client -> server, rolling player)
//!   - `GameMessage::DiceResult` variant       (server -> all clients broadcast)
//!   - `DiceRequestPayload` (validated on deserialization — non-empty pool,
//!     non-blank stat, NonZeroU32 difficulty)
//!   - `DiceThrowPayload`
//!   - `DiceResultPayload` (total: i32, rolls: Vec<DieGroupResult>)
//!   - `DieGroupResult { spec: DieSpec, faces: Vec<u32> }`
//!   - `DieSpec { sides: DieSides, count: NonZeroU8 }`
//!   - `DieSides` bounded enum (D4/D6/D8/D10/D12/D20/D100 + #[serde(other)] Unknown)
//!   - `ThrowParams { velocity, angular, position }`
//!   - `RollOutcome` (CritSuccess/Success/Fail/CritFail + #[serde(other)] Unknown)
//!
//! ACs tested:
//!   AC1 — compilation (types exist and construct with the new bounded types)
//!   AC2 — variants added to GameMessage enum (covered by round-trip tests)
//!   AC3 — serde round-trip preserves every field
//!   AC4 — deny_unknown_fields is enforced (wire-schema stability)
//!   AC5 — RollOutcome covers all four named variants AND forward-compat Unknown
//!   AC6 — new variants integrate cleanly (SCREAMING_CASE `type` tag)
//!   AC7 — ADR-074 fixture deserializes into the expected variant + fields
//!   AC8 — types live in the protocol crate (all six new public types probed)
//!
//! Review-pass additions:
//!   - Wire-string pinning for RollOutcome variants (was: discriminant-only)
//!   - Negative-total round trip (DiceResultPayload.total is i32, not u32)
//!   - Validated-deserialization rejection tests (empty pool, blank stat, difficulty=0)
//!   - DieSides::Unknown forward-compat test
//!   - RollOutcome::Unknown forward-compat test
//!   - DieGroupResult deny_unknown_fields test
//!   - AC8 wiring test expanded to construct all six new public types

use super::*;
use std::num::{NonZeroU32, NonZeroU8};

/// Build a single-d20 pool with count=1. Used by most tests.
fn d20_once() -> Vec<DieSpec> {
    vec![DieSpec {
        sides: DieSides::D20,
        count: NonZeroU8::new(1).unwrap(),
    }]
}

/// Build a 4d6+2d10 mixed pool for pool-grouping tests.
fn pool_4d6_2d10() -> Vec<DieSpec> {
    vec![
        DieSpec {
            sides: DieSides::D6,
            count: NonZeroU8::new(4).unwrap(),
        },
        DieSpec {
            sides: DieSides::D10,
            count: NonZeroU8::new(2).unwrap(),
        },
    ]
}

/// Build a minimal valid `ThrowParams` with all-zero values.
fn throw_params_zero() -> ThrowParams {
    ThrowParams {
        velocity: [0.0; 3],
        angular: [0.0; 3],
        position: [0.5, 0.5],
    }
}

/// DC 15 as a `NonZeroU32`. Most tests use this.
fn dc(value: u32) -> NonZeroU32 {
    NonZeroU32::new(value).expect("test DC must be non-zero")
}

// ============================================================================
// AC1: Supporting struct field coverage — DiceRequestPayload
// ============================================================================

#[test]
fn dice_request_payload_has_all_adr_074_fields() {
    let payload = DiceRequestPayload {
        request_id: "req-1".to_string(),
        rolling_player_id: "p1".to_string(),
        character_name: "Kira".to_string(),
        dice: d20_once(),
        modifier: 3,
        stat: "dexterity".to_string(),
        difficulty: dc(15),
        context: "Pick the lock".to_string(),
    };
    // Not vacuous: verifies every ADR-074 field is present with the correct type
    assert_eq!(payload.request_id, "req-1");
    assert_eq!(payload.rolling_player_id, "p1");
    assert_eq!(payload.character_name, "Kira");
    assert_eq!(payload.dice.len(), 1);
    assert_eq!(payload.dice[0].sides, DieSides::D20);
    assert_eq!(payload.dice[0].count.get(), 1);
    assert_eq!(payload.modifier, 3);
    assert_eq!(payload.stat, "dexterity");
    assert_eq!(payload.difficulty.get(), 15);
    assert_eq!(payload.context, "Pick the lock");
}

#[test]
fn dice_request_supports_negative_modifier() {
    // ADR-074: `modifier: i32` — must accept penalties, not just bonuses.
    let payload = DiceRequestPayload {
        request_id: "req-2".to_string(),
        rolling_player_id: "p1".to_string(),
        character_name: "Kira".to_string(),
        dice: d20_once(),
        modifier: -2,
        stat: "strength".to_string(),
        difficulty: dc(12),
        context: "Wounded, she heaves the door.".to_string(),
    };
    assert_eq!(payload.modifier, -2);
}

#[test]
fn dice_request_supports_pool_of_dice() {
    // ADR-074: "Dice pools — 4d6, 2d10 — thrown together in one gesture."
    let payload = DiceRequestPayload {
        request_id: "req-3".to_string(),
        rolling_player_id: "p1".to_string(),
        character_name: "Kira".to_string(),
        dice: pool_4d6_2d10(),
        modifier: 0,
        stat: "constitution".to_string(),
        difficulty: dc(18),
        context: "Channel raw power.".to_string(),
    };
    assert_eq!(payload.dice.len(), 2);
    assert_eq!(payload.dice[0].count.get(), 4);
    assert_eq!(payload.dice[0].sides, DieSides::D6);
    assert_eq!(payload.dice[1].sides, DieSides::D10);
    assert_eq!(payload.dice[1].count.get(), 2);
}

// ============================================================================
// AC1: DiceThrowPayload + ThrowParams — gesture capture
// ============================================================================

#[test]
fn throw_params_has_velocity_angular_position() {
    let tp = ThrowParams {
        velocity: [1.5, -2.25, 0.0],
        angular: [0.1, 0.2, 0.3],
        position: [0.42, 0.58],
    };
    assert_eq!(tp.velocity, [1.5, -2.25, 0.0]);
    assert_eq!(tp.angular, [0.1, 0.2, 0.3]);
    assert_eq!(tp.position, [0.42, 0.58]);
}

#[test]
fn dice_throw_payload_carries_request_id_and_params() {
    let payload = DiceThrowPayload {
        request_id: "req-1".to_string(),
        throw_params: ThrowParams {
            velocity: [0.1, 0.2, 0.3],
            angular: [0.4, 0.5, 0.6],
            position: [0.25, 0.75],
        },
        face: vec![17],
    };
    // Review fix #8: previously asserted velocity only; angular and position
    // were constructed with specific values but never checked. Every
    // ThrowParams field must be asserted so the test is load-bearing.
    assert_eq!(payload.request_id, "req-1");
    assert_eq!(payload.throw_params.velocity, [0.1, 0.2, 0.3]);
    assert_eq!(payload.throw_params.angular, [0.4, 0.5, 0.6]);
    assert_eq!(payload.throw_params.position, [0.25, 0.75]);
    assert_eq!(payload.face, vec![17]);
}

// ============================================================================
// AC1 + AC5: RollOutcome — all four named variants survive serde round-trip
//                          and pin to the exact ADR-074 wire strings
// ============================================================================

#[test]
fn roll_outcome_named_variants_round_trip_with_exact_wire_strings() {
    // Review fix #9: previously used `std::mem::discriminant` comparison, which
    // would pass even if serde mis-encoded variants as integers or lowercase.
    // ADR-074 specifies the wire format is named PascalCase strings. Pin them
    // literally.
    let cases = [
        (RollOutcome::CritSuccess, "\"CritSuccess\""),
        (RollOutcome::Success, "\"Success\""),
        (RollOutcome::Fail, "\"Fail\""),
        (RollOutcome::CritFail, "\"CritFail\""),
    ];
    for (outcome, expected_wire) in cases {
        let json = serde_json::to_string(&outcome).expect("serialize RollOutcome");
        assert_eq!(
            json, expected_wire,
            "RollOutcome must serialize to ADR-074 wire string exactly"
        );
        let restored: RollOutcome = serde_json::from_str(&json).expect("deserialize RollOutcome");
        assert_eq!(
            std::mem::discriminant(&outcome),
            std::mem::discriminant(&restored),
            "RollOutcome variant identity must survive round-trip"
        );
    }
}

#[test]
fn roll_outcome_unknown_variant_absorbs_future_wire_strings() {
    // Review fix #4: #[non_exhaustive] on the enum is a compile-time guard;
    // serde-level forward compatibility requires #[serde(other)] on a catch-all
    // variant. Verify the catch-all actually works: an older client receiving
    // a variant string it doesn't know about ("NearMiss", say) must fall
    // through to Unknown instead of Err-ing on deserialization.
    let restored: RollOutcome =
        serde_json::from_str("\"NearMiss\"").expect("unknown variants must fall to Unknown");
    assert!(
        matches!(restored, RollOutcome::Unknown),
        "unknown outcome string must deserialize to RollOutcome::Unknown"
    );

    let other: RollOutcome =
        serde_json::from_str("\"SomeFutureOutcome\"").expect("any unknown string must absorb");
    assert!(matches!(other, RollOutcome::Unknown));
}

// ============================================================================
// AC3: Serde round-trip — every field preserved for each of the three messages
// ============================================================================

#[test]
fn dice_request_serde_round_trip_preserves_every_field() {
    let msg = GameMessage::DiceRequest {
        payload: DiceRequestPayload {
            request_id: "req-42".to_string(),
            rolling_player_id: "kira".to_string(),
            character_name: "Kira the Sly".to_string(),
            dice: d20_once(),
            modifier: 3,
            stat: "dexterity".to_string(),
            difficulty: dc(15),
            context: "Pick the lock — you need a 12.".to_string(),
        },
        player_id: "server".to_string(),
    };
    let json = serde_json::to_string(&msg).expect("serialize DiceRequest");
    let restored: GameMessage = serde_json::from_str(&json).expect("deserialize DiceRequest");
    match restored {
        GameMessage::DiceRequest { payload, player_id } => {
            assert_eq!(player_id, "server");
            assert_eq!(payload.request_id, "req-42");
            assert_eq!(payload.rolling_player_id, "kira");
            assert_eq!(payload.character_name, "Kira the Sly");
            assert_eq!(payload.dice.len(), 1);
            assert_eq!(payload.dice[0].sides, DieSides::D20);
            assert_eq!(payload.dice[0].count.get(), 1);
            assert_eq!(payload.modifier, 3);
            assert_eq!(payload.stat, "dexterity");
            assert_eq!(payload.difficulty.get(), 15);
            assert_eq!(payload.context, "Pick the lock — you need a 12.");
        }
        _ => panic!("expected DiceRequest variant"),
    }
}

#[test]
fn dice_throw_serde_round_trip_preserves_every_field() {
    let msg = GameMessage::DiceThrow {
        payload: DiceThrowPayload {
            request_id: "req-42".to_string(),
            throw_params: ThrowParams {
                velocity: [1.0, 2.5, -0.75],
                angular: [0.1, -0.2, 0.3],
                position: [0.25, 0.75],
            },
            face: vec![14],
        },
        player_id: "kira".to_string(),
    };
    let json = serde_json::to_string(&msg).expect("serialize DiceThrow");
    let restored: GameMessage = serde_json::from_str(&json).expect("deserialize DiceThrow");
    match restored {
        GameMessage::DiceThrow { payload, player_id } => {
            assert_eq!(player_id, "kira");
            assert_eq!(payload.request_id, "req-42");
            assert_eq!(payload.throw_params.velocity, [1.0, 2.5, -0.75]);
            assert_eq!(payload.throw_params.angular, [0.1, -0.2, 0.3]);
            assert_eq!(payload.throw_params.position, [0.25, 0.75]);
            assert_eq!(payload.face, vec![14]);
        }
        _ => panic!("expected DiceThrow variant"),
    }
}

#[test]
fn dice_result_serde_round_trip_preserves_every_field() {
    let msg = GameMessage::DiceResult {
        payload: DiceResultPayload {
            request_id: "req-42".to_string(),
            rolling_player_id: "kira".to_string(),
            character_name: "Kira the Sly".to_string(),
            rolls: vec![DieGroupResult {
                spec: DieSpec {
                    sides: DieSides::D20,
                    count: NonZeroU8::new(1).unwrap(),
                },
                faces: vec![17],
            }],
            modifier: 3,
            total: 20,
            difficulty: dc(15),
            outcome: RollOutcome::Success,
            seed: 0x1234_5678_9ABC_DEF0_u64,
            throw_params: ThrowParams {
                velocity: [1.0, 2.5, -0.75],
                angular: [0.1, -0.2, 0.3],
                position: [0.25, 0.75],
            },
        },
        player_id: "server".to_string(),
    };
    let json = serde_json::to_string(&msg).expect("serialize DiceResult");
    let restored: GameMessage = serde_json::from_str(&json).expect("deserialize DiceResult");
    match restored {
        GameMessage::DiceResult { payload, player_id } => {
            assert_eq!(player_id, "server");
            assert_eq!(payload.request_id, "req-42");
            assert_eq!(payload.rolling_player_id, "kira");
            assert_eq!(payload.character_name, "Kira the Sly");
            assert_eq!(payload.rolls.len(), 1);
            assert_eq!(payload.rolls[0].spec.sides, DieSides::D20);
            assert_eq!(payload.rolls[0].spec.count.get(), 1);
            assert_eq!(payload.rolls[0].faces, vec![17]);
            assert_eq!(payload.modifier, 3);
            assert_eq!(payload.total, 20);
            assert_eq!(payload.difficulty.get(), 15);
            assert!(matches!(payload.outcome, RollOutcome::Success));
            assert_eq!(payload.seed, 0x1234_5678_9ABC_DEF0_u64);
            assert_eq!(payload.throw_params.velocity, [1.0, 2.5, -0.75]);
            assert_eq!(payload.throw_params.angular, [0.1, -0.2, 0.3]);
            assert_eq!(payload.throw_params.position, [0.25, 0.75]);
        }
        _ => panic!("expected DiceResult variant"),
    }
}

#[test]
fn dice_result_round_trip_with_mixed_pool_preserves_group_attribution() {
    // Review fix #6: pool grouping must survive round-trip. Before this change
    // rolls was Vec<u32> and the attribution was lost — a 4d6+2d10 pool became
    // an indistinguishable [3,5,2,6,7,9].
    // Review fix #10: previously used `if let` with no else panic arm, which
    // would silently pass on variant mismatch. Now uses match.
    let msg = GameMessage::DiceResult {
        payload: DiceResultPayload {
            request_id: "req-pool".to_string(),
            rolling_player_id: "kira".to_string(),
            character_name: "Kira".to_string(),
            rolls: vec![
                DieGroupResult {
                    spec: DieSpec {
                        sides: DieSides::D6,
                        count: NonZeroU8::new(4).unwrap(),
                    },
                    faces: vec![3, 5, 2, 6],
                },
                DieGroupResult {
                    spec: DieSpec {
                        sides: DieSides::D10,
                        count: NonZeroU8::new(2).unwrap(),
                    },
                    faces: vec![7, 9],
                },
            ],
            modifier: 0,
            total: 32,
            difficulty: dc(12),
            outcome: RollOutcome::Success,
            seed: 42,
            throw_params: throw_params_zero(),
        },
        player_id: "server".to_string(),
    };
    let json = serde_json::to_string(&msg).expect("serialize pool DiceResult");
    let restored: GameMessage = serde_json::from_str(&json).expect("deserialize pool DiceResult");
    match restored {
        GameMessage::DiceResult { payload, .. } => {
            assert_eq!(payload.rolls.len(), 2, "two groups must survive round-trip");
            assert_eq!(payload.rolls[0].spec.sides, DieSides::D6);
            assert_eq!(payload.rolls[0].faces, vec![3, 5, 2, 6]);
            assert_eq!(payload.rolls[1].spec.sides, DieSides::D10);
            assert_eq!(payload.rolls[1].faces, vec![7, 9]);
            assert_eq!(payload.total, 32);
        }
        _ => panic!("expected DiceResult variant from pool fixture"),
    }
}

#[test]
fn dice_result_negative_total_round_trip() {
    // Review fix #1: previously DiceResultPayload.total was u32, which silently
    // wrapped when sum(rolls) + modifier < 0. Now i32. This test locks in the
    // signed-total contract: rolls=[1], modifier=-5 gives total=-4, and the
    // wire format must preserve that through serialize + deserialize.
    let msg = GameMessage::DiceResult {
        payload: DiceResultPayload {
            request_id: "req-penalty".to_string(),
            rolling_player_id: "kira".to_string(),
            character_name: "Kira".to_string(),
            rolls: vec![DieGroupResult {
                spec: DieSpec {
                    sides: DieSides::D20,
                    count: NonZeroU8::new(1).unwrap(),
                },
                faces: vec![1],
            }],
            modifier: -5,
            total: -4,
            difficulty: dc(15),
            outcome: RollOutcome::Fail,
            seed: 7,
            throw_params: throw_params_zero(),
        },
        player_id: "server".to_string(),
    };
    let json = serde_json::to_string(&msg).expect("serialize negative-total DiceResult");
    let restored: GameMessage =
        serde_json::from_str(&json).expect("deserialize negative-total DiceResult");
    match restored {
        GameMessage::DiceResult { payload, .. } => {
            assert_eq!(payload.total, -4, "negative total must survive round-trip");
            assert_eq!(payload.modifier, -5);
        }
        _ => panic!("expected DiceResult variant from penalty fixture"),
    }
}

// ============================================================================
// AC6: SCREAMING_CASE type tags — wire compatibility with UI
// ============================================================================

#[test]
fn dice_request_serializes_with_dice_request_type_tag() {
    let msg = GameMessage::DiceRequest {
        payload: DiceRequestPayload {
            request_id: "req-1".to_string(),
            rolling_player_id: "p1".to_string(),
            character_name: "Kira".to_string(),
            dice: d20_once(),
            modifier: 0,
            stat: "wisdom".to_string(),
            difficulty: dc(10),
            context: "Listen at the door.".to_string(),
        },
        player_id: "server".to_string(),
    };
    let json = serde_json::to_string(&msg).expect("serialize");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        v["type"], "DICE_REQUEST",
        "must use SCREAMING_CASE type tag matching ADR-074 wire format"
    );
}

#[test]
fn dice_throw_serializes_with_dice_throw_type_tag() {
    let msg = GameMessage::DiceThrow {
        payload: DiceThrowPayload {
            request_id: "req-1".to_string(),
            throw_params: throw_params_zero(),
            face: vec![10],
        },
        player_id: "p1".to_string(),
    };
    let json = serde_json::to_string(&msg).expect("serialize");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        v["type"], "DICE_THROW",
        "must use SCREAMING_CASE type tag matching ADR-074 wire format"
    );
}

#[test]
fn dice_result_serializes_with_dice_result_type_tag() {
    let msg = GameMessage::DiceResult {
        payload: DiceResultPayload {
            request_id: "req-1".to_string(),
            rolling_player_id: "p1".to_string(),
            character_name: "Kira".to_string(),
            rolls: vec![DieGroupResult {
                spec: DieSpec {
                    sides: DieSides::D20,
                    count: NonZeroU8::new(1).unwrap(),
                },
                faces: vec![1],
            }],
            modifier: 0,
            total: 1,
            difficulty: dc(10),
            outcome: RollOutcome::CritFail,
            seed: 0,
            throw_params: throw_params_zero(),
        },
        player_id: "server".to_string(),
    };
    let json = serde_json::to_string(&msg).expect("serialize");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        v["type"], "DICE_RESULT",
        "must use SCREAMING_CASE type tag matching ADR-074 wire format"
    );
}

// ============================================================================
// AC4: deny_unknown_fields — wire schema stability (crate convention)
// ============================================================================

#[test]
fn dice_request_payload_rejects_unknown_fields() {
    // Matches the crate-wide convention: all payloads use
    // `#[serde(deny_unknown_fields)]` so that typos or drift in the UI client
    // fail loudly. `deny_unknown_fields` lives on `DiceRequestPayloadRaw` (the
    // deserialization intermediary) — the check is identical from the wire's
    // perspective.
    let bad_json = r#"{
        "request_id": "req-1",
        "rolling_player_id": "p1",
        "character_name": "Kira",
        "dice": [{"sides": 20, "count": 1}],
        "modifier": 3,
        "stat": "dexterity",
        "difficulty": 15,
        "context": "Pick the lock",
        "surprise_field": "should be rejected"
    }"#;
    let result: Result<DiceRequestPayload, _> = serde_json::from_str(bad_json);
    assert!(
        result.is_err(),
        "DiceRequestPayload must reject unknown fields (deny_unknown_fields convention)"
    );
}

#[test]
fn dice_throw_payload_rejects_unknown_fields() {
    let bad_json = r#"{
        "request_id": "req-1",
        "throw_params": {
            "velocity": [0.0, 0.0, 0.0],
            "angular": [0.0, 0.0, 0.0],
            "position": [0.5, 0.5]
        },
        "stowaway": 99
    }"#;
    let result: Result<DiceThrowPayload, _> = serde_json::from_str(bad_json);
    assert!(
        result.is_err(),
        "DiceThrowPayload must reject unknown fields (deny_unknown_fields convention)"
    );
}

#[test]
fn dice_result_payload_rejects_unknown_fields() {
    let bad_json = r#"{
        "request_id": "req-1",
        "rolling_player_id": "p1",
        "character_name": "Kira",
        "rolls": [{"spec": {"sides": 20, "count": 1}, "faces": [17]}],
        "modifier": 3,
        "total": 20,
        "difficulty": 15,
        "outcome": "Success",
        "seed": 0,
        "throw_params": {
            "velocity": [0.0, 0.0, 0.0],
            "angular": [0.0, 0.0, 0.0],
            "position": [0.5, 0.5]
        },
        "hidden_cheat": true
    }"#;
    let result: Result<DiceResultPayload, _> = serde_json::from_str(bad_json);
    assert!(
        result.is_err(),
        "DiceResultPayload must reject unknown fields (deny_unknown_fields convention)"
    );
}

#[test]
fn die_spec_rejects_unknown_fields() {
    let bad_json = r#"{"sides": 20, "count": 1, "weighted": true}"#;
    let result: Result<DieSpec, _> = serde_json::from_str(bad_json);
    assert!(
        result.is_err(),
        "DieSpec must reject unknown fields — 'weighted' would be a physics cheat"
    );
}

#[test]
fn die_group_result_rejects_unknown_fields() {
    // New type from review fix #6. Same crate convention — extra fields reject.
    let bad_json = r#"{
        "spec": {"sides": 20, "count": 1},
        "faces": [17],
        "multiplier": 2
    }"#;
    let result: Result<DieGroupResult, _> = serde_json::from_str(bad_json);
    assert!(
        result.is_err(),
        "DieGroupResult must reject unknown fields — 'multiplier' would silently alter outcomes"
    );
}

#[test]
fn dice_result_payload_rejects_face_count_mismatch() {
    // Cycle-2 review fix #2: the DieGroupResult invariant
    // `faces.len() == spec.count.get() as usize` must actually be enforced at
    // the wire boundary, not just documented. Send a payload where the declared
    // count is 4 but only 1 face value is present — deserialization must fail
    // with `DiceResultPayloadError::FaceCountMismatch`.
    let bad_json = r#"{
        "request_id": "cheat",
        "rolling_player_id": "p1",
        "character_name": "Kira",
        "rolls": [{"spec": {"sides": 6, "count": 4}, "faces": [6]}],
        "modifier": 0,
        "total": 6,
        "difficulty": 10,
        "outcome": "Success",
        "seed": 0,
        "throw_params": {
            "velocity": [0.0, 0.0, 0.0],
            "angular": [0.0, 0.0, 0.0],
            "position": [0.5, 0.5]
        }
    }"#;
    let result: Result<DiceResultPayload, _> = serde_json::from_str(bad_json);
    assert!(
        result.is_err(),
        "count=4 + faces=[6] must be rejected — DieGroupResult invariant"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("declared count=4") && err.contains("got 1 face"),
        "rejection must name the mismatch explicitly: got {err}"
    );
}

#[test]
fn dice_result_payload_accepts_correct_face_counts_for_pool() {
    // Happy path for the new invariant — a 4d6+2d10 pool with exactly 4 and 2
    // face values must deserialize cleanly.
    let ok_json = r#"{
        "request_id": "ok",
        "rolling_player_id": "p1",
        "character_name": "Kira",
        "rolls": [
            {"spec": {"sides": 6, "count": 4}, "faces": [3, 5, 2, 6]},
            {"spec": {"sides": 10, "count": 2}, "faces": [7, 9]}
        ],
        "modifier": 0,
        "total": 32,
        "difficulty": 12,
        "outcome": "Success",
        "seed": 0,
        "throw_params": {
            "velocity": [0.0, 0.0, 0.0],
            "angular": [0.0, 0.0, 0.0],
            "position": [0.5, 0.5]
        }
    }"#;
    let payload: DiceResultPayload =
        serde_json::from_str(ok_json).expect("valid pool must deserialize");
    assert_eq!(payload.rolls.len(), 2);
    assert_eq!(payload.rolls[0].faces.len(), 4);
    assert_eq!(payload.rolls[1].faces.len(), 2);
}

#[test]
fn throw_params_rejects_unknown_fields() {
    let bad_json = r#"{
        "velocity": [0.0, 0.0, 0.0],
        "angular": [0.0, 0.0, 0.0],
        "position": [0.5, 0.5],
        "magic_outcome_override": "CritSuccess"
    }"#;
    let result: Result<ThrowParams, _> = serde_json::from_str(bad_json);
    assert!(
        result.is_err(),
        "ThrowParams must reject unknown fields — server authority depends on this"
    );
}

// ============================================================================
// Validated deserialization — new invariants from review pass
// ============================================================================

#[test]
fn dice_request_payload_rejects_empty_dice_pool() {
    // Review fix #16: an empty pool (`dice: []`) is a nonsensical game state —
    // modifier-only "roll" with no dice actually thrown. TryFrom validation
    // rejects it at the wire boundary.
    let bad_json = r#"{
        "request_id": "req-1",
        "rolling_player_id": "p1",
        "character_name": "Kira",
        "dice": [],
        "modifier": 3,
        "stat": "dexterity",
        "difficulty": 15,
        "context": "empty pool test"
    }"#;
    let result: Result<DiceRequestPayload, _> = serde_json::from_str(bad_json);
    assert!(
        result.is_err(),
        "empty dice pool must be rejected at deserialization"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("dice pool is empty"),
        "rejection must cite the specific invariant: got {err}"
    );
}

#[test]
fn dice_request_payload_rejects_blank_stat() {
    // Review fix #5 (narrow form): stat=" " or "" passes `String` typing but
    // fails the TryFrom validator. Blank stat would reach the narrator prompt
    // and produce silent garbage downstream.
    let bad_json = r#"{
        "request_id": "req-1",
        "rolling_player_id": "p1",
        "character_name": "Kira",
        "dice": [{"sides": 20, "count": 1}],
        "modifier": 0,
        "stat": "   ",
        "difficulty": 15,
        "context": "blank stat test"
    }"#;
    let result: Result<DiceRequestPayload, _> = serde_json::from_str(bad_json);
    assert!(
        result.is_err(),
        "whitespace-only stat must be rejected at deserialization"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("stat field is blank"),
        "rejection must cite the specific invariant: got {err}"
    );
}

#[test]
fn dice_request_payload_rejects_zero_difficulty() {
    // Review fix #15: difficulty=0 would make every roll a guaranteed Success
    // regardless of modifier. NonZeroU32 at the type level + serde's built-in
    // NonZero rejection catches it at deserialization.
    let bad_json = r#"{
        "request_id": "req-1",
        "rolling_player_id": "p1",
        "character_name": "Kira",
        "dice": [{"sides": 20, "count": 1}],
        "modifier": 0,
        "stat": "dexterity",
        "difficulty": 0,
        "context": "zero DC test"
    }"#;
    let result: Result<DiceRequestPayload, _> = serde_json::from_str(bad_json);
    assert!(
        result.is_err(),
        "difficulty=0 must be rejected — NonZeroU32 typing prevents guaranteed-success DCs"
    );
}

#[test]
fn die_spec_rejects_zero_count() {
    // Review fix #2 (narrow form): count=0 was previously representable as u32.
    // NonZeroU8 at the type level rejects it at deserialization.
    let bad_json = r#"{"sides": 20, "count": 0}"#;
    let result: Result<DieSpec, _> = serde_json::from_str(bad_json);
    assert!(
        result.is_err(),
        "count=0 must be rejected — zero dice in a group is nonsensical"
    );
}

#[test]
fn die_sides_rejects_invalid_sides_with_unknown_fallback() {
    // Review fix #2 (cycle 1): previously `sides: u32` accepted any integer
    // including 0 (divide-by-zero), 3 (unspecified), and u32::MAX. Now
    // `DieSides` is a bounded enum with a `From<u32>` bridge where any
    // unrecognized integer maps to `Unknown`.
    //
    // Cycle-2 review fix: wire format is bare JSON integer (not quoted
    // string). The From/Into u32 bridge handles the integer ↔ enum mapping.
    let invalid_values = [
        "0",
        "1",
        "3",
        "7",
        "999",
        "4294967295", // u32::MAX
    ];
    for raw in invalid_values {
        let restored: DieSides =
            serde_json::from_str(raw).expect("unknown sides must fall through to Unknown");
        assert!(
            matches!(restored, DieSides::Unknown),
            "DieSides must not materialize invalid value {raw} as a real die"
        );
    }
}

#[test]
fn die_sides_unknown_round_trips_via_zero_sentinel() {
    // Cycle-2 review fix: the `From<DieSides> for u32` impl maps `Unknown` to
    // the sentinel `0`, and `0` is not in the accepted face-count set, so
    // `0 → Unknown → 0 → Unknown` is stable. This pins the sentinel so a
    // future refactor that changed the sentinel would fail this test.
    let unknown = DieSides::Unknown;
    let json = serde_json::to_string(&unknown).expect("serialize Unknown");
    assert_eq!(
        json, "0",
        "DieSides::Unknown must serialize as the sentinel integer 0"
    );
    let restored: DieSides = serde_json::from_str(&json).expect("deserialize sentinel");
    assert!(matches!(restored, DieSides::Unknown));
}

// ============================================================================
// AC7: ADR-074 wire fixture — full JSON round-trip matching the spec exactly
// ============================================================================

#[test]
fn dice_request_deserializes_from_adr_074_fixture() {
    // Matches ADR-074's DiceRequest example, adapted for the revised schema
    // (rolling_player_id rename, DieSides as string enum).
    let json = r#"{
        "type": "DICE_REQUEST",
        "payload": {
            "request_id": "req-abc",
            "rolling_player_id": "kira",
            "character_name": "Kira the Sly",
            "dice": [{"sides": 20, "count": 1}],
            "modifier": 3,
            "stat": "dexterity",
            "difficulty": 15,
            "context": "The ancient lock is corroded but stubborn."
        },
        "player_id": "server"
    }"#;
    let msg: GameMessage = serde_json::from_str(json).expect("deserialize ADR-074 fixture");
    match msg {
        GameMessage::DiceRequest { payload, player_id } => {
            assert_eq!(player_id, "server");
            assert_eq!(payload.request_id, "req-abc");
            assert_eq!(payload.rolling_player_id, "kira");
            assert_eq!(payload.character_name, "Kira the Sly");
            assert_eq!(payload.dice.len(), 1);
            assert_eq!(payload.dice[0].sides, DieSides::D20);
            assert_eq!(payload.modifier, 3);
            assert_eq!(payload.stat, "dexterity");
            assert_eq!(payload.difficulty.get(), 15);
            assert_eq!(
                payload.context,
                "The ancient lock is corroded but stubborn."
            );
        }
        _ => panic!("ADR-074 DICE_REQUEST fixture must deserialize to DiceRequest variant"),
    }
}

#[test]
fn dice_result_deserializes_from_adr_074_crit_success_fixture() {
    // ADR-074 specifies CritSuccess = "natural max (nat 20 on d20)"
    let json = r#"{
        "type": "DICE_RESULT",
        "payload": {
            "request_id": "req-abc",
            "rolling_player_id": "kira",
            "character_name": "Kira the Sly",
            "rolls": [{"spec": {"sides": 20, "count": 1}, "faces": [20]}],
            "modifier": 3,
            "total": 23,
            "difficulty": 15,
            "outcome": "CritSuccess",
            "seed": 42,
            "throw_params": {
                "velocity": [1.0, 2.0, -1.5],
                "angular": [0.1, 0.2, 0.3],
                "position": [0.5, 0.5]
            }
        },
        "player_id": "server"
    }"#;
    let msg: GameMessage =
        serde_json::from_str(json).expect("deserialize ADR-074 CritSuccess fixture");
    match msg {
        GameMessage::DiceResult { payload, .. } => {
            assert_eq!(payload.rolls.len(), 1);
            assert_eq!(payload.rolls[0].faces, vec![20]);
            assert_eq!(payload.rolls[0].spec.sides, DieSides::D20);
            assert_eq!(payload.total, 23);
            assert!(
                matches!(payload.outcome, RollOutcome::CritSuccess),
                "nat 20 fixture must deserialize to CritSuccess outcome"
            );
            assert_eq!(payload.seed, 42);
        }
        _ => panic!("expected DiceResult variant from CritSuccess fixture"),
    }
}

#[test]
fn dice_result_deserializes_from_adr_074_crit_fail_fixture() {
    // ADR-074: CritFail = "natural 1 on d20" — must be distinguishable from Fail
    // so the narrator can pick its tone (triumph vs dread vs comedy).
    let json = r#"{
        "type": "DICE_RESULT",
        "payload": {
            "request_id": "req-def",
            "rolling_player_id": "kira",
            "character_name": "Kira the Sly",
            "rolls": [{"spec": {"sides": 20, "count": 1}, "faces": [1]}],
            "modifier": 3,
            "total": 4,
            "difficulty": 18,
            "outcome": "CritFail",
            "seed": 99,
            "throw_params": {
                "velocity": [0.0, 0.0, 0.0],
                "angular": [0.0, 0.0, 0.0],
                "position": [0.5, 0.5]
            }
        },
        "player_id": "server"
    }"#;
    let msg: GameMessage =
        serde_json::from_str(json).expect("deserialize ADR-074 CritFail fixture");
    match msg {
        GameMessage::DiceResult { payload, .. } => {
            assert_eq!(payload.rolls.len(), 1);
            assert_eq!(payload.rolls[0].faces, vec![1]);
            assert!(
                matches!(payload.outcome, RollOutcome::CritFail),
                "nat 1 fixture must deserialize to CritFail — not plain Fail"
            );
        }
        _ => panic!("expected DiceResult variant from CritFail fixture"),
    }
}

// ============================================================================
// AC3: DieSpec round-trip — wire format is {"sides": <int>, "count": <int>}
// ============================================================================

#[test]
fn die_spec_serde_round_trip() {
    // Cycle-2 review fix: DieSides now serializes as a bare JSON integer
    // (not a quoted string). The test pins the exact JSON shape.
    let spec = DieSpec {
        sides: DieSides::D20,
        count: NonZeroU8::new(1).unwrap(),
    };
    let json = serde_json::to_string(&spec).expect("serialize DieSpec");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        v["sides"],
        serde_json::json!(20),
        "sides must be JSON integer 20, not string \"20\""
    );
    assert_eq!(v["count"], serde_json::json!(1));
    let restored: DieSpec = serde_json::from_str(&json).expect("deserialize DieSpec");
    assert_eq!(restored.sides, DieSides::D20);
    assert_eq!(restored.count.get(), 1);
}

#[test]
fn die_sides_covers_all_adr_074_tabletop_values() {
    // ADR-074: "4, 6, 8, 10, 12, 20, 100"
    // Cycle-2 review fix: wire is bare JSON integer via #[serde(from/into = "u32")].
    let cases = [
        (DieSides::D4, "4", 4_u32),
        (DieSides::D6, "6", 6),
        (DieSides::D8, "8", 8),
        (DieSides::D10, "10", 10),
        (DieSides::D12, "12", 12),
        (DieSides::D20, "20", 20),
        (DieSides::D100, "100", 100),
    ];
    for (variant, expected_wire, expected_faces) in cases {
        let json = serde_json::to_string(&variant).expect("serialize DieSides");
        assert_eq!(
            json, expected_wire,
            "DieSides::{variant:?} must serialize as bare JSON integer"
        );
        let restored: DieSides =
            serde_json::from_str(&json).expect("deserialize DieSides round-trip");
        assert_eq!(
            std::mem::discriminant(&variant),
            std::mem::discriminant(&restored)
        );
        assert_eq!(variant.faces(), Some(expected_faces));
    }
    // And Unknown has no face count — caller must treat as "reject the roll".
    assert_eq!(DieSides::Unknown.faces(), None);
}

// ============================================================================
// AC8: Wiring — all six new public types reachable via the crate root
// ============================================================================

#[test]
fn all_new_dice_public_types_reachable_via_crate_root() {
    // Review fix #12: previously only DieSpec/ThrowParams/RollOutcome were
    // probed. The AC8 wiring check must be exhaustive — if any of the six new
    // public types became unreachable via `lib.rs pub use message::*`, this
    // test would fail to compile.
    let _die_sides: DieSides = DieSides::D20;
    let _die_spec: DieSpec = DieSpec {
        sides: DieSides::D20,
        count: NonZeroU8::new(1).unwrap(),
    };
    let _die_group: DieGroupResult = DieGroupResult {
        spec: DieSpec {
            sides: DieSides::D20,
            count: NonZeroU8::new(1).unwrap(),
        },
        faces: vec![17],
    };
    let _throw_params: ThrowParams = throw_params_zero();
    let _outcome: RollOutcome = RollOutcome::Success;
    let _request_payload: DiceRequestPayload = DiceRequestPayload {
        request_id: "req-wire".to_string(),
        rolling_player_id: "p1".to_string(),
        character_name: "Kira".to_string(),
        dice: d20_once(),
        modifier: 0,
        stat: "wisdom".to_string(),
        difficulty: dc(10),
        context: "wiring probe".to_string(),
    };
    let _throw_payload: DiceThrowPayload = DiceThrowPayload {
        request_id: "req-wire".to_string(),
        throw_params: throw_params_zero(),
        face: vec![17],
    };
    let _result_payload: DiceResultPayload = DiceResultPayload {
        request_id: "req-wire".to_string(),
        rolling_player_id: "p1".to_string(),
        character_name: "Kira".to_string(),
        rolls: vec![_die_group.clone()],
        modifier: 0,
        total: 17,
        difficulty: dc(10),
        outcome: RollOutcome::Success,
        seed: 0,
        throw_params: throw_params_zero(),
    };

    // Meaningful assertions — not just type annotations.
    assert_eq!(_die_sides.faces(), Some(20));
    assert_eq!(_die_group.faces, vec![17]);
    assert_eq!(_request_payload.dice.len(), 1);
    assert_eq!(_throw_payload.request_id, "req-wire");
    assert_eq!(_result_payload.total, 17);
    assert!(matches!(_outcome, RollOutcome::Success));
}

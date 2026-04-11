//! Story 34-2: Dice resolution protocol — DiceRequest / DiceThrow / DiceResult
//!
//! RED phase — these tests reference types that don't exist yet.
//! They will fail to compile until Dev implements (per ADR-074):
//!   - GameMessage::DiceRequest variant    (server -> client broadcast)
//!   - GameMessage::DiceThrow variant      (client -> server, rolling player)
//!   - GameMessage::DiceResult variant     (server -> all clients broadcast)
//!   - DiceRequestPayload { request_id, player_id, character_name, dice,
//!                          modifier, stat, difficulty, context }
//!   - DiceThrowPayload   { request_id, throw_params }
//!   - DiceResultPayload  { request_id, player_id, character_name, rolls,
//!                          modifier, total, difficulty, outcome, seed, throw_params }
//!   - DieSpec            { sides, count }
//!   - ThrowParams        { velocity: [f32;3], angular: [f32;3], position: [f32;2] }
//!   - RollOutcome enum   { CritSuccess, Success, Fail, CritFail }
//!
//! All payload structs MUST derive Debug, Clone, PartialEq, Serialize, Deserialize
//! and MUST use `#[serde(deny_unknown_fields)]` to match the existing crate convention.
//!
//! ACs tested:
//!   AC1 — compilation (types exist and construct)
//!   AC2 — variants added to GameMessage enum
//!   AC3 — serde round-trip preserves every field
//!   AC4 — deny_unknown_fields is enforced (wire-schema stability)
//!   AC5 — RollOutcome covers all four variants and survives round-trip
//!   AC6 — new variants integrate cleanly (SCREAMING_CASE `type` tag)
//!   AC7 — ADR-074 fixture deserializes into the expected variant + fields
//!   AC8 — types live in the protocol crate (no wiring outside it — verified by
//!         tests compiling inside sidequest-protocol's own test module)

use super::*;

// ============================================================================
// AC1 + AC2: GameMessage variants exist — construct each of the three
// ============================================================================

#[test]
fn dice_request_variant_exists() {
    let msg = GameMessage::DiceRequest {
        payload: DiceRequestPayload {
            request_id: "req-1".to_string(),
            player_id: "p1".to_string(),
            character_name: "Kira".to_string(),
            dice: vec![DieSpec {
                sides: 20,
                count: 1,
            }],
            modifier: 3,
            stat: "dexterity".to_string(),
            difficulty: 15,
            context: "The ancient lock yields to skilled hands.".to_string(),
        },
        player_id: "server".to_string(),
    };
    assert!(matches!(msg, GameMessage::DiceRequest { .. }));
}

#[test]
fn dice_throw_variant_exists() {
    let msg = GameMessage::DiceThrow {
        payload: DiceThrowPayload {
            request_id: "req-1".to_string(),
            throw_params: ThrowParams {
                velocity: [1.0, 2.0, -3.0],
                angular: [0.5, -0.25, 1.5],
                position: [0.5, 0.5],
            },
        },
        player_id: "p1".to_string(),
    };
    assert!(matches!(msg, GameMessage::DiceThrow { .. }));
}

#[test]
fn dice_result_variant_exists() {
    let msg = GameMessage::DiceResult {
        payload: DiceResultPayload {
            request_id: "req-1".to_string(),
            player_id: "p1".to_string(),
            character_name: "Kira".to_string(),
            rolls: vec![17],
            modifier: 3,
            total: 20,
            difficulty: 15,
            outcome: RollOutcome::Success,
            seed: 0xDEADBEEF_u64,
            throw_params: ThrowParams {
                velocity: [1.0, 2.0, -3.0],
                angular: [0.5, -0.25, 1.5],
                position: [0.5, 0.5],
            },
        },
        player_id: "server".to_string(),
    };
    assert!(matches!(msg, GameMessage::DiceResult { .. }));
}

// ============================================================================
// AC1: Supporting struct field coverage — DiceRequestPayload
// ============================================================================

#[test]
fn dice_request_payload_has_all_adr_074_fields() {
    let payload = DiceRequestPayload {
        request_id: "req-1".to_string(),
        player_id: "p1".to_string(),
        character_name: "Kira".to_string(),
        dice: vec![DieSpec {
            sides: 20,
            count: 1,
        }],
        modifier: 3,
        stat: "dexterity".to_string(),
        difficulty: 15,
        context: "Pick the lock".to_string(),
    };
    // Not vacuous: verifies every ADR-074 field is present with the correct type
    assert_eq!(payload.request_id, "req-1");
    assert_eq!(payload.player_id, "p1");
    assert_eq!(payload.character_name, "Kira");
    assert_eq!(payload.dice.len(), 1);
    assert_eq!(payload.dice[0].sides, 20);
    assert_eq!(payload.dice[0].count, 1);
    assert_eq!(payload.modifier, 3);
    assert_eq!(payload.stat, "dexterity");
    assert_eq!(payload.difficulty, 15);
    assert_eq!(payload.context, "Pick the lock");
}

#[test]
fn dice_request_supports_negative_modifier() {
    // ADR-074: `modifier: i32` — must accept penalties, not just bonuses
    let payload = DiceRequestPayload {
        request_id: "req-2".to_string(),
        player_id: "p1".to_string(),
        character_name: "Kira".to_string(),
        dice: vec![DieSpec {
            sides: 20,
            count: 1,
        }],
        modifier: -2,
        stat: "strength".to_string(),
        difficulty: 12,
        context: "Wounded, she heaves the door.".to_string(),
    };
    assert_eq!(payload.modifier, -2);
}

#[test]
fn dice_request_supports_pool_of_dice() {
    // ADR-074: "Dice pools — 4d6, 2d10 — thrown together in one gesture."
    let payload = DiceRequestPayload {
        request_id: "req-3".to_string(),
        player_id: "p1".to_string(),
        character_name: "Kira".to_string(),
        dice: vec![
            DieSpec {
                sides: 6,
                count: 4,
            },
            DieSpec {
                sides: 10,
                count: 2,
            },
        ],
        modifier: 0,
        stat: "constitution".to_string(),
        difficulty: 18,
        context: "Channel raw power.".to_string(),
    };
    assert_eq!(payload.dice.len(), 2);
    assert_eq!(payload.dice[0].count, 4);
    assert_eq!(payload.dice[1].sides, 10);
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
            velocity: [0.0; 3],
            angular: [0.0; 3],
            position: [0.5, 0.5],
        },
    };
    assert_eq!(payload.request_id, "req-1");
    assert_eq!(payload.throw_params.velocity, [0.0, 0.0, 0.0]);
}

// ============================================================================
// AC1 + AC5: RollOutcome — all four variants
// ============================================================================

#[test]
fn roll_outcome_crit_success_variant() {
    let o = RollOutcome::CritSuccess;
    assert!(matches!(o, RollOutcome::CritSuccess));
}

#[test]
fn roll_outcome_success_variant() {
    let o = RollOutcome::Success;
    assert!(matches!(o, RollOutcome::Success));
}

#[test]
fn roll_outcome_fail_variant() {
    let o = RollOutcome::Fail;
    assert!(matches!(o, RollOutcome::Fail));
}

#[test]
fn roll_outcome_crit_fail_variant() {
    let o = RollOutcome::CritFail;
    assert!(matches!(o, RollOutcome::CritFail));
}

#[test]
fn roll_outcome_all_four_variants_round_trip() {
    // Story 34-2 AC: "All four variants (CritSuccess, Success, Fail, CritFail)
    // defined and tested." Verifies serde preserves the variant identity exactly —
    // a critical property because RollOutcome feeds the narrator tone (ADR-074
    // consequence: crit fail must be distinguishable from plain fail on the wire).
    for outcome in [
        RollOutcome::CritSuccess,
        RollOutcome::Success,
        RollOutcome::Fail,
        RollOutcome::CritFail,
    ] {
        let json = serde_json::to_string(&outcome).expect("serialize RollOutcome");
        let restored: RollOutcome =
            serde_json::from_str(&json).expect("deserialize RollOutcome");
        assert_eq!(
            std::mem::discriminant(&outcome),
            std::mem::discriminant(&restored),
            "RollOutcome variant must survive round-trip: {json}"
        );
    }
}

// ============================================================================
// AC3: Serde round-trip — every field preserved for each of the three messages
// ============================================================================

#[test]
fn dice_request_serde_round_trip_preserves_every_field() {
    let msg = GameMessage::DiceRequest {
        payload: DiceRequestPayload {
            request_id: "req-42".to_string(),
            player_id: "kira".to_string(),
            character_name: "Kira the Sly".to_string(),
            dice: vec![DieSpec {
                sides: 20,
                count: 1,
            }],
            modifier: 3,
            stat: "dexterity".to_string(),
            difficulty: 15,
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
            assert_eq!(payload.player_id, "kira");
            assert_eq!(payload.character_name, "Kira the Sly");
            assert_eq!(payload.dice.len(), 1);
            assert_eq!(payload.dice[0].sides, 20);
            assert_eq!(payload.dice[0].count, 1);
            assert_eq!(payload.modifier, 3);
            assert_eq!(payload.stat, "dexterity");
            assert_eq!(payload.difficulty, 15);
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
        }
        _ => panic!("expected DiceThrow variant"),
    }
}

#[test]
fn dice_result_serde_round_trip_preserves_every_field() {
    let msg = GameMessage::DiceResult {
        payload: DiceResultPayload {
            request_id: "req-42".to_string(),
            player_id: "kira".to_string(),
            character_name: "Kira the Sly".to_string(),
            rolls: vec![17],
            modifier: 3,
            total: 20,
            difficulty: 15,
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
            assert_eq!(payload.player_id, "kira");
            assert_eq!(payload.character_name, "Kira the Sly");
            assert_eq!(payload.rolls, vec![17]);
            assert_eq!(payload.modifier, 3);
            assert_eq!(payload.total, 20);
            assert_eq!(payload.difficulty, 15);
            assert!(matches!(payload.outcome, RollOutcome::Success));
            assert_eq!(payload.seed, 0x1234_5678_9ABC_DEF0_u64);
            assert_eq!(payload.throw_params.velocity, [1.0, 2.5, -0.75]);
        }
        _ => panic!("expected DiceResult variant"),
    }
}

#[test]
fn dice_result_round_trip_with_pool_rolls() {
    // Multiple dice in pool: verify Vec<u32> survives intact, not truncated.
    let msg = GameMessage::DiceResult {
        payload: DiceResultPayload {
            request_id: "req-pool".to_string(),
            player_id: "kira".to_string(),
            character_name: "Kira".to_string(),
            rolls: vec![3, 5, 2, 6], // 4d6
            modifier: 0,
            total: 16,
            difficulty: 12,
            outcome: RollOutcome::Success,
            seed: 42,
            throw_params: ThrowParams {
                velocity: [0.0; 3],
                angular: [0.0; 3],
                position: [0.5, 0.5],
            },
        },
        player_id: "server".to_string(),
    };
    let json = serde_json::to_string(&msg).expect("serialize pool DiceResult");
    let restored: GameMessage =
        serde_json::from_str(&json).expect("deserialize pool DiceResult");
    if let GameMessage::DiceResult { payload, .. } = restored {
        assert_eq!(payload.rolls, vec![3, 5, 2, 6]);
        assert_eq!(payload.total, 16);
    } else {
        panic!("expected DiceResult variant");
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
            player_id: "p1".to_string(),
            character_name: "Kira".to_string(),
            dice: vec![DieSpec {
                sides: 20,
                count: 1,
            }],
            modifier: 0,
            stat: "wisdom".to_string(),
            difficulty: 10,
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
            throw_params: ThrowParams {
                velocity: [0.0; 3],
                angular: [0.0; 3],
                position: [0.5, 0.5],
            },
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
            player_id: "p1".to_string(),
            character_name: "Kira".to_string(),
            rolls: vec![1],
            modifier: 0,
            total: 1,
            difficulty: 10,
            outcome: RollOutcome::CritFail,
            seed: 0,
            throw_params: ThrowParams {
                velocity: [0.0; 3],
                angular: [0.0; 3],
                position: [0.5, 0.5],
            },
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
    // Matches the existing crate convention: all payloads use
    // `#[serde(deny_unknown_fields)]` so that typos or drift in the UI client
    // fail loudly rather than silently dropping data.
    let bad_json = r#"{
        "request_id": "req-1",
        "player_id": "p1",
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
        "player_id": "p1",
        "character_name": "Kira",
        "rolls": [17],
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
// AC7: ADR-074 wire fixture — full JSON round-trip matching the spec exactly
// ============================================================================

#[test]
fn dice_request_deserializes_from_adr_074_fixture() {
    // This fixture matches the exact field set from ADR-074's DiceRequest example.
    // If Dev drifts from the ADR schema, this test fails loudly.
    let json = r#"{
        "type": "DICE_REQUEST",
        "payload": {
            "request_id": "req-abc",
            "player_id": "kira",
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
            assert_eq!(payload.player_id, "kira");
            assert_eq!(payload.character_name, "Kira the Sly");
            assert_eq!(payload.dice.len(), 1);
            assert_eq!(payload.dice[0].sides, 20);
            assert_eq!(payload.modifier, 3);
            assert_eq!(payload.stat, "dexterity");
            assert_eq!(payload.difficulty, 15);
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
            "player_id": "kira",
            "character_name": "Kira the Sly",
            "rolls": [20],
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
    if let GameMessage::DiceResult { payload, .. } = msg {
        assert_eq!(payload.rolls, vec![20]);
        assert_eq!(payload.total, 23);
        assert!(
            matches!(payload.outcome, RollOutcome::CritSuccess),
            "nat 20 fixture must deserialize to CritSuccess outcome"
        );
        assert_eq!(payload.seed, 42);
    } else {
        panic!("expected DiceResult variant from CritSuccess fixture");
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
            "player_id": "kira",
            "character_name": "Kira the Sly",
            "rolls": [1],
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
    if let GameMessage::DiceResult { payload, .. } = msg {
        assert_eq!(payload.rolls, vec![1]);
        assert!(
            matches!(payload.outcome, RollOutcome::CritFail),
            "nat 1 fixture must deserialize to CritFail — not plain Fail"
        );
    } else {
        panic!("expected DiceResult variant from CritFail fixture");
    }
}

// ============================================================================
// AC3: DieSpec round-trip — serializes as object with sides + count
// ============================================================================

#[test]
fn die_spec_serde_round_trip() {
    let spec = DieSpec {
        sides: 20,
        count: 1,
    };
    let json = serde_json::to_string(&spec).expect("serialize DieSpec");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["sides"], 20);
    assert_eq!(v["count"], 1);
    let restored: DieSpec = serde_json::from_str(&json).expect("deserialize DieSpec");
    assert_eq!(restored.sides, 20);
    assert_eq!(restored.count, 1);
}

#[test]
fn die_spec_handles_common_tabletop_sides() {
    // ADR-074: "4, 6, 8, 10, 12, 20, 100"
    for sides in [4_u32, 6, 8, 10, 12, 20, 100] {
        let spec = DieSpec { sides, count: 1 };
        let json = serde_json::to_string(&spec).expect("serialize");
        let restored: DieSpec = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.sides, sides, "d{sides} must round-trip");
    }
}

// ============================================================================
// AC8: Wiring — these types are in the protocol crate and re-exported at root
// ============================================================================

#[test]
fn dice_types_reachable_via_crate_root() {
    // This test exists because lib.rs does `pub use message::*`. If a new type
    // is gated behind a non-re-exported module, this test won't compile.
    // The `use super::*` at the top of this file goes through lib.rs's re-exports,
    // so this is a compile-time wiring check.
    let _spec: DieSpec = DieSpec {
        sides: 20,
        count: 1,
    };
    let _tp: ThrowParams = ThrowParams {
        velocity: [0.0; 3],
        angular: [0.0; 3],
        position: [0.5, 0.5],
    };
    let _outcome: RollOutcome = RollOutcome::Success;
    // Meaningful assertion: the variant we constructed is the one we expected.
    assert!(matches!(_outcome, RollOutcome::Success));
}

//! Wiring test for story 34-3: proves sidequest-server can reach
//! sidequest_game::dice::resolve_dice and compose a DiceResultPayload
//! from the resolved roll + echo fields from DiceRequestPayload.
//!
//! This is NOT a unit test for the resolver (those live in sidequest-game).
//! This is a reachability test: server crate → game crate → protocol crate,
//! round-tripping through serde to prove the types are compatible.

use std::num::{NonZeroU32, NonZeroU8};

use sidequest_game::dice::resolve_dice;
use sidequest_protocol::{
    DiceRequestPayload, DiceResultPayload, DieGroupResult, DieSpec, DieSides, RollOutcome,
    ThrowParams,
};

/// Construct a valid DiceRequestPayload, feed its fields to resolve_dice,
/// compose a DiceResultPayload from the result, and round-trip through serde.
#[test]
fn server_can_resolve_dice_and_compose_result_payload() {
    // --- 1. Build a DiceRequestPayload (as the dispatch layer would) ---
    let request = DiceRequestPayload {
        request_id: "test-req-001".to_string(),
        rolling_player_id: "player-1".to_string(),
        character_name: "Grimjaw".to_string(),
        dice: vec![DieSpec {
            sides: DieSides::D20,
            count: NonZeroU8::new(1).unwrap(),
        }],
        modifier: 3,
        stat: "strength".to_string(),
        difficulty: NonZeroU32::new(15).unwrap(),
        context: "You attempt to force open the rusted gate.".to_string(),
    };

    // --- 2. Call resolve_dice (the game-crate function) ---
    let seed = 42u64;
    let resolved = resolve_dice(&request.dice, request.modifier, request.difficulty, seed)
        .expect("resolve_dice should succeed for valid d20 input");

    // --- 3. Verify resolved output is sane ---
    assert_eq!(resolved.rolls.len(), 1, "Single d20 → one group");
    assert_eq!(resolved.rolls[0].faces.len(), 1, "1d20 → one face");
    let face = resolved.rolls[0].faces[0];
    assert!(
        (1..=20).contains(&face),
        "d20 face {face} out of range"
    );
    assert!(
        !matches!(resolved.outcome, RollOutcome::Unknown),
        "outcome must never be Unknown"
    );

    // --- 4. Compose DiceResultPayload (as dispatch would) ---
    let throw_params = ThrowParams {
        velocity: [1.0, 2.0, 0.5],
        angular: [0.1, 0.3, 0.2],
        position: [0.5, 0.5],
    };

    let result_payload = DiceResultPayload {
        request_id: request.request_id.clone(),
        rolling_player_id: request.rolling_player_id.clone(),
        character_name: request.character_name.clone(),
        rolls: resolved.rolls,
        modifier: request.modifier,
        total: resolved.total,
        difficulty: request.difficulty,
        outcome: resolved.outcome,
        seed,
        throw_params,
    };

    // --- 5. Round-trip through serde (proves wire compatibility) ---
    let json = serde_json::to_string(&result_payload)
        .expect("DiceResultPayload should serialize");
    let deserialized: DiceResultPayload = serde_json::from_str(&json)
        .expect("DiceResultPayload should round-trip through serde");

    assert_eq!(deserialized.request_id, "test-req-001");
    assert_eq!(deserialized.total, result_payload.total);
    assert_eq!(deserialized.outcome, result_payload.outcome);
    assert_eq!(deserialized.seed, 42);
}

/// Verify that the resolver's error types are usable from the server crate.
#[test]
fn server_can_handle_resolve_errors() {
    use sidequest_game::dice::ResolveError;

    // Empty pool
    let empty_result = resolve_dice(&[], 0, NonZeroU32::new(10).unwrap(), 1);
    assert!(matches!(empty_result, Err(ResolveError::EmptyPool)));

    // Unknown die
    let unknown_die = DieSpec {
        sides: DieSides::Unknown,
        count: NonZeroU8::new(1).unwrap(),
    };
    let unknown_result = resolve_dice(&[unknown_die], 0, NonZeroU32::new(10).unwrap(), 1);
    assert!(matches!(unknown_result, Err(ResolveError::UnknownDie)));
}

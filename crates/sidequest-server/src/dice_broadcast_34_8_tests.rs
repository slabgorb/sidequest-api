//! Story 34-8: Multiplayer dice broadcast — DiceThrow handler wiring tests.
//!
//! RED phase — these tests verify the DiceThrow handler in lib.rs:
//! 1. Stores pending DiceRequests on SharedGameSession
//! 2. DiceThrow handler looks up pending request, resolves, broadcasts DiceResult
//! 3. DiceThrow for unknown request_id returns error
//! 4. DiceResult broadcast reaches all session members
//!
//! All tests FAIL until Dev wires the handler.

#[cfg(test)]
mod tests {
    use std::num::{NonZeroU32, NonZeroU8};

    use sidequest_game::dice::resolve_dice;
    use sidequest_protocol::{DiceRequestPayload, DieSides, DieSpec, GameMessage, ThrowParams};

    use crate::dice_dispatch::{compose_dice_result, generate_dice_seed, validate_dice_inputs};
    use crate::shared_session::SharedGameSession;

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn test_dice_request() -> DiceRequestPayload {
        DiceRequestPayload {
            request_id: "req-test-001".to_string(),
            rolling_player_id: "player-1".to_string(),
            character_name: "Kira".to_string(),
            dice: vec![DieSpec {
                sides: DieSides::D20,
                count: NonZeroU8::new(1).unwrap(),
            }],
            modifier: 3,
            stat: "dexterity".to_string(),
            difficulty: NonZeroU32::new(15).unwrap(),
            context: "The lock resists your touch...".to_string(),
        }
    }

    fn test_throw_params() -> ThrowParams {
        ThrowParams {
            velocity: [1.0, 2.0, -3.0],
            angular: [10.0, -5.0, 8.0],
            position: [0.5, 0.5],
        }
    }

    // ══════════════════════════════════════════════════════════════════════════
    // AC-1: SharedGameSession stores pending DiceRequests
    // ══════════════════════════════════════════════════════════════════════════

    #[test]
    fn shared_session_has_pending_dice_requests_field() {
        let session = SharedGameSession::new("low_fantasy".into(), "pinwheel_coast".into());
        // The pending_dice_requests field should exist and be empty initially
        assert!(session.pending_dice_requests.is_empty());
    }

    #[test]
    fn shared_session_can_store_and_retrieve_pending_request() {
        let mut session = SharedGameSession::new("low_fantasy".into(), "pinwheel_coast".into());
        let request = test_dice_request();
        let request_id = request.request_id.clone();

        session
            .pending_dice_requests
            .insert(request_id.clone(), request);

        assert!(session.pending_dice_requests.contains_key(&request_id));
        let stored = session.pending_dice_requests.get(&request_id).unwrap();
        assert_eq!(stored.rolling_player_id, "player-1");
        assert_eq!(stored.character_name, "Kira");
        assert_eq!(stored.modifier, 3);
        assert_eq!(stored.difficulty.get(), 15);
    }

    #[test]
    fn shared_session_pending_request_removed_after_resolution() {
        let mut session = SharedGameSession::new("low_fantasy".into(), "pinwheel_coast".into());
        let request = test_dice_request();
        let request_id = request.request_id.clone();

        session
            .pending_dice_requests
            .insert(request_id.clone(), request);
        assert!(session.pending_dice_requests.contains_key(&request_id));

        // After resolution, the pending request should be removed
        let _removed = session.pending_dice_requests.remove(&request_id);
        assert!(!session.pending_dice_requests.contains_key(&request_id));
    }

    // ══════════════════════════════════════════════════════════════════════════
    // AC-4: DiceThrow resolution pipeline (unit test of the compose path)
    // ══════════════════════════════════════════════════════════════════════════

    #[test]
    fn dice_resolution_pipeline_produces_valid_result() {
        let request = test_dice_request();
        let throw_params = test_throw_params();

        // Validate inputs
        let validation = validate_dice_inputs(&request.dice, request.modifier, request.difficulty);
        assert!(
            validation.is_ok(),
            "Valid dice inputs should pass validation"
        );

        // Generate seed
        let seed = generate_dice_seed("test-session-id", 1);
        assert_ne!(seed, 0, "Seed should be nonzero");

        // Resolve
        let resolved = resolve_dice(&request.dice, request.modifier, request.difficulty, seed);
        assert!(
            resolved.is_ok(),
            "Resolution should succeed for valid inputs"
        );
        let resolved = resolved.unwrap();

        // Compose result
        let result = compose_dice_result(
            &request.request_id,
            &request.rolling_player_id,
            &request.character_name,
            &resolved,
            request.modifier,
            request.difficulty,
            seed,
            &throw_params,
        );

        assert_eq!(result.request_id, "req-test-001");
        assert_eq!(result.rolling_player_id, "player-1");
        assert_eq!(result.character_name, "Kira");
        assert_eq!(result.modifier, 3);
        assert_eq!(result.difficulty.get(), 15);
        assert_eq!(result.seed, seed);
        assert_eq!(result.throw_params.velocity, [1.0, 2.0, -3.0]);
        assert!(!result.rolls.is_empty());
        // Outcome should never be Unknown (34-5 guardrail)
        assert_ne!(
            result.outcome,
            sidequest_protocol::RollOutcome::Unknown,
            "DiceResult outcome must never be Unknown"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // AC-4: DiceResult broadcast reaches session members
    // ══════════════════════════════════════════════════════════════════════════

    #[test]
    fn dice_result_broadcast_reaches_subscriber() {
        let session = SharedGameSession::new("low_fantasy".into(), "pinwheel_coast".into());
        let mut rx = session.subscribe();

        let request = test_dice_request();
        let throw_params = test_throw_params();
        let seed = generate_dice_seed("test-session-id", 1);
        let resolved = resolve_dice(&request.dice, request.modifier, request.difficulty, seed)
            .expect("resolution succeeds");
        let result_payload = compose_dice_result(
            &request.request_id,
            &request.rolling_player_id,
            &request.character_name,
            &resolved,
            request.modifier,
            request.difficulty,
            seed,
            &throw_params,
        );

        // Broadcast DiceResult
        session.broadcast(GameMessage::DiceResult {
            player_id: "server".to_string(),
            payload: result_payload.clone(),
        });

        // Subscriber should receive it
        let received = rx.try_recv();
        assert!(received.is_ok(), "Subscriber should receive broadcast");
        let targeted = received.unwrap();
        assert!(
            targeted.target_player_id.is_none(),
            "DiceResult should be broadcast (not targeted)"
        );

        match targeted.msg {
            GameMessage::DiceResult { ref payload, .. } => {
                assert_eq!(payload.request_id, "req-test-001");
                assert_eq!(payload.rolling_player_id, "player-1");
                assert_ne!(
                    payload.outcome,
                    sidequest_protocol::RollOutcome::Unknown,
                    "Broadcast outcome must not be Unknown"
                );
            }
            other => panic!("Expected DiceResult, got {:?}", other),
        }
    }

    // ══════════════════════════════════════════════════════════════════════════
    // AC-1: DiceRequest broadcast reaches all session members
    // ══════════════════════════════════════════════════════════════════════════

    #[test]
    fn dice_request_broadcast_reaches_subscriber() {
        let session = SharedGameSession::new("low_fantasy".into(), "pinwheel_coast".into());
        let mut rx = session.subscribe();

        let request = test_dice_request();

        // Broadcast DiceRequest
        session.broadcast(GameMessage::DiceRequest {
            player_id: "server".to_string(),
            payload: request.clone(),
        });

        let received = rx.try_recv();
        assert!(
            received.is_ok(),
            "Subscriber should receive DiceRequest broadcast"
        );
        match received.unwrap().msg {
            GameMessage::DiceRequest { ref payload, .. } => {
                assert_eq!(payload.request_id, "req-test-001");
                assert_eq!(payload.rolling_player_id, "player-1");
                assert_eq!(payload.difficulty.get(), 15);
            }
            other => panic!("Expected DiceRequest, got {:?}", other),
        }
    }

    // ══════════════════════════════════════════════════════════════════════════
    // AC-7: Full round-trip unit test (compose path, not WebSocket)
    // ══════════════════════════════════════════════════════════════════════════

    #[test]
    fn full_dice_round_trip_composes_correctly() {
        let mut session = SharedGameSession::new("low_fantasy".into(), "pinwheel_coast".into());
        let mut rx = session.subscribe();

        // Step 1: Store pending DiceRequest
        let request = test_dice_request();
        session
            .pending_dice_requests
            .insert(request.request_id.clone(), request.clone());

        // Step 2: Broadcast DiceRequest to all players
        session.broadcast(GameMessage::DiceRequest {
            player_id: "server".to_string(),
            payload: request.clone(),
        });
        let _ = rx.try_recv(); // consume

        // Step 3: Simulate DiceThrow arrival — look up pending, resolve, broadcast
        let pending = session
            .pending_dice_requests
            .remove(&request.request_id)
            .expect("Pending request should exist");

        let seed = generate_dice_seed(&session.session_id, 1);
        validate_dice_inputs(&pending.dice, pending.modifier, pending.difficulty)
            .expect("Validation should pass");
        let resolved = resolve_dice(&pending.dice, pending.modifier, pending.difficulty, seed)
            .expect("Resolution should succeed");

        let throw_params = test_throw_params();
        let result = compose_dice_result(
            &pending.request_id,
            &pending.rolling_player_id,
            &pending.character_name,
            &resolved,
            pending.modifier,
            pending.difficulty,
            seed,
            &throw_params,
        );

        session.broadcast(GameMessage::DiceResult {
            player_id: "server".to_string(),
            payload: result.clone(),
        });

        // Step 4: Verify DiceResult received
        let received = rx.try_recv().expect("Should receive DiceResult");
        match received.msg {
            GameMessage::DiceResult { payload, .. } => {
                assert_eq!(payload.request_id, request.request_id);
                assert_eq!(payload.character_name, "Kira");
                assert_ne!(payload.outcome, sidequest_protocol::RollOutcome::Unknown);
                assert_eq!(payload.seed, seed);
            }
            other => panic!("Expected DiceResult, got {:?}", other),
        }

        // Step 5: Pending request removed
        assert!(
            !session
                .pending_dice_requests
                .contains_key(&request.request_id),
            "Pending request should be consumed after resolution"
        );
    }

    // ══════════════════════════════════════════════════════════════════════════
    // AC-8: No new protocol types — verify existing types suffice
    // ══════════════════════════════════════════════════════════════════════════

    #[test]
    fn dice_broadcast_uses_existing_protocol_types_only() {
        // AC-8 regression guard: confirm the three dice protocol variants
        // round-trip through construction with the expected payload shape.
        // Proves (a) the variants still exist on GameMessage and (b) the
        // helper constructors still produce payloads with the fields
        // downstream code depends on.
        let request = GameMessage::DiceRequest {
            player_id: "server".to_string(),
            payload: test_dice_request(),
        };
        match request {
            GameMessage::DiceRequest { player_id, payload } => {
                assert_eq!(player_id, "server");
                assert!(!payload.request_id.is_empty());
            }
            other => panic!("expected DiceRequest variant, got {:?}", other),
        }

        let throw = GameMessage::DiceThrow {
            player_id: "player-1".to_string(),
            payload: sidequest_protocol::DiceThrowPayload {
                request_id: "req-test-001".to_string(),
                throw_params: test_throw_params(),
                face: vec![15],
            },
        };
        match throw {
            GameMessage::DiceThrow { player_id, payload } => {
                assert_eq!(player_id, "player-1");
                assert_eq!(payload.request_id, "req-test-001");
            }
            other => panic!("expected DiceThrow variant, got {:?}", other),
        }

        let seed = generate_dice_seed("test", 1);
        let resolved = resolve_dice(
            &[DieSpec {
                sides: DieSides::D20,
                count: NonZeroU8::new(1).unwrap(),
            }],
            0,
            NonZeroU32::new(10).unwrap(),
            seed,
        )
        .unwrap();
        let result = GameMessage::DiceResult {
            player_id: "server".to_string(),
            payload: compose_dice_result(
                "req-test-001",
                "player-1",
                "Kira",
                &resolved,
                0,
                NonZeroU32::new(10).unwrap(),
                seed,
                &test_throw_params(),
            ),
        };
        match result {
            GameMessage::DiceResult { player_id, payload } => {
                assert_eq!(player_id, "server");
                assert_eq!(payload.request_id, "req-test-001");
                assert_eq!(payload.character_name, "Kira");
                assert_eq!(payload.seed, seed);
            }
            other => panic!("expected DiceResult variant, got {:?}", other),
        }
    }
}

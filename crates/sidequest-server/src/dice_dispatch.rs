//! Dice dispatch integration layer.
//!
//! Hosts the end-to-end `handle_dice_throw` async orchestrator (story 34-12,
//! physics-is-the-roll) plus the supporting pure helpers it composes:
//! dispatch-boundary validation (`validate_dice_inputs`), deterministic seed
//! generation for spectator replay (`generate_dice_seed`), and
//! `DiceResultPayload` composition from a `ResolvedRoll`. `lib.rs` delegates
//! the `DiceThrow` arm of `dispatch_message` here; integration tests can
//! drive `handle_dice_throw` directly against a real `SharedGameSession`
//! without rebuilding the full per-connection dispatch parameter set.

use std::num::NonZeroU32;
use std::sync::Arc;

use sidequest_game::dice::ResolvedRoll;
use sidequest_protocol::{
    DiceResultPayload, DiceThrowPayload, DieSides, DieSpec, GameMessage, ThrowParams,
};
use tokio::sync::Mutex;

use crate::shared_session::SharedGameSession;

/// Maximum DC value the dispatch layer accepts.
const MAX_DC: u32 = 100;

/// Maximum modifier magnitude the dispatch layer accepts.
const MAX_MODIFIER: i32 = 100;

/// Maximum number of die groups in a pool.
const MAX_POOL_GROUPS: usize = 10;

/// Dispatch-boundary validation errors for dice inputs.
///
/// These are checked before calling `resolve_dice`, preventing edge cases
/// in the pure resolver (34-3 delivery findings: DC truncation, modifier
/// overflow, pool count).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DiceInputError {
    /// Dice pool was empty.
    EmptyPool,
    /// A die in the pool has `DieSides::Unknown`.
    UnknownDie,
    /// DC is outside the game range (1..=100).
    DcOutOfRange { value: u32 },
    /// Modifier magnitude exceeds game range (-100..=100).
    ModifierOutOfRange { value: i32 },
    /// Too many die groups in the pool (> 10).
    PoolTooLarge { count: usize },
}

impl std::fmt::Display for DiceInputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyPool => write!(f, "dice pool is empty"),
            Self::UnknownDie => write!(f, "dice pool contains an unknown die type"),
            Self::DcOutOfRange { value } => {
                write!(f, "DC {value} is outside valid range (1..={MAX_DC})")
            }
            Self::ModifierOutOfRange { value } => {
                write!(
                    f,
                    "modifier {value} is outside valid range (-{MAX_MODIFIER}..={MAX_MODIFIER})"
                )
            }
            Self::PoolTooLarge { count } => {
                write!(f, "pool has {count} groups, maximum is {MAX_POOL_GROUPS}")
            }
        }
    }
}

impl std::error::Error for DiceInputError {}

/// Validate dice inputs at the dispatch boundary before calling `resolve_dice`.
///
/// Catches edge cases that the pure resolver doesn't guard against:
/// - Empty pool (also caught by resolver, but fail early here)
/// - Unknown die sides (also caught by resolver)
/// - DC > 100 (prevents `difficulty.get() as i32` truncation at extreme values)
/// - Modifier magnitude > 100 (prevents `face_sum + modifier` overflow)
/// - Pool group count > 10 (prevents unbounded allocation)
pub fn validate_dice_inputs(
    dice: &[DieSpec],
    modifier: i32,
    difficulty: NonZeroU32,
) -> Result<(), DiceInputError> {
    if dice.is_empty() {
        return Err(DiceInputError::EmptyPool);
    }
    for spec in dice {
        if spec.sides == DieSides::Unknown {
            return Err(DiceInputError::UnknownDie);
        }
    }
    if difficulty.get() > MAX_DC {
        return Err(DiceInputError::DcOutOfRange {
            value: difficulty.get(),
        });
    }
    if !(-MAX_MODIFIER..=MAX_MODIFIER).contains(&modifier) {
        return Err(DiceInputError::ModifierOutOfRange { value: modifier });
    }
    if dice.len() > MAX_POOL_GROUPS {
        return Err(DiceInputError::PoolTooLarge { count: dice.len() });
    }
    Ok(())
}

/// Generate a deterministic dice seed from session identity and turn number.
///
/// Uses FNV-1a (stable across Rust versions and platforms) — NOT DefaultHasher,
/// which is SipHash and explicitly documented as non-stable across releases.
/// This is a game-mechanical determinism contract: same (session_id, turn)
/// must always produce the same seed, even after Rust version upgrades.
///
/// No OS entropy, no client influence.
pub fn generate_dice_seed(session_id: &str, turn: u32) -> u64 {
    // FNV-1a: stable, well-specified, no dependencies
    const FNV_OFFSET: u64 = 14695981039346656037;
    const FNV_PRIME: u64 = 1099511628211;

    let mut h = FNV_OFFSET;
    for &b in session_id.as_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(FNV_PRIME);
    }
    for &b in turn.to_le_bytes().iter() {
        h ^= b as u64;
        h = h.wrapping_mul(FNV_PRIME);
    }
    // Ensure nonzero
    if h == 0 {
        1
    } else {
        h
    }
}

/// Type alias for the holder of a shared game session (double-Mutex wrapping
/// to match the dispatch layer's ownership model).
pub type SharedSessionHolder = Arc<Mutex<Option<Arc<Mutex<SharedGameSession>>>>>;

/// Error cases that bubble out of `handle_dice_throw` as wire ERROR messages.
#[derive(Debug)]
enum DiceThrowError {
    NoSharedSession,
    NoPendingRequest(String),
    ValidationFailed(String),
    ResolutionFailed(String),
}

impl DiceThrowError {
    fn into_wire_message(self, player_id: &str) -> GameMessage {
        let text = match self {
            Self::NoSharedSession => "No active game session for dice resolution".to_string(),
            Self::NoPendingRequest(req_id) => {
                format!("No pending dice request for request_id '{req_id}'")
            }
            Self::ValidationFailed(e) => format!("Dice validation failed: {e}"),
            Self::ResolutionFailed(e) => format!("Dice resolution failed: {e}"),
        };
        GameMessage::Error {
            payload: sidequest_protocol::ErrorPayload {
                message: text,
                reconnect_required: None,
            },
            player_id: player_id.to_string(),
        }
    }
}

/// Dispatch a `DiceThrow` wire message end-to-end: look up the pending
/// `DiceRequest`, validate inputs, resolve via client-reported faces
/// (physics-is-the-roll — story 34-12), broadcast the `DiceResult`, persist
/// the outcome into `pending_roll_outcome` for the next narration turn
/// (story 34-9), and return the direct response message.
///
/// This function is extracted from `dispatch_message`'s `DiceThrow` arm so
/// it can be driven directly from integration tests without rebuilding the
/// entire per-connection dispatch parameter set. The real dispatch arm
/// checks `session.is_playing()` before calling this; callers in tests
/// should replicate that precondition if they want to exercise it.
///
/// On success returns a single-element Vec containing the `DiceResult`
/// message. On failure returns a single-element Vec containing an `Error`
/// message matching the original dispatch behaviour.
pub async fn handle_dice_throw(
    payload: DiceThrowPayload,
    player_id: &str,
    shared_session_holder: &SharedSessionHolder,
    round: u32,
) -> Vec<GameMessage> {
    match handle_dice_throw_inner(payload, player_id, shared_session_holder, round).await {
        Ok(msg) => vec![msg],
        Err(e) => vec![e.into_wire_message(player_id)],
    }
}

async fn handle_dice_throw_inner(
    payload: DiceThrowPayload,
    player_id: &str,
    shared_session_holder: &SharedSessionHolder,
    round: u32,
) -> Result<GameMessage, DiceThrowError> {
    // Look up and remove the pending DiceRequest.
    let pending = {
        let holder_guard = shared_session_holder.lock().await;
        let Some(ref ss_arc) = *holder_guard else {
            return Err(DiceThrowError::NoSharedSession);
        };
        let mut ss = ss_arc.lock().await;
        ss.pending_dice_requests.remove(&payload.request_id)
    };

    let Some(pending_request) = pending else {
        tracing::warn!(
            request_id = %payload.request_id,
            player_id = %player_id,
            "dice.throw_no_pending — no pending DiceRequest for this request_id"
        );
        return Err(DiceThrowError::NoPendingRequest(payload.request_id));
    };

    // Story 34-11: OTEL — dice throw received. Fired on the hot path after
    // the pending request is confirmed, so the GM panel can see every
    // DiceThrow that made it past the correlation-id guard.
    crate::emit_dice_throw_received(
        &payload.request_id,
        &pending_request.rolling_player_id,
        &payload.throw_params,
    );

    // Validate inputs at dispatch boundary. Client-submitted invalid input is
    // a 4xx-class error — `warn!` (not `error!`) per the log-level convention
    // (`error!` is reserved for 5xx-class server faults to avoid alert fatigue).
    if let Err(e) = validate_dice_inputs(
        &pending_request.dice,
        pending_request.modifier,
        pending_request.difficulty,
    ) {
        tracing::warn!(
            request_id = %payload.request_id,
            error = %e,
            "dice.validation_failed"
        );
        return Err(DiceThrowError::ValidationFailed(e.to_string()));
    }

    // Generate the deterministic seed. It drives rotation for spectator
    // replay animation only — it does NOT drive the face value on this
    // path (physics-is-the-roll, story 34-12).
    let session_id = {
        let holder_guard = shared_session_holder.lock().await;
        let Some(ref ss_arc) = *holder_guard else {
            return Err(DiceThrowError::NoSharedSession);
        };
        let ss = ss_arc.lock().await;
        ss.session_id.clone()
    };
    let seed = generate_dice_seed(&session_id, round);

    tracing::info!(
        request_id = %payload.request_id,
        rolling_player = %player_id,
        face = ?payload.face,
        "dice.face_reported"
    );

    let resolved = sidequest_game::dice::resolve_dice_with_faces(
        &pending_request.dice,
        &payload.face,
        pending_request.modifier,
        pending_request.difficulty,
    )
    .map_err(|e| {
        tracing::error!(
            request_id = %payload.request_id,
            error = %e,
            "dice.resolution_failed"
        );
        DiceThrowError::ResolutionFailed(e.to_string())
    })?;

    let result_payload = compose_dice_result(
        &pending_request.request_id,
        &pending_request.rolling_player_id,
        &pending_request.character_name,
        &resolved,
        pending_request.modifier,
        pending_request.difficulty,
        seed,
        &payload.throw_params,
    );

    tracing::info!(
        request_id = %payload.request_id,
        rolling_player = %pending_request.rolling_player_id,
        total = resolved.total,
        outcome = ?resolved.outcome,
        seed = seed,
        "dice.result_resolved"
    );

    // Story 34-11: OTEL — dice result broadcast. The GM panel's lie-detector
    // span fires once per resolved roll with the final total, outcome, and
    // seed so we can verify physics-is-the-roll end-to-end without relying
    // on narrator self-reporting.
    crate::emit_dice_result_broadcast(&result_payload, &resolved);

    // Broadcast via shared session, and persist the outcome for the next
    // narration turn (story 34-9).
    {
        let holder_guard = shared_session_holder.lock().await;
        if let Some(ref ss_arc) = *holder_guard {
            let mut ss = ss_arc.lock().await;
            ss.pending_roll_outcome = Some(resolved.outcome);
            ss.broadcast(GameMessage::DiceResult {
                player_id: "server".to_string(),
                payload: result_payload.clone(),
            });
        }
    }

    Ok(GameMessage::DiceResult {
        player_id: "server".to_string(),
        payload: result_payload,
    })
}

/// Compose a `DiceResultPayload` from a `ResolvedRoll` and echo fields.
///
/// This is the mapping from the game-crate's resolution output to the
/// wire-protocol result type. The dispatch layer calls this after
/// `resolve_dice` succeeds.
#[allow(clippy::too_many_arguments)] // 1:1 mapping of wire protocol fields
pub fn compose_dice_result(
    request_id: &str,
    rolling_player_id: &str,
    character_name: &str,
    resolved: &ResolvedRoll,
    modifier: i32,
    difficulty: NonZeroU32,
    seed: u64,
    throw_params: &ThrowParams,
) -> DiceResultPayload {
    DiceResultPayload {
        request_id: request_id.to_string(),
        rolling_player_id: rolling_player_id.to_string(),
        character_name: character_name.to_string(),
        rolls: resolved.rolls.clone(),
        modifier,
        total: resolved.total,
        difficulty,
        outcome: resolved.outcome,
        seed,
        throw_params: throw_params.clone(),
    }
}

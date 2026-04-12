//! Dice dispatch integration layer (story 34-4).
//!
//! Pure functions for dispatch boundary validation, seed generation, and
//! DiceResult composition. These are called from the dispatch pipeline
//! (`lib.rs` and `dispatch/beat.rs`) but are independently testable.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::num::NonZeroU32;

use sidequest_game::dice::ResolvedRoll;
use sidequest_protocol::{DiceResultPayload, DieSides, DieSpec, ThrowParams};

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
                write!(
                    f,
                    "pool has {count} groups, maximum is {MAX_POOL_GROUPS}"
                )
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
    if modifier > MAX_MODIFIER || modifier < -MAX_MODIFIER {
        return Err(DiceInputError::ModifierOutOfRange { value: modifier });
    }
    if dice.len() > MAX_POOL_GROUPS {
        return Err(DiceInputError::PoolTooLarge { count: dice.len() });
    }
    Ok(())
}

/// Generate a deterministic dice seed from session identity and turn number.
///
/// The seed is derived from hashing session_id + turn — no OS entropy, no
/// client influence. Two calls with the same (session_id, turn) produce
/// the same seed. Different sessions or turns produce different seeds.
pub fn generate_dice_seed(session_id: &str, turn: u32) -> u64 {
    let mut hasher = DefaultHasher::new();
    session_id.hash(&mut hasher);
    turn.hash(&mut hasher);
    // Ensure nonzero — DefaultHasher can theoretically return 0
    let h = hasher.finish();
    if h == 0 { 1 } else { h }
}

/// Compose a `DiceResultPayload` from a `ResolvedRoll` and echo fields.
///
/// This is the mapping from the game-crate's resolution output to the
/// wire-protocol result type. The dispatch layer calls this after
/// `resolve_dice` succeeds.
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

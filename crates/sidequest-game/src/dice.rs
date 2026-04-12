//! Pure-function dice resolution engine (story 34-3).
//!
//! Resolves a dice pool against a difficulty class using seeded RNG.
//! No I/O, no wall-clock time, no shared state, no OS entropy.

use std::num::NonZeroU32;

use rand::{rngs::StdRng, Rng, SeedableRng};
use sidequest_protocol::{DieGroupResult, DieSides, DieSpec, RollOutcome};

/// Result of resolving a dice pool against a DC.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedRoll {
    /// Per-group face values, paired with the originating `DieSpec`.
    pub rolls: Vec<DieGroupResult>,
    /// Sum of all face values plus modifier.
    pub total: i32,
    /// Outcome classification (never `RollOutcome::Unknown`).
    pub outcome: RollOutcome,
}

/// Errors from `resolve_dice`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResolveError {
    /// A `DieSpec` in the pool has `DieSides::Unknown`.
    UnknownDie,
    /// The dice pool was empty.
    EmptyPool,
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownDie => write!(f, "dice pool contains an unknown die type"),
            Self::EmptyPool => write!(f, "dice pool is empty"),
        }
    }
}

impl std::error::Error for ResolveError {}

/// Resolve a dice pool against a difficulty class.
///
/// Pure function: deterministic from `(dice, modifier, difficulty, seed)`.
/// Uses `StdRng::seed_from_u64` for reproducibility.
///
/// # Crit semantics (locked by Keith 2026-04-11)
///
/// - Any d20 face of 20 → `CritSuccess` (regardless of DC/modifier)
/// - Any d20 face of 1 → `CritFail` (regardless of DC/modifier)
/// - If both 20 and 1 appear in the same pool, `CritSuccess` wins
/// - Non-d20 dice never trigger crit classification
pub fn resolve_dice(
    dice: &[DieSpec],
    modifier: i32,
    difficulty: NonZeroU32,
    seed: u64,
) -> Result<ResolvedRoll, ResolveError> {
    if dice.is_empty() {
        return Err(ResolveError::EmptyPool);
    }

    // Validate all dice before rolling (fail-fast on Unknown)
    for spec in dice {
        if spec.sides.faces().is_none() {
            return Err(ResolveError::UnknownDie);
        }
    }

    let mut rng = StdRng::seed_from_u64(seed);
    let mut rolls = Vec::with_capacity(dice.len());
    let mut face_sum: i32 = 0;
    let mut has_d20_nat20 = false;
    let mut has_d20_nat1 = false;
    let mut has_d20 = false;

    for spec in dice {
        let sides = spec.sides.faces().unwrap(); // safe: validated above
        let count = spec.count.get() as usize;
        let mut faces = Vec::with_capacity(count);

        for _ in 0..count {
            let face = rng.random_range(1u32..=sides);
            faces.push(face);
            face_sum += face as i32;

            if spec.sides == DieSides::D20 {
                has_d20 = true;
                if face == 20 {
                    has_d20_nat20 = true;
                }
                if face == 1 {
                    has_d20_nat1 = true;
                }
            }
        }

        rolls.push(DieGroupResult { spec: *spec, faces });
    }

    let total = face_sum + modifier;

    let outcome = if has_d20 && has_d20_nat20 {
        // CritSuccess wins over CritFail when both present
        RollOutcome::CritSuccess
    } else if has_d20 && has_d20_nat1 {
        RollOutcome::CritFail
    } else if total >= difficulty.get() as i32 {
        RollOutcome::Success
    } else {
        RollOutcome::Fail
    };

    Ok(ResolvedRoll {
        rolls,
        total,
        outcome,
    })
}

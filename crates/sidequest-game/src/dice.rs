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

/// Errors from `resolve_dice` / `resolve_dice_with_faces`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResolveError {
    /// A `DieSpec` in the pool has `DieSides::Unknown`.
    UnknownDie,
    /// The dice pool was empty.
    EmptyPool,
    /// Client-reported face count does not match the pool's physical die count.
    FaceCountMismatch {
        /// Total die count implied by the pool (sum of `DieSpec.count`).
        expected: usize,
        /// Number of face values the client submitted.
        actual: usize,
    },
    /// A client-reported face is outside `1..=sides` for its die type.
    FaceOutOfRange {
        /// Flat-order index of the offending die within the pool.
        die_index: usize,
        /// The out-of-range face value the client reported.
        face: u32,
        /// Number of sides on the die that would have produced a valid face.
        sides: u32,
    },
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownDie => write!(f, "dice pool contains an unknown die type"),
            Self::EmptyPool => write!(f, "dice pool is empty"),
            Self::FaceCountMismatch { expected, actual } => write!(
                f,
                "client-reported face count ({actual}) does not match pool size ({expected})"
            ),
            Self::FaceOutOfRange {
                die_index,
                face,
                sides,
            } => write!(
                f,
                "die {die_index} face {face} is out of range for a d{sides}"
            ),
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

/// Resolve a dice pool using client-reported face values (physics-is-the-roll).
///
/// This is the physics-authoritative path (story 34-12). The client runs
/// Rapier locally, reads each settled die face, and submits them with
/// `DiceThrowPayload.face`. The server validates the faces against the
/// pool and computes total/crit/outcome from them — no RNG, no seed.
///
/// `faces` is flat-order across the pool: for `[{d20,1}, {d6,3}]` the
/// expected length is 4 and the order is `[d20_face, d6_a, d6_b, d6_c]`.
///
/// Crit semantics match `resolve_dice` exactly (Keith 2026-04-11):
/// - Any d20 face of 20 → `CritSuccess`
/// - Any d20 face of 1 → `CritFail`
/// - CritSuccess wins over CritFail when both present
pub fn resolve_dice_with_faces(
    dice: &[DieSpec],
    faces: &[u32],
    modifier: i32,
    difficulty: NonZeroU32,
) -> Result<ResolvedRoll, ResolveError> {
    if dice.is_empty() {
        return Err(ResolveError::EmptyPool);
    }

    for spec in dice {
        if spec.sides.faces().is_none() {
            return Err(ResolveError::UnknownDie);
        }
    }

    let expected_count: usize = dice.iter().map(|s| s.count.get() as usize).sum();
    if faces.len() != expected_count {
        return Err(ResolveError::FaceCountMismatch {
            expected: expected_count,
            actual: faces.len(),
        });
    }

    let mut rolls = Vec::with_capacity(dice.len());
    let mut face_sum: i32 = 0;
    let mut has_d20_nat20 = false;
    let mut has_d20_nat1 = false;
    let mut has_d20 = false;
    let mut flat_idx: usize = 0;

    for spec in dice {
        let sides = spec.sides.faces().unwrap();
        let count = spec.count.get() as usize;
        let mut group_faces = Vec::with_capacity(count);

        for _ in 0..count {
            let face = faces[flat_idx];
            if face < 1 || face > sides {
                return Err(ResolveError::FaceOutOfRange {
                    die_index: flat_idx,
                    face,
                    sides,
                });
            }
            group_faces.push(face);
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
            flat_idx += 1;
        }

        rolls.push(DieGroupResult {
            spec: *spec,
            faces: group_faces,
        });
    }

    let total = face_sum + modifier;

    let outcome = if has_d20 && has_d20_nat20 {
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

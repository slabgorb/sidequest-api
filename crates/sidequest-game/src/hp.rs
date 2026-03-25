//! HP clamping — single implementation, no more Python duplication bug.
//!
//! port-lessons.md #6: progression.py:59 doesn't clamp to zero.
//! This function is the ONLY place HP arithmetic should happen.

/// Clamp HP after applying a delta.
///
/// Result is always in `[0, max_hp]`. Fixes the Python bug where
/// `min(hp + delta, max_hp)` could produce negative values.
///
/// # Panics
/// Panics if `max_hp` is negative (invalid game state).
pub fn clamp_hp(current: i32, delta: i32, max_hp: i32) -> i32 {
    assert!(max_hp >= 0, "max_hp must not be negative");
    (current as i64 + delta as i64).clamp(0, max_hp as i64) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    // === Basic clamping ===

    #[test]
    fn heal_within_bounds() {
        assert_eq!(clamp_hp(5, 3, 10), 8);
    }

    #[test]
    fn damage_within_bounds() {
        assert_eq!(clamp_hp(8, -3, 10), 5);
    }

    // === Upper bound (max_hp) ===

    #[test]
    fn heal_capped_at_max() {
        // Can't exceed max_hp
        assert_eq!(clamp_hp(8, 5, 10), 10);
    }

    #[test]
    fn heal_at_max_stays_at_max() {
        assert_eq!(clamp_hp(10, 3, 10), 10);
    }

    // === Lower bound (zero floor) — the Python bug fix ===

    #[test]
    fn damage_floored_at_zero() {
        // This is the bug: Python allowed negative HP
        assert_eq!(clamp_hp(3, -10, 10), 0);
    }

    #[test]
    fn massive_damage_still_zero() {
        assert_eq!(clamp_hp(5, -100, 20), 0);
    }

    #[test]
    fn damage_to_exactly_zero() {
        assert_eq!(clamp_hp(5, -5, 10), 0);
    }

    #[test]
    fn already_at_zero_take_damage() {
        assert_eq!(clamp_hp(0, -5, 10), 0);
    }

    // === Edge cases ===

    #[test]
    fn zero_delta() {
        assert_eq!(clamp_hp(5, 0, 10), 5);
    }

    #[test]
    fn max_hp_is_one() {
        assert_eq!(clamp_hp(1, -1, 1), 0);
        assert_eq!(clamp_hp(0, 1, 1), 1);
        assert_eq!(clamp_hp(1, 1, 1), 1);
    }

    #[test]
    fn max_hp_is_zero() {
        // Degenerate but shouldn't panic
        assert_eq!(clamp_hp(0, 0, 0), 0);
        assert_eq!(clamp_hp(0, 5, 0), 0);
    }

    #[test]
    #[should_panic(expected = "max_hp must not be negative")]
    fn negative_max_hp_panics() {
        clamp_hp(5, 0, -1);
    }

    // === Overflow safety ===

    #[test]
    fn large_positive_delta_clamped() {
        assert_eq!(clamp_hp(0, i32::MAX, 100), 100);
    }

    #[test]
    fn large_negative_delta_clamped() {
        assert_eq!(clamp_hp(100, i32::MIN + 100, 100), 0);
    }
}

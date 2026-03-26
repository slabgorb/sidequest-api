//! Progression — pure functions mapping level to stats.
//!
//! HP and defense use a soft cap at level 10 (diminishing returns).
//! Damage scales linearly (+10% per level).
//! XP thresholds are 100 * level.

/// Scale a stat with a soft cap at level 10.
///
/// Below level 10: linear growth (~10% per level).
/// Above level 10: diminishing returns (square root scaling on the excess).
fn soft_cap_stat(base: i32, level: u32) -> i32 {
    if level <= 1 {
        return base;
    }
    if level <= 10 {
        base + (base as f64 * 0.1 * (level - 1) as f64) as i32
    } else {
        let at_10 = base + (base as f64 * 0.9) as i32;
        let excess = (level - 10) as f64;
        at_10 + (base as f64 * 0.1 * excess.sqrt()) as i32
    }
}

/// Scale base HP by level with a soft cap at level 10.
pub fn level_to_hp(base: i32, level: u32) -> i32 {
    soft_cap_stat(base, level)
}

/// Scale base damage by level. Linear: +10% per level above 1.
pub fn level_to_damage(base: i32, level: u32) -> i32 {
    if level <= 1 {
        return base;
    }
    base + (base as f64 * 0.1 * (level - 1) as f64) as i32
}

/// Scale base defense by level with a soft cap at level 10.
pub fn level_to_defense(base: i32, level: u32) -> i32 {
    soft_cap_stat(base, level)
}

/// XP required to reach a given level. Threshold is 100 * level.
pub fn xp_for_level(level: u32) -> u32 {
    100 * level
}

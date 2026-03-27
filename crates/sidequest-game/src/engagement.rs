//! Engagement multiplier — scales trope progression by player activity.
//!
//! Story 6-3: When a player goes passive, the world pushes harder.
//! Active players get breathing room (0.5x), passive players get
//! accelerated trope escalation (up to 2.0x).

/// Turns since last meaningful action → multiplier on trope tick rate.
///
/// Returns a scaling factor between 0.5 (very active) and 2.0 (very passive).
/// "Meaningful action" is defined by intent classification upstream.
pub fn engagement_multiplier(turns_since_meaningful: u32) -> f32 {
    match turns_since_meaningful {
        0..=1 => 0.5,
        2..=3 => 1.0,
        4..=6 => 1.5,
        _ => 2.0,
    }
}

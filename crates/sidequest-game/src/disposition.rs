//! Disposition newtype — single source of truth for NPC attitude thresholds.
//!
//! Replaces the Python dual-enum problem (npc.py Attitude ±10, dialogue.py DispositionLevel ±25).
//! One type, one set of thresholds, one `attitude()` derivation.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Attitude derived from a numeric disposition value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Attitude {
    /// Disposition > 10
    Friendly,
    /// Disposition between -10 and 10 (inclusive)
    Neutral,
    /// Disposition < -10
    Hostile,
}

impl fmt::Display for Attitude {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Friendly => write!(f, "friendly"),
            Self::Neutral => write!(f, "neutral"),
            Self::Hostile => write!(f, "hostile"),
        }
    }
}

/// Numeric disposition value that derives a qualitative [`Attitude`].
///
/// Thresholds (from ADR-020):
/// - `> 10` → Friendly
/// - `-10..=10` → Neutral
/// - `< -10` → Hostile
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(transparent)]
pub struct Disposition(i32);

impl Disposition {
    /// Create a new Disposition from a raw integer.
    pub fn new(value: i32) -> Self {
        Self(value)
    }

    /// Get the raw numeric value.
    pub fn value(&self) -> i32 {
        self.0
    }

    /// Derive the qualitative attitude from the numeric disposition.
    pub fn attitude(&self) -> Attitude {
        match self.0 {
            d if d > 10 => Attitude::Friendly,
            d if d < -10 => Attitude::Hostile,
            _ => Attitude::Neutral,
        }
    }

    /// Apply a delta to the disposition value.
    pub fn apply_delta(&mut self, delta: i32) {
        self.0 = self.0.saturating_add(delta);
    }
}

impl fmt::Display for Disposition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.0, self.attitude())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // === Attitude derivation (ADR-020 thresholds) ===

    #[test]
    fn friendly_above_10() {
        assert_eq!(Disposition::new(11).attitude(), Attitude::Friendly);
    }

    #[test]
    fn friendly_at_50() {
        assert_eq!(Disposition::new(50).attitude(), Attitude::Friendly);
    }

    #[test]
    fn hostile_below_neg10() {
        assert_eq!(Disposition::new(-11).attitude(), Attitude::Hostile);
    }

    #[test]
    fn hostile_at_neg50() {
        assert_eq!(Disposition::new(-50).attitude(), Attitude::Hostile);
    }

    #[test]
    fn neutral_at_zero() {
        assert_eq!(Disposition::new(0).attitude(), Attitude::Neutral);
    }

    #[test]
    fn neutral_at_positive_boundary() {
        assert_eq!(Disposition::new(10).attitude(), Attitude::Neutral);
    }

    #[test]
    fn neutral_at_negative_boundary() {
        assert_eq!(Disposition::new(-10).attitude(), Attitude::Neutral);
    }

    // === Boundary transitions ===

    #[test]
    fn boundary_neutral_to_friendly() {
        // 10 is neutral, 11 is friendly
        assert_eq!(Disposition::new(10).attitude(), Attitude::Neutral);
        assert_eq!(Disposition::new(11).attitude(), Attitude::Friendly);
    }

    #[test]
    fn boundary_neutral_to_hostile() {
        // -10 is neutral, -11 is hostile
        assert_eq!(Disposition::new(-10).attitude(), Attitude::Neutral);
        assert_eq!(Disposition::new(-11).attitude(), Attitude::Hostile);
    }

    // === Delta application ===

    #[test]
    fn apply_positive_delta() {
        let mut d = Disposition::new(0);
        d.apply_delta(15);
        assert_eq!(d.value(), 15);
        assert_eq!(d.attitude(), Attitude::Friendly);
    }

    #[test]
    fn apply_negative_delta() {
        let mut d = Disposition::new(0);
        d.apply_delta(-15);
        assert_eq!(d.value(), -15);
        assert_eq!(d.attitude(), Attitude::Hostile);
    }

    #[test]
    fn apply_delta_crosses_threshold() {
        let mut d = Disposition::new(8);
        assert_eq!(d.attitude(), Attitude::Neutral);
        d.apply_delta(5);
        assert_eq!(d.attitude(), Attitude::Friendly);
    }

    // === Default ===

    #[test]
    fn default_is_zero_neutral() {
        let d = Disposition::default();
        assert_eq!(d.value(), 0);
        assert_eq!(d.attitude(), Attitude::Neutral);
    }

    // === Serde round-trip ===

    #[test]
    fn serde_roundtrip() {
        let d = Disposition::new(15);
        let json = serde_json::to_string(&d).unwrap();
        assert_eq!(json, "15");
        let back: Disposition = serde_json::from_str(&json).unwrap();
        assert_eq!(back, d);
    }

    #[test]
    fn attitude_serde_roundtrip() {
        let a = Attitude::Friendly;
        let json = serde_json::to_string(&a).unwrap();
        assert_eq!(json, r#""friendly""#);
        let back: Attitude = serde_json::from_str(&json).unwrap();
        assert_eq!(back, a);
    }

    // === Display ===

    #[test]
    fn display_includes_attitude() {
        let d = Disposition::new(15);
        let s = format!("{d}");
        assert!(s.contains("friendly"), "display should include attitude: {s}");
    }

    #[test]
    fn attitude_display() {
        assert_eq!(Attitude::Friendly.to_string(), "friendly");
        assert_eq!(Attitude::Neutral.to_string(), "neutral");
        assert_eq!(Attitude::Hostile.to_string(), "hostile");
    }
}

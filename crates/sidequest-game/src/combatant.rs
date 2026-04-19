//! Combatant trait — shared interface for Character, NPC, and Enemy.
//!
//! Epic 39 (story 39-2): HP is gone. The trait now exposes the EdgePool
//! composure axis — `edge()` / `max_edge()` / `is_broken()`. Legacy
//! `hp()` / `max_hp()` / `ac()` / `is_alive()` / `hp_fraction()` methods
//! are removed; callers that read "alive" read `!is_broken()`, and
//! "bloodied" checks read `edge_fraction()`.

/// Common interface for anything that participates in combat or composure-driven scenes.
///
/// Character, NPC, and (eventually) Enemy all implement this trait.
/// The `is_broken` / `edge_fraction` defaults derive their behaviour from
/// the per-type `edge()` / `max_edge()` accessors.
pub trait Combatant {
    /// The combatant's display name.
    fn name(&self) -> &str;

    /// Current composure (EdgePool `current`, clamped to `[0, max_edge]`).
    fn edge(&self) -> i32;

    /// Maximum composure (EdgePool `max`; may be mid-scene reduced).
    fn max_edge(&self) -> i32;

    /// Character level.
    fn level(&self) -> u32;

    /// Whether the combatant is broken (composure at or below zero).
    fn is_broken(&self) -> bool {
        self.edge() <= 0
    }

    /// Current composure as a fraction of max (0.0 to 1.0).
    ///
    /// Returns 0.0 when `max_edge == 0`. Pure accessor — no side effects,
    /// no telemetry. The `combatant.bloodied`-equivalent OTEL emission
    /// will live at per-turn state ship sites in a later story, not in
    /// this trait method.
    fn edge_fraction(&self) -> f64 {
        if self.max_edge() == 0 {
            return 0.0;
        }
        self.edge() as f64 / self.max_edge() as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestCombatant {
        name: String,
        edge: i32,
        max_edge: i32,
        level: u32,
    }

    impl Combatant for TestCombatant {
        fn name(&self) -> &str {
            &self.name
        }
        fn edge(&self) -> i32 {
            self.edge
        }
        fn max_edge(&self) -> i32 {
            self.max_edge
        }
        fn level(&self) -> u32 {
            self.level
        }
    }

    fn warrior() -> TestCombatant {
        TestCombatant {
            name: "Grog".to_string(),
            edge: 20,
            max_edge: 30,
            level: 3,
        }
    }

    #[test]
    fn not_broken_with_positive_edge() {
        assert!(!warrior().is_broken());
    }

    #[test]
    fn broken_at_zero_edge() {
        let c = TestCombatant {
            edge: 0,
            ..warrior()
        };
        assert!(c.is_broken());
    }

    #[test]
    fn not_broken_at_one_edge() {
        let c = TestCombatant {
            edge: 1,
            ..warrior()
        };
        assert!(!c.is_broken());
    }

    #[test]
    fn full_edge_fraction() {
        let c = TestCombatant {
            edge: 30,
            max_edge: 30,
            ..warrior()
        };
        assert!((c.edge_fraction() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn half_edge_fraction() {
        let c = TestCombatant {
            edge: 15,
            max_edge: 30,
            ..warrior()
        };
        assert!((c.edge_fraction() - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn zero_edge_fraction() {
        let c = TestCombatant {
            edge: 0,
            max_edge: 30,
            ..warrior()
        };
        assert!((c.edge_fraction() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn zero_max_edge_returns_zero_fraction() {
        let c = TestCombatant {
            edge: 0,
            max_edge: 0,
            ..warrior()
        };
        assert!((c.edge_fraction() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn accessors_return_correct_values() {
        let c = warrior();
        assert_eq!(c.name(), "Grog");
        assert_eq!(c.edge(), 20);
        assert_eq!(c.max_edge(), 30);
        assert_eq!(c.level(), 3);
    }
}

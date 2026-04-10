//! Combatant trait — shared interface for Character, NPC, and Enemy.
//!
//! port-lessons.md #10: Character, NPC, and Enemy all independently define
//! name, hp, max_hp, level, ac. This trait unifies the interface.

use sidequest_telemetry::{WatcherEventBuilder, WatcherEventType};

/// Common interface for anything that participates in combat.
///
/// Character, NPC, and Enemy all implement this trait. Default methods
/// provide derived behavior (is_alive, hp_percentage) with no duplication.
pub trait Combatant {
    /// The combatant's display name.
    fn name(&self) -> &str;

    /// Current hit points.
    fn hp(&self) -> i32;

    /// Maximum hit points.
    fn max_hp(&self) -> i32;

    /// Character level.
    fn level(&self) -> u32;

    /// Armor class.
    fn ac(&self) -> i32;

    /// Whether the combatant is alive (HP > 0).
    fn is_alive(&self) -> bool {
        self.hp() > 0
    }

    /// Current HP as a fraction of max HP (0.0 to 1.0).
    ///
    /// Emits a `combatant.bloodied` watcher event when the result is strictly
    /// less than 0.5 (story 35-10). Threshold-gated so the GM panel sees
    /// meaningful "this combatant is now bloodied" signals without flooding
    /// the channel on every state-build query. Skips emission when
    /// `max_hp == 0` because there is no meaningful HP state to report.
    fn hp_fraction(&self) -> f64 {
        if self.max_hp() == 0 {
            return 0.0;
        }
        let frac = self.hp() as f64 / self.max_hp() as f64;
        if frac < 0.5 {
            WatcherEventBuilder::new("combatant", WatcherEventType::StateTransition)
                .field("action", "bloodied")
                .field("name", self.name().to_string())
                .field("hp", self.hp())
                .field("max_hp", self.max_hp())
                .field("hp_fraction", frac)
                .send();
        }
        frac
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal test combatant for exercising default trait methods.
    struct TestCombatant {
        name: String,
        hp: i32,
        max_hp: i32,
        level: u32,
        ac: i32,
    }

    impl Combatant for TestCombatant {
        fn name(&self) -> &str {
            &self.name
        }
        fn hp(&self) -> i32 {
            self.hp
        }
        fn max_hp(&self) -> i32 {
            self.max_hp
        }
        fn level(&self) -> u32 {
            self.level
        }
        fn ac(&self) -> i32 {
            self.ac
        }
    }

    fn warrior() -> TestCombatant {
        TestCombatant {
            name: "Grog".to_string(),
            hp: 20,
            max_hp: 30,
            level: 3,
            ac: 15,
        }
    }

    // === is_alive ===

    #[test]
    fn alive_with_positive_hp() {
        assert!(warrior().is_alive());
    }

    #[test]
    fn dead_at_zero_hp() {
        let c = TestCombatant { hp: 0, ..warrior() };
        assert!(!c.is_alive());
    }

    #[test]
    fn alive_at_one_hp() {
        let c = TestCombatant { hp: 1, ..warrior() };
        assert!(c.is_alive());
    }

    // === hp_fraction ===

    #[test]
    fn full_hp_fraction() {
        let c = TestCombatant {
            hp: 30,
            max_hp: 30,
            ..warrior()
        };
        assert!((c.hp_fraction() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn half_hp_fraction() {
        let c = TestCombatant {
            hp: 15,
            max_hp: 30,
            ..warrior()
        };
        assert!((c.hp_fraction() - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn zero_hp_fraction() {
        let c = TestCombatant {
            hp: 0,
            max_hp: 30,
            ..warrior()
        };
        assert!((c.hp_fraction() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn zero_max_hp_returns_zero_fraction() {
        let c = TestCombatant {
            hp: 0,
            max_hp: 0,
            ..warrior()
        };
        assert!((c.hp_fraction() - 0.0).abs() < f64::EPSILON);
    }

    // === accessor contracts ===

    #[test]
    fn accessors_return_correct_values() {
        let c = warrior();
        assert_eq!(c.name(), "Grog");
        assert_eq!(c.hp(), 20);
        assert_eq!(c.max_hp(), 30);
        assert_eq!(c.level(), 3);
        assert_eq!(c.ac(), 15);
    }
}

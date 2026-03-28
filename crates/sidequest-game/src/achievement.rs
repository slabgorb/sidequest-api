//! Achievement system — fires when trope transitions match trigger conditions.
//!
//! Story F7: Achievements track narrative milestones tied to trope lifecycle.
//! "Subverted" is a special trigger: fires when status=Resolved AND any note
//! starts with "subverted" (case-insensitive).

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::trope::{TropeState, TropeStatus};

/// An achievement definition — ties a narrative milestone to a trope trigger.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Achievement {
    /// Unique identifier.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Flavor text shown on unlock.
    pub description: String,
    /// Which trope definition this achievement watches.
    pub trope_id: String,
    /// Status that triggers this: "activated" | "progressing" | "resolved" | "subverted".
    pub trigger_status: String,
    /// Optional emoji for UI display.
    pub emoji: Option<String>,
}

/// Tracks achievement definitions and which have been earned.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AchievementTracker {
    /// All achievement definitions.
    pub achievements: Vec<Achievement>,
    /// IDs of achievements already earned (dedup guard).
    pub earned: HashSet<String>,
}

impl AchievementTracker {
    /// Create a new tracker with the given definitions.
    pub fn new(achievements: Vec<Achievement>) -> Self {
        Self {
            achievements,
            earned: HashSet::new(),
        }
    }

    /// Check a trope transition and return any newly earned achievements.
    ///
    /// `old_status` is the status before the transition. The current status
    /// and notes are read from `trope`.
    ///
    /// Rules:
    /// - No transition (old == new) -> empty.
    /// - "subverted" fires when new status is Resolved AND any note starts
    ///   with "subverted" (case-insensitive).
    /// - Both "resolved" and "subverted" can fire on the same transition.
    /// - Once earned, cannot earn again.
    pub fn check_transition(
        &mut self,
        trope: &TropeState,
        old_status: TropeStatus,
    ) -> Vec<Achievement> {
        let new_status = trope.status();

        // No transition -> no achievements
        if old_status == new_status {
            return Vec::new();
        }

        // Build the set of trigger strings that match this transition
        let mut triggers: Vec<&str> = Vec::new();
        match new_status {
            TropeStatus::Active => triggers.push("activated"),
            TropeStatus::Progressing => triggers.push("progressing"),
            TropeStatus::Resolved => {
                triggers.push("resolved");
                // Check for subversion: any note starts with "subverted"
                if trope
                    .notes()
                    .iter()
                    .any(|n| n.trim_start().to_lowercase().starts_with("subverted"))
                {
                    triggers.push("subverted");
                }
            }
            TropeStatus::Dormant => {} // No achievements for going dormant
        }

        let trope_id = trope.trope_definition_id();

        let mut newly_earned = Vec::new();
        for achievement in &self.achievements {
            if self.earned.contains(&achievement.id) {
                continue;
            }
            if achievement.trope_id != trope_id {
                continue;
            }
            if triggers.contains(&achievement.trigger_status.as_str()) {
                self.earned.insert(achievement.id.clone());
                newly_earned.push(achievement.clone());
            }
        }

        newly_earned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_trope(id: &str, status: TropeStatus, notes: Vec<String>) -> TropeState {
        let mut ts = TropeState::new(id);
        ts.set_status(status);
        for n in notes {
            ts.add_note(n);
        }
        ts
    }

    fn sample_achievements() -> Vec<Achievement> {
        vec![
            Achievement {
                id: "ach-1".into(),
                name: "First Steps".into(),
                description: "A trope begins to unfold.".into(),
                trope_id: "betrayal".into(),
                trigger_status: "activated".into(),
                emoji: None,
            },
            Achievement {
                id: "ach-2".into(),
                name: "Deepening".into(),
                description: "The betrayal thickens.".into(),
                trope_id: "betrayal".into(),
                trigger_status: "progressing".into(),
                emoji: None,
            },
            Achievement {
                id: "ach-3".into(),
                name: "Resolution".into(),
                description: "The betrayal is resolved.".into(),
                trope_id: "betrayal".into(),
                trigger_status: "resolved".into(),
                emoji: None,
            },
            Achievement {
                id: "ach-4".into(),
                name: "Twist!".into(),
                description: "The betrayal was subverted.".into(),
                trope_id: "betrayal".into(),
                trigger_status: "subverted".into(),
                emoji: None,
            },
            Achievement {
                id: "ach-5".into(),
                name: "Other Trope".into(),
                description: "Different trope activated.".into(),
                trope_id: "redemption".into(),
                trigger_status: "activated".into(),
                emoji: None,
            },
        ]
    }

    #[test]
    fn test_no_transition_returns_empty() {
        let mut tracker = AchievementTracker::new(sample_achievements());
        let trope = make_trope("betrayal", TropeStatus::Active, vec![]);
        let earned = tracker.check_transition(&trope, TropeStatus::Active);
        assert!(earned.is_empty());
    }

    #[test]
    fn test_activated_transition() {
        let mut tracker = AchievementTracker::new(sample_achievements());
        let trope = make_trope("betrayal", TropeStatus::Active, vec![]);
        let earned = tracker.check_transition(&trope, TropeStatus::Dormant);
        assert_eq!(earned.len(), 1);
        assert_eq!(earned[0].id, "ach-1");
    }

    #[test]
    fn test_progressing_transition() {
        let mut tracker = AchievementTracker::new(sample_achievements());
        let trope = make_trope("betrayal", TropeStatus::Progressing, vec![]);
        let earned = tracker.check_transition(&trope, TropeStatus::Active);
        assert_eq!(earned.len(), 1);
        assert_eq!(earned[0].id, "ach-2");
    }

    #[test]
    fn test_resolved_transition() {
        let mut tracker = AchievementTracker::new(sample_achievements());
        let trope = make_trope("betrayal", TropeStatus::Resolved, vec![]);
        let earned = tracker.check_transition(&trope, TropeStatus::Progressing);
        assert_eq!(earned.len(), 1);
        assert_eq!(earned[0].id, "ach-3");
    }

    #[test]
    fn test_subverted_fires_with_resolved() {
        let mut tracker = AchievementTracker::new(sample_achievements());
        let trope = make_trope(
            "betrayal",
            TropeStatus::Resolved,
            vec!["Subverted by the hero's sacrifice".into()],
        );
        let earned = tracker.check_transition(&trope, TropeStatus::Progressing);
        assert_eq!(earned.len(), 2);
        let ids: Vec<&str> = earned.iter().map(|a| a.id.as_str()).collect();
        assert!(ids.contains(&"ach-3"), "resolved should fire");
        assert!(ids.contains(&"ach-4"), "subverted should fire");
    }

    #[test]
    fn test_subverted_case_insensitive() {
        let mut tracker = AchievementTracker::new(sample_achievements());
        let trope = make_trope(
            "betrayal",
            TropeStatus::Resolved,
            vec!["SUBVERTED expectations".into()],
        );
        let earned = tracker.check_transition(&trope, TropeStatus::Progressing);
        let ids: Vec<&str> = earned.iter().map(|a| a.id.as_str()).collect();
        assert!(ids.contains(&"ach-4"), "subverted should match case-insensitive");
    }

    #[test]
    fn test_dedup_cannot_earn_twice() {
        let mut tracker = AchievementTracker::new(sample_achievements());
        let trope = make_trope("betrayal", TropeStatus::Active, vec![]);

        let first = tracker.check_transition(&trope, TropeStatus::Dormant);
        assert_eq!(first.len(), 1);

        let trope2 = make_trope("betrayal", TropeStatus::Active, vec![]);
        let second = tracker.check_transition(&trope2, TropeStatus::Dormant);
        assert!(second.is_empty(), "should not earn same achievement twice");
    }

    #[test]
    fn test_wrong_trope_id_no_match() {
        let mut tracker = AchievementTracker::new(sample_achievements());
        let trope = make_trope("redemption", TropeStatus::Active, vec![]);
        let earned = tracker.check_transition(&trope, TropeStatus::Dormant);
        assert_eq!(earned.len(), 1);
        assert_eq!(earned[0].id, "ach-5");
    }

    #[test]
    fn test_resolved_without_subversion_note() {
        let mut tracker = AchievementTracker::new(sample_achievements());
        let trope = make_trope(
            "betrayal",
            TropeStatus::Resolved,
            vec!["Completed naturally".into()],
        );
        let earned = tracker.check_transition(&trope, TropeStatus::Progressing);
        assert_eq!(earned.len(), 1);
        assert_eq!(earned[0].id, "ach-3", "only resolved, not subverted");
    }
}

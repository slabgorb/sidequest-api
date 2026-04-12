//! Affinity progression — tiered skill/ability trees driven by genre pack definitions.
//!
//! Story F8: Characters accumulate progress in affinities through actions.
//! When progress reaches a tier threshold, the character advances to the next tier,
//! unlocking cumulative abilities. Max tier is 3 (Master).

use serde::{Deserialize, Serialize};

/// Tier labels for display.
pub const TIER_NAMES: [&str; 4] = ["Unawakened", "Novice", "Adept", "Master"];

/// Maximum tier index.
pub const MAX_TIER: u8 = 3;

/// Per-character affinity tracking state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AffinityState {
    /// Affinity name (matches genre pack affinity definition).
    pub name: String,
    /// Current tier: 0 = Unawakened, 1 = Novice, 2 = Adept, 3 = Master.
    pub tier: u8,
    /// Action counter toward the next tier threshold.
    pub progress: u32,
}

impl AffinityState {
    /// Create a new affinity at tier 0 with zero progress.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            tier: 0,
            progress: 0,
        }
    }

    /// Human-readable tier name.
    pub fn tier_name(&self) -> &'static str {
        TIER_NAMES[self.tier.min(MAX_TIER) as usize]
    }
}

/// Event fired when a character advances to a new affinity tier.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AffinityTierUpEvent {
    /// Which affinity advanced.
    pub affinity_name: String,
    /// Previous tier.
    pub old_tier: u8,
    /// New tier.
    pub new_tier: u8,
    /// Character who advanced.
    pub character_name: String,
    /// Narration hint for the narrator (from genre pack, or default).
    pub narration_hint: String,
}

/// Check all affinities on a character against genre pack thresholds.
/// Mutates progress/tier in place, returns events for any tier-ups.
///
/// `thresholds_for` maps affinity name -> tier thresholds array.
/// `narration_hint_for` maps (affinity_name, new_tier) -> optional hint text.
pub fn check_affinity_thresholds(
    affinities: &mut [AffinityState],
    character_name: &str,
    thresholds_for: &dyn Fn(&str) -> Option<Vec<u32>>,
    narration_hint_for: &dyn Fn(&str, u8) -> Option<String>,
) -> Vec<AffinityTierUpEvent> {
    let mut events = Vec::new();

    for aff in affinities.iter_mut() {
        if aff.tier >= MAX_TIER {
            continue;
        }

        let thresholds = match thresholds_for(&aff.name) {
            Some(t) => t,
            None => continue,
        };

        // Tier index into thresholds: tier 0->1 uses thresholds[0], 1->2 uses thresholds[1], etc.
        loop {
            if aff.tier >= MAX_TIER {
                break;
            }
            let idx = aff.tier as usize;
            if idx >= thresholds.len() {
                break;
            }
            let threshold = thresholds[idx];
            if threshold == 0 || aff.progress < threshold {
                break;
            }
            // Tier up: subtract threshold, remainder carries forward.
            aff.progress -= threshold;
            let old_tier = aff.tier;
            aff.tier += 1;

            let hint = narration_hint_for(&aff.name, aff.tier).unwrap_or_else(|| {
                format!(
                    "{} has reached {} tier in {}!",
                    character_name, TIER_NAMES[aff.tier as usize], aff.name
                )
            });

            events.push(AffinityTierUpEvent {
                affinity_name: aff.name.clone(),
                old_tier,
                new_tier: aff.tier,
                character_name: character_name.to_string(),
                narration_hint: hint,
            });
        }
    }

    events
}

/// Resolve cumulative abilities for a character based on current affinity tiers.
///
/// Returns union of all abilities from tier 0 through current tier for each affinity.
/// `abilities_for` maps (affinity_name, tier) -> list of ability names at that tier.
pub fn resolve_abilities(
    affinities: &[AffinityState],
    abilities_for: &dyn Fn(&str, u8) -> Vec<String>,
) -> Vec<String> {
    let mut result = Vec::new();
    for aff in affinities {
        for tier in 0..=aff.tier {
            let abilities = abilities_for(&aff.name, tier);
            for ability in abilities {
                if !result.contains(&ability) {
                    result.push(ability);
                }
            }
        }
    }
    result
}

/// Format resolved abilities into a narrator prompt context block.
/// Follows the same pattern as `format_lore_context` and `format_chase_context`.
/// Returns an empty string if no abilities are provided.
pub fn format_abilities_context(abilities: &[String]) -> String {
    if abilities.is_empty() {
        return String::new();
    }
    let mut out = String::from("Character Abilities:\n");
    for ability in abilities {
        out.push_str(&format!("- {ability}\n"));
    }
    out
}

/// Increment progress for a named affinity. Creates the affinity at tier 0 if absent.
/// Returns true if the affinity existed (or was created).
pub fn increment_affinity_progress(
    affinities: &mut Vec<AffinityState>,
    affinity_name: &str,
    amount: u32,
) -> bool {
    if let Some(aff) = affinities.iter_mut().find(|a| a.name == affinity_name) {
        aff.progress += amount;
        true
    } else {
        affinities.push(AffinityState {
            name: affinity_name.to_string(),
            tier: 0,
            progress: amount,
        });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_thresholds(name: &str) -> Option<Vec<u32>> {
        match name {
            "Fire Magic" => Some(vec![5, 10, 20]),
            "Stealth" => Some(vec![3, 6, 12]),
            _ => None,
        }
    }

    fn sample_hints(name: &str, tier: u8) -> Option<String> {
        match (name, tier) {
            ("Fire Magic", 1) => Some("Flames flicker at your fingertips.".to_string()),
            ("Fire Magic", 2) => Some("Fire bends to your will.".to_string()),
            ("Fire Magic", 3) => Some("You are the inferno incarnate.".to_string()),
            _ => None,
        }
    }

    fn sample_abilities(name: &str, tier: u8) -> Vec<String> {
        match (name, tier) {
            ("Fire Magic", 0) => vec!["Spark".to_string()],
            ("Fire Magic", 1) => vec!["Fireball".to_string()],
            ("Fire Magic", 2) => vec!["Wall of Fire".to_string()],
            ("Fire Magic", 3) => vec!["Meteor".to_string()],
            ("Stealth", 0) => vec!["Sneak".to_string()],
            ("Stealth", 1) => vec!["Shadow Step".to_string()],
            _ => vec![],
        }
    }

    #[test]
    fn new_affinity_starts_at_tier_zero() {
        let aff = AffinityState::new("Fire Magic");
        assert_eq!(aff.tier, 0);
        assert_eq!(aff.progress, 0);
        assert_eq!(aff.tier_name(), "Unawakened");
    }

    #[test]
    fn no_tier_up_below_threshold() {
        let mut affinities = vec![AffinityState {
            name: "Fire Magic".to_string(),
            tier: 0,
            progress: 4,
        }];
        let events =
            check_affinity_thresholds(&mut affinities, "Thorn", &sample_thresholds, &sample_hints);
        assert!(events.is_empty());
        assert_eq!(affinities[0].tier, 0);
        assert_eq!(affinities[0].progress, 4);
    }

    #[test]
    fn tier_up_at_threshold() {
        let mut affinities = vec![AffinityState {
            name: "Fire Magic".to_string(),
            tier: 0,
            progress: 5,
        }];
        let events =
            check_affinity_thresholds(&mut affinities, "Thorn", &sample_thresholds, &sample_hints);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].old_tier, 0);
        assert_eq!(events[0].new_tier, 1);
        assert_eq!(
            events[0].narration_hint,
            "Flames flicker at your fingertips."
        );
        assert_eq!(affinities[0].tier, 1);
        assert_eq!(affinities[0].progress, 0);
    }

    #[test]
    fn remainder_carries_forward() {
        let mut affinities = vec![AffinityState {
            name: "Fire Magic".to_string(),
            tier: 0,
            progress: 7,
        }];
        let events =
            check_affinity_thresholds(&mut affinities, "Thorn", &sample_thresholds, &sample_hints);
        assert_eq!(events.len(), 1);
        assert_eq!(affinities[0].tier, 1);
        assert_eq!(affinities[0].progress, 2); // 7 - 5 = 2
    }

    #[test]
    fn multi_tier_up_in_one_check() {
        let mut affinities = vec![AffinityState {
            name: "Fire Magic".to_string(),
            tier: 0,
            progress: 16,
        }];
        let events =
            check_affinity_thresholds(&mut affinities, "Thorn", &sample_thresholds, &sample_hints);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].new_tier, 1);
        assert_eq!(events[1].new_tier, 2);
        assert_eq!(affinities[0].tier, 2);
        assert_eq!(affinities[0].progress, 1); // 16 - 5 - 10 = 1
    }

    #[test]
    fn max_tier_caps_at_three() {
        let mut affinities = vec![AffinityState {
            name: "Fire Magic".to_string(),
            tier: 3,
            progress: 999,
        }];
        let events =
            check_affinity_thresholds(&mut affinities, "Thorn", &sample_thresholds, &sample_hints);
        assert!(events.is_empty());
        assert_eq!(affinities[0].tier, 3);
    }

    #[test]
    fn resolve_abilities_cumulative() {
        let affinities = vec![
            AffinityState {
                name: "Fire Magic".to_string(),
                tier: 2,
                progress: 0,
            },
            AffinityState {
                name: "Stealth".to_string(),
                tier: 1,
                progress: 0,
            },
        ];
        let abilities = resolve_abilities(&affinities, &sample_abilities);
        assert!(abilities.contains(&"Spark".to_string()));
        assert!(abilities.contains(&"Fireball".to_string()));
        assert!(abilities.contains(&"Wall of Fire".to_string()));
        assert!(!abilities.contains(&"Meteor".to_string()));
        assert!(abilities.contains(&"Sneak".to_string()));
        assert!(abilities.contains(&"Shadow Step".to_string()));
    }

    #[test]
    fn resolve_abilities_no_duplicates() {
        let affinities = vec![AffinityState {
            name: "Fire Magic".to_string(),
            tier: 1,
            progress: 0,
        }];
        let abilities = resolve_abilities(&affinities, &sample_abilities);
        assert_eq!(abilities, vec!["Spark".to_string(), "Fireball".to_string()]);
    }

    #[test]
    fn increment_creates_if_absent() {
        let mut affinities = vec![];
        increment_affinity_progress(&mut affinities, "Fire Magic", 3);
        assert_eq!(affinities.len(), 1);
        assert_eq!(affinities[0].name, "Fire Magic");
        assert_eq!(affinities[0].progress, 3);
        assert_eq!(affinities[0].tier, 0);
    }

    #[test]
    fn increment_adds_to_existing() {
        let mut affinities = vec![AffinityState {
            name: "Fire Magic".to_string(),
            tier: 1,
            progress: 4,
        }];
        increment_affinity_progress(&mut affinities, "Fire Magic", 3);
        assert_eq!(affinities[0].progress, 7);
        assert_eq!(affinities[0].tier, 1);
    }

    #[test]
    fn default_narration_hint_used_when_none_provided() {
        let mut affinities = vec![AffinityState {
            name: "Stealth".to_string(),
            tier: 0,
            progress: 3,
        }];
        let events =
            check_affinity_thresholds(&mut affinities, "Thorn", &sample_thresholds, &|_, _| None);
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].narration_hint,
            "Thorn has reached Novice tier in Stealth!"
        );
    }

    #[test]
    fn unknown_affinity_skipped() {
        let mut affinities = vec![AffinityState {
            name: "Unknown".to_string(),
            tier: 0,
            progress: 100,
        }];
        let events =
            check_affinity_thresholds(&mut affinities, "Thorn", &sample_thresholds, &sample_hints);
        assert!(events.is_empty());
    }

    #[test]
    fn serde_roundtrip() {
        let aff = AffinityState {
            name: "Fire Magic".to_string(),
            tier: 2,
            progress: 7,
        };
        let json = serde_json::to_string(&aff).unwrap();
        let back: AffinityState = serde_json::from_str(&json).unwrap();
        assert_eq!(aff, back);
    }
}

//! Structured Encounter System — universal encounter engine.
//!
//! Generalizes ChaseState into a YAML-declarable encounter engine
//! for standoffs, negotiations, net combat, ship combat, and any
//! future structured encounter type. Story 16-2.
//!
//! Key design: string-keyed encounter types replace hardcoded enums,
//! EncounterMetric replaces separation_distance, SecondaryStats
//! replaces RigStats, EncounterActor replaces ChaseActor.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::chase::ChaseState;
use crate::chase_depth::{RigStats, RigType};
use crate::combat::CombatState;

/// Direction a metric moves toward resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum MetricDirection {
    /// Metric increases toward threshold_high (e.g., tension in a standoff).
    Ascending,
    /// Metric decreases toward threshold_low (e.g., hull integrity).
    Descending,
    /// Metric can swing either way (e.g., leverage in a negotiation).
    Bidirectional,
}

/// Narrative arc phase for structured encounters.
///
/// Universal across all encounter types — the same dramatic shape
/// as ChasePhase but not locked to chase semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EncounterPhase {
    /// Initial setup.
    Setup,
    /// Encounter begins.
    Opening,
    /// Tension rises.
    Escalation,
    /// Peak intensity.
    Climax,
    /// Winding down.
    Resolution,
}

impl EncounterPhase {
    /// Drama weight for this phase (used by cinematography).
    /// Same values as ChasePhase — the dramatic arc is universal.
    pub fn drama_weight(self) -> f64 {
        match self {
            EncounterPhase::Setup => 0.70,
            EncounterPhase::Opening => 0.75,
            EncounterPhase::Escalation => 0.80,
            EncounterPhase::Climax => 0.95,
            EncounterPhase::Resolution => 0.70,
        }
    }
}

/// A single stat in a secondary stats block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatValue {
    /// Current value.
    pub current: i32,
    /// Maximum value.
    pub max: i32,
}

/// Generic secondary stats block — generalizes RigStats.
///
/// String-keyed so genre packs can declare arbitrary stats:
/// hp/fuel/speed/armor/maneuver for vehicles, shields/hull/engines
/// for ships, focus/nerve for standoffs, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecondaryStats {
    /// Named stats (e.g., "hp" → {current: 15, max: 15}).
    pub stats: HashMap<String, StatValue>,
    /// Computed damage tier label (e.g., "PRISTINE", "FAILING").
    pub damage_tier: Option<String>,
}

impl SecondaryStats {
    /// Create SecondaryStats from a RigType, matching RigStats::from_type() values.
    ///
    /// Convenience constructor so chase encounters can express rig stats
    /// in the generic format.
    pub fn rig(rig_type: RigType) -> Self {
        Self::from_rig_stats(&RigStats::from_type(rig_type))
    }

    /// Convert a RigStats instance into the generic SecondaryStats format.
    ///
    /// Single authority for RigStats → SecondaryStats transformation.
    /// Used by both the rig() convenience constructor and from_chase_state() migration.
    pub fn from_rig_stats(rig: &RigStats) -> Self {
        let mut stats = HashMap::new();
        stats.insert(
            "hp".to_string(),
            StatValue {
                current: rig.rig_hp,
                max: rig.max_rig_hp,
            },
        );
        stats.insert(
            "speed".to_string(),
            StatValue {
                current: rig.speed,
                max: rig.speed,
            },
        );
        stats.insert(
            "armor".to_string(),
            StatValue {
                current: rig.armor,
                max: rig.armor,
            },
        );
        stats.insert(
            "maneuver".to_string(),
            StatValue {
                current: rig.maneuver,
                max: rig.maneuver,
            },
        );
        stats.insert(
            "fuel".to_string(),
            StatValue {
                current: rig.fuel,
                max: rig.max_fuel,
            },
        );

        let damage_tier = Some(format!("{}", rig.damage_tier()));

        SecondaryStats { stats, damage_tier }
    }
}

/// A character assigned to an encounter role.
///
/// String-keyed roles replace the ChaseRole enum — genre packs can
/// declare arbitrary roles (driver, gunner, duelist, netrunner, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncounterActor {
    /// Character name.
    pub name: String,
    /// Role in the encounter (string-keyed, genre-defined).
    pub role: String,
}

/// The primary metric being tracked in the encounter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncounterMetric {
    /// Metric name (e.g., "separation", "tension", "leverage").
    pub name: String,
    /// Current value.
    pub current: i32,
    /// Starting value.
    pub starting: i32,
    /// Which direction resolves the encounter.
    pub direction: MetricDirection,
    /// Upper resolution threshold (metric >= this → resolved).
    pub threshold_high: Option<i32>,
    /// Lower resolution threshold (metric <= this → resolved).
    pub threshold_low: Option<i32>,
}

/// A universal structured encounter — the generalization of ChaseState.
///
/// String-keyed encounter_type replaces hardcoded ChaseType enum.
/// EncounterMetric replaces separation_distance. SecondaryStats
/// replaces RigStats. EncounterActor replaces ChaseActor with
/// string-keyed roles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredEncounter {
    /// Encounter type key (e.g., "chase", "standoff", "negotiation").
    pub encounter_type: String,
    /// Primary metric being tracked.
    pub metric: EncounterMetric,
    /// Current beat number.
    pub beat: u32,
    /// Current narrative phase.
    pub structured_phase: Option<EncounterPhase>,
    /// Optional secondary stats block.
    pub secondary_stats: Option<SecondaryStats>,
    /// Participants and their roles.
    pub actors: Vec<EncounterActor>,
    /// Outcome description if resolved.
    pub outcome: Option<String>,
    /// Whether the encounter has been resolved.
    pub resolved: bool,
    /// Mood override for MusicDirector.
    pub mood_override: Option<String>,
    /// Hints for the narrator.
    pub narrator_hints: Vec<String>,
}

impl StructuredEncounter {
    /// Create a chase-type encounter from the old ChaseState parameters.
    ///
    /// Maps chase semantics onto the generic encounter model:
    /// - separation_distance → metric with name "separation", Ascending direction
    /// - goal → threshold_high
    /// - rig → SecondaryStats via rig() constructor
    pub fn chase(escape_threshold: f64, rig_type: Option<RigType>, goal: i32) -> Self {
        let secondary_stats = rig_type.map(SecondaryStats::rig);

        Self {
            encounter_type: "chase".to_string(),
            metric: EncounterMetric {
                name: "separation".to_string(),
                current: 0,
                starting: 0,
                direction: MetricDirection::Ascending,
                threshold_high: Some(goal),
                threshold_low: None,
            },
            beat: 0,
            structured_phase: Some(EncounterPhase::Setup),
            secondary_stats,
            actors: vec![],
            outcome: None,
            resolved: false,
            mood_override: None,
            narrator_hints: vec![],
        }
    }

    /// Create a combat-type encounter.
    ///
    /// Maps combat semantics onto the generic encounter model:
    /// - HP → Descending metric (threshold_low = 0)
    /// - combatants → actors with role "combatant"
    /// - Starts at beat 0 in Setup phase
    pub fn combat(combatants: Vec<String>, hp: i32) -> Self {
        let actors = combatants
            .into_iter()
            .map(|name| EncounterActor {
                name,
                role: "combatant".to_string(),
            })
            .collect();

        Self {
            encounter_type: "combat".to_string(),
            metric: EncounterMetric {
                name: "hp".to_string(),
                current: hp,
                starting: hp,
                direction: MetricDirection::Descending,
                threshold_high: None,
                threshold_low: Some(0),
            },
            beat: 0,
            structured_phase: Some(EncounterPhase::Setup),
            secondary_stats: None,
            actors,
            outcome: None,
            resolved: false,
            mood_override: None,
            narrator_hints: vec![],
        }
    }

    /// Convert an existing CombatState into a StructuredEncounter.
    ///
    /// Maps combat fields onto the generic encounter model:
    /// - round → beat
    /// - turn_order → actors with role "combatant"
    /// - in_combat → !resolved
    /// - damage_log → narrator_hints (human-readable summaries)
    /// - status effects → narrator_hints
    pub fn from_combat_state(combat: &CombatState) -> Self {
        let actors = combat
            .turn_order()
            .iter()
            .map(|name| EncounterActor {
                name: name.clone(),
                role: "combatant".to_string(),
            })
            .collect();

        let mut narrator_hints: Vec<String> = Vec::new();

        // Preserve damage log as narrator hints
        for event in combat.damage_log() {
            narrator_hints.push(format!(
                "{} dealt {} damage to {} (round {})",
                event.attacker, event.damage, event.target, event.round
            ));
        }

        // Preserve status effects as narrator hints
        for name in combat.turn_order() {
            let effects = combat.effects_on(name);
            for effect in &effects {
                narrator_hints.push(format!(
                    "{}: {:?} ({} rounds)",
                    name,
                    effect.kind(),
                    effect.remaining_rounds()
                ));
            }
        }

        // Map combat phase based on round progression
        let structured_phase = if combat.in_combat() {
            Some(match combat.round() {
                1 => EncounterPhase::Opening,
                2..=3 => EncounterPhase::Escalation,
                4..=5 => EncounterPhase::Climax,
                _ => EncounterPhase::Resolution,
            })
        } else {
            None
        };

        Self {
            encounter_type: "combat".to_string(),
            metric: EncounterMetric {
                name: "hp".to_string(),
                current: 0,
                starting: 0,
                direction: MetricDirection::Descending,
                threshold_high: None,
                threshold_low: Some(0),
            },
            beat: combat.round(),
            structured_phase,
            secondary_stats: None,
            actors,
            outcome: None,
            resolved: !combat.in_combat(),
            mood_override: Some("combat".to_string()),
            narrator_hints,
        }
    }

    /// Convert an old ChaseState into a StructuredEncounter.
    ///
    /// Used for backward-compatible deserialization of old save files.
    pub fn from_chase_state(chase: &ChaseState) -> Self {
        let secondary_stats = chase.rig().map(SecondaryStats::from_rig_stats);

        let structured_phase = chase.structured_phase().map(|p| match p {
            crate::chase_depth::ChasePhase::Setup => EncounterPhase::Setup,
            crate::chase_depth::ChasePhase::Opening => EncounterPhase::Opening,
            crate::chase_depth::ChasePhase::Escalation => EncounterPhase::Escalation,
            crate::chase_depth::ChasePhase::Climax => EncounterPhase::Climax,
            crate::chase_depth::ChasePhase::Resolution => EncounterPhase::Resolution,
        });

        let actors = chase
            .actors()
            .iter()
            .map(|a| EncounterActor {
                name: a.name.clone(),
                role: format!("{}", a.role),
            })
            .collect();

        Self {
            encounter_type: "chase".to_string(),
            metric: EncounterMetric {
                name: "separation".to_string(),
                current: chase.separation(),
                starting: 0,
                direction: MetricDirection::Ascending,
                threshold_high: Some(chase.goal()),
                threshold_low: None,
            },
            beat: chase.beat(),
            structured_phase,
            secondary_stats,
            actors,
            outcome: chase.outcome().map(|o| format!("{:?}", o)),
            resolved: chase.is_resolved(),
            mood_override: None,
            narrator_hints: vec![],
        }
    }
}

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

use sidequest_genre::ConfrontationDef;
use sidequest_telemetry::{WatcherEventBuilder, WatcherEventType};

use crate::chase_depth::{RigStats, RigType};

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
    /// Per-actor structured state for resolution modes that track per-pilot
    /// descriptors between turns (e.g., bearing, range, energy, gun_solution).
    /// Used by `SealedLetterLookup` confrontations (ADR-077).
    #[serde(default)]
    pub per_actor_state: HashMap<String, serde_json::Value>,
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
    pub fn chase(_escape_threshold: f64, rig_type: Option<RigType>, goal: i32) -> Self {
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
                per_actor_state: HashMap::new(),
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

    // from_combat_state() and from_chase_state() deleted in story 28-9.
    // StructuredEncounter is now created directly from ConfrontationDef or apply_beat().

    /// Create a StructuredEncounter from a genre-pack ConfrontationDef.
    ///
    /// Maps the YAML-declared confrontation type onto the runtime encounter model:
    /// - confrontation_type → encounter_type
    /// - MetricDef → EncounterMetric (direction string → MetricDirection enum)
    /// - SecondaryStatDef → SecondaryStats (default max 10, current = max)
    /// - mood → mood_override
    pub fn from_confrontation_def(def: &ConfrontationDef) -> Self {
        let direction = match def.metric.direction.as_str() {
            "ascending" => MetricDirection::Ascending,
            "descending" => MetricDirection::Descending,
            "bidirectional" => MetricDirection::Bidirectional,
            _ => MetricDirection::Ascending,
        };

        let secondary_stats = if def.secondary_stats.is_empty() {
            None
        } else {
            let mut stats = HashMap::new();
            for stat_def in &def.secondary_stats {
                let max = 10; // default max for confrontation secondary stats
                stats.insert(stat_def.name.clone(), StatValue { current: max, max });
            }
            Some(SecondaryStats {
                stats,
                damage_tier: None,
            })
        };

        Self {
            encounter_type: def.confrontation_type.clone(),
            metric: EncounterMetric {
                name: def.metric.name.clone(),
                current: def.metric.starting,
                starting: def.metric.starting,
                direction,
                threshold_high: def.metric.threshold_high,
                threshold_low: def.metric.threshold_low,
            },
            beat: 0,
            structured_phase: Some(EncounterPhase::Setup),
            secondary_stats,
            actors: vec![],
            outcome: None,
            resolved: false,
            mood_override: def.mood.clone(),
            narrator_hints: vec![],
        }
    }

    /// Apply a beat action to the encounter, mutating the primary metric.
    ///
    /// Looks up the beat by ID in the confrontation definition, applies its
    /// metric_delta, increments the beat counter, checks for resolution
    /// (threshold crossing or resolution flag), and updates the phase.
    pub fn apply_beat(&mut self, beat_id: &str, def: &ConfrontationDef) -> Result<(), String> {
        if self.resolved {
            return Err("encounter is already resolved".to_string());
        }

        let beat = def
            .beats
            .iter()
            .find(|b| b.id == beat_id)
            .ok_or_else(|| format!("unknown beat id '{}'", beat_id))?;

        // Capture pre-mutation state for OTEL
        let metric_before = self.metric.current;
        let old_phase = self.structured_phase;

        // Apply metric delta, clamping to 0 for ascending metrics
        self.metric.current += beat.metric_delta;
        if self.metric.direction == MetricDirection::Ascending && self.metric.current < 0 {
            self.metric.current = 0;
        }

        self.beat += 1;

        // Check resolution: beat flag or threshold crossing
        let is_resolution_beat = beat.resolution.unwrap_or(false);
        let threshold_crossed = match self.metric.direction {
            MetricDirection::Ascending => self
                .metric
                .threshold_high
                .is_some_and(|t| self.metric.current >= t),
            MetricDirection::Descending => self
                .metric
                .threshold_low
                .is_some_and(|t| self.metric.current <= t),
            MetricDirection::Bidirectional => {
                let high = self
                    .metric
                    .threshold_high
                    .is_some_and(|t| self.metric.current >= t);
                let low = self
                    .metric
                    .threshold_low
                    .is_some_and(|t| self.metric.current <= t);
                high || low
            }
        };

        if is_resolution_beat || threshold_crossed {
            self.resolved = true;
            self.structured_phase = Some(EncounterPhase::Resolution);
        } else {
            // Phase transitions by beat number (same arc as chase)
            self.structured_phase = Some(match self.beat {
                0 => EncounterPhase::Setup,
                1 => EncounterPhase::Opening,
                2..=4 => EncounterPhase::Escalation,
                _ => EncounterPhase::Climax,
            });
        }

        // OTEL: encounter.beat_applied
        let phase_str = self
            .structured_phase
            .map(|p| format!("{:?}", p))
            .unwrap_or_else(|| "Unknown".to_string());

        // OTEL: encounter state-machine event — uses the `event=` field key
        // (not `action=`) so the GM panel's standard filter picks it up.
        // Story 37-14 fix #5: renamed from `action="beat_applied"` to
        // `event="encounter.state.beat_applied"` — the `state.` prefix
        // disambiguates the inner state-machine emission from the
        // dispatch-layer `encounter.beat_applied` event (which fires from
        // `apply_beat_dispatch` with different fields and attribution).
        WatcherEventBuilder::new("encounter", WatcherEventType::StateTransition)
            .field("event", "encounter.state.beat_applied")
            .field("encounter_type", &self.encounter_type)
            .field("beat_id", beat_id)
            .field("stat_check", &beat.stat_check)
            .field("metric_before", metric_before)
            .field("metric_after", self.metric.current)
            .field("phase", &phase_str)
            .send();

        // OTEL: encounter.state.resolved (if resolution just triggered).
        // The `state.` prefix disambiguates from dispatch/mod.rs's
        // `encounter.resolved` event (which fires from the
        // `encounter_just_resolved` detection one layer up).
        if self.resolved {
            WatcherEventBuilder::new("encounter", WatcherEventType::StateTransition)
                .field("event", "encounter.state.resolved")
                .field("encounter_type", &self.encounter_type)
                .field("beats_total", self.beat)
                .field("outcome", self.outcome.as_deref().unwrap_or("none"))
                .send();
        }

        // OTEL: encounter.state.phase_transition (only if phase actually
        // changed). No collision at the dispatch layer, but kept under the
        // `state.` prefix for consistency with its sibling events.
        if self.structured_phase != old_phase {
            let old_str = old_phase
                .map(|p| format!("{:?}", p))
                .unwrap_or_else(|| "None".to_string());
            WatcherEventBuilder::new("encounter", WatcherEventType::StateTransition)
                .field("event", "encounter.state.phase_transition")
                .field("encounter_type", &self.encounter_type)
                .field("old_phase", &old_str)
                .field("new_phase", &phase_str)
                .send();
        }

        Ok(())
    }

    /// Return the escalation target from the confrontation definition.
    pub fn escalation_target(&self, def: &ConfrontationDef) -> Option<String> {
        def.escalates_to.clone()
    }

    /// Produce a combat encounter from a resolved encounter, carrying actors.
    ///
    /// Returns None if the encounter is not yet resolved.
    pub fn escalate_to_combat(&self) -> Option<StructuredEncounter> {
        if !self.resolved {
            return None;
        }

        // OTEL: encounter.state.escalated — the `state.` prefix
        // disambiguates from dispatch/beat.rs's `encounter.escalation_started`
        // event (which fires from handle_applied_side_effects one layer up).
        WatcherEventBuilder::new("encounter", WatcherEventType::StateTransition)
            .field("event", "encounter.state.escalated")
            .field("from_type", &self.encounter_type)
            .field("to_type", "combat")
            .send();

        let actors = self
            .actors
            .iter()
            .map(|a| EncounterActor {
                name: a.name.clone(),
                role: "combatant".to_string(),
                per_actor_state: HashMap::new(),
            })
            .collect();

        Some(StructuredEncounter {
            encounter_type: "combat".to_string(),
            metric: EncounterMetric {
                name: "morale".to_string(),
                current: 100,
                starting: 100,
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
            mood_override: Some("combat".to_string()),
            narrator_hints: vec![],
        })
    }

    /// Format narrator prompt context for a structured encounter.
    ///
    /// Produces a context block like `[STANDOFF]` or `[NEGOTIATION]`
    /// with metric state, available beats, secondary stats, and
    /// cinematography hints.
    pub fn format_encounter_context(&self, def: &ConfrontationDef) -> String {
        let type_upper = self.encounter_type.to_uppercase();
        let phase_name = self
            .structured_phase
            .map(|p| format!("{:?}", p).to_uppercase())
            .unwrap_or_else(|| "UNKNOWN".to_string());

        let threshold = match self.metric.direction {
            MetricDirection::Ascending => self
                .metric
                .threshold_high
                .map(|t| format!("/{}", t))
                .unwrap_or_default(),
            MetricDirection::Descending => self
                .metric
                .threshold_low
                .map(|t| format!("/{}", t))
                .unwrap_or_default(),
            MetricDirection::Bidirectional => {
                let parts: Vec<String> = [
                    self.metric.threshold_low.map(|t| format!("low:{}", t)),
                    self.metric.threshold_high.map(|t| format!("high:{}", t)),
                ]
                .into_iter()
                .flatten()
                .collect();
                if parts.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", parts.join(", "))
                }
            }
        };

        let mut lines = vec![
            format!("[{}]", type_upper),
            format!(
                "Phase: {} | Beat: {} | {}: {}{}",
                phase_name,
                self.beat,
                capitalize(&self.metric.name),
                self.metric.current,
                threshold,
            ),
        ];

        // Secondary stats
        if let Some(ref stats) = self.secondary_stats {
            for (name, val) in &stats.stats {
                lines.push(format!(
                    "{}: {}/{} — spendable",
                    capitalize(name),
                    val.current,
                    val.max,
                ));
            }
        }

        // Actors — tell the narrator who is in this encounter so it can
        // emit beat_selections for each NPC, not just the player.
        // Without this, the narrator guesses NPCs from narrative context (pattern 5: LLM compensation).
        if !self.actors.is_empty() {
            let actor_list: Vec<String> = self
                .actors
                .iter()
                .map(|a| format!("{} ({})", a.name, a.role))
                .collect();
            lines.push(format!("Actors: {}", actor_list.join(", ")));
            lines.push(
                "Include a beat_selection for EVERY actor each round — player AND NPCs."
                    .to_string(),
            );
        }

        // Available beats
        lines.push("Available:".to_string());
        for (i, beat) in def.beats.iter().enumerate() {
            let mut desc = format!(
                "  {}. {} (id: {}) [{}]",
                i + 1,
                beat.label,
                beat.id,
                beat.stat_check
            );
            if beat.metric_delta != 0 {
                let sign = if beat.metric_delta > 0 { "+" } else { "" };
                desc.push_str(&format!(
                    " ({} {}{})",
                    self.metric.name, sign, beat.metric_delta
                ));
            }
            if let Some(ref reveals) = beat.reveals {
                desc.push_str(&format!(", reveals {}", reveals));
            }
            if let Some(ref risk) = beat.risk {
                desc.push_str(&format!(", risk: {}", risk));
            }
            if beat.resolution.unwrap_or(false) {
                desc.push_str(", resolves encounter");
            }
            if let Some(gd) = beat.gold_delta {
                let sign = if gd > 0 { "+" } else { "" };
                desc.push_str(&format!(", gold: {}{}", sign, gd));
            }
            if let Some(ref hint) = beat.narrator_hint {
                desc.push_str(&format!(", narrator_hint: {}", hint));
            }
            lines.push(desc);
        }

        // Cinematography hints — close-up, slow-motion for tense encounters
        let drama = self
            .structured_phase
            .map(|p| p.drama_weight())
            .unwrap_or(0.7);
        if drama >= 0.80 {
            lines.push(
                "Camera: Close-up, slow-motion | Pace: Peak intensity | Sentences: 2-4".to_string(),
            );
        } else {
            lines.push("Camera: Close-up | Pace: Building tension | Sentences: 2-4".to_string());
        }

        lines.join("\n")
    }
}

/// Capitalize the first letter of a string.
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

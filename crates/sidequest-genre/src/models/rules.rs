//! Game rules, resource declarations, and confrontation types from `rules.yaml`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════
// Resource declarations (story 16-1)
// ═══════════════════════════════════════════════════════════

/// Raw representation for deserialization with validation.
#[derive(Debug, Clone, Deserialize, Serialize)]
struct ResourceDeclarationRaw {
    name: String,
    label: String,
    min: f64,
    max: f64,
    starting: f64,
    voluntary: bool,
    decay_per_turn: f64,
}

/// Genre resource declaration (e.g., Luck, Humanity, Heat).
///
/// Declares a named resource that the narrator should track and reference.
/// Lightweight precursor to the formal ResourcePool (story 16-10).
/// Validates on deserialization: max >= min, starting in [min, max].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "ResourceDeclarationRaw")]
pub struct ResourceDeclaration {
    /// Internal name (e.g., "luck", "humanity").
    pub name: String,
    /// Display label (e.g., "Luck", "Humanity").
    pub label: String,
    /// Minimum value.
    pub min: f64,
    /// Maximum value.
    pub max: f64,
    /// Starting value for new sessions.
    pub starting: f64,
    /// Whether the player can voluntarily spend this resource.
    pub voluntary: bool,
    /// Automatic change per turn (e.g., -0.1 for Heat decay). 0.0 = no decay.
    pub decay_per_turn: f64,
}

impl TryFrom<ResourceDeclarationRaw> for ResourceDeclaration {
    type Error = String;

    fn try_from(raw: ResourceDeclarationRaw) -> Result<Self, Self::Error> {
        if raw.max < raw.min {
            return Err(format!(
                "resource '{}': max ({}) must be >= min ({})",
                raw.name, raw.max, raw.min
            ));
        }
        if raw.starting < raw.min || raw.starting > raw.max {
            return Err(format!(
                "resource '{}': starting ({}) must be in [{}, {}]",
                raw.name, raw.starting, raw.min, raw.max
            ));
        }
        Ok(Self {
            name: raw.name,
            label: raw.label,
            min: raw.min,
            max: raw.max,
            starting: raw.starting,
            voluntary: raw.voluntary,
            decay_per_turn: raw.decay_per_turn,
        })
    }
}

// ═══════════════════════════════════════════════════════════
// Confrontation declarations (story 16-3)
// ═══════════════════════════════════════════════════════════

const VALID_CATEGORIES: &[&str] = &["combat", "social", "pre_combat", "movement"];
const VALID_DIRECTIONS: &[&str] = &["ascending", "descending", "bidirectional"];

/// A secondary stat derived from an ability score, usable during confrontations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecondaryStatDef {
    /// Internal name (e.g., "focus", "shields").
    pub name: String,
    /// Ability score this stat derives from.
    pub source_stat: String,
    /// Whether the player can voluntarily spend this stat.
    pub spendable: bool,
}

/// Raw beat definition for deserialization with validation.
#[derive(Debug, Clone, Deserialize)]
struct RawBeatDef {
    id: String,
    label: String,
    metric_delta: i32,
    stat_check: String,
    #[serde(default)]
    risk: Option<String>,
    #[serde(default)]
    reveals: Option<String>,
    #[serde(default)]
    resolution: Option<bool>,
}

/// A single action available during a confrontation.
///
/// Validated on deserialization: id must not be empty.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "RawBeatDef")]
pub struct BeatDef {
    /// Unique identifier within the confrontation (e.g., "attack", "draw").
    pub id: String,
    /// Display label.
    pub label: String,
    /// How much this beat changes the primary metric.
    pub metric_delta: i32,
    /// Ability score checked when performing this beat.
    pub stat_check: String,
    /// Risk description if this beat can backfire.
    #[serde(default)]
    pub risk: Option<String>,
    /// What information this beat reveals.
    #[serde(default)]
    pub reveals: Option<String>,
    /// Whether this beat can resolve the confrontation.
    #[serde(default)]
    pub resolution: Option<bool>,
}

impl TryFrom<RawBeatDef> for BeatDef {
    type Error = String;

    fn try_from(raw: RawBeatDef) -> Result<Self, Self::Error> {
        if raw.id.is_empty() {
            return Err("beat id must not be empty".to_string());
        }
        Ok(Self {
            id: raw.id,
            label: raw.label,
            metric_delta: raw.metric_delta,
            stat_check: raw.stat_check,
            risk: raw.risk,
            reveals: raw.reveals,
            resolution: raw.resolution,
        })
    }
}

/// Raw metric definition for deserialization with validation.
#[derive(Debug, Clone, Deserialize)]
struct RawMetricDef {
    name: String,
    direction: String,
    starting: i32,
    #[serde(default)]
    threshold_high: Option<i32>,
    #[serde(default)]
    threshold_low: Option<i32>,
}

/// The primary tracking metric for a confrontation type.
///
/// Validated on deserialization: direction must be ascending/descending/bidirectional.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "RawMetricDef")]
pub struct MetricDef {
    /// Metric name (e.g., "hp", "tension", "leverage").
    pub name: String,
    /// Direction: "ascending", "descending", or "bidirectional".
    pub direction: String,
    /// Starting value at confrontation begin.
    pub starting: i32,
    /// Upper threshold for resolution (if applicable).
    #[serde(default)]
    pub threshold_high: Option<i32>,
    /// Lower threshold for resolution (if applicable).
    #[serde(default)]
    pub threshold_low: Option<i32>,
}

impl TryFrom<RawMetricDef> for MetricDef {
    type Error = String;

    fn try_from(raw: RawMetricDef) -> Result<Self, Self::Error> {
        if !VALID_DIRECTIONS.contains(&raw.direction.as_str()) {
            return Err(format!(
                "invalid metric direction '{}': must be one of {:?}",
                raw.direction, VALID_DIRECTIONS
            ));
        }
        Ok(Self {
            name: raw.name,
            direction: raw.direction,
            starting: raw.starting,
            threshold_high: raw.threshold_high,
            threshold_low: raw.threshold_low,
        })
    }
}

/// Raw confrontation definition for deserialization with validation.
#[derive(Debug, Clone, Deserialize)]
struct RawConfrontationDef {
    #[serde(rename = "type")]
    confrontation_type: String,
    label: String,
    category: String,
    metric: MetricDef,
    beats: Vec<BeatDef>,
    #[serde(default)]
    secondary_stats: Vec<SecondaryStatDef>,
    #[serde(default)]
    escalates_to: Option<String>,
    #[serde(default)]
    mood: Option<String>,
}

/// A confrontation type declared by a genre pack in rules.yaml.
///
/// Validated on deserialization: type must not be empty, category must be valid,
/// beats must not be empty, beat IDs must be unique.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "RawConfrontationDef")]
pub struct ConfrontationDef {
    /// Confrontation type identifier (e.g., "combat", "standoff", "negotiation").
    /// Serializes as `type` in YAML.
    #[serde(rename = "type")]
    pub confrontation_type: String,
    /// Display label.
    pub label: String,
    /// Category: "combat", "social", "pre_combat", or "movement".
    pub category: String,
    /// Primary tracking metric.
    pub metric: MetricDef,
    /// Available actions during this confrontation.
    pub beats: Vec<BeatDef>,
    /// Optional secondary stats derived from ability scores.
    #[serde(default)]
    pub secondary_stats: Vec<SecondaryStatDef>,
    /// Confrontation type this can escalate to (e.g., standoff → combat).
    #[serde(default)]
    pub escalates_to: Option<String>,
    /// Mood override for MusicDirector.
    #[serde(default)]
    pub mood: Option<String>,
}

impl TryFrom<RawConfrontationDef> for ConfrontationDef {
    type Error = String;

    fn try_from(raw: RawConfrontationDef) -> Result<Self, Self::Error> {
        if raw.confrontation_type.is_empty() {
            return Err("confrontation type must not be empty".to_string());
        }
        if !VALID_CATEGORIES.contains(&raw.category.as_str()) {
            return Err(format!(
                "invalid confrontation category '{}': must be one of {:?}",
                raw.category, VALID_CATEGORIES
            ));
        }
        if raw.beats.is_empty() {
            return Err(format!(
                "confrontation '{}' must have at least one beat",
                raw.confrontation_type
            ));
        }
        // Check for duplicate beat IDs
        let mut seen = std::collections::HashSet::new();
        for beat in &raw.beats {
            if !seen.insert(&beat.id) {
                return Err(format!(
                    "confrontation '{}' has duplicate beat id '{}'",
                    raw.confrontation_type, beat.id
                ));
            }
        }
        Ok(Self {
            confrontation_type: raw.confrontation_type,
            label: raw.label,
            category: raw.category,
            metric: raw.metric,
            beats: raw.beats,
            secondary_stats: raw.secondary_stats,
            escalates_to: raw.escalates_to,
            mood: raw.mood,
        })
    }
}

// ═══════════════════════════════════════════════════════════
// rules.yaml
// ═══════════════════════════════════════════════════════════

/// Game rules configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RulesConfig {
    /// Narrative tone (e.g., "gonzo-sincere").
    #[serde(default)]
    pub tone: String,
    /// How deadly combat is.
    #[serde(default)]
    pub lethality: String,
    /// Magic system level.
    pub magic_level: String,
    /// How ability scores are generated.
    pub stat_generation: String,
    /// Point budget for point-buy generation.
    pub point_buy_budget: u32,
    /// Names for the six ability scores.
    pub ability_score_names: Vec<String>,
    /// Available character classes.
    pub allowed_classes: Vec<String>,
    /// Available character races.
    pub allowed_races: Vec<String>,
    /// Base HP per class.
    pub class_hp_bases: HashMap<String, u32>,
    /// Default character class.
    #[serde(default)]
    pub default_class: Option<String>,
    /// Default race.
    #[serde(default)]
    pub default_race: Option<String>,
    /// Default HP value.
    #[serde(default)]
    pub default_hp: Option<u32>,
    /// Default AC value.
    #[serde(default)]
    pub default_ac: Option<u32>,
    /// Display label for "Race" (genre-specific, e.g. "Origin", "Background").
    #[serde(default)]
    pub race_label: Option<String>,
    /// Display label for "Class" (genre-specific, e.g. "Discipline", "Role").
    #[serde(default)]
    pub class_label: Option<String>,
    /// Default starting location description.
    #[serde(default)]
    pub default_location: Option<String>,
    /// Default time of day.
    #[serde(default)]
    pub default_time_of_day: Option<String>,
    /// HP formula string (e.g., "class_base * level").
    #[serde(default)]
    pub hp_formula: Option<String>,
    /// Spells that are banned.
    #[serde(default)]
    pub banned_spells: Vec<String>,
    /// Custom rule overrides (lethality, injury_system, etc.).
    #[serde(default)]
    pub custom_rules: HashMap<String, String>,
    /// Stat fields to show in the UI.
    #[serde(default)]
    pub stat_display_fields: Vec<String>,
    /// Base tension values per encounter type.
    #[serde(default)]
    pub encounter_base_tension: HashMap<String, f64>,
    /// Genre resource declarations (story 16-1). Empty for genres without resources.
    #[serde(default)]
    pub resources: Vec<ResourceDeclaration>,
    /// Confrontation type declarations (story 16-3). Empty for genres without confrontations.
    #[serde(default)]
    pub confrontations: Vec<ConfrontationDef>,
}

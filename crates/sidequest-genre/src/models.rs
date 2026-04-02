//! Genre pack model structs.
//!
//! Structs use `#[serde(deny_unknown_fields)]` where appropriate to catch YAML
//! typos. Content structs that genre packs extend use `#[serde(flatten)]` extras
//! bags instead, allowing genre-specific fields without breaking deserialization.

use rand::Rng;
use serde::{Deserialize, Deserializer, Serialize};
use sidequest_protocol::NonBlankString;
use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════
// Pacing thresholds (loaded from pacing.yaml, consumed by game crate)
// ═══════════════════════════════════════════════════════════

/// Genre-tunable breakpoints for pacing decisions.
///
/// Loaded from an optional `pacing.yaml` in the genre pack directory.
/// Missing fields fall back to defaults via `#[serde(default)]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DramaThresholds {
    /// Drama weight at or above which delivery switches from Instant to Sentence.
    pub sentence_delivery_min: f64,
    /// Drama weight above which delivery switches from Sentence to Streaming.
    pub streaming_delivery_min: f64,
    /// Drama weight above which image rendering is triggered (beat filter).
    pub render_threshold: f64,
    /// Consecutive boring turns before an escalation beat hint is injected.
    pub escalation_streak: u32,
    /// Number of boring turns to reach action_tension 1.0 (gambler's ramp length).
    pub ramp_length: u32,
}

impl Default for DramaThresholds {
    fn default() -> Self {
        Self {
            sentence_delivery_min: 0.30,
            streaming_delivery_min: 0.70,
            render_threshold: 0.40,
            escalation_streak: 5,
            ramp_length: 8,
        }
    }
}

// ═══════════════════════════════════════════════════════════
// OCEAN personality profile (loaded from archetypes.yaml, consumed by game crate)
// ═══════════════════════════════════════════════════════════

/// Clamp a value to the 0.0–10.0 range.
fn clamp_dimension(v: f64) -> f64 {
    v.clamp(0.0, 10.0)
}

/// Deserialize an f64 and clamp it to [0.0, 10.0].
fn deserialize_clamped<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
    let v = f64::deserialize(deserializer)?;
    Ok(clamp_dimension(v))
}

fn neutral() -> f64 {
    5.0
}

/// Big Five (OCEAN) personality profile.
///
/// Each dimension is an f64 in the range 0.0–10.0. Out-of-range values are
/// clamped on deserialization. Default is 5.0 (neutral) for all dimensions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OceanProfile {
    /// Openness to experience.
    #[serde(default = "neutral", deserialize_with = "deserialize_clamped")]
    pub openness: f64,
    /// Conscientiousness.
    #[serde(default = "neutral", deserialize_with = "deserialize_clamped")]
    pub conscientiousness: f64,
    /// Extraversion.
    #[serde(default = "neutral", deserialize_with = "deserialize_clamped")]
    pub extraversion: f64,
    /// Agreeableness.
    #[serde(default = "neutral", deserialize_with = "deserialize_clamped")]
    pub agreeableness: f64,
    /// Neuroticism.
    #[serde(default = "neutral", deserialize_with = "deserialize_clamped")]
    pub neuroticism: f64,
}

impl Default for OceanProfile {
    fn default() -> Self {
        Self {
            openness: 5.0,
            conscientiousness: 5.0,
            extraversion: 5.0,
            agreeableness: 5.0,
            neuroticism: 5.0,
        }
    }
}

impl OceanProfile {
    /// Generate a fully random OCEAN profile with values in [0.0, 10.0].
    pub fn random() -> Self {
        let mut rng = rand::rng();
        Self {
            openness: rng.random_range(0.0..=10.0),
            conscientiousness: rng.random_range(0.0..=10.0),
            extraversion: rng.random_range(0.0..=10.0),
            agreeableness: rng.random_range(0.0..=10.0),
            neuroticism: rng.random_range(0.0..=10.0),
        }
    }

    /// Produce a natural-language behavioral summary from OCEAN scores.
    ///
    /// Dimensions with extreme scores (low 0–3, high 7–10) contribute
    /// adjectives; mid-range dimensions are omitted. An all-neutral profile
    /// returns a fallback phrase.
    pub fn behavioral_summary(&self) -> String {
        let dimensions: &[(f64, &str, &str)] = &[
            (self.openness, "conventional and practical", "curious and imaginative"),
            (self.conscientiousness, "spontaneous and flexible", "meticulous and disciplined"),
            (self.extraversion, "reserved and quiet", "outgoing and talkative"),
            (self.agreeableness, "competitive and blunt", "cooperative and empathetic"),
            (self.neuroticism, "calm and steady", "anxious and volatile"),
        ];

        let descriptors: Vec<&str> = dimensions
            .iter()
            .filter_map(|&(score, low, high)| {
                if score <= 3.0 {
                    Some(low)
                } else if score >= 7.0 {
                    Some(high)
                } else {
                    None
                }
            })
            .collect();

        match descriptors.len() {
            0 => "balanced temperament".to_string(),
            1 => descriptors[0].to_string(),
            _ => {
                let (last, rest) = descriptors.split_last().unwrap();
                format!("{}, and {last}", rest.join(", "))
            }
        }
    }

    /// Return a new profile jittered by up to ±`max_delta` per dimension,
    /// clamped to [0.0, 10.0].
    pub fn with_jitter(&self, max_delta: f64) -> Self {
        let mut rng = rand::rng();
        let mut jitter = |base: f64| -> f64 {
            let offset = rng.random_range(-max_delta..=max_delta);
            clamp_dimension(base + offset)
        };
        Self {
            openness: jitter(self.openness),
            conscientiousness: jitter(self.conscientiousness),
            extraversion: jitter(self.extraversion),
            agreeableness: jitter(self.agreeableness),
            neuroticism: jitter(self.neuroticism),
        }
    }
}

// ═══════════════════════════════════════════════════════════
// OCEAN dimension enum & shift log (story 10-5)
// ═══════════════════════════════════════════════════════════

/// One of the Big Five personality dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OceanDimension {
    /// Openness to experience.
    Openness,
    /// Conscientiousness.
    Conscientiousness,
    /// Extraversion.
    Extraversion,
    /// Agreeableness.
    Agreeableness,
    /// Neuroticism.
    Neuroticism,
}

/// A single recorded personality shift.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OceanShift {
    /// Which OCEAN dimension changed.
    pub dimension: OceanDimension,
    /// Value before the shift.
    pub old_value: f64,
    /// Value after the shift (clamped to 0.0–10.0).
    pub new_value: f64,
    /// Free-text reason for the change.
    pub cause: String,
    /// Game turn when the shift occurred.
    pub turn: u32,
}

/// Append-only log of personality shifts.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OceanShiftLog {
    shifts: Vec<OceanShift>,
}

impl OceanShiftLog {
    /// Append a shift entry.
    pub fn push(&mut self, shift: OceanShift) {
        self.shifts.push(shift);
    }

    /// Return all recorded shifts.
    pub fn shifts(&self) -> &[OceanShift] {
        &self.shifts
    }

    /// Return shifts for a specific dimension.
    pub fn shifts_for(&self, dimension: OceanDimension) -> Vec<&OceanShift> {
        self.shifts.iter().filter(|s| s.dimension == dimension).collect()
    }
}

impl OceanProfile {
    /// Apply a delta to a single dimension, clamp, log the shift, and return
    /// the new value.
    pub fn apply_shift(
        &mut self,
        dimension: OceanDimension,
        delta: f64,
        cause: String,
        turn: u32,
        log: &mut OceanShiftLog,
    ) -> f64 {
        let old_value = self.get(dimension);
        let new_value = (old_value + delta).clamp(0.0, 10.0);
        match dimension {
            OceanDimension::Openness => self.openness = new_value,
            OceanDimension::Conscientiousness => self.conscientiousness = new_value,
            OceanDimension::Extraversion => self.extraversion = new_value,
            OceanDimension::Agreeableness => self.agreeableness = new_value,
            OceanDimension::Neuroticism => self.neuroticism = new_value,
        }
        log.push(OceanShift {
            dimension,
            old_value,
            new_value,
            cause,
            turn,
        });
        new_value
    }

    /// Read a dimension's current value.
    pub fn get(&self, dimension: OceanDimension) -> f64 {
        match dimension {
            OceanDimension::Openness => self.openness,
            OceanDimension::Conscientiousness => self.conscientiousness,
            OceanDimension::Extraversion => self.extraversion,
            OceanDimension::Agreeableness => self.agreeableness,
            OceanDimension::Neuroticism => self.neuroticism,
        }
    }
}

// ═══════════════════════════════════════════════════════════
// Top-level aggregates (assembled by loader, not deserialized directly)
// ═══════════════════════════════════════════════════════════

/// A fully-loaded genre pack with all YAML files assembled.
#[derive(Debug, Clone)]
pub struct GenrePack {
    /// Pack metadata from `pack.yaml`.
    pub meta: PackMeta,
    /// Game rules from `rules.yaml`.
    pub rules: RulesConfig,
    /// Genre-level lore from `lore.yaml`.
    pub lore: Lore,
    /// UI theme from `theme.yaml`.
    pub theme: GenreTheme,
    /// NPC archetypes from `archetypes.yaml`.
    pub archetypes: Vec<NpcArchetype>,
    /// Character creation scenes from `char_creation.yaml`.
    pub char_creation: Vec<CharCreationScene>,
    /// Image generation style from `visual_style.yaml`.
    pub visual_style: VisualStyle,
    /// Progression system from `progression.yaml`.
    pub progression: ProgressionConfig,
    /// Narrative axes from `axes.yaml`.
    pub axes: AxesConfig,
    /// Audio configuration from `audio.yaml`.
    pub audio: AudioConfig,
    /// Name generation cultures from `cultures.yaml`.
    pub cultures: Vec<Culture>,
    /// LLM prompt templates from `prompts.yaml`.
    pub prompts: Prompts,
    /// Genre-level tropes from `tropes.yaml` (may be empty).
    pub tropes: Vec<TropeDefinition>,
    /// Chase/beat vocabulary from `beat_vocabulary.yaml`.
    pub beat_vocabulary: Option<BeatVocabulary>,
    /// Trope-linked achievements from `achievements.yaml`.
    pub achievements: Vec<Achievement>,
    /// TTS voice presets from `voice_presets.yaml`.
    pub voice_presets: Option<VoicePresets>,
    /// Per-class power tier descriptions from `power_tiers.yaml`.
    pub power_tiers: HashMap<String, Vec<PowerTier>>,
    /// Worlds loaded from `worlds/*/`.
    pub worlds: HashMap<String, World>,
    /// Scenario packs loaded from `scenarios/*/`.
    pub scenarios: HashMap<String, ScenarioPack>,
    /// Pacing thresholds from `pacing.yaml` (optional per genre pack).
    pub drama_thresholds: Option<DramaThresholds>,
    /// Item catalog and starting loadouts from `inventory.yaml` (optional per genre pack).
    pub inventory: Option<InventoryConfig>,
    /// Opening scenario hooks from `openings.yaml` (optional per genre pack).
    pub openings: Vec<OpeningHook>,
}

/// A world within a genre pack, assembled from `worlds/{slug}/`.
#[derive(Debug, Clone)]
pub struct World {
    /// World metadata from `world.yaml`.
    pub config: WorldConfig,
    /// World-specific lore from `lore.yaml`.
    pub lore: WorldLore,
    /// Historical legends from `legends.yaml`.
    pub legends: Vec<Legend>,
    /// Map regions and routes from `cartography.yaml`.
    pub cartography: CartographyConfig,
    /// World-specific cultures from `cultures.yaml`.
    pub cultures: Vec<Culture>,
    /// World-specific tropes from `tropes.yaml` (resolved inheritance).
    pub tropes: Vec<TropeDefinition>,
    /// World-level NPC archetypes (overrides genre-level when present).
    pub archetypes: Vec<NpcArchetype>,
    /// World-level visual style (overrides genre-level when present).
    /// Stored as raw JSON because world visual_style can have richer structure
    /// than the genre-level VisualStyle struct (e.g., per-region overrides with maps).
    pub visual_style: Option<serde_json::Value>,
    /// World-level campaign history (road_warrior format — not in low_fantasy).
    pub history: Option<serde_json::Value>,
    /// Raw legends data (preserves origin_myth and other map-format data).
    pub legends_raw: Option<serde_json::Value>,
}

// ═══════════════════════════════════════════════════════════
// pack.yaml
// ═══════════════════════════════════════════════════════════

/// Genre pack metadata.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PackMeta {
    /// Display name of the genre pack.
    pub name: NonBlankString,
    /// Semantic version string.
    pub version: String,
    /// Short description.
    pub description: String,
    /// Minimum engine version required.
    pub min_sidequest_version: String,
    /// Whether to enable refine hooks.
    #[serde(default)]
    pub refine_hooks: Option<bool>,
    /// Creative inspirations.
    #[serde(default)]
    pub inspirations: Vec<Inspiration>,
    /// Era range string (e.g., "1910s-1930s").
    #[serde(default)]
    pub era_range: Option<String>,
    /// Core vibe description.
    #[serde(default)]
    pub core_vibe: Option<String>,
    /// Emotional tone keywords.
    #[serde(default)]
    pub emotional_tone: Vec<String>,
    /// What differentiates this genre.
    #[serde(default)]
    pub differentiation: Option<String>,
}

/// A creative inspiration reference.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Inspiration {
    /// Source name.
    pub name: String,
    /// Which element is borrowed.
    pub element: String,
}

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

// ═══════════════════════════════════════════════════════════
// theme.yaml
// ═══════════════════════════════════════════════════════════

/// UI theme colors and typography.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GenreTheme {
    /// Primary color hex.
    pub primary: String,
    /// Secondary color hex.
    pub secondary: String,
    /// Accent color hex.
    pub accent: String,
    /// Background color hex.
    pub background: String,
    /// Surface color hex.
    pub surface: String,
    /// Text color hex.
    pub text: String,
    /// Border style name.
    pub border_style: String,
    /// Web font family.
    pub web_font_family: String,
    /// Section break (dinkus) configuration.
    pub dinkus: Dinkus,
    /// Session opener configuration.
    pub session_opener: SessionOpener,
}

impl GenreTheme {
    /// Generate the base CSS for the client from theme.yaml fields.
    ///
    /// Produces `:root` CSS variables, body styles, and base component classes.
    /// Genre-specific `client_theme.css` overrides should be appended after this.
    pub fn generate_css(&self) -> String {
        let dinkus_glyph = self
            .dinkus
            .glyph
            .get("light")
            .map(|s| s.as_str())
            .unwrap_or("◇");

        let font = if self.web_font_family.contains(',') {
            self.web_font_family.clone()
        } else {
            format!("'{}', Georgia, 'Times New Roman', serif", self.web_font_family)
        };

        format!(
            r#":root {{
  --primary: {primary};
  --secondary: {secondary};
  --accent: {accent};
  --background: {background};
  --surface: {surface};
  --text: {text};
  --dinkus-glyph: '{dinkus_glyph}';
}}

body {{
  background-color: var(--background);
  color: var(--text);
  font-family: {font};
  margin: 0 auto;
  padding: 16px;
  max-width: 720px;
  line-height: 1.6;
}}

.narration-block {{ margin-bottom: 1em; }}

.whisper {{
  border-left: 3px solid var(--accent);
  padding-left: 12px;
  font-style: italic;
  opacity: 0.85;
}}

.scene-image {{ max-width: 100%; border-radius: 4px; margin: 8px 0; }}

.drop-cap-block img {{ float: left; width: 80px; height: 80px; margin: 0 12px 4px 0; }}

.visually-hidden {{
  position: absolute;
  width: 1px; height: 1px;
  overflow: hidden;
  clip: rect(0, 0, 0, 0);
  white-space: nowrap;
}}

.dinkus {{
  text-align: center;
  margin: 24px 0;
  opacity: 0.6;
  letter-spacing: 0.3em;
  font-size: 1.2em;
  user-select: none;
}}

.pull-quote {{
  text-align: center;
  font-size: 1.2em;
  font-style: italic;
  border-left: 4px solid var(--accent);
  margin: 1.5em auto;
  padding: 0.8em 1.2em;
  max-width: 85%;
}}

.session-opener {{
  font-size: 1.15em;
  border-bottom: 2px solid var(--accent);
  margin: 0 0 32px 0;
  padding: 16px 0 24px 0;
}}
"#,
            primary = self.primary,
            secondary = self.secondary,
            accent = self.accent,
            background = self.background,
            surface = self.surface,
            text = self.text,
            dinkus_glyph = dinkus_glyph,
            font = font,
        )
    }
}

/// Section break (dinkus) glyphs.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Dinkus {
    /// Whether dinkus is enabled.
    pub enabled: bool,
    /// Minimum paragraphs between dinkus.
    pub cooldown: u32,
    /// Default weight level.
    pub default_weight: String,
    /// Glyph strings keyed by weight (light, medium, heavy).
    pub glyph: HashMap<String, String>,
}

/// Session opener configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SessionOpener {
    /// Whether session openers are enabled.
    pub enabled: bool,
}

// ═══════════════════════════════════════════════════════════
// lore.yaml (genre-level)
// ═══════════════════════════════════════════════════════════

/// Genre-level lore.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Lore {
    /// World name (may be empty at genre level).
    pub world_name: String,
    /// History text.
    pub history: String,
    /// Geography description (may be empty).
    pub geography: String,
    /// Cosmology / religion / metaphysics.
    pub cosmology: String,
    /// Factions (some genre-level packs include factions at the top level).
    #[serde(default)]
    pub factions: Vec<Faction>,
    /// Genre-specific lore extensions (setting_anchor, etc.).
    #[serde(flatten)]
    pub extras: HashMap<String, serde_json::Value>,
}

// ═══════════════════════════════════════════════════════════
// lore.yaml (world-level) — extends genre lore with factions
// ═══════════════════════════════════════════════════════════

/// World-specific lore with factions.
///
/// Accepts both the low_fantasy format (world_name/history/geography/cosmology)
/// and the road_warrior format (setting/faction_relations/daily_life).
/// Genre-specific fields land in `extras` for AI prompt injection.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorldLore {
    /// World name (low_fantasy format).
    #[serde(default)]
    pub world_name: Option<String>,
    /// History text (low_fantasy format).
    #[serde(default)]
    pub history: Option<String>,
    /// Geography description (low_fantasy format).
    #[serde(default)]
    pub geography: Option<String>,
    /// Cosmology text (low_fantasy format).
    #[serde(default)]
    pub cosmology: Option<String>,
    /// Political factions (simple format — name/description/disposition).
    #[serde(default)]
    pub factions: Vec<Faction>,
    /// Genre-specific lore extensions (setting, faction_relations, daily_life, etc.).
    #[serde(flatten)]
    pub extras: HashMap<String, serde_json::Value>,
}

/// A political or social faction.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Faction {
    /// Faction name.
    pub name: String,
    /// Description of the faction.
    pub description: String,
    /// Starting disposition toward the player.
    #[serde(default)]
    pub disposition: String,
    /// Genre-specific faction extensions.
    #[serde(flatten)]
    pub extras: HashMap<String, serde_json::Value>,
}

// ═══════════════════════════════════════════════════════════
// archetypes.yaml
// ═══════════════════════════════════════════════════════════

/// An NPC archetype template.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NpcArchetype {
    /// Archetype name.
    pub name: NonBlankString,
    /// Description.
    pub description: String,
    /// Personality trait keywords.
    pub personality_traits: Vec<String>,
    /// Typical character classes.
    pub typical_classes: Vec<String>,
    /// Typical character races.
    pub typical_races: Vec<String>,
    /// Stat name → [min, max] ranges.
    pub stat_ranges: HashMap<String, [i32; 2]>,
    /// Starting inventory suggestions.
    pub inventory_hints: Vec<String>,
    /// Speech pattern descriptions.
    pub dialogue_quirks: Vec<String>,
    /// Default disposition value.
    pub disposition_default: i32,
    /// Item catalog references for starting gear.
    #[serde(default)]
    pub catalog_items: Vec<String>,
    /// Optional OCEAN personality baseline for this archetype.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ocean: Option<OceanProfile>,
}

// ═══════════════════════════════════════════════════════════
// char_creation.yaml
// ═══════════════════════════════════════════════════════════

/// A character creation scene with narrative choices.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CharCreationScene {
    /// Scene identifier.
    pub id: String,
    /// Display title.
    pub title: String,
    /// Narrator text for this scene.
    pub narration: String,
    /// Player choices (may be empty for the final confirmation scene).
    pub choices: Vec<CharCreationChoice>,
    /// Whether this scene allows freeform text input.
    #[serde(default)]
    pub allows_freeform: Option<bool>,
    /// Optional followup prompt — if present, the builder enters AwaitingFollowup
    /// after a choice is made, waiting for the player's freeform elaboration.
    #[serde(default)]
    pub hook_prompt: Option<String>,
}

/// A choice within a character creation scene.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CharCreationChoice {
    /// Display label.
    pub label: String,
    /// Description text.
    pub description: String,
    /// Game-mechanical effects of this choice.
    pub mechanical_effects: MechanicalEffects,
}

/// Mechanical effects of a character creation choice.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MechanicalEffects {
    /// Suggested class.
    #[serde(default)]
    pub class_hint: Option<String>,
    /// Suggested race.
    #[serde(default)]
    pub race_hint: Option<String>,
    /// Suggested mutation.
    #[serde(default)]
    pub mutation_hint: Option<String>,
    /// Suggested starting item.
    #[serde(default)]
    pub item_hint: Option<String>,
    /// Suggested affinity.
    #[serde(default)]
    pub affinity_hint: Option<String>,
    /// Suggested training path.
    #[serde(default)]
    pub training_hint: Option<String>,
    /// Background context.
    #[serde(default)]
    pub background: Option<String>,
    /// Personality trait hint.
    #[serde(default)]
    pub personality_trait: Option<String>,
    /// Emotional state hint.
    #[serde(default)]
    pub emotional_state: Option<String>,
    /// Relationship hint.
    #[serde(default)]
    pub relationship: Option<String>,
    /// Goals hint.
    #[serde(default)]
    pub goals: Option<String>,
    /// Whether freeform input is allowed.
    #[serde(default)]
    pub allows_freeform: Option<bool>,
    /// Rig type hint (vehicle-based genres).
    #[serde(default)]
    pub rig_type_hint: Option<String>,
    /// Rig trait hint.
    #[serde(default)]
    pub rig_trait: Option<String>,
    /// Catch phrase or hook.
    #[serde(default, rename = "catch")]
    pub catch_phrase: Option<String>,
    /// Stat bonuses from this choice (e.g. {"Strength": 2, "Agility": -1}).
    #[serde(default)]
    pub stat_bonuses: HashMap<String, i32>,
    /// Pronoun hint from character creation (e.g. "she/her", "he/him", "they/them").
    #[serde(default)]
    pub pronoun_hint: Option<String>,
}

// ═══════════════════════════════════════════════════════════
// visual_style.yaml
// ═══════════════════════════════════════════════════════════

/// Image generation style configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VisualStyle {
    /// Positive prompt suffix for image generation.
    pub positive_suffix: String,
    /// Negative prompt for image generation.
    pub negative_prompt: String,
    /// Preferred image generation model.
    pub preferred_model: String,
    /// Base random seed.
    pub base_seed: u32,
    /// Location-tag → style override mappings.
    #[serde(default)]
    pub visual_tag_overrides: HashMap<String, String>,
}

// ═══════════════════════════════════════════════════════════
// tropes.yaml
// ═══════════════════════════════════════════════════════════

/// A narrative trope definition (genre-level or world-level).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TropeDefinition {
    /// Trope identifier (optional — some tropes use name-based slugs).
    #[serde(default)]
    pub id: Option<String>,
    /// Display name.
    pub name: NonBlankString,
    /// Optional description.
    #[serde(default)]
    pub description: Option<String>,
    /// Narrative category (conflict, revelation, recurring, climax, etc.).
    #[serde(default)]
    pub category: String,
    /// Player actions or events that activate this trope.
    #[serde(default)]
    pub triggers: Vec<String>,
    /// Narrator guidance when this trope is active.
    #[serde(default)]
    pub narrative_hints: Vec<String>,
    /// Base tension level (0.0–1.0). None means "inherit from parent" during merge.
    #[serde(default)]
    pub tension_level: Option<f64>,
    /// Suggested ways the trope can resolve.
    #[serde(default)]
    pub resolution_hints: Option<Vec<String>>,
    /// Resolution patterns (used by abstract tropes).
    #[serde(default)]
    pub resolution_patterns: Option<Vec<String>>,
    /// Categorization tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Escalation steps keyed by progression value.
    #[serde(default)]
    pub escalation: Vec<TropeEscalation>,
    /// Passive progression configuration.
    #[serde(default)]
    pub passive_progression: Option<PassiveProgression>,
    /// Whether this is an abstract archetype (must be extended by world tropes).
    #[serde(default, rename = "abstract")]
    pub is_abstract: bool,
    /// Parent trope slug to inherit from.
    #[serde(default)]
    pub extends: Option<String>,
}

/// A single escalation step within a trope.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TropeEscalation {
    /// Progression threshold (0.0–1.0) at which this fires.
    pub at: f64,
    /// Narrative event description.
    pub event: String,
    /// NPCs involved in this escalation.
    #[serde(default)]
    pub npcs_involved: Vec<String>,
    /// What's at stake.
    #[serde(default)]
    pub stakes: String,
}

/// Passive progression configuration for a trope.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PassiveProgression {
    /// Progression per game turn.
    #[serde(default)]
    pub rate_per_turn: f64,
    /// Progression per in-game day.
    #[serde(default)]
    pub rate_per_day: f64,
    /// Keywords that accelerate progression.
    #[serde(default)]
    pub accelerators: Vec<String>,
    /// Keywords that decelerate progression.
    #[serde(default)]
    pub decelerators: Vec<String>,
    /// Bonus per accelerator match.
    #[serde(default)]
    pub accelerator_bonus: f64,
    /// Penalty per decelerator match.
    #[serde(default)]
    pub decelerator_penalty: f64,
}

// ═══════════════════════════════════════════════════════════
// progression.yaml
// ═══════════════════════════════════════════════════════════

/// Character progression configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProgressionConfig {
    /// Skill/affinity trees.
    #[serde(default)]
    pub affinities: Vec<Affinity>,
    /// Categories for milestone tracking.
    #[serde(default)]
    pub milestone_categories: Vec<String>,
    /// Milestones required per level.
    #[serde(default)]
    pub milestones_per_level: u32,
    /// Maximum character level.
    #[serde(default)]
    pub max_level: u32,
    /// Item naming/power-up thresholds.
    #[serde(default)]
    pub item_evolution: Option<ItemEvolution>,
    /// Per-level stat bonuses.
    #[serde(default)]
    pub level_bonuses: Option<LevelBonuses>,
    /// Wealth tier labels.
    #[serde(default)]
    pub wealth_tiers: Vec<WealthTier>,
}

/// A skill/affinity tree.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Affinity {
    /// Affinity name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Player actions that earn XP in this affinity.
    pub triggers: Vec<String>,
    /// XP thresholds for each tier.
    pub tier_thresholds: Vec<u32>,
    /// Unlockable abilities per tier.
    #[serde(default)]
    pub unlocks: Option<AffinityUnlocks>,
}

/// Tier unlocks for an affinity (fixed set: tier_1, tier_2, tier_3).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AffinityUnlocks {
    /// Tier 0 (starting) abilities.
    #[serde(default)]
    pub tier_0: Option<AffinityTier>,
    /// Tier 1 abilities.
    #[serde(default)]
    pub tier_1: Option<AffinityTier>,
    /// Tier 2 abilities.
    #[serde(default)]
    pub tier_2: Option<AffinityTier>,
    /// Tier 3 abilities.
    #[serde(default)]
    pub tier_3: Option<AffinityTier>,
}

/// A single tier within an affinity.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AffinityTier {
    /// Tier display name.
    pub name: String,
    /// Description of reaching this tier.
    pub description: String,
    /// Abilities unlocked at this tier.
    pub abilities: Vec<Ability>,
}

/// An ability within an affinity tier.
///
/// Can be either a simple string description or a full struct with
/// name, experience narrative, and limits.
#[derive(Debug, Clone, Serialize)]
pub struct Ability {
    /// Ability name.
    pub name: String,
    /// Narrative description of using the ability.
    pub experience: String,
    /// Limitations and costs.
    pub limits: String,
}

impl<'de> Deserialize<'de> for Ability {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum AbilityRepr {
            Simple(String),
            Full {
                name: String,
                experience: String,
                limits: String,
            },
        }

        match AbilityRepr::deserialize(deserializer)? {
            AbilityRepr::Simple(s) => Ok(Ability {
                name: s,
                experience: String::new(),
                limits: String::new(),
            }),
            AbilityRepr::Full {
                name,
                experience,
                limits,
            } => Ok(Ability {
                name,
                experience,
                limits,
            }),
        }
    }
}

/// Item evolution thresholds.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ItemEvolution {
    /// Bond threshold for naming an item.
    #[serde(default)]
    pub naming_threshold: f64,
    /// Bond threshold for powering up.
    #[serde(default)]
    pub power_up_threshold: f64,
}

/// Per-level bonuses.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LevelBonuses {
    /// Stat points gained per level.
    #[serde(default)]
    pub stat_points: u32,
    /// HP bonus strategy (e.g., "class_based").
    #[serde(default)]
    pub hp_bonus: String,
}

/// A wealth tier with optional gold cap.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WealthTier {
    /// Maximum gold for this tier (None = no cap).
    pub max_gold: Option<u32>,
    /// Display label.
    pub label: String,
}

// ═══════════════════════════════════════════════════════════
// axes.yaml
// ═══════════════════════════════════════════════════════════

/// Narrative axis configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AxesConfig {
    /// Axis definitions.
    pub definitions: Vec<AxisDefinition>,
    /// Per-axis pole modifiers (axis_id → pole_name → prompt text).
    #[serde(default)]
    pub modifiers: HashMap<String, HashMap<String, String>>,
    /// Named axis presets.
    #[serde(default)]
    pub presets: Vec<AxisPreset>,
}

/// A single narrative axis definition.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AxisDefinition {
    /// Axis identifier.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Low and high pole labels.
    pub poles: Vec<String>,
    /// Default value (0.0–1.0).
    pub default: f64,
}

/// A preset combination of axis values.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AxisPreset {
    /// Preset name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Axis values.
    pub values: HashMap<String, f64>,
}

// ═══════════════════════════════════════════════════════════
// audio.yaml
// ═══════════════════════════════════════════════════════════

/// Audio configuration for music, SFX, and voice.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AudioConfig {
    /// Mood → track list mappings.
    pub mood_tracks: HashMap<String, Vec<MoodTrack>>,
    /// SFX category → file path list.
    pub sfx_library: HashMap<String, Vec<String>>,
    /// Creature type → voice preset.
    pub creature_voice_presets: HashMap<String, CreatureVoicePreset>,
    /// Mixer volume settings.
    pub mixer: MixerConfig,
    /// Themed music collections.
    #[serde(default)]
    pub themes: Vec<AudioTheme>,
    /// AI music generation configuration.
    #[serde(default)]
    pub ai_generation: Option<AudioAiGeneration>,
    /// Mood keyword mappings (mood → keyword list).
    #[serde(default)]
    pub mood_keywords: HashMap<String, Vec<String>>,
    /// Mixer defaults (alternative name for mixer in some packs).
    #[serde(default)]
    pub mixer_defaults: Option<MixerConfig>,
}

/// AI music generation configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AudioAiGeneration {
    /// Whether AI generation is enabled.
    #[serde(default)]
    pub enabled: bool,
    /// Model name (e.g., "musicgen_small").
    #[serde(default)]
    pub model: Option<String>,
    /// Maximum generation time in seconds.
    #[serde(default)]
    pub max_generation_time_s: Option<u32>,
    /// Whether to cache generated audio.
    #[serde(default)]
    pub cache_generated: Option<bool>,
}

/// Default energy level for tracks without an explicit energy field.
fn default_energy() -> f64 {
    0.5
}

/// A single music track.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MoodTrack {
    /// File path.
    pub path: String,
    /// Track title.
    pub title: String,
    /// Beats per minute.
    pub bpm: u32,
    /// Energy level (0.0–1.0) for mood intensity matching.
    #[serde(default = "default_energy")]
    pub energy: f64,
}

/// Voice preset for a creature type.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreatureVoicePreset {
    /// Creature type identifier.
    pub creature_type: String,
    /// Description of the voice.
    pub description: String,
    /// Pitch multiplier.
    pub pitch: f64,
    /// Rate multiplier.
    pub rate: f64,
    /// Audio effects chain.
    #[serde(default)]
    pub effects: Vec<AudioEffect>,
}

/// An audio effect in a processing chain.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AudioEffect {
    /// Effect type (reverb, lowpass_filter, highpass_filter, compressor).
    #[serde(rename = "type")]
    pub effect_type: String,
    /// Effect parameters (e.g., room_size, cutoff_frequency_hz).
    #[serde(default)]
    pub params: HashMap<String, f64>,
}

/// Mixer volume configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MixerConfig {
    /// Music volume (0.0–1.0).
    pub music_volume: f64,
    /// SFX volume (0.0–1.0).
    pub sfx_volume: f64,
    /// Voice volume (0.0–1.0).
    pub voice_volume: f64,
    /// Whether to duck music during voice.
    pub duck_music_for_voice: bool,
    /// Ducking amount in decibels.
    pub duck_amount_db: f64,
    /// Default crossfade duration in milliseconds.
    pub crossfade_default_ms: u32,
}

/// A themed music collection with variations.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AudioTheme {
    /// Theme name.
    pub name: String,
    /// Associated mood.
    pub mood: String,
    /// Base prompt text.
    pub base_prompt: String,
    /// Track variations.
    pub variations: Vec<AudioVariation>,
}

/// A single variation within an audio theme.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AudioVariation {
    /// Variation type (full, ambient, sparse, overture, resolution, tension_build).
    #[serde(rename = "type")]
    pub variation_type: String,
    /// File path.
    pub path: String,
}

// ═══════════════════════════════════════════════════════════
// cultures.yaml
// ═══════════════════════════════════════════════════════════

/// A name-generation culture.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Culture {
    /// Culture name.
    pub name: NonBlankString,
    /// Description.
    pub description: String,
    /// Named generation slots.
    pub slots: HashMap<String, CultureSlot>,
    /// Person name patterns using slot references.
    pub person_patterns: Vec<String>,
    /// Place name patterns using slot references.
    pub place_patterns: Vec<String>,
}

/// A name-generation slot — corpus-based, word-list-based, or file-based.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CultureSlot {
    /// Markov corpus references (for generated names).
    #[serde(default)]
    pub corpora: Option<Vec<CorpusRef>>,
    /// Markov chain lookback depth.
    #[serde(default)]
    pub lookback: Option<u32>,
    /// Fixed word list (for deterministic slots).
    #[serde(default)]
    pub word_list: Option<Vec<String>>,
    /// Plain text file of names (one per line) in corpus/.
    /// Used by real-world-name genres (pulp_noir, victoria) instead of Markov.
    #[serde(default)]
    pub names_file: Option<String>,
    /// Files containing words to reject from generation.
    #[serde(default)]
    pub reject_files: Vec<String>,
}

/// A reference to a Markov corpus file.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CorpusRef {
    /// Corpus filename.
    pub corpus: String,
    /// Blending weight.
    pub weight: f64,
}

// ═══════════════════════════════════════════════════════════
// prompts.yaml
// ═══════════════════════════════════════════════════════════

/// LLM prompt templates for different agent roles.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Prompts {
    /// Narrator system prompt.
    pub narrator: String,
    /// Combat narrator prompt.
    pub combat: String,
    /// NPC behavior prompt.
    pub npc: String,
    /// World state tracking prompt.
    pub world_state: String,
    /// Chase scene prompt.
    #[serde(default)]
    pub chase: Option<String>,
    /// Scene transition hint templates.
    #[serde(default)]
    pub transition_hints: HashMap<String, String>,
}

// ═══════════════════════════════════════════════════════════
// openings.yaml
// ═══════════════════════════════════════════════════════════

/// An opening scenario hook that constrains the narrator's first turn.
///
/// Each genre pack can define multiple opening hooks to ensure variety.
/// One is selected randomly at session start and injected into the
/// narrator's first-turn context.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpeningHook {
    /// Unique identifier within the genre (e.g. "arena_challenge").
    pub id: String,
    /// Archetype category (e.g. "challenge", "mystery", "chase", "survival", "standoff", "arrival").
    pub archetype: String,
    /// Situation description injected as narrator guidance — what's happening, what the vibe is.
    pub situation: String,
    /// Tone directive (e.g. "tense, competitive").
    pub tone: String,
    /// Patterns the narrator must avoid in this opening.
    #[serde(default)]
    pub avoid: Vec<String>,
    /// Synthetic first-turn action that replaces the generic "I look around".
    pub first_turn_seed: String,
}

// ═══════════════════════════════════════════════════════════
// beat_vocabulary.yaml
// ═══════════════════════════════════════════════════════════

/// Chase/beat vocabulary configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BeatVocabulary {
    /// Obstacles that can appear during chases.
    pub obstacles: Vec<BeatObstacle>,
}

/// A chase obstacle.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BeatObstacle {
    /// Obstacle name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Stat used for the check.
    pub stat_check: String,
    /// Penalty on failure.
    pub failure_penalty: String,
    /// Categorization tags.
    pub tags: Vec<String>,
}

// ═══════════════════════════════════════════════════════════
// achievements.yaml
// ═══════════════════════════════════════════════════════════

/// An achievement linked to trope progression.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Achievement {
    /// Achievement identifier.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Trope that triggers this achievement.
    pub trope_id: String,
    /// Trope status that triggers (activated, progressing, resolved).
    pub trigger_status: String,
    /// Display emoji.
    pub emoji: String,
}

// ═══════════════════════════════════════════════════════════
// power_tiers.yaml
// ═══════════════════════════════════════════════════════════

/// A power tier description for a character class at a level range.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PowerTier {
    /// Level range [min, max].
    pub level_range: [u32; 2],
    /// Tier label.
    pub label: String,
    /// Player appearance description.
    pub player: String,
    /// NPC appearance description (absent for max-level tiers — no level-10 NPCs).
    #[serde(default)]
    pub npc: Option<String>,
}

// ═══════════════════════════════════════════════════════════
// voice_presets.yaml
// ═══════════════════════════════════════════════════════════

/// TTS voice preset configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VoicePresets {
    /// Narrator voice configuration.
    pub narrator: VoiceConfig,
    /// Per-archetype voice configurations.
    #[serde(default)]
    pub characters: HashMap<String, VoiceConfig>,
}

/// A single TTS voice configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VoiceConfig {
    /// Piper ONNX model name.
    pub model: String,
    /// Pitch multiplier.
    pub pitch: f64,
    /// Rate multiplier.
    pub rate: f64,
    /// Audio effects chain.
    #[serde(default)]
    pub effects: Vec<AudioEffect>,
}

// ═══════════════════════════════════════════════════════════
// world.yaml
// ═══════════════════════════════════════════════════════════

/// World metadata.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorldConfig {
    /// World display name.
    pub name: String,
    /// URL-safe slug (optional — can be inferred from directory name).
    #[serde(default)]
    pub slug: String,
    /// Description text.
    pub description: String,
    /// Starting location description.
    #[serde(default)]
    pub starting_location: String,
    /// Axis values for this world.
    #[serde(default)]
    pub axis_snapshot: HashMap<String, f64>,
    /// Historical era (e.g., "Late 1970s").
    #[serde(default)]
    pub era: Option<String>,
    /// Tonal description for AI narration.
    #[serde(default)]
    pub tone: Option<String>,
    /// Genre-specific extensions (factions, faction_count, etc.).
    /// Captured for AI prompt injection without engine-level typing.
    #[serde(flatten)]
    pub extras: HashMap<String, serde_json::Value>,
}

// ═══════════════════════════════════════════════════════════
// cartography.yaml
// ═══════════════════════════════════════════════════════════

/// Navigation mode for a world's cartography.
///
/// `Region` (default) uses freeform location strings with region metadata.
/// `RoomGraph` uses validated room IDs with checked exits — required for
/// dungeon crawl genre packs where room transitions drive game mechanics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NavigationMode {
    /// Freeform region-based navigation (default for all existing genre packs).
    Region,
    /// Validated room graph with checked exits (dungeon crawl mode).
    RoomGraph,
}

impl Default for NavigationMode {
    fn default() -> Self {
        Self::Region
    }
}

/// A single exit from a room to another room.
///
/// Tagged enum discriminated by `type` in YAML/JSON. Each variant carries
/// its own metadata (e.g., `is_locked` for doors, `discovered` for secrets).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RoomExit {
    /// Normal door: bidirectional by default, optionally locked.
    Door {
        /// Target room ID this exit leads to.
        target: String,
        /// Whether the door is locked.
        #[serde(default)]
        is_locked: bool,
    },
    /// Open corridor: bidirectional.
    Corridor {
        /// Target room ID this exit leads to.
        target: String,
    },
    /// One-way drop (no reverse required).
    ChuteDown {
        /// Target room ID this exit leads to.
        target: String,
    },
    /// One-way ascent (no reverse required, rare).
    ChuteUp {
        /// Target room ID this exit leads to.
        target: String,
    },
    /// Secret passage: bidirectional but hidden until discovered.
    Secret {
        /// Target room ID this exit leads to.
        target: String,
        /// Whether the passage has been discovered.
        #[serde(default)]
        discovered: bool,
    },
}

impl RoomExit {
    /// Target room ID this exit leads to.
    pub fn target(&self) -> &str {
        match self {
            RoomExit::Door { target, .. }
            | RoomExit::Corridor { target }
            | RoomExit::ChuteDown { target }
            | RoomExit::ChuteUp { target }
            | RoomExit::Secret { target, .. } => target,
        }
    }

    /// Whether this exit requires a return path from the target room.
    pub fn requires_reverse(&self) -> bool {
        matches!(
            self,
            RoomExit::Door { .. } | RoomExit::Corridor { .. } | RoomExit::Secret { .. }
        )
    }

    /// Display name for UI/narration.
    pub fn display_name(&self) -> &str {
        match self {
            RoomExit::Door { .. } => "door",
            RoomExit::Corridor { .. } => "corridor",
            RoomExit::ChuteDown { .. } => "chute down",
            RoomExit::ChuteUp { .. } => "chute up",
            RoomExit::Secret { .. } => "secret passage",
        }
    }
}

/// A room in the dungeon room graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomDef {
    /// Unique room identifier (slug).
    pub id: String,
    /// Display name.
    pub name: String,
    /// Room type: "entrance", "normal", "boss", "treasure", "dead_end".
    pub room_type: String,
    /// Physical dimensions for layout (width, height in grid units).
    #[serde(default = "default_room_size")]
    pub size: (u32, u32),
    /// How much Keeper awareness escalates per transition (0.8–1.5).
    #[serde(default = "default_keeper_awareness_modifier")]
    pub keeper_awareness_modifier: f64,
    /// Exits leading to other rooms.
    #[serde(default)]
    pub exits: Vec<RoomExit>,
    /// Optional description for UI/lore.
    #[serde(default)]
    pub description: Option<String>,
}

fn default_room_size() -> (u32, u32) {
    (1, 1)
}

fn default_keeper_awareness_modifier() -> f64 {
    1.0
}

/// Map and region configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CartographyConfig {
    /// World name.
    #[serde(default)]
    pub world_name: String,
    /// Starting region slug (or starting room ID in room_graph mode).
    #[serde(default)]
    pub starting_region: String,
    /// Map style prompt for image generation.
    #[serde(default)]
    pub map_style: String,
    /// Map resolution in pixels [width, height] (null if not specified).
    #[serde(default)]
    pub map_resolution: Option<[u32; 2]>,
    /// Navigation mode — Region (default) or RoomGraph.
    #[serde(default)]
    pub navigation_mode: NavigationMode,
    /// Regions keyed by slug (used in Region mode).
    #[serde(default)]
    pub regions: HashMap<String, Region>,
    /// Routes between regions (used in Region mode).
    #[serde(default)]
    pub routes: Vec<Route>,
    /// Room definitions (used in RoomGraph mode). `None` for region-based packs.
    #[serde(default)]
    pub rooms: Option<Vec<RoomDef>>,
}

/// A map region.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Region {
    /// Display name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Slugs of adjacent regions.
    #[serde(default)]
    pub adjacent: Vec<String>,
    /// Named landmarks (either simple strings or detailed objects).
    #[serde(default)]
    pub landmarks: Vec<Landmark>,
    /// Origin char-creation scene (if any).
    #[serde(default)]
    pub origin: Option<String>,
    /// Rivers passing through (strings or detailed objects).
    #[serde(default)]
    pub rivers: Vec<Landmark>,
    /// Settlements in this region (strings or detailed objects).
    #[serde(default)]
    pub settlements: Vec<Landmark>,
    /// Terrain type (e.g., "elevated_expressway", "coastal_mountain_pass").
    #[serde(default)]
    pub terrain: Option<String>,
    /// Faction controlling this region.
    #[serde(default)]
    pub controlled_by: Option<String>,
    /// Genre-specific region extensions (chase_profile, etc.).
    #[serde(flatten)]
    pub extras: HashMap<String, serde_json::Value>,
}

/// A landmark — either a simple name string or a detailed object.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum Landmark {
    /// Simple landmark name.
    Name(String),
    /// Detailed landmark with type and description.
    Detailed {
        /// Landmark name.
        name: String,
        /// Landmark type (crater, shrine, etc.).
        #[serde(rename = "type")]
        landmark_type: String,
        /// Description.
        description: String,
    },
}

impl<'de> Deserialize<'de> for Landmark {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum LandmarkRepr {
            Name(String),
            Detailed {
                name: String,
                #[serde(rename = "type")]
                landmark_type: String,
                description: String,
            },
        }

        match LandmarkRepr::deserialize(deserializer)? {
            LandmarkRepr::Name(s) => Ok(Landmark::Name(s)),
            LandmarkRepr::Detailed {
                name,
                landmark_type,
                description,
            } => Ok(Landmark::Detailed {
                name,
                landmark_type,
                description,
            }),
        }
    }
}

/// A route between regions.
///
/// Supports two formats:
/// - Point-to-point (low_fantasy): from_id, to_id, distance, danger
/// - Waypoint-based (road_warrior): id, waypoints, difficulty
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Route {
    /// Route name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Route slug (waypoint format).
    #[serde(default)]
    pub id: Option<String>,
    /// Source region slug (point-to-point format).
    #[serde(default)]
    pub from_id: Option<String>,
    /// Destination region slug (point-to-point format).
    #[serde(default)]
    pub to_id: Option<String>,
    /// Travel distance category (point-to-point format).
    #[serde(default)]
    pub distance: Option<String>,
    /// Danger level (point-to-point format).
    #[serde(default)]
    pub danger: Option<String>,
    /// Ordered waypoints (waypoint format).
    #[serde(default)]
    pub waypoints: Vec<String>,
    /// Difficulty level (waypoint format).
    #[serde(default)]
    pub difficulty: Option<String>,
    /// Genre-specific route extensions (faction_crossings, etc.).
    #[serde(flatten)]
    pub extras: HashMap<String, serde_json::Value>,
}

// ═══════════════════════════════════════════════════════════
// legends.yaml
// ═══════════════════════════════════════════════════════════

/// A historical legend.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Legend {
    /// Legend name.
    pub name: String,
    /// Summary text (also accepts "description" from road_warrior format).
    #[serde(default, alias = "description")]
    pub summary: String,
    /// Historical era.
    #[serde(default)]
    pub era: String,
    /// Cultures affected.
    #[serde(default)]
    pub affected_cultures: Vec<String>,
    /// Impact on those cultures.
    #[serde(default)]
    pub cultural_impact: String,
    /// Grudges between factions.
    #[serde(default)]
    pub faction_grudges: Vec<FactionGrudge>,
    /// Knowledge lost due to this event.
    #[serde(default)]
    pub lost_arts: Vec<String>,
    /// Monuments related to this legend.
    #[serde(default)]
    pub monuments: Vec<String>,
    /// Physical scars on the landscape from this event.
    #[serde(default)]
    pub terrain_scars: Vec<TerrainScar>,
}

/// A physical scar on the landscape from a historical event.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TerrainScar {
    /// Scar name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Region slug (may be empty).
    #[serde(default)]
    pub region: String,
    /// Scar type (crater, dead_zone, etc.).
    #[serde(rename = "type")]
    pub scar_type: String,
}

/// A grudge between two factions from a historical event.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FactionGrudge {
    /// The faction holding the grudge.
    pub from: String,
    /// The faction being resented.
    pub to: String,
    /// Why the grudge exists.
    pub reason: String,
}

// ═══════════════════════════════════════════════════════════
// Scenario pack models (from scenarios/*/)
// ═══════════════════════════════════════════════════════════

/// A scenario pack — assembled from scenario.yaml + supporting files.
///
/// Fields from scenario.yaml are required; fields from supplementary files
/// (assignment_matrix, clue_graph, etc.) default to empty and are populated
/// by the loader after reading the additional YAML files.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioPack {
    /// Scenario display name.
    pub name: NonBlankString,
    /// Semantic version.
    pub version: String,
    /// Description.
    pub description: String,
    /// Expected play time in minutes.
    pub duration_minutes: u32,
    /// Maximum number of players.
    pub max_players: u32,
    /// Available player roles.
    pub player_roles: Vec<PlayerRole>,
    /// Pacing and act structure.
    pub pacing: Pacing,
    /// Suspect/motive/method assignment matrix.
    #[serde(default)]
    pub assignment_matrix: AssignmentMatrix,
    /// Clue dependency graph.
    #[serde(default)]
    pub clue_graph: ClueGraph,
    /// Atmosphere/weather variants.
    #[serde(default)]
    pub atmosphere_matrix: AtmosphereMatrix,
    /// NPC definitions with guilty/innocent branches.
    #[serde(default)]
    pub npcs: Vec<ScenarioNpc>,
}

/// A player role within a scenario.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerRole {
    /// Role identifier.
    pub id: String,
    /// Suggested archetype description.
    pub archetype_hint: String,
    /// Narrative position text.
    pub narrative_position: String,
    /// Required character hooks.
    #[serde(default)]
    pub required_hooks: Vec<RoleHook>,
    /// Constraints on this role.
    #[serde(default)]
    pub constraints: Vec<String>,
    /// Flavor text suggestions.
    #[serde(default)]
    pub suggested_flavors: Vec<String>,
}

/// A required hook for a player role.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoleHook {
    /// Hook type (MOTIVATION, RELATIONSHIP, etc.).
    #[serde(rename = "type")]
    pub hook_type: String,
    /// Prompt question.
    pub prompt: String,
}

/// Scenario pacing and act structure.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Pacing {
    /// Total scene budget.
    pub scene_budget: u32,
    /// Act definitions.
    pub acts: Vec<Act>,
    /// Pressure events triggered at specific scenes.
    #[serde(default)]
    pub pressure_events: Vec<PressureEvent>,
    /// Escalation beats at progression thresholds.
    #[serde(default)]
    pub escalation_beats: Vec<EscalationBeat>,
}

/// An act within a scenario's pacing structure.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Act {
    /// Act identifier.
    pub id: String,
    /// Act name.
    pub name: String,
    /// Number of scenes in this act.
    pub scenes: u32,
    /// Trope progression range [start, end].
    pub trope_range: [f64; 2],
    /// Narrator tone guidance.
    pub narrator_tone: String,
}

/// A pressure event triggered at a specific scene.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PressureEvent {
    /// Scene number that triggers this event.
    pub at_scene: u32,
    /// Event description.
    pub event: String,
}

/// An escalation beat at a progression threshold.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EscalationBeat {
    /// Progression threshold (0.0–1.0).
    pub at: f64,
    /// Injected narrative text.
    pub inject: String,
}

/// Suspect/motive/method assignment matrix.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AssignmentMatrix {
    /// Suspect definitions.
    #[serde(default)]
    pub suspects: Vec<Suspect>,
    /// Available motives.
    #[serde(default)]
    pub motives: Vec<String>,
    /// Available methods.
    #[serde(default)]
    pub methods: Vec<String>,
    /// Available opportunities.
    #[serde(default)]
    pub opportunities: Vec<String>,
}

/// A suspect in the assignment matrix.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Suspect {
    /// Suspect identifier.
    pub id: String,
    /// Reference to an NPC archetype.
    pub archetype_ref: String,
    /// Whether this suspect can be the guilty party.
    pub can_be_guilty: bool,
    /// Possible motives for this suspect.
    pub motives: Vec<String>,
    /// Possible methods for this suspect.
    pub methods: Vec<String>,
    /// Possible opportunities for this suspect.
    pub opportunities: Vec<String>,
}

/// Clue dependency graph.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ClueGraph {
    /// Clue nodes.
    #[serde(default)]
    pub nodes: Vec<ClueNode>,
}

/// A single clue node in the graph.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClueNode {
    /// Clue identifier.
    pub id: String,
    /// Clue type (physical, testimonial, behavioral).
    #[serde(rename = "type")]
    pub clue_type: String,
    /// Description.
    pub description: String,
    /// How the clue is discovered.
    pub discovery_method: String,
    /// Visibility level.
    pub visibility: String,
    /// Locations where this clue can be found.
    #[serde(default)]
    pub locations: Vec<String>,
    /// Suspect IDs this clue implicates.
    #[serde(default)]
    pub implicates: Vec<String>,
    /// Prerequisite clue IDs.
    #[serde(default)]
    pub requires: Vec<String>,
    /// Whether this is a red herring.
    #[serde(default)]
    pub red_herring: bool,
}

/// Atmosphere variant matrix.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AtmosphereMatrix {
    /// Atmosphere variants.
    #[serde(default)]
    pub variants: Vec<AtmosphereVariant>,
}

/// A single atmosphere variant.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AtmosphereVariant {
    /// Variant identifier.
    pub id: String,
    /// Weather description.
    pub weather: String,
    /// Setting status (doors_locked, lights_dimmed, normal).
    pub setting_status: String,
    /// Mood baseline description.
    pub mood_baseline: String,
    /// Concurrent event (null if none).
    pub concurrent_event: Option<String>,
    /// Per-NPC mood overrides.
    #[serde(default)]
    pub npc_mood_overrides: HashMap<String, String>,
}

/// An NPC within a scenario with branching behavior.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioNpc {
    /// NPC identifier.
    pub id: String,
    /// Reference to an archetype.
    pub archetype_ref: String,
    /// Display name.
    pub name: String,
    /// Starting beliefs and knowledge.
    pub initial_beliefs: InitialBeliefs,
    /// Behavior when this NPC is the guilty party.
    pub when_guilty: WhenGuilty,
    /// Behavior when this NPC is innocent.
    pub when_innocent: WhenInnocent,
}

/// An NPC's initial beliefs and suspicions.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InitialBeliefs {
    /// Known facts.
    #[serde(default)]
    pub facts: Vec<String>,
    /// Suspicions about other suspects.
    #[serde(default)]
    pub suspicions: Vec<Suspicion>,
}

/// A suspicion one NPC has about another.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Suspicion {
    /// Target suspect ID.
    pub target: String,
    /// Confidence level (0.0–1.0).
    pub confidence: f64,
    /// Basis for the suspicion.
    pub basis: String,
}

/// NPC behavior when guilty.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WhenGuilty {
    /// What actually happened.
    pub truth: String,
    /// The NPC's false alibi.
    pub cover_story: String,
    /// Clue IDs that break the cover story.
    #[serde(default)]
    pub breaking_evidence: Vec<String>,
}

/// NPC behavior when innocent.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WhenInnocent {
    /// What the NPC was actually doing.
    pub actual_activity: String,
    /// Who the NPC suspects.
    #[serde(default)]
    pub suspicion: String,
    /// The NPC's secret (unrelated to the crime).
    #[serde(default)]
    pub secret: String,
}

// ═══════════════════════════════════════════════════════════
// inventory.yaml
// ═══════════════════════════════════════════════════════════

/// Complete inventory configuration from `inventory.yaml`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InventoryConfig {
    /// Currency system (optional — some genre packs don't define one).
    #[serde(default)]
    pub currency: Option<CurrencyConfig>,
    /// Full item catalog.
    #[serde(default)]
    pub item_catalog: Vec<CatalogItem>,
    /// Starting equipment per archetype/class. Key = archetype/class name, value = item IDs.
    #[serde(default)]
    pub starting_equipment: HashMap<String, Vec<String>>,
    /// Starting gold per archetype/class.
    #[serde(default)]
    pub starting_gold: HashMap<String, u32>,
    /// Inventory philosophy (carry limits, restrictions).
    #[serde(default)]
    pub philosophy: Option<InventoryPhilosophy>,
}

/// Currency system definition.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CurrencyConfig {
    /// Currency name (e.g., "gold", "credits", "Dollars").
    pub name: String,
    /// Denomination names or name→multiplier map.
    /// Accepts either a list of strings or a map of name→value.
    #[serde(default)]
    pub denominations: serde_json::Value,
}

/// A single item in the genre pack's item catalog.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CatalogItem {
    /// Unique item identifier (e.g., "sword_iron").
    pub id: String,
    /// Display name.
    pub name: String,
    /// Item description.
    pub description: String,
    /// Category: weapon, armor, tool, consumable, treasure, misc.
    pub category: String,
    /// Base value in currency.
    #[serde(default)]
    pub value: u32,
    /// Weight in abstract units.
    #[serde(default)]
    pub weight: f64,
    /// Rarity: common, uncommon, rare, legendary.
    #[serde(default)]
    pub rarity: String,
    /// Power level (0-5 scale).
    #[serde(default)]
    pub power_level: u32,
    /// Searchable tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Flavor text / lore.
    #[serde(default)]
    pub lore: String,
    /// Narrative weight for how much the narrator should mention this item.
    #[serde(default)]
    pub narrative_weight: serde_json::Value,
}

/// Inventory philosophy configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InventoryPhilosophy {
    /// Maximum carry weight.
    #[serde(default)]
    pub carry_limit: Option<u32>,
    /// Item categories that are restricted.
    #[serde(default)]
    pub restricted_categories: Vec<String>,
    /// Progression gates for item access.
    #[serde(default)]
    pub progression_gates: HashMap<String, serde_json::Value>,
}

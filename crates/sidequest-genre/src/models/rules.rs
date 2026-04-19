//! Game rules, resource declarations, and confrontation types from `rules.yaml`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════
// Initiative rules (story 13-12)
// ═══════════════════════════════════════════════════════════

/// Maps an encounter type to its primary stat for turn ordering.
///
/// Each genre pack can define per-encounter-type initiative rules so that
/// different situations weight different ability scores (e.g., combat → DEX,
/// social → CHA).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InitiativeRule {
    /// The ability score name that drives initiative for this encounter type.
    pub primary_stat: String,
    /// Narrator-facing description of why this stat matters.
    pub description: String,
}

// ═══════════════════════════════════════════════════════════
// Resource declarations (story 16-1)
// ═══════════════════════════════════════════════════════════

/// A threshold on a resource declaration — fires an event when the value crosses below it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceThresholdDecl {
    /// The value at which this threshold fires (crossed downward).
    pub at: f64,
    /// Event identifier emitted when this threshold is crossed.
    pub event_id: String,
    /// Hint text injected into narrator prompt when crossed.
    pub narrator_hint: String,
}

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
    #[serde(default)]
    thresholds: Vec<ResourceThresholdDecl>,
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
    /// Thresholds that fire events when the value crosses below them (story 16-12).
    #[serde(default)]
    pub thresholds: Vec<ResourceThresholdDecl>,
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
            thresholds: raw.thresholds,
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
    #[serde(default)]
    effect: Option<String>,
    #[serde(default)]
    consequence: Option<String>,
    #[serde(default)]
    requires: Option<String>,
    #[serde(default)]
    narrator_hint: Option<String>,
    #[serde(default)]
    gold_delta: Option<i32>,
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
    /// Narrative effect on success (e.g., "opponent disposition +5").
    #[serde(default)]
    pub effect: Option<String>,
    /// What happens on resolution or critical failure.
    #[serde(default)]
    pub consequence: Option<String>,
    /// Precondition for using this beat (e.g., "must have discovered relevant clue").
    #[serde(default)]
    pub requires: Option<String>,
    /// Guidance for the narrator LLM when this beat is executed.
    #[serde(default)]
    pub narrator_hint: Option<String>,
    /// Gold change applied to the player's inventory when this beat resolves.
    /// Positive = player gains gold, negative = player loses gold (ante, bet, etc.).
    #[serde(default)]
    pub gold_delta: Option<i32>,
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
            effect: raw.effect,
            consequence: raw.consequence,
            requires: raw.requires,
            narrator_hint: raw.narrator_hint,
            gold_delta: raw.gold_delta,
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

/// How a confrontation resolves each turn.
///
/// `BeatSelection` is the default: the narrator selects a beat per turn and the
/// engine applies it to the shared metric. `SealedLetterLookup` is the
/// simultaneous-commit variant introduced by ADR-077 (Dogfight Subsystem): each
/// actor commits a choice privately, and the engine resolves via cross-product
/// lookup in an interaction table.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionMode {
    /// Existing behavior — narrator selects a beat, engine applies it.
    #[default]
    BeatSelection,
    /// Simultaneous-commit table lookup (e.g., dogfight maneuvers).
    SealedLetterLookup,
}

// ───────────────────────────────────────────────────────────
// Interaction table (story 38-4) — sealed-letter lookup data
// ───────────────────────────────────────────────────────────

/// Raw interaction cell for deserialization with validation.
#[derive(Debug, Clone, Deserialize)]
struct RawInteractionCell {
    pair: Vec<String>,
    #[serde(default)]
    name: String,
    #[serde(default)]
    shape: String,
    #[serde(default)]
    red_view: serde_yaml::Value,
    #[serde(default)]
    blue_view: serde_yaml::Value,
    #[serde(default)]
    narration_hint: String,
    /// Post-playtest calibration tags (story 38-9).
    #[serde(default)]
    tags: Vec<String>,
    /// Rationale for delta adjustments on failing cells (story 38-9).
    #[serde(default)]
    calibration_notes: Option<String>,
}

/// A single cell of a sealed-letter interaction table.
///
/// Each cell is keyed by the `(red_maneuver, blue_maneuver)` pair and carries
/// the two descriptor-delta views the engine will merge into each pilot's
/// perspective, plus a narration hint for the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "RawInteractionCell")]
pub struct InteractionCell {
    /// `(red, blue)` maneuver pair keying this cell.
    pub pair: (String, String),
    /// Short human-facing cell name (e.g., "Clean merge").
    pub name: String,
    /// Shape tag for design review (e.g., "passive vs evasive").
    pub shape: String,
    /// Descriptor delta applied to the red pilot's view.
    pub red_view: serde_yaml::Value,
    /// Descriptor delta applied to the blue pilot's view.
    pub blue_view: serde_yaml::Value,
    /// Narrator hint describing the beat for the LLM.
    pub narration_hint: String,
    /// Post-playtest calibration tags: exciting, calibrated, lopsided, confusing, dull (story 38-9).
    pub tags: Vec<String>,
    /// Rationale for delta adjustments on failing cells (story 38-9).
    pub calibration_notes: Option<String>,
}

impl TryFrom<RawInteractionCell> for InteractionCell {
    type Error = String;

    fn try_from(raw: RawInteractionCell) -> Result<Self, Self::Error> {
        let [red, blue]: [String; 2] = raw.pair.try_into().map_err(|v: Vec<String>| {
            format!(
                "interaction cell pair must have exactly 2 elements, got {}",
                v.len()
            )
        })?;
        Ok(Self {
            pair: (red, blue),
            name: raw.name,
            shape: raw.shape,
            red_view: raw.red_view,
            blue_view: raw.blue_view,
            narration_hint: raw.narration_hint,
            tags: raw.tags,
            calibration_notes: raw.calibration_notes,
        })
    }
}

/// Raw interaction table for deserialization with validation.
#[derive(Debug, Clone, Deserialize)]
struct RawInteractionTable {
    version: String,
    starting_state: String,
    #[serde(default)]
    maneuvers_consumed: Vec<String>,
    cells: Vec<InteractionCell>,
    /// Hull damage per hit severity tier (story 38-7).
    #[serde(default)]
    damage_increments: Option<HashMap<String, i64>>,
    /// Starting hull pool for damage math (story 38-7).
    #[serde(default)]
    starting_hull: Option<i64>,
}

/// A sealed-letter interaction table — cross-product lookup between two
/// simultaneously-committed maneuvers, producing per-viewer descriptor deltas
/// and a narration hint.
///
/// Validated on deserialization: version must not be empty, cells must not
/// be empty, and every `(red, blue)` pair must be unique. If `damage_increments`
/// is present, all three severity tiers must be defined with positive, ordered
/// values (story 38-7).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "RawInteractionTable")]
pub struct InteractionTable {
    /// Table schema version (e.g., "0.1.0").
    pub version: String,
    /// Starting descriptor state that all cells deltas apply against (e.g., "merge").
    pub starting_state: String,
    /// Maneuver IDs this table covers.
    pub maneuvers_consumed: Vec<String>,
    /// Cells in the cross-product lookup.
    pub cells: Vec<InteractionCell>,
    /// Hull damage per hit severity tier: graze, clean, devastating (story 38-7).
    pub damage_increments: Option<HashMap<String, i64>>,
    /// Starting hull pool value for damage math (story 38-7).
    pub starting_hull: Option<i64>,
}

impl TryFrom<RawInteractionTable> for InteractionTable {
    type Error = String;

    fn try_from(raw: RawInteractionTable) -> Result<Self, Self::Error> {
        if raw.version.is_empty() {
            return Err("interaction table version must not be empty".to_string());
        }
        if raw.cells.is_empty() {
            return Err("interaction table must have at least one cell".to_string());
        }
        let mut seen = std::collections::HashSet::new();
        for cell in &raw.cells {
            let key = (cell.pair.0.clone(), cell.pair.1.clone());
            if !seen.insert(key) {
                return Err(format!(
                    "duplicate interaction cell pair: ({}, {})",
                    cell.pair.0, cell.pair.1
                ));
            }
        }

        // Validate damage_increments if present (story 38-7).
        if let Some(ref increments) = raw.damage_increments {
            for tier in &["graze", "clean", "devastating"] {
                match increments.get(*tier) {
                    None => {
                        return Err(format!(
                            "damage_increments missing required severity tier: '{}'",
                            tier
                        ));
                    }
                    Some(&val) if val <= 0 => {
                        return Err(format!(
                            "damage_increments '{}' must be positive, got {}",
                            tier, val
                        ));
                    }
                    _ => {}
                }
            }
        }

        Ok(Self {
            version: raw.version,
            starting_state: raw.starting_state,
            maneuvers_consumed: raw.maneuvers_consumed,
            cells: raw.cells,
            damage_increments: raw.damage_increments,
            starting_hull: raw.starting_hull,
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
    #[serde(default)]
    resolution_mode: ResolutionMode,
    metric: MetricDef,
    beats: Vec<BeatDef>,
    #[serde(default)]
    secondary_stats: Vec<SecondaryStatDef>,
    #[serde(default)]
    escalates_to: Option<String>,
    #[serde(default)]
    mood: Option<String>,
    #[serde(default)]
    interaction_table: Option<InteractionTable>,
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
    /// How this confrontation resolves turns. Defaults to `BeatSelection`.
    #[serde(default)]
    pub resolution_mode: ResolutionMode,
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
    /// Sealed-letter interaction table for simultaneous-commit resolution (story 38-4).
    /// Populated for confrontations with `resolution_mode: sealed_letter_lookup`.
    /// May be loaded inline or via a `_from: subpath.yaml` pointer in rules.yaml.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interaction_table: Option<InteractionTable>,
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
            resolution_mode: raw.resolution_mode,
            metric: raw.metric,
            beats: raw.beats,
            secondary_stats: raw.secondary_stats,
            escalates_to: raw.escalates_to,
            mood: raw.mood,
            interaction_table: raw.interaction_table,
        })
    }
}

// ═══════════════════════════════════════════════════════════
// Edge / Composure config (story 39-3)
// ═══════════════════════════════════════════════════════════

/// Direction in which an `EdgeThresholdDecl` fires.
///
/// Currently all EdgePool thresholds fire on downward crossings by
/// construction (see `sidequest-game::thresholds::detect_crossings`),
/// but the genre YAML may declare `direction: crossing_down` explicitly.
/// `#[non_exhaustive]` leaves room for `CrossingUp` (buff-side thresholds)
/// in a future story without breaking external match arms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CrossingDirection {
    /// Threshold fires when the pool value crosses from above to below `at`.
    CrossingDown,
}

/// Recovery behaviour for an edge pool at a named cadence (e.g. on
/// confrontation resolution, on long rest).
///
/// Only `full` is authored in heavy_metal today. `#[non_exhaustive]` so
/// 39-6 can add `Partial { amount: i32 }` without a breaking change.
/// Using an enum rather than `Option<String>` means YAML typos like
/// `Full` or `fulll` fail at load time, not silently at runtime inside
/// the rest handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum RecoveryBehaviour {
    /// Restore the pool to `max` at this cadence.
    Full,
}

/// Downward threshold declared in `edge_config.thresholds`.
///
/// Parsed verbatim from YAML; mapped onto `EdgePool.thresholds`
/// (`sidequest-game::creature_core::EdgeThreshold`) by
/// `edge_pool_from_config` in `sidequest-game::creature_core`
/// (wired in story 39-3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeThresholdDecl {
    /// Value at which the threshold fires (crossed downward).
    pub at: i32,
    /// Event identifier (e.g. `edge_strained`, `composure_break`).
    pub event_id: String,
    /// Narrator hint injected when crossed.
    pub narrator_hint: String,
    /// Crossing direction (currently all thresholds are `crossing_down`
    /// by construction — see `CrossingDirection`). Optional in YAML for
    /// backwards compatibility with decls that omit the tag.
    #[serde(default)]
    pub direction: Option<CrossingDirection>,
}

/// Default recovery behaviour for composure pools.
///
/// Drives how Edge refills between confrontations and after rests.
/// Parsed verbatim; wired into the beat/rest system in 39-4/39-6.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct EdgeRecoveryDefaults {
    /// Refill behaviour when a confrontation resolves.
    #[serde(default)]
    pub on_resolution: Option<RecoveryBehaviour>,
    /// Refill behaviour on long rest.
    #[serde(default)]
    pub on_long_rest: Option<RecoveryBehaviour>,
    /// Refill amount when a new confrontation begins before a breath
    /// was taken (0 = carry whatever remains).
    #[serde(default)]
    pub between_back_to_back: Option<i32>,
}

/// Per-genre Edge / Composure configuration.
///
/// Replaces the deprecated HP scaffolding (`hp_formula`, `class_hp_bases`,
/// `default_hp`, `default_ac`, `stat_display_fields`) for packs migrated to
/// Edge. Authored per §1 of `heavy_metal/_drafts/edge-advancement-content.md`.
///
/// A pack that has migrated to Edge MUST declare `edge_config`. Packs still
/// using the phantom-HP scaffold keep their legacy fields until their own
/// migration story — the loader tolerates both shapes so other packs keep
/// parsing, but each pack owns its own migration moment.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EdgeConfig {
    /// Per-class base Edge capacity. Keyed by class name as it appears in
    /// `allowed_classes` (e.g. "Fighter").
    pub base_max_by_class: HashMap<String, i32>,
    /// Default recovery behaviour for this genre.
    #[serde(default)]
    pub recovery_defaults: EdgeRecoveryDefaults,
    /// Downward thresholds (narrator-facing). Typical heavy_metal shape:
    /// one entry at `at: 1` (edge_strained), one at `at: 0`
    /// (composure_break).
    #[serde(default)]
    pub thresholds: Vec<EdgeThresholdDecl>,
    /// Character-sheet display field order (e.g. `[edge, max_edge,
    /// composure_state]`). Read declaratively by the UI.
    #[serde(default)]
    pub display_fields: Vec<String>,
}

// ═══════════════════════════════════════════════════════════
// rules.yaml
// ═══════════════════════════════════════════════════════════

/// Game rules configuration.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RulesConfig {
    /// Narrative tone (e.g., "gonzo-sincere").
    #[serde(default)]
    pub tone: String,
    /// How deadly combat is.
    #[serde(default)]
    pub lethality: String,
    /// Magic system level.
    #[serde(default)]
    pub magic_level: String,
    /// How ability scores are generated.
    #[serde(default)]
    pub stat_generation: String,
    /// Point budget for point-buy generation.
    #[serde(default)]
    pub point_buy_budget: u32,
    /// Names for the six ability scores.
    #[serde(default)]
    pub ability_score_names: Vec<String>,
    /// Available character classes.
    #[serde(default)]
    pub allowed_classes: Vec<String>,
    /// Available character races.
    #[serde(default)]
    pub allowed_races: Vec<String>,
    /// Base HP per class. Legacy — only populated for packs that have
    /// not yet migrated to `edge_config` (story 39-3+).
    #[serde(default)]
    pub class_hp_bases: HashMap<String, u32>,
    /// Per-genre Edge / Composure configuration (story 39-3). `None` for
    /// packs still on the phantom-HP scaffold; `Some` once migrated.
    #[serde(default)]
    pub edge_config: Option<EdgeConfig>,
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
    /// Affinity name that receives progress when gold is extracted to surface (story 19-9).
    /// None for genres without treasure-as-XP. E.g., "Plunderer" for caverns_and_claudes.
    #[serde(default)]
    pub xp_affinity: Option<String>,
    /// Initiative rules mapping encounter types to primary stats (story 13-12).
    /// Empty for genres that haven't authored initiative rules yet.
    #[serde(default)]
    pub initiative_rules: HashMap<String, InitiativeRule>,
}

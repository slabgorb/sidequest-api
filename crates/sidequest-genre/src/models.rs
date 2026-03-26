//! Genre pack model structs.
//!
//! Every struct uses `#[serde(deny_unknown_fields)]` to catch YAML typos at
//! deserialization time — the key Rust improvement over Python's `extra="allow"`.

use serde::{Deserialize, Serialize};
use sidequest_protocol::NonBlankString;
use std::collections::HashMap;

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
// rules.yaml
// ═══════════════════════════════════════════════════════════

/// Game rules configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
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
}

// ═══════════════════════════════════════════════════════════
// theme.yaml
// ═══════════════════════════════════════════════════════════

/// UI theme colors and typography.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
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

/// Section break (dinkus) glyphs.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct SessionOpener {
    /// Whether session openers are enabled.
    pub enabled: bool,
}

// ═══════════════════════════════════════════════════════════
// lore.yaml (genre-level)
// ═══════════════════════════════════════════════════════════

/// Genre-level lore.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
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
}

// ═══════════════════════════════════════════════════════════
// lore.yaml (world-level) — extends genre lore with factions
// ═══════════════════════════════════════════════════════════

/// World-specific lore with factions.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorldLore {
    /// World name.
    pub world_name: String,
    /// History text.
    pub history: String,
    /// Geography description.
    pub geography: String,
    /// Cosmology text.
    pub cosmology: String,
    /// Political factions.
    #[serde(default)]
    pub factions: Vec<Faction>,
}

/// A political or social faction.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Faction {
    /// Faction name.
    pub name: String,
    /// Description of the faction.
    pub description: String,
    /// Starting disposition toward the player.
    pub disposition: String,
}

// ═══════════════════════════════════════════════════════════
// archetypes.yaml
// ═══════════════════════════════════════════════════════════

/// An NPC archetype template.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
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
}

// ═══════════════════════════════════════════════════════════
// char_creation.yaml
// ═══════════════════════════════════════════════════════════

/// A character creation scene with narrative choices.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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
}

// ═══════════════════════════════════════════════════════════
// visual_style.yaml
// ═══════════════════════════════════════════════════════════

/// Image generation style configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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
    pub affinities: Vec<Affinity>,
    /// Categories for milestone tracking.
    pub milestone_categories: Vec<String>,
    /// Milestones required per level.
    pub milestones_per_level: u32,
    /// Maximum character level.
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
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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

/// A single music track.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MoodTrack {
    /// File path.
    pub path: String,
    /// Track title.
    pub title: String,
    /// Beats per minute.
    pub bpm: u32,
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
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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

/// A name-generation slot — either corpus-based or word-list-based.
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
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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
    /// URL-safe slug.
    pub slug: String,
    /// Description text.
    pub description: String,
    /// Starting location description.
    #[serde(default)]
    pub starting_location: String,
    /// Axis values for this world.
    #[serde(default)]
    pub axis_snapshot: HashMap<String, f64>,
}

// ═══════════════════════════════════════════════════════════
// cartography.yaml
// ═══════════════════════════════════════════════════════════

/// Map and region configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CartographyConfig {
    /// World name.
    #[serde(default)]
    pub world_name: String,
    /// Starting region slug.
    pub starting_region: String,
    /// Map style prompt for image generation.
    #[serde(default)]
    pub map_style: String,
    /// Map resolution in pixels [width, height] (null if not specified).
    #[serde(default)]
    pub map_resolution: Option<[u32; 2]>,
    /// Regions keyed by slug.
    #[serde(default)]
    pub regions: HashMap<String, Region>,
    /// Routes between regions.
    #[serde(default)]
    pub routes: Vec<Route>,
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

/// A route between two regions.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Route {
    /// Route name.
    pub name: String,
    /// Source region slug.
    pub from_id: String,
    /// Destination region slug.
    pub to_id: String,
    /// Travel distance category.
    pub distance: String,
    /// Danger level.
    pub danger: String,
    /// Description.
    pub description: String,
}

// ═══════════════════════════════════════════════════════════
// legends.yaml
// ═══════════════════════════════════════════════════════════

/// A historical legend.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Legend {
    /// Legend name.
    pub name: String,
    /// Summary text.
    pub summary: String,
    /// Historical era.
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

//! Top-level aggregates: GenrePack and World, plus pack metadata.

use super::*;
use super::archetype_axes::BaseArchetypes;
use super::archetype_constraints::ArchetypeConstraints;
use super::archetype_funnels::ArchetypeFunnels;
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
    /// Pacing thresholds from `pacing.yaml` (optional per genre pack).
    pub drama_thresholds: Option<DramaThresholds>,
    /// Item catalog and starting loadouts from `inventory.yaml` (optional per genre pack).
    pub inventory: Option<InventoryConfig>,
    /// Opening scenario hooks from `openings.yaml` (optional per genre pack).
    pub openings: Vec<OpeningHook>,
    /// Random backstory tables from `backstory_tables.yaml` (optional per genre pack).
    pub backstory_tables: Option<BackstoryTables>,
    /// Random equipment tables from `equipment_tables.yaml` (optional per genre pack).
    /// Consumed by scenes with `equipment_generation: random_table`. Story 31-3.
    pub equipment_tables: Option<EquipmentTables>,
    /// Base archetype definitions loaded from content root.
    pub base_archetypes: Option<BaseArchetypes>,
    /// Genre-level archetype constraints and flavor.
    pub archetype_constraints: Option<ArchetypeConstraints>,
    /// NPC traits database loaded from content root `npc_traits.yaml`.
    pub npc_traits: Option<super::npc_traits::NpcTraitsDatabase>,
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
    /// Portrait manifest — rich appearance descriptions for NPC portrait generation.
    /// Loaded from `portrait_manifest.yaml` if present.
    pub portrait_manifest: Vec<PortraitManifestEntry>,
    /// World-level archetype funnels.
    pub archetype_funnels: Option<ArchetypeFunnels>,
}

/// A character entry in a portrait manifest — provides rich visual descriptions
/// for Flux portrait generation with identity consistency.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortraitManifestEntry {
    /// Character name (must match NPC registry name).
    pub name: String,
    /// Character role (e.g., "Senior Lamplighter and Fragment Researcher").
    #[serde(default)]
    pub role: String,
    /// Character type: npc_major, npc_supporting, player.
    #[serde(default, rename = "type")]
    pub character_type: String,
    /// Detailed physical appearance for Flux prompt.
    #[serde(default)]
    pub appearance: String,
    /// Cultural/fashion context for visual consistency.
    #[serde(default)]
    pub culture_aesthetic: String,
    /// Genre-specific visual elements (magical auras, tech, etc.).
    #[serde(default)]
    pub element_visual: String,
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
    /// Short blurb for the lobby genre picker.
    #[serde(default)]
    pub lobby_blurb: Option<String>,
    /// Recommended player count range.
    #[serde(default)]
    pub recommended_players: Option<RecommendedPlayers>,
}

/// Recommended player count for a genre pack.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RecommendedPlayers {
    pub min: u8,
    pub max: u8,
    #[serde(default)]
    pub sweet_spot: Option<u8>,
}

/// A creative inspiration reference.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Inspiration {
    /// Source name.
    pub name: String,
    /// Which element is borrowed.
    pub element: String,
}

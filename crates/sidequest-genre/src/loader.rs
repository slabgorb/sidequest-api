//! Unified genre pack loader.
//!
//! A single function loads an entire genre pack from a directory, reading all
//! YAML files and assembling them into a typed `GenrePack`. This replaces the
//! 4 different loading patterns in the Python codebase.

use crate::error::GenreError;
use crate::genre_code::GenreCode;
use crate::models::archetype_axes::BaseArchetypes;
use crate::models::archetype_constraints::ArchetypeConstraints;
use crate::models::archetype_funnels::ArchetypeFunnels;
use crate::models::npc_traits::NpcTraitsDatabase;
use crate::models::*;
use crate::resolve::resolve_trope_inheritance;
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

/// Load a complete genre pack from a directory.
///
/// Reads all YAML files, loads worlds and scenarios, resolves trope inheritance,
/// and returns a fully assembled `GenrePack`.
pub fn load_genre_pack(path: &Path) -> Result<GenrePack, GenreError> {
    if !path.exists() || !path.is_dir() {
        return Err(GenreError::LoadError {
            path: path.display().to_string(),
            detail: "directory does not exist".into(),
        });
    }

    // Load required files
    let meta: PackMeta = load_yaml(&path.join("pack.yaml"))?;
    let rules: RulesConfig = load_rules_config(&path.join("rules.yaml"), path)?;
    let lore: Lore = load_yaml(&path.join("lore.yaml"))?;
    let theme: GenreTheme = load_yaml(&path.join("theme.yaml"))?;
    let archetypes: Vec<NpcArchetype> = load_yaml(&path.join("archetypes.yaml"))?;
    let char_creation: Vec<CharCreationScene> = load_yaml(&path.join("char_creation.yaml"))?;
    let visual_style: VisualStyle = load_yaml(&path.join("visual_style.yaml"))?;
    let progression: ProgressionConfig = load_yaml(&path.join("progression.yaml"))?;
    let axes: AxesConfig = load_yaml(&path.join("axes.yaml"))?;
    let audio: AudioConfig = load_yaml(&path.join("audio.yaml"))?;
    let cultures: Vec<Culture> = load_yaml(&path.join("cultures.yaml"))?;
    let prompts: Prompts = load_yaml(&path.join("prompts.yaml"))?;

    // Load required genre-level tropes
    let genre_tropes: Vec<TropeDefinition> = load_yaml(&path.join("tropes.yaml"))?;

    // Load optional files
    let achievements: Vec<Achievement> =
        load_yaml_optional(&path.join("achievements.yaml"))?.unwrap_or_default();
    let power_tiers: HashMap<String, Vec<PowerTier>> =
        load_yaml_optional(&path.join("power_tiers.yaml"))?.unwrap_or_default();
    let beat_vocabulary: Option<BeatVocabulary> =
        load_yaml_optional(&path.join("beat_vocabulary.yaml"))?;
    let voice_presets: Option<VoicePresets> = load_yaml_optional(&path.join("voice_presets.yaml"))?;
    let drama_thresholds: Option<DramaThresholds> = load_yaml_optional(&path.join("pacing.yaml"))?;
    let inventory: Option<InventoryConfig> = load_yaml_optional(&path.join("inventory.yaml"))?;
    let openings: Vec<OpeningHook> =
        load_yaml_optional(&path.join("openings.yaml"))?.unwrap_or_default();
    let backstory_tables: Option<BackstoryTables> =
        load_yaml_optional(&path.join("backstory_tables.yaml"))?;
    let equipment_tables: Option<EquipmentTables> =
        load_yaml_optional(&path.join("equipment_tables.yaml"))?;

    let base_archetypes: Option<BaseArchetypes> = path
        .parent() // genre_packs/
        .and_then(|p| p.parent()) // content root
        .map(|root| load_yaml_optional(&root.join("archetypes_base.yaml")))
        .transpose()?
        .flatten();
    let npc_traits: Option<NpcTraitsDatabase> = path
        .parent() // genre_packs/
        .and_then(|p| p.parent()) // content root
        .map(|root| load_yaml_optional(&root.join("npc_traits.yaml")))
        .transpose()?
        .flatten();

    let archetype_constraints: Option<ArchetypeConstraints> =
        load_yaml_optional(&path.join("archetype_constraints.yaml"))?;

    // Load worlds and scenarios from subdirectories
    let worlds = load_subdirectories(path, "worlds", |p| load_single_world(p, &genre_tropes))?;
    let scenarios = load_subdirectories(path, "scenarios", load_single_scenario)?;

    Ok(GenrePack {
        meta,
        rules,
        lore,
        theme,
        archetypes,
        char_creation,
        visual_style,
        progression,
        axes,
        audio,
        cultures,
        prompts,
        tropes: genre_tropes,
        beat_vocabulary,
        achievements,
        voice_presets,
        power_tiers,
        worlds,
        scenarios,
        drama_thresholds,
        inventory,
        openings,
        backstory_tables,
        equipment_tables,
        base_archetypes,
        archetype_constraints,
        npc_traits,
    })
}

/// Create a LoadError from a path and a displayable error.
fn load_error(path: &Path, e: impl std::fmt::Display) -> GenreError {
    GenreError::LoadError {
        path: path.display().to_string(),
        detail: e.to_string(),
    }
}

/// Load and parse a required YAML file.
fn load_yaml<T: DeserializeOwned>(path: &Path) -> Result<T, GenreError> {
    let content = std::fs::read_to_string(path).map_err(|e| load_error(path, e))?;
    serde_yaml::from_str(&content).map_err(|e| load_error(path, e))
}

/// Load and parse an optional YAML file (returns None if file doesn't exist).
fn load_yaml_optional<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, GenreError> {
    if !path.exists() {
        return Ok(None);
    }
    load_yaml(path).map(Some)
}

/// Load a genre's advancement tree (Story 39-5 / ADR-078).
///
/// Dual-location rule, per the GM amendment (2026-04-15):
///
/// 1. If `{genre}/progression.yaml` has `mechanical_effects` on any
///    affinity tier, harvest those tiers into the tree.
/// 2. Otherwise, if `{genre}/advancements.yaml` exists, load the tree
///    from it directly.
/// 3. If a genre carries mechanical_effects on `progression.yaml` tiers
///    AND a sibling `advancements.yaml` file, the loader returns a
///    `GenreError::ValidationError` naming both files. No silent
///    fallback — picking one host would mask a genre-author bug.
/// 4. If neither host is present, return `AdvancementTree::default()`
///    (empty). This is not a fallback in the forbidden sense — an
///    empty tree is a valid genre configuration (no mechanical
///    advancements). The dual-host check still fires loudly when
///    both files coexist.
///
/// Returns the assembled [`AdvancementTree`].
pub fn load_advancement_tree(genre_dir: &Path) -> Result<AdvancementTree, GenreError> {
    let progression_path = genre_dir.join("progression.yaml");
    let advancements_path = genre_dir.join("advancements.yaml");

    let progression_tiers = harvest_progression_mechanical_effects(&progression_path)?;
    let advancements_file_exists = advancements_path.is_file();

    if !progression_tiers.is_empty() && advancements_file_exists {
        return Err(GenreError::ValidationError {
            message: format!(
                "genre at {} carries mechanical_effects on progression.yaml affinity tiers AND \
                 a sibling advancements.yaml file; these hosts are mutually exclusive — pick one \
                 (paths: {}, {})",
                genre_dir.display(),
                progression_path.display(),
                advancements_path.display(),
            ),
        });
    }

    if !progression_tiers.is_empty() {
        return Ok(AdvancementTree {
            tiers: progression_tiers,
        });
    }

    if advancements_file_exists {
        let tree: AdvancementTree = load_yaml(&advancements_path)?;
        return Ok(tree);
    }

    Ok(AdvancementTree::default())
}

/// Harvest `mechanical_effects` from each populated affinity tier of a
/// genre's `progression.yaml`, yielding an auto-named
/// [`AdvancementTier`] per host.
///
/// Returns an empty vec when the file is absent or when no tier carries
/// mechanical_effects. The dual-host guard in [`load_advancement_tree`]
/// treats that empty result as "no progression host present."
fn harvest_progression_mechanical_effects(
    progression_path: &Path,
) -> Result<Vec<AdvancementTier>, GenreError> {
    if !progression_path.is_file() {
        return Ok(Vec::new());
    }
    let progression: ProgressionConfig = load_yaml(progression_path)?;
    let mut tiers: Vec<AdvancementTier> = Vec::new();
    for affinity in &progression.affinities {
        let Some(unlocks) = affinity.unlocks.as_ref() else {
            continue;
        };
        for (level, maybe_tier) in [
            (0u32, &unlocks.tier_0),
            (1u32, &unlocks.tier_1),
            (2u32, &unlocks.tier_2),
            (3u32, &unlocks.tier_3),
        ] {
            let Some(tier) = maybe_tier.as_ref() else {
                continue;
            };
            let Some(effects) = tier.mechanical_effects.as_ref() else {
                continue;
            };
            if effects.is_empty() {
                continue;
            }
            let id = format!("{}_t{}", affinity.name.to_lowercase(), level);
            let milestone = format!("{}_t{}_milestone", affinity.name.to_lowercase(), level);
            let raw = RawAdvancementTierHarvest {
                id,
                required_milestone: milestone,
                class_gates: Vec::new(),
                effects: effects.clone(),
            };
            // Run the harvested tier through the same validated-constructor
            // path as YAML-authored tiers so invariants (non-blank id / milestone)
            // are enforced uniformly.
            let built =
                AdvancementTier::try_from(raw).map_err(|e| GenreError::ValidationError {
                    message: format!(
                        "harvested progression tier from {}: {}",
                        progression_path.display(),
                        e
                    ),
                })?;
            tiers.push(built);
        }
    }
    Ok(tiers)
}

/// Locally-scoped conversion target for harvested progression tiers.
/// Uses the same field shape as `RawAdvancementTier` so the
/// `TryFrom<RawAdvancementTier>` validation runs for us.
struct RawAdvancementTierHarvest {
    id: String,
    required_milestone: String,
    class_gates: Vec<String>,
    effects: Vec<AdvancementEffect>,
}

impl TryFrom<RawAdvancementTierHarvest> for AdvancementTier {
    type Error = crate::models::advancement::AdvancementTierError;

    fn try_from(raw: RawAdvancementTierHarvest) -> Result<Self, Self::Error> {
        use crate::models::advancement::AdvancementTierError as E;
        if raw.id.trim().is_empty() {
            return Err(E::BlankId);
        }
        if raw.required_milestone.trim().is_empty() {
            return Err(E::BlankRequiredMilestone);
        }
        Ok(Self {
            id: raw.id,
            required_milestone: raw.required_milestone,
            class_gates: raw.class_gates,
            effects: raw.effects,
        })
    }
}

/// Load all subdirectories of `{pack_path}/{subdir}/` into a HashMap,
/// applying `loader` to each subdirectory.
fn load_subdirectories<T, F>(
    pack_path: &Path,
    subdir: &str,
    loader: F,
) -> Result<HashMap<String, T>, GenreError>
where
    F: Fn(&Path) -> Result<T, GenreError>,
{
    let dir = pack_path.join(subdir);
    if !dir.exists() {
        return Ok(HashMap::new());
    }

    let mut result = HashMap::new();
    let entries = std::fs::read_dir(&dir).map_err(|e| load_error(&dir, e))?;

    for entry in entries {
        let entry = entry.map_err(|e| load_error(&dir, e))?;
        let entry_path = entry.path();
        if entry_path.is_dir() {
            let slug = entry.file_name().to_string_lossy().to_string();
            let item = loader(&entry_path)?;
            result.insert(slug, item);
        }
    }

    Ok(result)
}

/// Load a single world from its directory.
fn load_single_world(
    world_path: &Path,
    genre_tropes: &[TropeDefinition],
) -> Result<World, GenreError> {
    let config: WorldConfig = load_yaml(&world_path.join("world.yaml"))?;
    let lore: WorldLore = load_yaml(&world_path.join("lore.yaml"))?;
    let mut cartography: CartographyConfig = load_yaml(&world_path.join("cartography.yaml"))?;

    // When navigation_mode is RoomGraph, load rooms from a separate rooms.yaml file
    if cartography.navigation_mode == NavigationMode::RoomGraph {
        let rooms: Option<Vec<RoomDef>> = load_yaml_optional(&world_path.join("rooms.yaml"))?;
        cartography.rooms = rooms;
    }

    let cultures: Vec<Culture> =
        load_yaml_optional(&world_path.join("cultures.yaml"))?.unwrap_or_default();

    // Legends: accept either Vec<Legend> (low_fantasy) or map with "legends" key (road_warrior).
    // Keep the raw value for AI prompt injection of origin_myth etc.
    let legends_path = world_path.join("legends.yaml");
    let (legends, legends_raw) = load_legends_flexible(&legends_path)?;

    // Load world tropes and resolve inheritance from genre-level tropes
    let raw_world_tropes: Vec<TropeDefinition> =
        load_yaml_optional(&world_path.join("tropes.yaml"))?.unwrap_or_default();
    let tropes = if raw_world_tropes.is_empty() {
        Vec::new()
    } else {
        resolve_trope_inheritance(genre_tropes, &raw_world_tropes)?
    };

    // Optional world-level overrides
    let archetypes: Vec<NpcArchetype> =
        load_yaml_optional(&world_path.join("archetypes.yaml"))?.unwrap_or_default();
    let visual_style: Option<serde_json::Value> =
        load_yaml_optional(&world_path.join("visual_style.yaml"))?;
    let history: Option<serde_json::Value> = load_yaml_optional(&world_path.join("history.yaml"))?;

    let archetype_funnels: Option<ArchetypeFunnels> =
        load_yaml_optional(&world_path.join("archetype_funnels.yaml"))?;

    // World-tier opening hooks and chargen scenes.
    //
    // Interim surgical override ahead of the full Phase 2 layered-content
    // migration: when a world supplies its own `openings.yaml` or
    // `char_creation.yaml`, consumers prefer those over the genre-tier
    // list. This stops named genre-tier content (e.g. the Long Foundry
    // covenant openings under heavy_metal) from leaking into every world.
    let openings: Vec<OpeningHook> =
        load_yaml_optional(&world_path.join("openings.yaml"))?.unwrap_or_default();
    let char_creation: Vec<CharCreationScene> =
        load_yaml_optional(&world_path.join("char_creation.yaml"))?.unwrap_or_default();

    // Portrait manifest — rich appearance descriptions for NPC portrait generation.
    let portrait_manifest: Vec<PortraitManifestEntry> = {
        #[derive(serde::Deserialize)]
        struct PortraitManifestWrapper {
            #[serde(default)]
            characters: Vec<PortraitManifestEntry>,
        }
        load_yaml_optional::<PortraitManifestWrapper>(&world_path.join("portrait_manifest.yaml"))?
            .map(|w| w.characters)
            .unwrap_or_default()
    };

    Ok(World {
        config,
        lore,
        legends,
        cartography,
        cultures,
        tropes,
        archetypes,
        visual_style,
        history,
        legends_raw,
        portrait_manifest,
        archetype_funnels,
        openings,
        char_creation,
    })
}

/// Load legends.yaml flexibly: accepts Vec<Legend> or a map with a "legends" key.
fn load_legends_flexible(
    path: &Path,
) -> Result<(Vec<Legend>, Option<serde_json::Value>), GenreError> {
    if !path.exists() {
        return Ok((Vec::new(), None));
    }

    let content = std::fs::read_to_string(path).map_err(|e| load_error(path, e))?;

    // Try as Vec<Legend> first (low_fantasy format)
    if let Ok(legends) = serde_yaml::from_str::<Vec<Legend>>(&content) {
        return Ok((legends, None));
    }

    // Try as a map — extract "legends" key if present, keep full raw value
    let raw: serde_json::Value = serde_yaml::from_str(&content).map_err(|e| load_error(path, e))?;

    let legends = if let Some(legends_val) = raw.get("legends") {
        serde_json::from_value::<Vec<Legend>>(legends_val.clone())
            .map_err(|e| load_error(path, e))?
    } else {
        Vec::new()
    };

    Ok((legends, Some(raw)))
}

/// Multi-path genre pack loader.
///
/// Searches a list of directories in order for genre packs, loading the first
/// match found. Supports the search order: local, home, install.
pub struct GenreLoader {
    search_paths: Vec<PathBuf>,
}

impl GenreLoader {
    /// Create a loader with the given search paths (checked in order).
    pub fn new(search_paths: Vec<PathBuf>) -> Self {
        Self { search_paths }
    }

    /// Find the directory for a genre code by searching all paths.
    ///
    /// Returns the first path where `{search_path}/{genre_code}/` exists as a directory.
    pub fn find(&self, code: &GenreCode) -> Result<PathBuf, GenreError> {
        let mut searched = Vec::new();
        for base in &self.search_paths {
            let candidate = base.join(code.as_str());
            if candidate.is_dir() {
                return Ok(candidate);
            }
            searched.push(base.display().to_string());
        }
        Err(GenreError::NotFound {
            code: code.as_str().to_string(),
            searched,
        })
    }

    /// Find and load a genre pack by code.
    ///
    /// Call `pack.validate()` separately for cross-reference validation (phase 2).
    pub fn load(&self, code: &GenreCode) -> Result<GenrePack, GenreError> {
        let path = self.find(code)?;
        load_genre_pack(&path)
    }
}

// ───────────────────────────────────────────────────────────
// Story 38-4 — Interaction table loader + `_from` pointer
// ───────────────────────────────────────────────────────────

/// Load a standalone interaction table YAML file.
///
/// Thin wrapper around [`load_yaml`] that enforces the "no silent fallbacks"
/// rule — a missing file surfaces as [`GenreError::LoadError`] rather than an
/// empty/default table. Validation (non-empty cells, unique pair keys) runs
/// through the `InteractionTable` `TryFrom` impl.
pub fn load_interaction_table(path: &Path) -> Result<InteractionTable, GenreError> {
    load_yaml(path)
}

/// Load and resolve `rules.yaml`, honoring `_from:` pointers on
/// confrontation `interaction_table` fields.
///
/// A confrontation may carry its interaction table inline:
///
/// ```yaml
/// confrontations:
///   - type: dogfight
///     interaction_table:
///       version: "0.1.0"
///       cells: [ ... ]
/// ```
///
/// …or reference a sibling file pack-relative:
///
/// ```yaml
/// confrontations:
///   - type: dogfight
///     interaction_table:
///       _from: dogfight/interactions_mvp.yaml
/// ```
///
/// The resolver:
/// - reads `rules.yaml` as a raw `serde_yaml::Value`,
/// - walks `confrontations[].interaction_table` entries, substituting any
///   `{ _from: <relpath> }` mapping with the content of the referenced file,
/// - rejects absolute paths and parent-directory traversal (no sandbox escape),
/// - rejects nested `_from` chains (no unbounded recursive input — Rule #15),
/// - then deserializes the resolved tree into [`RulesConfig`], running all
///   the existing `TryFrom` validators on the merged data.
pub fn load_rules_config(rules_path: &Path, pack_dir: &Path) -> Result<RulesConfig, GenreError> {
    let content = std::fs::read_to_string(rules_path).map_err(|e| load_error(rules_path, e))?;
    let mut value: serde_yaml::Value =
        serde_yaml::from_str(&content).map_err(|e| load_error(rules_path, e))?;

    if let Some(mapping) = value.as_mapping_mut() {
        let confrontations_key = serde_yaml::Value::String("confrontations".to_string());
        if let Some(confrontations) = mapping.get_mut(&confrontations_key) {
            if let Some(seq) = confrontations.as_sequence_mut() {
                for conf in seq.iter_mut() {
                    resolve_confrontation_from_pointers(conf, pack_dir)?;
                }
            }
        }
    }

    serde_yaml::from_value(value).map_err(|e| load_error(rules_path, e))
}

/// Walk a single confrontation YAML value and resolve any `_from` pointers on
/// its `interaction_table` field.
fn resolve_confrontation_from_pointers(
    conf: &mut serde_yaml::Value,
    pack_dir: &Path,
) -> Result<(), GenreError> {
    let Some(mapping) = conf.as_mapping_mut() else {
        return Ok(());
    };
    let interaction_key = serde_yaml::Value::String("interaction_table".to_string());
    let Some(it_value) = mapping.get_mut(&interaction_key) else {
        return Ok(());
    };
    let Some(from_rel) = extract_from_pointer(it_value) else {
        return Ok(());
    };
    let resolved = resolve_from_pointer(&from_rel, pack_dir)?;
    *it_value = resolved;
    Ok(())
}

/// Build the `_from` key value used to probe `serde_yaml::Mapping`s. Single
/// source of truth so the string literal never drifts between call sites.
fn from_key() -> serde_yaml::Value {
    serde_yaml::Value::String("_from".to_string())
}

/// If `value` is a mapping of shape `{ _from: "relpath" }` (single key),
/// return the string. Otherwise return `None`.
fn extract_from_pointer(value: &serde_yaml::Value) -> Option<String> {
    let mapping = value.as_mapping()?;
    if mapping.len() != 1 {
        return None;
    }
    let from_val = mapping.get(from_key())?;
    from_val.as_str().map(|s| s.to_string())
}

/// Read a `_from`-referenced sub-file, enforcing pack-relative path safety
/// and rejecting nested `_from` chains.
fn resolve_from_pointer(rel: &str, pack_dir: &Path) -> Result<serde_yaml::Value, GenreError> {
    let rel_path = Path::new(rel);

    if rel_path.is_absolute() {
        return Err(GenreError::LoadError {
            path: rel.to_string(),
            detail: format!("_from path must be pack-relative (got absolute path: {rel})"),
        });
    }

    for component in rel_path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir => {
                return Err(GenreError::LoadError {
                    path: rel.to_string(),
                    detail: format!(
                        "_from path must not contain parent-directory traversal: {rel}"
                    ),
                });
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(GenreError::LoadError {
                    path: rel.to_string(),
                    detail: format!("_from path must be pack-relative: {rel}"),
                });
            }
        }
    }

    let full = pack_dir.join(rel_path);
    let content = std::fs::read_to_string(&full).map_err(|e| load_error(&full, e))?;
    let value: serde_yaml::Value =
        serde_yaml::from_str(&content).map_err(|e| load_error(&full, e))?;

    // Reject nested `_from` chains — the sub-file must be a concrete body,
    // not another pointer. Keeps the resolver non-recursive (Rule #15).
    if let Some(mapping) = value.as_mapping() {
        if mapping.contains_key(from_key()) {
            return Err(GenreError::LoadError {
                path: full.display().to_string(),
                detail: "nested _from pointers are not allowed".to_string(),
            });
        }
    }

    Ok(value)
}

/// Load a single scenario from its directory.
///
/// Reads scenario.yaml for the base, then overlays assignment_matrix.yaml,
/// clue_graph.yaml, atmosphere_matrix.yaml, and npcs.yaml.
fn load_single_scenario(scenario_path: &Path) -> Result<ScenarioPack, GenreError> {
    let mut scenario: ScenarioPack = load_yaml(&scenario_path.join("scenario.yaml"))?;

    // Overlay supplementary files
    if let Some(matrix) = load_yaml_optional(&scenario_path.join("assignment_matrix.yaml"))? {
        scenario.assignment_matrix = matrix;
    }
    if let Some(graph) = load_yaml_optional(&scenario_path.join("clue_graph.yaml"))? {
        scenario.clue_graph = graph;
    }
    if let Some(atmo) = load_yaml_optional(&scenario_path.join("atmosphere_matrix.yaml"))? {
        scenario.atmosphere_matrix = atmo;
    }
    if let Some(npcs) = load_yaml_optional::<Vec<ScenarioNpc>>(&scenario_path.join("npcs.yaml"))? {
        scenario.npcs = npcs;
    }

    Ok(scenario)
}

//! Unified genre pack loader.
//!
//! A single function loads an entire genre pack from a directory, reading all
//! YAML files and assembling them into a typed `GenrePack`. This replaces the
//! 4 different loading patterns in the Python codebase.

use crate::error::GenreError;
use crate::genre_code::GenreCode;
use crate::models::*;
use crate::resolve::resolve_trope_inheritance;
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
    let rules: RulesConfig = load_yaml(&path.join("rules.yaml"))?;
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
    let drama_thresholds: Option<DramaThresholds> =
        load_yaml_optional(&path.join("pacing.yaml"))?;
    let inventory: Option<InventoryConfig> =
        load_yaml_optional(&path.join("inventory.yaml"))?;
    let openings: Vec<OpeningHook> =
        load_yaml_optional(&path.join("openings.yaml"))?.unwrap_or_default();

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
    let cartography: CartographyConfig = load_yaml(&world_path.join("cartography.yaml"))?;
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
    let history: Option<serde_json::Value> =
        load_yaml_optional(&world_path.join("history.yaml"))?;

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
    })
}

/// Load legends.yaml flexibly: accepts Vec<Legend> or a map with a "legends" key.
fn load_legends_flexible(path: &Path) -> Result<(Vec<Legend>, Option<serde_json::Value>), GenreError> {
    if !path.exists() {
        return Ok((Vec::new(), None));
    }

    let content = std::fs::read_to_string(path).map_err(|e| load_error(path, e))?;

    // Try as Vec<Legend> first (low_fantasy format)
    if let Ok(legends) = serde_yaml::from_str::<Vec<Legend>>(&content) {
        return Ok((legends, None));
    }

    // Try as a map — extract "legends" key if present, keep full raw value
    let raw: serde_json::Value =
        serde_yaml::from_str(&content).map_err(|e| load_error(path, e))?;

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

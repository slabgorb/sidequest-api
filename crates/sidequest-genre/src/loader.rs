//! Unified genre pack loader.
//!
//! A single function loads an entire genre pack from a directory, reading all
//! YAML files and assembling them into a typed `GenrePack`. This replaces the
//! 4 different loading patterns in the Python codebase.

use crate::error::GenreError;
use crate::models::*;
use crate::resolve::resolve_trope_inheritance;
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::path::Path;

/// Load a complete genre pack from a directory.
///
/// Reads all YAML files, loads worlds and scenarios, resolves trope inheritance,
/// and returns a fully assembled `GenrePack`.
pub fn load_genre_pack(path: &Path) -> Result<GenrePack, GenreError> {
    if !path.exists() || !path.is_dir() {
        return Err(GenreError::LoadError {
            path: path.display().to_string(),
            source: "directory does not exist".into(),
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

    // Load optional files
    let genre_tropes: Vec<TropeDefinition> =
        load_yaml_optional(&path.join("tropes.yaml"))?.unwrap_or_default();
    let beat_vocabulary: Option<BeatVocabulary> =
        load_yaml_optional(&path.join("beat_vocabulary.yaml"))?;
    let achievements: Vec<Achievement> =
        load_yaml_optional(&path.join("achievements.yaml"))?.unwrap_or_default();
    let voice_presets: Option<VoicePresets> =
        load_yaml_optional(&path.join("voice_presets.yaml"))?;
    let power_tiers: HashMap<String, Vec<PowerTier>> =
        load_yaml_optional(&path.join("power_tiers.yaml"))?.unwrap_or_default();

    // Load worlds
    let worlds = load_worlds(path, &genre_tropes)?;

    // Load scenarios
    let scenarios = load_scenarios(path)?;

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
    })
}

/// Load and parse a required YAML file.
fn load_yaml<T: DeserializeOwned>(path: &Path) -> Result<T, GenreError> {
    let content = std::fs::read_to_string(path).map_err(|e| GenreError::LoadError {
        path: path.display().to_string(),
        source: e.to_string(),
    })?;
    serde_yaml::from_str(&content).map_err(|e| GenreError::LoadError {
        path: path.display().to_string(),
        source: e.to_string(),
    })
}

/// Load and parse an optional YAML file (returns None if file doesn't exist).
fn load_yaml_optional<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, GenreError> {
    if !path.exists() {
        return Ok(None);
    }
    load_yaml(path).map(Some)
}

/// Load all worlds from `worlds/*/`.
fn load_worlds(
    pack_path: &Path,
    genre_tropes: &[TropeDefinition],
) -> Result<HashMap<String, World>, GenreError> {
    let worlds_dir = pack_path.join("worlds");
    if !worlds_dir.exists() {
        return Ok(HashMap::new());
    }

    let mut worlds = HashMap::new();
    let entries = std::fs::read_dir(&worlds_dir).map_err(|e| GenreError::LoadError {
        path: worlds_dir.display().to_string(),
        source: e.to_string(),
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| GenreError::LoadError {
            path: worlds_dir.display().to_string(),
            source: e.to_string(),
        })?;
        let world_path = entry.path();
        if world_path.is_dir() {
            let slug = entry
                .file_name()
                .to_string_lossy()
                .to_string();
            let world = load_single_world(&world_path, genre_tropes)?;
            worlds.insert(slug, world);
        }
    }

    Ok(worlds)
}

/// Load a single world from its directory.
fn load_single_world(
    world_path: &Path,
    genre_tropes: &[TropeDefinition],
) -> Result<World, GenreError> {
    let config: WorldConfig = load_yaml(&world_path.join("world.yaml"))?;
    let lore: WorldLore = load_yaml(&world_path.join("lore.yaml"))?;
    let legends: Vec<Legend> =
        load_yaml_optional(&world_path.join("legends.yaml"))?.unwrap_or_default();
    let cartography: CartographyConfig = load_yaml(&world_path.join("cartography.yaml"))?;
    let cultures: Vec<Culture> =
        load_yaml_optional(&world_path.join("cultures.yaml"))?.unwrap_or_default();

    // Load world tropes and resolve inheritance from genre-level tropes
    let raw_world_tropes: Vec<TropeDefinition> =
        load_yaml_optional(&world_path.join("tropes.yaml"))?.unwrap_or_default();
    let tropes = if raw_world_tropes.is_empty() {
        Vec::new()
    } else {
        resolve_trope_inheritance(genre_tropes, &raw_world_tropes)?
    };

    Ok(World {
        config,
        lore,
        legends,
        cartography,
        cultures,
        tropes,
    })
}

/// Load all scenarios from `scenarios/*/`.
fn load_scenarios(
    pack_path: &Path,
) -> Result<HashMap<String, ScenarioPack>, GenreError> {
    let scenarios_dir = pack_path.join("scenarios");
    if !scenarios_dir.exists() {
        return Ok(HashMap::new());
    }

    let mut scenarios = HashMap::new();
    let entries = std::fs::read_dir(&scenarios_dir).map_err(|e| GenreError::LoadError {
        path: scenarios_dir.display().to_string(),
        source: e.to_string(),
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| GenreError::LoadError {
            path: scenarios_dir.display().to_string(),
            source: e.to_string(),
        })?;
        let scenario_path = entry.path();
        if scenario_path.is_dir() {
            let slug = entry
                .file_name()
                .to_string_lossy()
                .to_string();
            let scenario = load_single_scenario(&scenario_path)?;
            scenarios.insert(slug, scenario);
        }
    }

    Ok(scenarios)
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

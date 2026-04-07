//! Genre pack schema validator.
//!
//! Loads every YAML file in a genre pack against its expected Rust type and
//! reports ALL deserialization errors at once, instead of failing on the first one.
//!
//! Usage:
//!   sidequest-validate --genre-packs-path ./genre_packs --genre caverns_and_claudes
//!   sidequest-validate --genre-packs-path ./genre_packs  # validates ALL packs

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use clap::Parser;
use serde::de::DeserializeOwned;

// Re-use all the model types from sidequest-genre (re-exported at crate root)
use sidequest_genre::*;

#[derive(Parser)]
#[command(
    name = "sidequest-validate",
    about = "Validate genre pack YAML files against Rust data models"
)]
struct Cli {
    /// Path to the genre_packs/ directory. Also reads SIDEQUEST_CONTENT_PATH env var.
    #[arg(long, env = "SIDEQUEST_CONTENT_PATH")]
    genre_packs_path: PathBuf,

    /// Genre slug to validate (e.g., caverns_and_claudes). Omit to validate all.
    #[arg(long)]
    genre: Option<String>,
}

// ── Validation result ���─────────────────────────────────────

struct FileResult {
    path: String,
    required: bool,
    status: FileStatus,
}

enum FileStatus {
    Ok,
    Missing,
    Error(String),
}

// ── Main ───────────────────────────────────────��───────────

fn main() {
    let cli = Cli::parse();

    let genres: Vec<String> = if let Some(ref genre) = cli.genre {
        vec![genre.clone()]
    } else {
        // Discover all genre pack directories
        match std::fs::read_dir(&cli.genre_packs_path) {
            Ok(entries) => entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .filter(|e| {
                    // Skip hidden directories and common non-pack dirs
                    let name = e.file_name().to_string_lossy().to_string();
                    !name.starts_with('.') && name != "shared"
                })
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect(),
            Err(e) => {
                eprintln!("ERROR: Cannot read genre_packs directory: {e}");
                std::process::exit(1);
            }
        }
    };

    let mut total_errors = 0;
    let mut total_ok = 0;
    let mut total_missing_optional = 0;

    for genre in &genres {
        let genre_dir = cli.genre_packs_path.join(genre);
        if !genre_dir.is_dir() {
            eprintln!("ERROR: Genre pack directory not found: {}", genre_dir.display());
            total_errors += 1;
            continue;
        }

        println!("\n━━━ {} ��━━", genre);
        let results = validate_genre_pack(&genre_dir);

        for r in &results {
            match &r.status {
                FileStatus::Ok => {
                    println!("  ✓ {}", r.path);
                    total_ok += 1;
                }
                FileStatus::Missing if r.required => {
                    println!("  ✗ {} — MISSING (required)", r.path);
                    total_errors += 1;
                }
                FileStatus::Missing => {
                    println!("  - {} — missing (optional)", r.path);
                    total_missing_optional += 1;
                }
                FileStatus::Error(msg) => {
                    println!("  ✗ {} — ERROR", r.path);
                    // Indent the error message for readability
                    for line in msg.lines() {
                        println!("      {line}");
                    }
                    total_errors += 1;
                }
            }
        }
    }

    // Summary
    println!("\n━━━ Summary ━━━");
    println!(
        "  {} ok, {} errors, {} missing optional",
        total_ok, total_errors, total_missing_optional
    );

    if total_errors > 0 {
        std::process::exit(1);
    }
}

// ── Per-genre validation ───────────────────────────────────

fn validate_genre_pack(pack_dir: &Path) -> Vec<FileResult> {
    let mut results = Vec::new();

    // Required genre-level files
    check_yaml::<PackMeta>(&mut results, pack_dir, "pack.yaml", true);
    check_yaml::<RulesConfig>(&mut results, pack_dir, "rules.yaml", true);
    check_yaml::<Lore>(&mut results, pack_dir, "lore.yaml", true);
    check_yaml::<GenreTheme>(&mut results, pack_dir, "theme.yaml", true);
    check_yaml::<Vec<NpcArchetype>>(&mut results, pack_dir, "archetypes.yaml", true);
    check_yaml::<Vec<CharCreationScene>>(&mut results, pack_dir, "char_creation.yaml", true);
    check_yaml::<VisualStyle>(&mut results, pack_dir, "visual_style.yaml", true);
    check_yaml::<ProgressionConfig>(&mut results, pack_dir, "progression.yaml", true);
    check_yaml::<AxesConfig>(&mut results, pack_dir, "axes.yaml", true);
    check_yaml::<AudioConfig>(&mut results, pack_dir, "audio.yaml", true);
    check_yaml::<Vec<Culture>>(&mut results, pack_dir, "cultures.yaml", true);
    check_yaml::<Prompts>(&mut results, pack_dir, "prompts.yaml", true);
    check_yaml::<Vec<TropeDefinition>>(&mut results, pack_dir, "tropes.yaml", true);

    // Optional genre-level files
    check_yaml::<Vec<Achievement>>(&mut results, pack_dir, "achievements.yaml", false);
    check_yaml::<HashMap<String, Vec<PowerTier>>>(&mut results, pack_dir, "power_tiers.yaml", false);
    check_yaml::<BeatVocabulary>(&mut results, pack_dir, "beat_vocabulary.yaml", false);
    check_yaml::<VoicePresets>(&mut results, pack_dir, "voice_presets.yaml", false);
    check_yaml::<DramaThresholds>(&mut results, pack_dir, "pacing.yaml", false);
    check_yaml::<InventoryConfig>(&mut results, pack_dir, "inventory.yaml", false);
    check_yaml::<Vec<OpeningHook>>(&mut results, pack_dir, "openings.yaml", false);

    // Validate worlds
    let worlds_dir = pack_dir.join("worlds");
    if worlds_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&worlds_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let world_path = entry.path();
                if world_path.is_dir() {
                    let world_slug = entry.file_name().to_string_lossy().to_string();
                    validate_world(&mut results, &world_path, &world_slug);
                }
            }
        }
    }

    // Validate scenarios
    let scenarios_dir = pack_dir.join("scenarios");
    if scenarios_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&scenarios_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let scenario_path = entry.path();
                if scenario_path.is_dir() {
                    let scenario_slug = entry.file_name().to_string_lossy().to_string();
                    let prefix = format!("scenarios/{scenario_slug}");
                    check_yaml::<ScenarioPack>(&mut results, &scenario_path, &format!("{prefix}/scenario.yaml"), true);
                }
            }
        }
    }

    results
}

fn validate_world(results: &mut Vec<FileResult>, world_path: &Path, world_slug: &str) {
    let prefix = format!("worlds/{world_slug}");

    // Required world files
    check_yaml::<WorldConfig>(results, world_path, &format!("{prefix}/world.yaml"), true);
    check_yaml::<WorldLore>(results, world_path, &format!("{prefix}/lore.yaml"), true);
    check_yaml::<CartographyConfig>(results, world_path, &format!("{prefix}/cartography.yaml"), true);

    // Optional world files
    check_yaml::<Vec<Culture>>(results, world_path, &format!("{prefix}/cultures.yaml"), false);
    check_yaml::<Vec<TropeDefinition>>(results, world_path, &format!("{prefix}/tropes.yaml"), false);
    check_yaml::<Vec<NpcArchetype>>(results, world_path, &format!("{prefix}/archetypes.yaml"), false);
    check_yaml::<serde_json::Value>(results, world_path, &format!("{prefix}/visual_style.yaml"), false);
    check_yaml::<serde_json::Value>(results, world_path, &format!("{prefix}/history.yaml"), false);
    check_yaml::<Vec<RoomDef>>(results, world_path, &format!("{prefix}/rooms.yaml"), false);
    check_yaml::<serde_json::Value>(results, world_path, &format!("{prefix}/creatures.yaml"), false);
    check_yaml::<serde_json::Value>(results, world_path, &format!("{prefix}/encounter_tables.yaml"), false);
    check_yaml::<serde_json::Value>(results, world_path, &format!("{prefix}/factions.yaml"), false);
    check_yaml::<Vec<OpeningHook>>(results, world_path, &format!("{prefix}/openings.yaml"), false);
    check_yaml::<DramaThresholds>(results, world_path, &format!("{prefix}/pacing.yaml"), false);

    // Legends: flexible format (Vec<Legend> or map with "legends" key)
    let legends_path = world_path.join("legends.yaml");
    if legends_path.exists() {
        let display_path = format!("{prefix}/legends.yaml");
        match std::fs::read_to_string(&legends_path) {
            Ok(content) => {
                // Try Vec<Legend> first, then map format
                if serde_yaml::from_str::<Vec<Legend>>(&content).is_ok() {
                    results.push(FileResult { path: display_path, required: true, status: FileStatus::Ok });
                } else if serde_yaml::from_str::<serde_json::Value>(&content).is_ok() {
                    // Accepts any valid YAML — the loader extracts "legends" key from map
                    results.push(FileResult { path: display_path, required: true, status: FileStatus::Ok });
                } else {
                    results.push(FileResult { path: display_path, required: true, status: FileStatus::Error("Invalid YAML".into()) });
                }
            }
            Err(e) => {
                results.push(FileResult { path: display_path, required: true, status: FileStatus::Error(e.to_string()) });
            }
        }
    } else {
        results.push(FileResult { path: format!("{prefix}/legends.yaml"), required: false, status: FileStatus::Missing });
    }
}

// ── Helpers ─────────────��──────────────────────────────────

fn check_yaml<T: DeserializeOwned>(
    results: &mut Vec<FileResult>,
    base_dir: &Path,
    display_path: &str,
    required: bool,
) {
    // Extract just the filename from the display path for the actual file lookup
    let filename = display_path.rsplit('/').next().unwrap_or(display_path);
    let file_path = base_dir.join(filename);

    if !file_path.exists() {
        results.push(FileResult {
            path: display_path.to_string(),
            required,
            status: if required { FileStatus::Missing } else { FileStatus::Missing },
        });
        return;
    }

    match std::fs::read_to_string(&file_path) {
        Ok(content) => match serde_yaml::from_str::<T>(&content) {
            Ok(_) => {
                results.push(FileResult {
                    path: display_path.to_string(),
                    required,
                    status: FileStatus::Ok,
                });
            }
            Err(e) => {
                results.push(FileResult {
                    path: display_path.to_string(),
                    required,
                    status: FileStatus::Error(e.to_string()),
                });
            }
        },
        Err(e) => {
            results.push(FileResult {
                path: display_path.to_string(),
                required,
                status: FileStatus::Error(format!("read error: {e}")),
            });
        }
    }
}

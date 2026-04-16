//! `sidequest-fixture` CLI — load/list/dump scene fixtures.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use sidequest_fixture::{hydrate_fixture, load_fixture, save_path_for, FixtureError};
use sidequest_game::persistence::{SessionStore, SqliteStore};
use sidequest_genre::{GenreCode, GenreLoader, GenrePack};

#[derive(Parser, Debug)]
#[command(name = "sidequest-fixture", about = "Scene harness fixture loader")]
struct Args {
    /// Root directory of the content repo (genre_packs/ lives underneath).
    #[arg(
        long,
        env = "SIDEQUEST_CONTENT",
        default_value = "../sidequest-content"
    )]
    content_root: PathBuf,

    /// Root directory where fixtures are stored.
    #[arg(long, env = "SIDEQUEST_FIXTURES", default_value = "scenarios/fixtures")]
    fixtures_root: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Hydrate a fixture and write it to the standard save path.
    Load { name: String },
    /// List available fixtures in `fixtures_root`.
    List,
    /// Hydrate a fixture and print the resulting GameSnapshot as JSON.
    Dump { name: String },
}

#[derive(Debug, thiserror::Error)]
enum CliError {
    #[error(transparent)]
    Fixture(#[from] FixtureError),
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("genre loader: {0}")]
    Genre(String),
    #[error("save path not UTF-8")]
    SavePathNotUtf8,
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("persistence: {0}")]
    Persist(#[from] sidequest_game::persistence::PersistError),
}

fn main() -> ExitCode {
    let args = Args::parse();
    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("sidequest-fixture: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Args) -> Result<(), CliError> {
    match args.command {
        Command::Load { name } => cmd_load(&args.content_root, &args.fixtures_root, &name),
        Command::List => cmd_list(&args.fixtures_root),
        Command::Dump { name } => cmd_dump(&args.content_root, &args.fixtures_root, &name),
    }
}

fn fixture_path(fixtures_root: &std::path::Path, name: &str) -> PathBuf {
    if name.ends_with(".yaml") || name.contains('/') {
        PathBuf::from(name)
    } else {
        fixtures_root.join(format!("{name}.yaml"))
    }
}

fn load_pack(content_root: &std::path::Path, genre: &str) -> Result<GenrePack, CliError> {
    let loader = GenreLoader::new(vec![content_root.join("genre_packs")]);
    let code = GenreCode::new(genre).map_err(|e| CliError::Genre(format!("{e}")))?;
    loader
        .load(&code)
        .map_err(|e| CliError::Genre(format!("{e}")))
}

fn cmd_load(
    content_root: &std::path::Path,
    fixtures_root: &std::path::Path,
    name: &str,
) -> Result<(), CliError> {
    let path = fixture_path(fixtures_root, name);
    let fixture = load_fixture(&path)?;
    let pack = load_pack(content_root, &fixture.genre)?;
    let snapshot = hydrate_fixture(&fixture, &pack)?;

    let save_path = save_path_for(&fixture.genre, &fixture.world, &fixture.player_name);
    if let Some(parent) = save_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let store = SqliteStore::open(save_path.to_str().ok_or(CliError::SavePathNotUtf8)?)?;
    store.init_session(&fixture.genre, &fixture.world)?;
    store.save(&snapshot)?;

    println!(
        "loaded fixture '{}' → {}",
        fixture.name,
        save_path.display()
    );
    println!("  genre:  {}", fixture.genre);
    println!("  world:  {}", fixture.world);
    println!("  player: {}", fixture.player_name);
    Ok(())
}

fn cmd_list(fixtures_root: &std::path::Path) -> Result<(), CliError> {
    if !fixtures_root.exists() {
        println!("no fixtures directory at {}", fixtures_root.display());
        return Ok(());
    }
    let mut entries: Vec<_> = std::fs::read_dir(fixtures_root)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("yaml"))
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        match load_fixture(&path) {
            Ok(f) => println!("{stem:<20} {}", f.description),
            Err(e) => println!("{stem:<20} [error: {e}]"),
        }
    }
    Ok(())
}

fn cmd_dump(
    content_root: &std::path::Path,
    fixtures_root: &std::path::Path,
    name: &str,
) -> Result<(), CliError> {
    let path = fixture_path(fixtures_root, name);
    let fixture = load_fixture(&path)?;
    let pack = load_pack(content_root, &fixture.genre)?;
    let snapshot = hydrate_fixture(&fixture, &pack)?;
    println!("{}", serde_json::to_string_pretty(&snapshot)?);
    Ok(())
}

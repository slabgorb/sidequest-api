//! Dev-only scene harness route.
//!
//! Mounted only when `DEV_SCENES=1` is set in the server environment. Loads
//! a fixture file, hydrates it against the live genre cache, and writes the
//! resulting snapshot to the standard save path so that the normal
//! `dispatch_connect` restore path picks it up when the UI connects with
//! matching `{playerName, genre, world}`.
//!
//! Do not enable in production. The route trusts the fixture path and
//! writes arbitrary save files.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use sidequest_fixture::{hydrate_fixture, load_fixture, save_path_for};
use sidequest_game::persistence::{SessionStore, SqliteStore};
use sidequest_genre::GenreCode;

use crate::AppState;
use crate::{WatcherEventBuilder, WatcherEventType};

#[derive(Serialize)]
pub struct SceneMetadata {
    pub fixture_name: String,
    pub player_name: String,
    pub genre: String,
    pub world: String,
    pub has_encounter: bool,
}

/// Build the router for dev scene routes. Mounted by `build_router` under
/// `/dev` when `DEV_SCENES=1`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/scene/:name", get(get_scene_metadata).post(load_scene))
        .route("/scenes", get(list_scenes))
}

/// Directory fixtures live in. Defaults to `scenarios/fixtures` relative to
/// the current working directory; override with `SIDEQUEST_FIXTURES`.
fn fixtures_root() -> std::path::PathBuf {
    std::env::var("SIDEQUEST_FIXTURES")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("scenarios/fixtures"))
}

fn fixture_path(name: &str) -> std::path::PathBuf {
    fixtures_root().join(format!("{name}.yaml"))
}

fn err(status: StatusCode, msg: impl Into<String>) -> (StatusCode, String) {
    (status, msg.into())
}

/// GET /dev/scene/:name — read the fixture and return scene coordinates.
/// Does not write a save; useful for the UI `?scene=` handler to look up
/// `{playerName, genre, world}` before connecting.
pub async fn get_scene_metadata(
    Path(name): Path<String>,
    State(_state): State<AppState>,
) -> Result<Json<SceneMetadata>, (StatusCode, String)> {
    let fixture = load_fixture(&fixture_path(&name))
        .map_err(|e| err(StatusCode::NOT_FOUND, format!("fixture '{name}': {e}")))?;
    Ok(Json(SceneMetadata {
        fixture_name: fixture.name.clone(),
        player_name: fixture.player_name.clone(),
        genre: fixture.genre.clone(),
        world: fixture.world.clone(),
        has_encounter: true,
    }))
}

/// POST /dev/scene/:name — hydrate fixture + write save file. Subsequent
/// `SESSION_EVENT{connect}` with matching player/genre/world triggers the
/// normal restore path, landing directly on the encounter.
pub async fn load_scene(
    Path(name): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<SceneMetadata>, (StatusCode, String)> {
    let fixture = load_fixture(&fixture_path(&name))
        .map_err(|e| err(StatusCode::NOT_FOUND, format!("fixture '{name}': {e}")))?;

    let code = GenreCode::new(&fixture.genre).map_err(|e| {
        err(
            StatusCode::BAD_REQUEST,
            format!("invalid genre code '{}': {e}", fixture.genre),
        )
    })?;
    let pack = state
        .genre_cache()
        .get_or_load(&code, state.genre_loader())
        .map_err(|e| {
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("genre pack '{}' load failed: {e}", fixture.genre),
            )
        })?;

    let snapshot = hydrate_fixture(&fixture, &pack).map_err(|e| {
        err(
            StatusCode::BAD_REQUEST,
            format!("fixture hydration failed: {e}"),
        )
    })?;

    let save_path = save_path_for(&fixture.genre, &fixture.world, &fixture.player_name);
    if let Some(parent) = save_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("mkdir {}: {e}", parent.display()),
            )
        })?;
    }
    let path_str = save_path.to_str().ok_or_else(|| {
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "save path not UTF-8".to_string(),
        )
    })?;
    let store = SqliteStore::open(path_str).map_err(|e| {
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("open save db: {e}"),
        )
    })?;
    store
        .init_session(&fixture.genre, &fixture.world)
        .map_err(|e| {
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("init session: {e}"),
            )
        })?;
    store.save(&snapshot).map_err(|e| {
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("save snapshot: {e}"),
        )
    })?;

    // OTEL — visible in GM panel's scene harness subsystem view.
    WatcherEventBuilder::new("scene_harness", WatcherEventType::StateTransition)
        .field("event", "scene.loaded")
        .field("fixture_name", name.as_str())
        .field("genre", fixture.genre.as_str())
        .field("world", fixture.world.as_str())
        .field("player_name", fixture.player_name.as_str())
        .field("has_encounter", true)
        .send();

    tracing::info!(
        fixture = %name,
        genre = %fixture.genre,
        world = %fixture.world,
        save_path = %save_path.display(),
        "scene_harness.loaded"
    );

    Ok(Json(SceneMetadata {
        fixture_name: fixture.name.clone(),
        player_name: fixture.player_name.clone(),
        genre: fixture.genre.clone(),
        world: fixture.world.clone(),
        has_encounter: snapshot.encounter.is_some(),
    }))
}

/// GET /dev/scenes — list available fixtures by stem name.
pub async fn list_scenes(State(_state): State<AppState>) -> Json<Vec<String>> {
    let root = fixtures_root();
    let mut names = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("yaml") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    names.push(stem.to_string());
                }
            }
        }
    }
    names.sort();
    Json(names)
}

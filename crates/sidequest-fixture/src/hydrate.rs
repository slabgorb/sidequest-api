//! Fixture → `GameSnapshot` hydration.
//!
//! Hydration is single-character only — `party[1..]` is out of scope.
//! Beats/metrics come from the genre-pack `ConfrontationDef`, never the
//! fixture. Missing worlds or unknown confrontation types are hard errors.

use std::path::{Path, PathBuf};

use sidequest_game::encounter::StructuredEncounter;
use sidequest_game::state::GameSnapshot;
use sidequest_game::Character;
use sidequest_genre::{ConfrontationDef, GenrePack};

use crate::error::FixtureError;
use crate::schema::Fixture;

/// Load a fixture YAML file from disk. No hydration — use `hydrate_fixture`
/// next with a loaded `GenrePack`.
pub fn load_fixture(path: &Path) -> Result<Fixture, FixtureError> {
    if !path.exists() {
        return Err(FixtureError::NotFound(path.to_path_buf()));
    }
    let text = std::fs::read_to_string(path).map_err(|source| FixtureError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    serde_yaml::from_str(&text).map_err(|source| FixtureError::Parse {
        path: path.to_path_buf(),
        source,
    })
}

/// Hydrate a fixture into a `GameSnapshot` using the genre pack for
/// confrontation def resolution and world validation.
pub fn hydrate_fixture(fixture: &Fixture, pack: &GenrePack) -> Result<GameSnapshot, FixtureError> {
    // Validate world slug exists in the pack.
    if !pack.worlds.contains_key(&fixture.world) {
        return Err(FixtureError::UnknownWorld {
            genre: fixture.genre.clone(),
            world: fixture.world.clone(),
        });
    }

    // Deserialize Character via serde_json round-trip. Character's own serde
    // rules (NonBlankString, flattened CreatureCore) fire here.
    let character: Character = serde_json::from_value(fixture.character.clone())?;

    // Resolve ConfrontationDef — the only source of beats/metrics.
    let def = find_confrontation_def(
        &pack.rules.confrontations,
        &fixture.encounter.confrontation_type,
    )
    .ok_or_else(|| FixtureError::UnknownConfrontationType {
        genre: fixture.genre.clone(),
        confrontation_type: fixture.encounter.confrontation_type.clone(),
    })?;

    let mut encounter = StructuredEncounter::from_confrontation_def(def);

    // Populate encounter actors from the fixture's character + NPCs.
    // The player character is "red" (first actor); the first NPC is "blue".
    // This gives the sealed-letter resolution handler named actors to
    // match against committed maneuvers.
    encounter.actors.push(sidequest_game::encounter::EncounterActor {
        name: character.core.name.to_string(),
        role: "red".to_string(),
        per_actor_state: std::collections::HashMap::new(),
    });
    if let Some(npc) = fixture.npcs.first() {
        encounter.actors.push(sidequest_game::encounter::EncounterActor {
            name: npc.name.clone(),
            role: "blue".to_string(),
            per_actor_state: std::collections::HashMap::new(),
        });
    }

    let snapshot = GameSnapshot {
        genre_slug: fixture.genre.clone(),
        world_slug: fixture.world.clone(),
        characters: vec![character],
        location: fixture.location.clone(),
        encounter: Some(encounter),
        ..Default::default()
    };

    Ok(snapshot)
}

fn find_confrontation_def<'a>(
    defs: &'a [ConfrontationDef],
    confrontation_type: &str,
) -> Option<&'a ConfrontationDef> {
    defs.iter()
        .find(|d| d.confrontation_type == confrontation_type)
}

/// Build the SQLite save path for a fixture:
/// `{sidequest_home}/saves/{genre}/{world}/{player_name}/save.db`.
///
/// Matches the normal persistence layout used by `SqliteStore` — the server
/// checks for `{player_name}/save.db`, not `{player_name}.db`.
///
/// `sidequest_home` defaults to `~/.sidequest` and can be overridden with
/// the `SIDEQUEST_HOME` environment variable (used by the integration test).
pub fn save_path_for(genre: &str, world: &str, player_name: &str) -> PathBuf {
    let home = std::env::var("SIDEQUEST_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".sidequest")
        });
    home.join("saves")
        .join(genre)
        .join(world)
        .join(player_name)
        .join("save.db")
}

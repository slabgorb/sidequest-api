//! Debug API endpoints for OTEL dashboard.
//!
//! `GET /api/debug/state` — serializes all active `SharedGameSession`s from
//! Rust memory. Returns the real game state, not the broken partial snapshot
//! from watcher events. The dashboard fetches this on demand.

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::AppState;

/// Serializable view of a `PlayerState` — extracts displayable fields,
/// skips non-serializable handles (session, builder).
#[derive(Serialize)]
pub struct PlayerStateView {
    pub player_name: String,
    pub character_name: Option<String>,
    pub character_class: String,
    pub character_hp: i32,
    pub character_max_hp: i32,
    pub character_level: u32,
    pub character_xp: u32,
    pub region_id: String,
    pub display_location: String,
    pub inventory: sidequest_game::Inventory,
}

/// Serializable view of a trope's runtime state.
#[derive(Serialize)]
pub struct TropeStateView {
    pub trope_definition_id: String,
    pub status: String,
    pub progression: f64,
}

/// Serializable view of `SharedGameSession` — everything the dashboard needs.
#[derive(Serialize)]
pub struct SessionStateView {
    pub session_key: String,
    pub genre_slug: String,
    pub world_slug: String,
    pub current_location: String,
    pub discovered_regions: Vec<String>,
    pub narration_history_len: usize,
    pub turn_mode: String,
    pub npc_registry: Vec<sidequest_game::NpcRegistryEntry>,
    pub trope_states: Vec<TropeStateView>,
    pub players: Vec<PlayerStateView>,
    pub player_count: usize,
    pub has_music_director: bool,
    pub has_audio_mixer: bool,
    pub region_names: Vec<(String, String)>,
}

/// `GET /api/debug/state` — full session state from Rust memory.
pub async fn debug_state(State(state): State<AppState>) -> Json<Vec<SessionStateView>> {
    let sessions = state.inner.sessions.lock().unwrap().clone();
    let mut views = Vec::with_capacity(sessions.len());

    for (key, ss_arc) in &sessions {
        let ss = ss_arc.lock().await;

        let players: Vec<PlayerStateView> = ss
            .players
            .iter()
            .map(|(_pid, ps)| PlayerStateView {
                player_name: ps.player_name.clone(),
                character_name: ps.character_name.clone(),
                character_class: ps.character_class.clone(),
                character_hp: ps.character_hp,
                character_max_hp: ps.character_max_hp,
                character_level: ps.character_level,
                character_xp: ps.character_xp,
                region_id: ps.region_id.clone(),
                display_location: ps.display_location.clone(),
                inventory: ps.inventory.clone(),
            })
            .collect();

        let trope_states: Vec<TropeStateView> = ss
            .trope_states
            .iter()
            .map(|ts| TropeStateView {
                trope_definition_id: ts.trope_definition_id().to_string(),
                status: format!("{:?}", ts.status()),
                progression: ts.progression(),
            })
            .collect();

        views.push(SessionStateView {
            session_key: key.clone(),
            genre_slug: ss.genre_slug.clone(),
            world_slug: ss.world_slug.clone(),
            current_location: ss.current_location.clone(),
            discovered_regions: ss.discovered_regions.clone(),
            narration_history_len: ss.narration_history.len(),
            turn_mode: format!("{:?}", ss.turn_mode),
            npc_registry: ss.npc_registry.clone(),
            trope_states,
            players,
            player_count: ss.player_count(),
            has_music_director: ss.music_director.is_some(),
            has_audio_mixer: ss.audio_mixer.is_some(),
            region_names: ss.region_names.clone(),
        });
    }

    Json(views)
}

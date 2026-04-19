//! Session connect and character creation dispatch.
//!
//! Handles SESSION_EVENT{connect} (new + returning players) and
//! CHARACTER_CREATION messages (chargen scene choices + confirmation).

use std::collections::HashMap;
use std::sync::Arc;

use sidequest_game::builder::CharacterBuilder;
use sidequest_game::character::ProvenancePanelExt;
use sidequest_game::session_restore;
use sidequest_genre::GenreCode;
use sidequest_protocol::{
    AudioCuePayload, ChapterMarkerPayload, CharacterCreationPayload, CharacterState, GameMessage,
    InitialState, MapUpdatePayload, NarrationEndPayload, NarrationPayload, PartyMember,
    PartyStatusPayload, SessionEventPayload,
};

use crate::npc_context;
use crate::session::Session;
use crate::shared_session;
use crate::{error_response, AppState, NpcRegistryEntry, WatcherEventBuilder, WatcherEventType};

/// Per-session mutable state for the connect handshake. Bundles the 29
/// individual mutable references that `dispatch_connect` previously took
/// (story 36-2).
pub(crate) struct ConnectContext<'a> {
    pub session: &'a mut Session,
    pub builder: &'a mut Option<CharacterBuilder>,
    pub player_name_store: &'a mut Option<String>,
    pub character_json_store: &'a mut Option<serde_json::Value>,
    pub character_name_store: &'a mut Option<String>,
    pub character_hp: &'a mut i32,
    pub character_max_hp: &'a mut i32,
    pub current_location: &'a mut String,
    pub discovered_regions: &'a mut Vec<String>,
    pub trope_defs: &'a mut Vec<sidequest_genre::TropeDefinition>,
    pub world_context: &'a mut String,
    pub axes_config: &'a mut Option<sidequest_genre::AxesConfig>,
    pub axis_values: &'a mut Vec<sidequest_game::axis::AxisValue>,
    pub visual_style: &'a mut Option<sidequest_genre::VisualStyle>,
    pub music_director: &'a mut Option<sidequest_game::MusicDirector>,
    pub audio_mixer: &'a std::sync::Arc<tokio::sync::Mutex<Option<sidequest_game::AudioMixer>>>,
    pub prerender_scheduler:
        &'a std::sync::Arc<tokio::sync::Mutex<Option<sidequest_game::PrerenderScheduler>>>,
    pub turn_manager: &'a mut sidequest_game::TurnManager,
    pub npc_registry: &'a mut Vec<NpcRegistryEntry>,
    pub lore_store: &'a std::sync::Arc<tokio::sync::Mutex<sidequest_game::LoreStore>>,
    pub opening_seed: &'a mut Option<String>,
    pub opening_directive: &'a mut Option<String>,
    pub inventory: &'a mut sidequest_game::Inventory,
    pub snapshot: &'a mut sidequest_game::state::GameSnapshot,
}

/// Output slots and shared state for genre pack loading during character
/// creation initialization (story 36-2).
pub(crate) struct ChargenInitContext<'a> {
    pub builder: &'a mut Option<CharacterBuilder>,
    pub trope_defs_out: &'a mut Vec<sidequest_genre::TropeDefinition>,
    pub world_context_out: &'a mut String,
    pub visual_style_out: &'a mut Option<sidequest_genre::VisualStyle>,
    pub axes_config_out: &'a mut Option<sidequest_genre::AxesConfig>,
    pub music_director_out: &'a mut Option<sidequest_game::MusicDirector>,
    pub audio_mixer_lock:
        &'a std::sync::Arc<tokio::sync::Mutex<Option<sidequest_game::AudioMixer>>>,
    pub prerender_lock:
        &'a std::sync::Arc<tokio::sync::Mutex<Option<sidequest_game::PrerenderScheduler>>>,
    pub lore_store: &'a std::sync::Arc<tokio::sync::Mutex<sidequest_game::LoreStore>>,
    pub opening_seed_out: &'a mut Option<String>,
    pub opening_directive_out: &'a mut Option<String>,
}

/// Full mutable state for character creation dispatch — character state,
/// session state, and shared infrastructure (story 36-2).
pub(crate) struct ChargenDispatchContext<'a> {
    pub session: &'a mut Session,
    pub builder: &'a mut Option<CharacterBuilder>,
    pub player_name_store: &'a mut Option<String>,
    pub character_json_store: &'a mut Option<serde_json::Value>,
    pub character_name_store: &'a mut Option<String>,
    pub character_hp: &'a mut i32,
    pub character_max_hp: &'a mut i32,
    pub character_level: &'a mut u32,
    pub character_xp: &'a mut u32,
    pub current_location: &'a mut String,
    pub inventory: &'a mut sidequest_game::Inventory,
    pub trope_states: &'a mut Vec<sidequest_game::trope::TropeState>,
    pub trope_defs: &'a mut Vec<sidequest_genre::TropeDefinition>,
    pub world_context: &'a str,
    pub opening_seed: &'a Option<String>,
    pub opening_directive: &'a mut Option<String>,
    pub axes_config: &'a Option<sidequest_genre::AxesConfig>,
    pub axis_values: &'a mut Vec<sidequest_game::axis::AxisValue>,
    pub visual_style: &'a Option<sidequest_genre::VisualStyle>,
    pub npc_registry: &'a mut Vec<NpcRegistryEntry>,
    pub narration_history: &'a mut Vec<String>,
    pub discovered_regions: &'a mut Vec<String>,
    pub turn_manager: &'a mut sidequest_game::TurnManager,
    pub lore_store: &'a std::sync::Arc<tokio::sync::Mutex<sidequest_game::LoreStore>>,
    pub lore_embed_tx:
        &'a tokio::sync::mpsc::UnboundedSender<super::lore_embed_worker::EmbedRequest>,
    pub shared_session_holder: &'a Arc<
        tokio::sync::Mutex<Option<Arc<tokio::sync::Mutex<shared_session::SharedGameSession>>>>,
    >,
    pub music_director: &'a mut Option<sidequest_game::MusicDirector>,
    pub audio_mixer: &'a std::sync::Arc<tokio::sync::Mutex<Option<sidequest_game::AudioMixer>>>,
    pub prerender_scheduler:
        &'a std::sync::Arc<tokio::sync::Mutex<Option<sidequest_game::PrerenderScheduler>>>,
    pub continuity_corrections: &'a mut String,
    pub quest_log: &'a mut HashMap<String, String>,
    pub genie_wishes: &'a mut Vec<sidequest_game::GenieWish>,
    pub achievement_tracker: &'a mut sidequest_game::achievement::AchievementTracker,
    pub snapshot: &'a mut sidequest_game::state::GameSnapshot,
    pub narrator_verbosity: sidequest_protocol::NarratorVerbosity,
    pub narrator_vocabulary: sidequest_protocol::NarratorVocabulary,
    pub pending_trope_context: &'a mut Option<String>,
    pub tx: &'a tokio::sync::mpsc::Sender<sidequest_protocol::GameMessage>,
}

pub(crate) async fn dispatch_connect(
    payload: &SessionEventPayload,
    ctx: &mut ConnectContext<'_>,
    state: &AppState,
    player_id: &str,
) -> Vec<GameMessage> {
    let session = &mut *ctx.session;
    let builder = &mut *ctx.builder;
    let player_name_store = &mut *ctx.player_name_store;
    let character_json_store = &mut *ctx.character_json_store;
    let character_name_store = &mut *ctx.character_name_store;
    let character_hp = &mut *ctx.character_hp;
    let character_max_hp = &mut *ctx.character_max_hp;
    let current_location = &mut *ctx.current_location;
    let discovered_regions = &mut *ctx.discovered_regions;
    let trope_defs = &mut *ctx.trope_defs;
    let world_context = &mut *ctx.world_context;
    let axes_config = &mut *ctx.axes_config;
    let axis_values = &mut *ctx.axis_values;
    let visual_style = &mut *ctx.visual_style;
    let music_director = &mut *ctx.music_director;
    let audio_mixer = ctx.audio_mixer;
    let prerender_scheduler = ctx.prerender_scheduler;
    let turn_manager = &mut *ctx.turn_manager;
    let npc_registry = &mut *ctx.npc_registry;
    let lore_store = ctx.lore_store;
    let opening_seed = &mut *ctx.opening_seed;
    let opening_directive = &mut *ctx.opening_directive;
    let inventory = &mut *ctx.inventory;
    let snapshot = &mut *ctx.snapshot;
    let genre = payload.genre.as_deref().unwrap_or("");
    let world = payload.world.as_deref().unwrap_or("");
    let pname = payload.player_name.as_deref().unwrap_or("Player");

    // Story 30-2: Reset narrator session on every connect. This forces the next
    // narrator prompt to use Full tier, ensuring genre/world context is always
    // grounded even when switching between games on a running server.
    state.game_service().reset_narrator_session_for_connect();

    // Check for returning player — load from SQLite (now keyed by player name)
    let returning = state.persistence().exists(genre, world, pname).await;

    match session.handle_connect(genre, world, pname) {
        Ok(mut connected_msg) => {
            let mut responses = Vec::new();
            *player_name_store = Some(pname.to_string());

            if returning {
                // Returning player — load snapshot from SQLite (keyed by player name)
                match state.persistence().load(genre, world, pname).await {
                    Ok(Some(saved)) => {
                        // Permadeath: if the player died, wipe the save and restart at chargen
                        if saved.snapshot.player_dead {
                            tracing::info!(
                                genre = %genre, world = %world, player = %pname,
                                "permadeath.save_wiped — player_dead on reconnect, restarting chargen"
                            );
                            if let Err(e) = state.persistence().delete(genre, world, pname).await {
                                tracing::warn!(error = %e, "Failed to delete dead player save");
                            }
                            responses.push(connected_msg); // has_character stays None → chargen
                            return responses;
                        }

                        if let GameMessage::SessionEvent {
                            ref mut payload, ..
                        } = connected_msg
                        {
                            payload.has_character = Some(true);
                        }
                        responses.push(connected_msg);

                        // Extract complete character state from saved snapshot (story 18-9, story 26-3).
                        // session_restore ensures ALL character state is restored, not just base attributes.
                        match session_restore::extract_character_state(&saved.snapshot) {
                            Some(restored) => {
                                // Capture values for telemetry before moving them
                                let char_name = restored.character_name.clone();
                                let level = restored.level;
                                let edge = restored.edge;
                                let max_edge = restored.max_edge;
                                let inv_count = restored.inventory.items.len();
                                let facts_count = restored.known_facts.len();

                                // Move values into mutable references
                                *character_json_store = Some(restored.character_json);
                                *character_name_store =
                                    Some(restored.character_name.as_str().to_string());
                                *character_hp = edge;
                                *character_max_hp = max_edge;
                                *inventory = restored.inventory;

                                // Emit OTEL span for session restore (story 26-3)
                                WatcherEventBuilder::new(
                                    "session_restore",
                                    WatcherEventType::StateTransition,
                                )
                                .field("event", "character_restored")
                                .field("character_name", char_name.as_str())
                                .field("level", level)
                                .field("edge", edge)
                                .field("max_edge", max_edge)
                                .field("inventory_count", inv_count)
                                .field("facts_count", facts_count)
                                .field("player", pname)
                                .field("genre", genre)
                                .field("world", world)
                                .send();

                                tracing::info!(
                                    character = %char_name,
                                    level = level,
                                    edge = edge,
                                    inventory_count = inv_count,
                                    facts_count = facts_count,
                                    "session_restore.character_restored"
                                );
                            }
                            None => {
                                tracing::error!(
                                    player = %pname,
                                    genre = %genre,
                                    world = %world,
                                    "session_restore.character_missing — no characters in saved snapshot"
                                );
                                return vec![error_response(
                                    player_id,
                                    "Saved game corrupted: no character data found",
                                )];
                            }
                        }
                        // Restore location, regions, turn state, and NPC registry from snapshot
                        *current_location = saved.snapshot.location.clone();
                        *discovered_regions = saved.snapshot.discovered_regions.clone();
                        *turn_manager = saved.snapshot.turn_manager.clone();
                        *npc_registry = saved.snapshot.npc_registry.clone();
                        *axis_values = saved.snapshot.axis_values.clone();
                        // Restore canonical snapshot for dispatch pipeline (story 15-8)
                        *snapshot = saved.snapshot.clone();

                        // Backfill discovered_rooms for stale saves that predate room_graph wiring.
                        // If navigation_mode is RoomGraph but discovered_rooms is empty, seed
                        // the entrance room so the Automapper has something to render.
                        if snapshot.discovered_rooms.is_empty() {
                            let rooms_for_backfill: Vec<sidequest_genre::RoomDef> =
                                GenreCode::new(genre)
                                    .ok()
                                    .and_then(|gc| {
                                        state
                                            .genre_cache()
                                            .get_or_load(&gc, state.genre_loader())
                                            .ok()
                                    })
                                    .and_then(|pack| pack.worlds.get(world).cloned())
                                    .filter(|w| {
                                        w.cartography.navigation_mode
                                            == sidequest_genre::NavigationMode::RoomGraph
                                    })
                                    .and_then(|w| w.cartography.rooms.clone())
                                    .unwrap_or_default();
                            if !rooms_for_backfill.is_empty() {
                                sidequest_game::room_movement::init_room_graph_location(
                                    snapshot,
                                    &rooms_for_backfill,
                                );
                                *current_location = snapshot.location.clone();
                                tracing::info!(
                                    location = %snapshot.location,
                                    discovered_rooms = snapshot.discovered_rooms.len(),
                                    "room_graph.backfill — seeded entrance for stale save"
                                );
                                WatcherEventBuilder::new(
                                    "room_graph",
                                    WatcherEventType::StateTransition,
                                )
                                .field("event", "room_graph.backfill")
                                .field("location", snapshot.location.as_str())
                                .field("source", "stale_save")
                                .send();
                            }
                        }

                        // Story 26-8: emit location restore event for GM panel visibility
                        WatcherEventBuilder::new("location", WatcherEventType::StateTransition)
                            .field("event", "location.restored")
                            .field("location", saved.snapshot.location.as_str())
                            .field(
                                "discovered_regions",
                                saved.snapshot.discovered_regions.len(),
                            )
                            .field("source", "save_file")
                            .send();

                        // Transition session to Playing
                        if let Err(e) = session.complete_character_creation() {
                            tracing::error!(error = %e, state = %session.state_name(), "Failed to transition session to Playing on reconnect");
                            return vec![error_response(
                                player_id,
                                &format!("Session transition failed: {e}"),
                            )];
                        }

                        let ready = GameMessage::SessionEvent {
                            payload: SessionEventPayload {
                                event: "ready".to_string(),
                                player_name: None,
                                genre: None,
                                world: None,
                                has_character: None,
                                initial_state: Some(InitialState {
                                    characters: saved
                                        .snapshot
                                        .characters
                                        .iter()
                                        .map(|c| CharacterState {
                                            name: c.core.name.clone(),
                                            hp: c.core.edge.current,
                                            max_hp: c.core.edge.max,
                                            level: c.core.level,
                                            class: c.char_class.as_str().to_string(),
                                            statuses: c.core.statuses.clone(),
                                            inventory: c
                                                .core
                                                .inventory
                                                .items
                                                .iter()
                                                .map(|i| i.name.as_str().to_string())
                                                .collect(),
                                            archetype_provenance: c.archetype_provenance.clone(),
                                        })
                                        .collect(),
                                    location: sidequest_protocol::NonBlankString::new(
                                        &saved.snapshot.location,
                                    )
                                    .expect(
                                        "saved snapshot location is non-empty by load invariant",
                                    ),
                                    quests: saved.snapshot.quest_log.clone(),
                                    turn_count: saved.snapshot.turn_manager.round().into(),
                                }),
                                css: None,
                                image_cooldown_seconds: None,
                                narrator_verbosity: None,
                                narrator_vocabulary: None,
                            },
                            player_id: player_id.to_string(),
                        };
                        responses.push(ready);

                        // PARTY_STATUS — single-player reconnect. PARTY_STATUS
                        // is now the single source of truth for the character
                        // sheet and inventory (CHARACTER_SHEET / INVENTORY
                        // messages were deleted), so the reconnecting client
                        // receives all per-member state in this one message.
                        {
                            let member = saved.snapshot.characters.first().map(|c| {
                                // Build the sheet facet from the saved character.
                                let sheet = sidequest_protocol::CharacterSheetDetails {
                                    race: c.race.clone(),
                                    stats: c.stats.iter().map(|(k, v)| (k.clone(), *v)).collect(),
                                    abilities: c.abilities.iter().map(|a| a.name.clone()).collect(),
                                    backstory: c.backstory.clone(),
                                    personality: c.core.personality.clone(),
                                    pronouns: sidequest_protocol::NonBlankString::new(&c.pronouns)
                                        .ok(),
                                    equipment: c
                                        .core
                                        .inventory
                                        .items
                                        .iter()
                                        .map(|i| {
                                            if i.equipped {
                                                format!("{} [equipped]", i.name)
                                            } else {
                                                i.name.as_str().to_string()
                                            }
                                        })
                                        .collect(),
                                };
                                // Inventory facet from the live dispatch-scope `inventory`.
                                let inventory_payload =
                                    crate::shared_session::inventory_payload_from(inventory);
                                PartyMember {
                                    player_id: sidequest_protocol::NonBlankString::new(player_id)
                                        .expect("player_id is non-empty at connect time"),
                                    name: sidequest_protocol::NonBlankString::new(
                                        player_name_store.as_deref().unwrap_or("Player"),
                                    )
                                    .expect("player name falls back to literal \"Player\""),
                                    character_name: Some(c.core.name.clone()),
                                    current_hp: c.core.edge.current,
                                    max_hp: c.core.edge.max,
                                    statuses: c.core.statuses.clone(),
                                    class: c.char_class.clone(),
                                    level: c.core.level,
                                    portrait_url: None,
                                    current_location: sidequest_protocol::NonBlankString::new(
                                        current_location,
                                    )
                                    .ok(),
                                    sheet: Some(sheet),
                                    inventory: Some(inventory_payload),
                                }
                            });
                            let members = member.into_iter().collect();
                            responses.push(GameMessage::PartyStatus {
                                payload: PartyStatusPayload { members },
                                player_id: player_id.to_string(),
                            });
                        }

                        // CHAPTER_MARKER for current location
                        if !saved.snapshot.location.is_empty() {
                            responses.push(GameMessage::ChapterMarker {
                                payload: ChapterMarkerPayload {
                                    title: Some(saved.snapshot.location.clone()),
                                    location: Some(saved.snapshot.location.clone()),
                                },
                                player_id: player_id.to_string(),
                            });
                        }

                        // JOURNAL_RESPONSE — backfill knowledge entries on
                        // reconnect. known_facts are persisted in the snapshot
                        // but were never re-sent to the UI, causing the
                        // Knowledge tab to show "empty" after session re-entry.
                        if let Some(c) = saved.snapshot.characters.first() {
                            if !c.known_facts.is_empty() {
                                let filter = sidequest_game::journal::JournalFilter {
                                    category: None,
                                    sort_by: sidequest_protocol::JournalSortOrder::Time,
                                };
                                let entries = sidequest_game::journal::build_journal_entries(
                                    &c.known_facts,
                                    &filter,
                                );
                                tracing::info!(
                                    entry_count = entries.len(),
                                    "session_restore.journal_backfill"
                                );
                                responses.push(GameMessage::JournalResponse {
                                    payload: sidequest_protocol::JournalResponsePayload { entries },
                                    player_id: player_id.to_string(),
                                });
                            }
                        }

                        // SCRAPBOOK replay — load persisted entries so the
                        // gallery re-populates on session resume.
                        //
                        // Story 37-28: verify that each entry's image_url
                        // still resolves to an existing file on disk. URLs
                        // in the /api/scrapbook/ form live under the save
                        // directory (durable); legacy /api/renders/ URLs
                        // live in the global renders pool (volatile). On a
                        // missing file we emit a loud ValidationWarning
                        // (event = "scrapbook.image_missing") so the GM
                        // panel can see orphaned entries — a tracing::warn
                        // is not visible there. No silent fallback.
                        match state
                            .persistence()
                            .load_scrapbook_entries(genre, world, pname)
                            .await
                        {
                            Ok(entries) if !entries.is_empty() => {
                                tracing::info!(
                                    entry_count = entries.len(),
                                    "session_restore.scrapbook_replay"
                                );
                                for entry in entries {
                                    if let Some(url) = entry.image_url.as_ref() {
                                        if let Some(disk_path) = resolve_scrapbook_image_path(
                                            url.as_str(),
                                            state.save_dir(),
                                        ) {
                                            if !disk_path.exists() {
                                                WatcherEventBuilder::new(
                                                    "scrapbook",
                                                    WatcherEventType::ValidationWarning,
                                                )
                                                .field("event", "scrapbook.image_missing")
                                                .field("turn_id", entry.turn_id)
                                                .field("image_url", url.as_str())
                                                .field("disk_path", disk_path.display().to_string())
                                                .field("genre", genre)
                                                .field("world", world)
                                                .field("player", pname)
                                                .send();
                                                tracing::error!(
                                                    turn_id = entry.turn_id,
                                                    url = %url.as_str(),
                                                    disk_path = %disk_path.display(),
                                                    "scrapbook.image_missing — persisted manifest row points to a file that no longer exists on disk"
                                                );
                                            }
                                        } else {
                                            // Unrecognized URL shape — also loud.
                                            WatcherEventBuilder::new(
                                                "scrapbook",
                                                WatcherEventType::ValidationWarning,
                                            )
                                            .field("event", "scrapbook.image_url_unresolvable")
                                            .field("turn_id", entry.turn_id)
                                            .field("image_url", url.as_str())
                                            .send();
                                            tracing::error!(
                                                turn_id = entry.turn_id,
                                                url = %url.as_str(),
                                                "scrapbook.image_url_unresolvable — URL does not match /api/scrapbook/ or /api/renders/ shape"
                                            );
                                        }
                                    }
                                    responses.push(GameMessage::ScrapbookEntry {
                                        payload: entry,
                                        player_id: player_id.to_string(),
                                    });
                                }
                            }
                            Ok(_) => {}
                            Err(e) => {
                                tracing::warn!(error = %e, "session_restore.scrapbook_load_failed");
                            }
                        }

                        // Last NARRATION — recap, last narrative log entry, or
                        // a location-based fallback built from the current
                        // RoomDef description. The fallback case was added
                        // after sq-playtest 2026-04-09 found that a player
                        // could reconnect into a valid session (correct
                        // stats, correct location) with a BLANK narrative
                        // panel whenever `narrative_log` had no rows — e.g.
                        // if the player closed the tab right after chargen
                        // without completing a turn. A silent blank panel
                        // is the worst possible state: the player has no
                        // "what was I doing?" context and nothing to react
                        // to.
                        //
                        // Source priority:
                        //   1. `saved.recap` — rich "Previously On..." format
                        //      generated from the last 3 narrative_log rows
                        //      (built by PersistenceWorker::load).
                        //   2. `saved.snapshot.narrative_log.last()` — raw
                        //      last entry if recap was skipped for any reason.
                        //   3. Location fallback: look up the current room in
                        //      the genre pack's cartography.rooms list and
                        //      synthesize a "You find yourself at NAME. DESC"
                        //      string. Guaranteed to produce narration as long
                        //      as the room is in the genre pack.
                        //   4. Terminal fallback: "You find yourself at
                        //      {location}." — plain text from snapshot.location.
                        let (recap_text, recap_source) = {
                            if let Some(text) = saved.recap.clone() {
                                (Some(text), "recap")
                            } else if let Some(entry) = saved.snapshot.narrative_log.last() {
                                (Some(entry.content.clone()), "narrative_log_last")
                            } else {
                                // Location fallback: pull the current RoomDef
                                // from the genre pack. Same loader pattern as
                                // the room_graph backfill above — keep it
                                // inline rather than factored so future edits
                                // can see both branches together.
                                let current_room_desc: Option<String> = GenreCode::new(genre)
                                    .ok()
                                    .and_then(|gc| {
                                        state
                                            .genre_cache()
                                            .get_or_load(&gc, state.genre_loader())
                                            .ok()
                                    })
                                    .and_then(|pack| pack.worlds.get(world).cloned())
                                    .and_then(|w| w.cartography.rooms.clone())
                                    .and_then(|rooms| {
                                        rooms
                                            .into_iter()
                                            .find(|r| r.id == saved.snapshot.location)
                                            .map(|r| match r.description.as_deref() {
                                                Some(desc) if !desc.is_empty() => format!(
                                                    "You find yourself at {}.\n\n{}",
                                                    r.name, desc
                                                ),
                                                _ => format!("You find yourself at {}.", r.name),
                                            })
                                    });
                                if let Some(text) = current_room_desc {
                                    (Some(text), "room_description_fallback")
                                } else if !saved.snapshot.location.is_empty() {
                                    (
                                        Some(format!(
                                            "You find yourself at {}.",
                                            saved.snapshot.location
                                        )),
                                        "location_only_fallback",
                                    )
                                } else {
                                    (None, "none")
                                }
                            }
                        };
                        WatcherEventBuilder::new("narration", WatcherEventType::StateTransition)
                            .field("event", "reconnect.narration_source")
                            .field("source", recap_source)
                            .field("has_text", recap_text.is_some())
                            .field("location", saved.snapshot.location.as_str())
                            .field("narrative_log_rows", saved.snapshot.narrative_log.len())
                            .field("player_id", player_id)
                            .send();
                        if let Some(text) = recap_text {
                            let text_nbs = sidequest_protocol::NonBlankString::new(&text)
                                .expect("reconnect recap text is non-empty when Some(...)");
                            responses.push(GameMessage::Narration {
                                payload: NarrationPayload {
                                    text: text_nbs,
                                    state_delta: None,
                                    footnotes: vec![],
                                },
                                player_id: player_id.to_string(),
                            });
                            responses.push(GameMessage::NarrationEnd {
                                payload: NarrationEndPayload { state_delta: None },
                                player_id: player_id.to_string(),
                            });
                        }

                        // PARTY_STATUS — single-player reconnect. PARTY_STATUS
                        // is now the single source of truth for the character
                        // sheet and inventory (CHARACTER_SHEET / INVENTORY
                        // messages were deleted), so the reconnecting client
                        // receives all per-member state in this one message.
                        {
                            let member = saved.snapshot.characters.first().map(|c| {
                                // Build the sheet facet from the saved character.
                                let sheet = sidequest_protocol::CharacterSheetDetails {
                                    race: c.race.clone(),
                                    stats: c.stats.iter().map(|(k, v)| (k.clone(), *v)).collect(),
                                    abilities: c.abilities.iter().map(|a| a.name.clone()).collect(),
                                    backstory: c.backstory.clone(),
                                    personality: c.core.personality.clone(),
                                    pronouns: sidequest_protocol::NonBlankString::new(&c.pronouns)
                                        .ok(),
                                    equipment: c
                                        .core
                                        .inventory
                                        .items
                                        .iter()
                                        .map(|i| {
                                            if i.equipped {
                                                format!("{} [equipped]", i.name)
                                            } else {
                                                i.name.as_str().to_string()
                                            }
                                        })
                                        .collect(),
                                };
                                // Inventory facet from the live dispatch-scope `inventory`.
                                let inventory_payload =
                                    crate::shared_session::inventory_payload_from(inventory);
                                PartyMember {
                                    player_id: sidequest_protocol::NonBlankString::new(player_id)
                                        .expect("player_id is non-empty at connect time"),
                                    name: sidequest_protocol::NonBlankString::new(
                                        player_name_store.as_deref().unwrap_or("Player"),
                                    )
                                    .expect("player name falls back to literal \"Player\""),
                                    character_name: Some(c.core.name.clone()),
                                    current_hp: c.core.edge.current,
                                    max_hp: c.core.edge.max,
                                    statuses: c.core.statuses.clone(),
                                    class: c.char_class.clone(),
                                    level: c.core.level,
                                    portrait_url: None,
                                    current_location: sidequest_protocol::NonBlankString::new(
                                        current_location,
                                    )
                                    .ok(),
                                    sheet: Some(sheet),
                                    inventory: Some(inventory_payload),
                                }
                            });
                            let members = member.into_iter().collect();
                            responses.push(GameMessage::PartyStatus {
                                payload: PartyStatusPayload { members },
                                player_id: player_id.to_string(),
                            });
                        }

                        // JOURNAL_RESPONSE — backfill knowledge entries (second
                        // reconnect path). Same logic as the first reconnect path.
                        if let Some(c) = saved.snapshot.characters.first() {
                            if !c.known_facts.is_empty() {
                                let filter = sidequest_game::journal::JournalFilter {
                                    category: None,
                                    sort_by: sidequest_protocol::JournalSortOrder::Time,
                                };
                                let entries = sidequest_game::journal::build_journal_entries(
                                    &c.known_facts,
                                    &filter,
                                );
                                tracing::info!(
                                    entry_count = entries.len(),
                                    "session_restore.journal_backfill"
                                );
                                responses.push(GameMessage::JournalResponse {
                                    payload: sidequest_protocol::JournalResponsePayload { entries },
                                    player_id: player_id.to_string(),
                                });
                            }
                        }

                        // MAP_UPDATE — replay explored map state so the client
                        // can show the Automapper overlay immediately on reconnect.
                        // Without this the M hotkey stays gated on null mapData.
                        {
                            let explored_locs: Vec<sidequest_protocol::ExploredLocation> = {
                                // Try room graph mode first
                                let rooms: Vec<sidequest_genre::RoomDef> = GenreCode::new(genre)
                                    .ok()
                                    .and_then(|gc| {
                                        state
                                            .genre_cache()
                                            .get_or_load(&gc, state.genre_loader())
                                            .ok()
                                    })
                                    .and_then(|pack| pack.worlds.get(world).cloned())
                                    .filter(|w| {
                                        w.cartography.navigation_mode
                                            == sidequest_genre::NavigationMode::RoomGraph
                                    })
                                    .and_then(|w| w.cartography.rooms.clone())
                                    .unwrap_or_default();
                                if !rooms.is_empty() {
                                    sidequest_game::build_room_graph_explored(
                                        &rooms,
                                        &saved.snapshot.discovered_rooms,
                                        &saved.snapshot.location,
                                    )
                                } else {
                                    saved
                                        .snapshot
                                        .discovered_regions
                                        .iter()
                                        .filter_map(|name| {
                                            sidequest_protocol::NonBlankString::new(name).ok().map(
                                                |nbs_name| sidequest_protocol::ExploredLocation {
                                                    // Region mode has no separate slug — id mirrors name.
                                                    id: name.clone(),
                                                    name: nbs_name,
                                                    x: 0,
                                                    y: 0,
                                                    location_type: String::new(),
                                                    connections: vec![],
                                                    room_exits: vec![],
                                                    room_type: String::new(),
                                                    size: None,
                                                    is_current_room: name
                                                        == &saved.snapshot.location,
                                                    tactical_grid: None,
                                                },
                                            )
                                        })
                                        .collect()
                                }
                            };
                            let explored_count = explored_locs.len();
                            let cartography_meta: Option<sidequest_protocol::CartographyMetadata> =
                                GenreCode::new(genre)
                                    .ok()
                                    .and_then(|gc| {
                                        state
                                            .genre_cache()
                                            .get_or_load(&gc, state.genre_loader())
                                            .ok()
                                    })
                                    .and_then(|pack| pack.worlds.get(world).cloned())
                                    .map(|w| {
                                        let nav_mode = match w.cartography.navigation_mode {
                                            sidequest_genre::NavigationMode::Region => "region",
                                            sidequest_genre::NavigationMode::RoomGraph => {
                                                "room_graph"
                                            }
                                            sidequest_genre::NavigationMode::Hierarchical => {
                                                "hierarchical"
                                            }
                                        };
                                        sidequest_protocol::CartographyMetadata {
                                            navigation_mode: nav_mode.to_string(),
                                            starting_region: w.cartography.starting_region.clone(),
                                            regions: w
                                                .cartography
                                                .regions
                                                .iter()
                                                .filter_map(|(slug, r)| {
                                                    let name =
                                                        sidequest_protocol::NonBlankString::new(
                                                            &r.name,
                                                        )
                                                        .ok()?;
                                                    Some((
                                                        slug.clone(),
                                                        sidequest_protocol::CartographyRegion {
                                                            name,
                                                            description: r.description.clone(),
                                                            adjacent: r.adjacent.clone(),
                                                        },
                                                    ))
                                                })
                                                .collect(),
                                            routes: w
                                                .cartography
                                                .routes
                                                .iter()
                                                .filter_map(|r| {
                                                    let name =
                                                        sidequest_protocol::NonBlankString::new(
                                                            &r.name,
                                                        )
                                                        .ok()?;
                                                    Some(sidequest_protocol::CartographyRoute {
                                                        name,
                                                        description: r.description.clone(),
                                                        from_id: r.from_id.clone(),
                                                        to_id: r.to_id.clone(),
                                                    })
                                                })
                                                .collect(),
                                        }
                                    });
                            super::emit_map_update_telemetry(
                                "reconnect",
                                player_id,
                                &saved.snapshot.location,
                                &explored_locs,
                                cartography_meta.as_ref(),
                            );
                            let map_location =
                                sidequest_protocol::NonBlankString::new(&saved.snapshot.location)
                                    .expect(
                                        "saved snapshot location is non-empty by load invariant",
                                    );
                            let map_region = sidequest_protocol::NonBlankString::new(
                                &saved.snapshot.current_region,
                            )
                            .unwrap_or_else(|_| map_location.clone());
                            responses.push(GameMessage::MapUpdate {
                                payload: MapUpdatePayload {
                                    current_location: map_location,
                                    region: map_region,
                                    explored: explored_locs,
                                    fog_bounds: None,
                                    cartography: cartography_meta,
                                },
                                player_id: player_id.to_string(),
                            });
                            tracing::info!(
                                explored_count,
                                location = %saved.snapshot.location,
                                "map_update.reconnect — replayed explored state for automapper"
                            );
                        }

                        // INVENTORY reconnect-replay was deleted: inventory is
                        // now nested inside the PARTY_STATUS above. OTEL still
                        // fires so the GM panel keeps its reconnect marker.
                        WatcherEventBuilder::new("inventory", WatcherEventType::StateTransition)
                            .field("event", "inventory.reconnect")
                            .field("item_count", inventory.carried().count())
                            .field("gold", inventory.gold as usize)
                            .send();

                        // Initialize audio subsystems for returning player
                        if let Ok(genre_code) = GenreCode::new(genre) {
                            if let Ok(pack) = state
                                .genre_cache()
                                .get_or_load(&genre_code, state.genre_loader())
                            {
                                *visual_style = Some(pack.visual_style.clone());
                                *axes_config = Some(pack.axes.clone());
                                *music_director =
                                    Some(sidequest_game::MusicDirector::new(&pack.audio));
                                *audio_mixer.lock().await = Some(sidequest_game::AudioMixer::new());
                                *prerender_scheduler.lock().await =
                                    Some(sidequest_game::PrerenderScheduler::new(
                                        sidequest_game::PrerenderConfig::default(),
                                    ));

                                // Emit CONFRONTATION on session restore if the
                                // saved snapshot has an active encounter.
                                // Without this, a reload lands the player in an
                                // encounter that the UI doesn't know about until
                                // the next turn completes — every save with an
                                // active encounter is effectively broken on
                                // reload. See docs/plans/scene-harness.md
                                // "RED FIX #1".
                                if let Some(ref enc) = snapshot.encounter {
                                    let msg = super::response::build_confrontation_message(
                                        enc,
                                        npc_registry,
                                        &pack.rules.confrontations,
                                        genre,
                                        player_id,
                                    );
                                    responses.push(msg);
                                    WatcherEventBuilder::new(
                                        "encounter",
                                        WatcherEventType::StateTransition,
                                    )
                                    .field("event", "confrontation.restored")
                                    .field("encounter_type", enc.encounter_type.as_str())
                                    .field("beat", enc.beat as i64)
                                    .field("actor_count", enc.actors.len())
                                    .send();
                                    tracing::info!(
                                        encounter_type = %enc.encounter_type,
                                        actors = enc.actors.len(),
                                        "confrontation.restored — emitted CONFRONTATION on session restore"
                                    );
                                }

                                // Load trope definitions for returning player (same logic as start_character_creation)
                                let mut all_tropes = pack.tropes.clone();
                                if let Some(w) = pack.worlds.get(world) {
                                    all_tropes.extend(w.tropes.clone());
                                }
                                for trope in &mut all_tropes {
                                    if trope.id.is_none() {
                                        let slug = trope
                                            .name
                                            .as_str()
                                            .to_lowercase()
                                            .replace(' ', "-")
                                            .replace(
                                                |c: char| !c.is_alphanumeric() && c != '-',
                                                "",
                                            );
                                        trope.id = Some(slug);
                                    }
                                }
                                all_tropes.retain(|t| !t.is_abstract);
                                *trope_defs = all_tropes;
                                tracing::info!(count = trope_defs.len(), genre = %genre, "Loaded trope definitions for returning player");

                                tracing::info!(genre = %genre, "Audio subsystems initialized for returning player");

                                // Send genre-pack mixer config so the frontend
                                // initializes channel volumes per-genre.
                                responses.push(mixer_config_cue(&pack.audio.mixer, player_id));

                                // Seed lore store from genre pack (story 11-4)
                                let lore_count = {
                                    let mut store = lore_store.lock().await;
                                    sidequest_game::seed_lore_from_genre_pack(&mut store, &pack)
                                };
                                tracing::info!(
                                    count = lore_count,
                                    genre = %genre,
                                    "rag.lore_store_seeded"
                                );

                                // Story 15-24: Restore persisted lore fragments from SQLite.
                                match state
                                    .persistence()
                                    .load_lore_fragments(genre, world, pname)
                                    .await
                                {
                                    Ok(fragments) => {
                                        let restored_count = fragments.len();
                                        let mut store = lore_store.lock().await;
                                        for fragment in fragments {
                                            let _ = store.add(fragment);
                                        }
                                        drop(store);
                                        if restored_count > 0 {
                                            tracing::info!(
                                                count = restored_count,
                                                genre = %genre,
                                                "lore.fragments_restored"
                                            );
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!(error = %e, "lore.fragments_restore_failed");
                                    }
                                }

                                // Materialize world from genre pack history for returning player (Story 15-18).
                                // The saved snapshot may have advanced in maturity since last save;
                                // re-materializing ensures world_history reflects current campaign maturity.
                                {
                                    let history_value = pack
                                        .worlds
                                        .get(world)
                                        .and_then(|w| w.history.as_ref())
                                        .cloned()
                                        .unwrap_or(serde_json::Value::Null);
                                    match sidequest_game::parse_history_chapters(&history_value) {
                                        Ok(chapters) => {
                                            let prev_maturity = snapshot.campaign_maturity.clone();
                                            sidequest_game::materialize_world(snapshot, &chapters);
                                            WatcherEventBuilder::new(
                                                "world_materialization",
                                                WatcherEventType::StateTransition,
                                            )
                                            .field("event", "world_materialized")
                                            .field("genre", genre)
                                            .field("world", world)
                                            .field("chapters_available", chapters.len())
                                            .field("chapters_applied", snapshot.world_history.len())
                                            .field("prev_maturity", format!("{:?}", prev_maturity))
                                            .field(
                                                "new_maturity",
                                                format!("{:?}", snapshot.campaign_maturity),
                                            )
                                            .field("trigger", "returning_player_reconnect")
                                            .send();
                                            tracing::info!(
                                                genre = %genre,
                                                world = %world,
                                                chapters_available = chapters.len(),
                                                chapters_applied = snapshot.world_history.len(),
                                                maturity = ?snapshot.campaign_maturity,
                                                "world_materialization.applied — returning player"
                                            );
                                        }
                                        Err(e) => {
                                            tracing::warn!(
                                                error = %e,
                                                genre = %genre,
                                                world = %world,
                                                "world_materialization.parse_failed — returning player history chapters"
                                            );
                                        }
                                    }
                                }

                                // Inject culture reference for returning player
                                let cultures = pack
                                    .worlds
                                    .get(world)
                                    .filter(|w| !w.cultures.is_empty())
                                    .map(|w| w.cultures.as_slice())
                                    .unwrap_or(&pack.cultures);
                                let culture_ref = npc_context::build_culture_reference(cultures);
                                if !culture_ref.is_empty() {
                                    world_context.push_str(&culture_ref);
                                }
                            }
                        }

                        tracing::info!(
                            player = %pname,
                            genre = %genre,
                            world = %world,
                            "Player reconnected from saved session"
                        );
                    }
                    Ok(None) => {
                        // Save file exists but no game state — treat as new player
                        tracing::warn!(genre = %genre, world = %world, "Save file exists but empty");
                        responses.push(connected_msg);
                        responses.extend(
                            start_character_creation(
                                &mut ChargenInitContext {
                                    builder,
                                    trope_defs_out: trope_defs,
                                    world_context_out: world_context,
                                    visual_style_out: visual_style,
                                    axes_config_out: axes_config,
                                    music_director_out: music_director,
                                    audio_mixer_lock: audio_mixer,
                                    prerender_lock: prerender_scheduler,
                                    lore_store,
                                    opening_seed_out: opening_seed,
                                    opening_directive_out: opening_directive,
                                },
                                genre,
                                world,
                                state,
                                player_id,
                                pname,
                            )
                            .await,
                        );
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to load saved session, starting fresh");
                        responses.push(connected_msg);
                        responses.extend(
                            start_character_creation(
                                &mut ChargenInitContext {
                                    builder,
                                    trope_defs_out: trope_defs,
                                    world_context_out: world_context,
                                    visual_style_out: visual_style,
                                    axes_config_out: axes_config,
                                    music_director_out: music_director,
                                    audio_mixer_lock: audio_mixer,
                                    prerender_lock: prerender_scheduler,
                                    lore_store,
                                    opening_seed_out: opening_seed,
                                    opening_directive_out: opening_directive,
                                },
                                genre,
                                world,
                                state,
                                player_id,
                                pname,
                            )
                            .await,
                        );
                    }
                }
            } else {
                // New player — send connected, then start character creation
                responses.push(connected_msg);
                responses.extend(
                    start_character_creation(
                        &mut ChargenInitContext {
                            builder,
                            trope_defs_out: trope_defs,
                            world_context_out: world_context,
                            visual_style_out: visual_style,
                            axes_config_out: axes_config,
                            music_director_out: music_director,
                            audio_mixer_lock: audio_mixer,
                            prerender_lock: prerender_scheduler,
                            lore_store,
                            opening_seed_out: opening_seed,
                            opening_directive_out: opening_directive,
                        },
                        genre,
                        world,
                        state,
                        player_id,
                        pname,
                    )
                    .await,
                );
            }

            // Send theme_css SESSION_EVENT if the genre pack has a client_theme.css
            let css_path = state
                .genre_packs_path()
                .join(genre)
                .join("client_theme.css");
            if let Ok(css) = tokio::fs::read_to_string(&css_path).await {
                responses.push(GameMessage::SessionEvent {
                    payload: SessionEventPayload {
                        event: "theme_css".to_string(),
                        player_name: None,
                        genre: None,
                        world: None,
                        has_character: None,
                        initial_state: None,
                        css: Some(css),
                        image_cooldown_seconds: None,
                        narrator_verbosity: None,
                        narrator_vocabulary: None,
                    },
                    player_id: player_id.to_string(),
                });
            }

            responses
        }
        Err(e) => {
            vec![error_response(player_id, &e.to_string())]
        }
    }
}

/// Load genre pack, create CharacterBuilder, return first scene message + trope defs + world context.
pub(crate) async fn start_character_creation(
    ictx: &mut ChargenInitContext<'_>,
    genre: &str,
    world_slug: &str,
    state: &AppState,
    player_id: &str,
    lobby_name: &str,
) -> Vec<GameMessage> {
    let builder = &mut *ictx.builder;
    let trope_defs_out = &mut *ictx.trope_defs_out;
    let world_context_out = &mut *ictx.world_context_out;
    let visual_style_out = &mut *ictx.visual_style_out;
    let axes_config_out = &mut *ictx.axes_config_out;
    let music_director_out = &mut *ictx.music_director_out;
    let audio_mixer_lock = ictx.audio_mixer_lock;
    let prerender_lock = ictx.prerender_lock;
    let lore_store = ictx.lore_store;
    let opening_seed_out = &mut *ictx.opening_seed_out;
    let opening_directive_out = &mut *ictx.opening_directive_out;
    let genre_code = match GenreCode::new(genre) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(genre = %genre, error = %e, "Invalid genre code");
            return vec![];
        }
    };

    let pack = match state
        .genre_cache()
        .get_or_load(&genre_code, state.genre_loader())
    {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(genre = %genre, error = %e, "Failed to load genre pack");
            return vec![];
        }
    };

    *visual_style_out = Some(pack.visual_style.clone());
    *axes_config_out = Some(pack.axes.clone());

    // Initialize audio subsystems from genre pack
    *music_director_out = Some(sidequest_game::MusicDirector::new(&pack.audio));
    *audio_mixer_lock.lock().await = Some(sidequest_game::AudioMixer::new());
    *prerender_lock.lock().await = Some(sidequest_game::PrerenderScheduler::new(
        sidequest_game::PrerenderConfig::default(),
    ));
    tracing::info!(genre = %genre, "Audio subsystems initialized from genre pack");

    // Seed lore store from genre pack (story 11-4)
    let lore_count = {
        let mut store = lore_store.lock().await;
        sidequest_game::seed_lore_from_genre_pack(&mut store, &pack)
    };
    tracing::info!(count = lore_count, genre = %genre, "rag.lore_store_seeded");

    // Extract trope definitions from the genre pack for per-session use.
    // Collect from genre-level tropes and all world tropes.
    // Auto-generate IDs from names for tropes that don't have explicit IDs,
    // and filter out abstract archetypes (they need world-level specialization).
    let mut all_tropes = pack.tropes.clone();
    for world in pack.worlds.values() {
        all_tropes.extend(world.tropes.clone());
    }
    // Backfill missing IDs from name slugs so seeding/tick can match them
    for trope in &mut all_tropes {
        if trope.id.is_none() {
            let slug = trope
                .name
                .as_str()
                .to_lowercase()
                .replace(' ', "-")
                .replace(|c: char| !c.is_alphanumeric() && c != '-', "");
            trope.id = Some(slug);
        }
    }
    // Filter out abstract archetypes — they are templates, not activatable tropes
    all_tropes.retain(|t| !t.is_abstract);
    *trope_defs_out = all_tropes;
    tracing::info!(count = trope_defs_out.len(), genre = %genre, "Loaded trope definitions (abstract filtered, IDs backfilled)");

    // Extract world description for narrator prompt context
    if let Some(world) = pack.worlds.get(world_slug) {
        let mut ctx = format!("World: {}", world.config.name);
        ctx.push_str(&format!("\n{}", world.config.description));
        if let Some(ref history) = world.lore.history {
            ctx.push_str(&format!("\nHistory: {}", history));
        }
        if let Some(ref geography) = world.lore.geography {
            ctx.push_str(&format!("\nGeography: {}", geography));
        }
        *world_context_out = ctx;
        tracing::info!(world = %world_slug, context_len = world_context_out.len(), "Loaded world context");
    }

    // Inject name bank context from cultures (prefer world-specific, fall back to genre-level)
    let cultures = pack
        .worlds
        .get(world_slug)
        .filter(|w| !w.cultures.is_empty())
        .map(|w| w.cultures.as_slice())
        .unwrap_or(&pack.cultures);
    let culture_ref = npc_context::build_culture_reference(cultures);
    if !culture_ref.is_empty() {
        world_context_out.push_str(&culture_ref);
    }

    // Select a random opening hook — prefer world-tier list when present,
    // fall back to genre-tier list. Mirrors the `cultures` lookup above and
    // stops genre-tier openings (e.g. Long Foundry covenant hooks under
    // heavy_metal) from leaking into every world's chargen.
    let (openings, openings_tier) = pack
        .worlds
        .get(world_slug)
        .filter(|w| !w.openings.is_empty())
        .map(|w| (w.openings.as_slice(), "world"))
        .unwrap_or((pack.openings.as_slice(), "genre"));

    if !openings.is_empty() {
        use rand::Rng;
        let idx = rand::rng().random_range(0..openings.len());
        let hook = &openings[idx];

        // Build opening directive — injected into Early zone on turn 0 only via DispatchContext
        let mut directive = format!(
            "=== OPENING SCENARIO ===\nArchetype: {}\nSituation: {}\nTone: {}",
            hook.archetype, hook.situation, hook.tone
        );
        if !hook.avoid.is_empty() {
            directive.push_str(&format!("\nAVOID: {}", hook.avoid.join("; ")));
        }
        directive.push_str("\n=== END OPENING ===");
        *opening_directive_out = Some(directive);

        *opening_seed_out = Some(hook.first_turn_seed.clone());

        tracing::info!(
            otel_event = "content.resolve",
            content_axis = "openings",
            content_genre = %genre,
            content_world = %world_slug,
            content_source_tier = openings_tier,
            hook_id = %hook.id,
            archetype = %hook.archetype,
            "opening_hook_selected"
        );
    }

    // Prefer world-tier chargen scenes when present, fall back to genre-tier.
    let (scenes, char_creation_tier) = pack
        .worlds
        .get(world_slug)
        .filter(|w| !w.char_creation.is_empty())
        .map(|w| (w.char_creation.clone(), "world"))
        .unwrap_or_else(|| (pack.char_creation.clone(), "genre"));

    tracing::info!(
        otel_event = "content.resolve",
        content_axis = "char_creation",
        content_genre = %genre,
        content_world = %world_slug,
        content_source_tier = char_creation_tier,
        scene_count = scenes.len(),
        "char_creation_scenes_selected"
    );

    if scenes.is_empty() {
        tracing::warn!(genre = %genre, "No character creation scenes");
        return vec![];
    }

    let b = match CharacterBuilder::try_new(scenes, &pack.rules, pack.backstory_tables.clone()) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(error = ?e, "Failed to create CharacterBuilder");
            return vec![];
        }
    };
    // Story 31-3: wire optional equipment_tables into the builder so scenes
    // with `equipment_generation: random_table` can roll starting inventory.
    let b = if let Some(ref equipment_tables) = pack.equipment_tables {
        b.with_equipment_tables(equipment_tables.clone())
    } else {
        b
    };
    // Thread the lobby-provided player name into the builder so scene narration
    // templates with `{name}` resolve in genres without a name-entry scene
    // (playtest 2026-04-19 seal narration bug).
    let b = b.with_lobby_name(lobby_name);

    // Display-only scenes (no choices, no freeform) are now first-class:
    // they emit CHARACTER_CREATION messages with input_type="continue" and
    // wait for the player to acknowledge via a "continue" phase message.
    // No silent skipping — every scene's narration is shown to the player.
    let scene_msg = b.to_scene_message(player_id);
    *builder = Some(b);

    // Send genre-pack mixer config so the frontend initializes per-genre volumes.
    let mut msgs = vec![mixer_config_cue(&pack.audio.mixer, player_id)];
    msgs.push(scene_msg);
    msgs
}

/// Handle CHARACTER_CREATION messages (client choices).
pub(crate) async fn dispatch_character_creation(
    payload: &CharacterCreationPayload,
    cctx: &mut ChargenDispatchContext<'_>,
    state: &AppState,
    player_id: &str,
) -> Vec<GameMessage> {
    let session = &mut *cctx.session;
    let builder = &mut *cctx.builder;
    let player_name_store = &mut *cctx.player_name_store;
    let character_json_store = &mut *cctx.character_json_store;
    let character_name_store = &mut *cctx.character_name_store;
    let character_hp = &mut *cctx.character_hp;
    let character_max_hp = &mut *cctx.character_max_hp;
    let character_level = &mut *cctx.character_level;
    let character_xp = &mut *cctx.character_xp;
    let current_location = &mut *cctx.current_location;
    let inventory = &mut *cctx.inventory;
    let trope_states = &mut *cctx.trope_states;
    let trope_defs = &mut *cctx.trope_defs;
    let world_context: &str = cctx.world_context;
    let opening_seed = cctx.opening_seed;
    let opening_directive = &mut *cctx.opening_directive;
    let axes_config = cctx.axes_config;
    let axis_values = &mut *cctx.axis_values;
    let visual_style = cctx.visual_style;
    let npc_registry = &mut *cctx.npc_registry;
    let narration_history = &mut *cctx.narration_history;
    let discovered_regions = &mut *cctx.discovered_regions;
    let turn_manager = &mut *cctx.turn_manager;
    let lore_store = cctx.lore_store;
    let lore_embed_tx = cctx.lore_embed_tx;
    let shared_session_holder = cctx.shared_session_holder;
    let music_director = &mut *cctx.music_director;
    let audio_mixer = cctx.audio_mixer;
    let prerender_scheduler = cctx.prerender_scheduler;
    let continuity_corrections = &mut *cctx.continuity_corrections;
    let quest_log = &mut *cctx.quest_log;
    let genie_wishes = &mut *cctx.genie_wishes;
    let achievement_tracker = &mut *cctx.achievement_tracker;
    let snapshot = &mut *cctx.snapshot;
    let narrator_verbosity = cctx.narrator_verbosity;
    let narrator_vocabulary = cctx.narrator_vocabulary;
    let pending_trope_context = &mut *cctx.pending_trope_context;
    let tx = cctx.tx;
    let b = match builder.as_mut() {
        Some(b) => b,
        None => return vec![error_response(player_id, "No character builder active")],
    };

    // Pack lookup for Confirmation-phase summary rendering. Loaded once here
    // because `scene` / `continue` can both transition into Confirmation after
    // apply_choice / apply_auto_advance, and the confirmation summary renderer
    // needs the pack to resolve `starting_equipment` (60798b6).
    let confirmation_pack = session
        .genre_slug()
        .and_then(|g| GenreCode::new(g).ok())
        .and_then(|gc| {
            state
                .genre_cache()
                .get_or_load(&gc, state.genre_loader())
                .ok()
        });

    // Handle navigation actions (back/edit) before phase dispatch.
    // The UI sends { action: "back" } or { action: "edit", target_step: N }
    // to navigate within chargen without progressing forward.
    if let Some(action) = payload.action.as_deref() {
        match action {
            "back" => {
                WatcherEventBuilder::new("character_creation", WatcherEventType::StateTransition)
                    .field("action", "back")
                    .field("from_scene", b.current_scene_index())
                    .field("player_id", player_id)
                    .send();

                if let Err(e) = b.go_back() {
                    return vec![error_response(
                        player_id,
                        &format!("Cannot go back: {:?}", e),
                    )];
                }
                return vec![b.to_scene_message(player_id)];
            }
            "edit" => {
                let target_step = match payload.target_step {
                    Some(t) => t as usize,
                    None => {
                        return vec![error_response(
                            player_id,
                            "action:edit requires target_step field",
                        )];
                    }
                };
                WatcherEventBuilder::new("character_creation", WatcherEventType::StateTransition)
                    .field("action", "edit")
                    .field("target_step", target_step)
                    .field("player_id", player_id)
                    .send();

                if let Err(e) = b.go_to_scene(target_step) {
                    return vec![error_response(
                        player_id,
                        &format!("Cannot edit scene {}: {:?}", target_step, e),
                    )];
                }
                return vec![b.to_scene_message(player_id)];
            }
            other => {
                return vec![error_response(
                    player_id,
                    &format!("Unknown chargen action: {}", other),
                )];
            }
        }
    }

    let phase = payload.phase.as_str();
    tracing::info!(phase = %phase, player_id = %player_id, "Character creation phase");

    match phase {
        "scene" => {
            let choice_str = payload.choice.as_deref().unwrap_or("1");

            // Try parsing as 1-based numeric index first
            let resolved_index = if let Ok(n) = choice_str.parse::<usize>() {
                Some(n.saturating_sub(1))
            } else {
                // Not a number — try matching against choice labels (case-insensitive)
                let scene = b.current_scene();
                scene
                    .choices
                    .iter()
                    .position(|c| c.label.eq_ignore_ascii_case(choice_str))
            };

            WatcherEventBuilder::new("character_creation", WatcherEventType::StateTransition)
                .field("phase", phase)
                .field("choice_raw", choice_str)
                .field("resolved_index", format!("{:?}", resolved_index))
                .field("player_id", player_id)
                .send();

            if let Some(index) = resolved_index {
                if let Err(e) = b.apply_choice(index) {
                    return vec![error_response(
                        player_id,
                        &format!("Invalid choice: {:?}", e),
                    )];
                }
            } else {
                // Freeform text input — use apply_freeform if the scene allows it
                if let Err(e) = b.apply_freeform(choice_str) {
                    return vec![error_response(
                        player_id,
                        &format!("Invalid freeform input: {:?}", e),
                    )];
                }
            }

            // After applying the choice the builder advances to the next scene
            // (or to Confirmation). Display-only scenes are now first-class —
            // they get their own message with input_type="continue" and wait
            // for the player to acknowledge.
            if b.is_confirmation() {
                match confirmation_pack.as_ref() {
                    Some(pack) => vec![super::chargen_summary::render_confirmation_summary(
                        b,
                        pack,
                        player_name_store.as_deref(),
                        player_id,
                    )],
                    None => vec![error_response(
                        player_id,
                        "Failed to load genre pack for confirmation summary",
                    )],
                }
            } else {
                vec![b.to_scene_message(player_id)]
            }
        }
        "continue" => {
            // Player acknowledged a display-only scene. Advance and emit
            // whatever comes next (another scene, or confirmation).
            tracing::info!(player_id = %player_id, "chargen.continue acknowledged");
            WatcherEventBuilder::new("character_creation", WatcherEventType::StateTransition)
                .field("phase", phase)
                .field("player_id", player_id)
                .send();
            if let Err(e) = b.apply_auto_advance() {
                return vec![error_response(
                    player_id,
                    &format!("Cannot continue from current scene: {:?}", e),
                )];
            }
            if b.is_confirmation() {
                match confirmation_pack.as_ref() {
                    Some(pack) => vec![super::chargen_summary::render_confirmation_summary(
                        b,
                        pack,
                        player_name_store.as_deref(),
                        player_id,
                    )],
                    None => vec![error_response(
                        player_id,
                        "Failed to load genre pack for confirmation summary",
                    )],
                }
            } else {
                vec![b.to_scene_message(player_id)]
            }
        }
        "confirmation" => {
            // Story 30-1: Build the character — use the name from the name-entry scene
            // if available, otherwise fall back to the player connection name, then "Player".
            // Do NOT fall back to payload.choice — that's the UI button index (e.g. "1"),
            // not a real character name.
            let name_from_scene = b.character_name().map(|s| s.to_string());
            let char_name = name_from_scene
                .clone()
                .unwrap_or_else(|| player_name_store.as_deref().unwrap_or("Player").to_string());

            WatcherEventBuilder::new("character_creation", WatcherEventType::StateTransition)
                .field("event", "name_resolved")
                .field("char_name", char_name.as_str())
                .field(
                    "source",
                    if name_from_scene.is_some() {
                        "name_scene"
                    } else {
                        "player_name_fallback"
                    },
                )
                .field("player_id", player_id)
                .send();

            match b.build(&char_name) {
                Ok(mut character) => {
                    let char_json = serde_json::to_value(&character).unwrap_or_default();

                    WatcherEventBuilder::new(
                        "character_creation",
                        WatcherEventType::StateTransition,
                    )
                    .field("event", "character_built")
                    .field("name", character.core.name.as_str())
                    .field("class", character.char_class.as_str())
                    .field("race", character.race.as_str())
                    .field("hp", character.core.edge.current)
                    .send();

                    // Resolve archetype through full pipeline (base → constraints → funnels)
                    // if the builder set axis hints during chargen.
                    if let Some(ref archetype_raw) = character.resolved_archetype.clone() {
                        if let Some((jungian, rpg_role)) = archetype_raw.split_once('/') {
                            let genre_slug = session.genre_slug().unwrap_or("").to_string();
                            let world_slug = session.world_slug().unwrap_or("").to_string();
                            if let Ok(gc) = GenreCode::new(&genre_slug) {
                                if let Ok(pack) =
                                    state.genre_cache().get_or_load(&gc, state.genre_loader())
                                {
                                    if let (Some(ref base), Some(ref constraints)) =
                                        (&pack.base_archetypes, &pack.archetype_constraints)
                                    {
                                        let world_funnels = pack
                                            .worlds
                                            .get(&world_slug)
                                            .and_then(|w| w.archetype_funnels.as_ref());

                                        match sidequest_genre::archetype::resolve_archetype(
                                            jungian,
                                            rpg_role,
                                            base,
                                            constraints,
                                            world_funnels,
                                            &genre_slug,
                                            Some(&world_slug),
                                        ) {
                                            Ok(result) => {
                                                // Phase G3: one call sets both
                                                // `resolved_archetype` (display
                                                // name) and `archetype_provenance`
                                                // (tier + merge trail), keeping
                                                // them in lockstep. The
                                                // `Resolved<ArchetypeResolved>`
                                                // is reconstructed from the
                                                // shim's `ArchetypeResolution` —
                                                // they carry the same value +
                                                // provenance but the shim also
                                                // exposes `source` and `weight`
                                                // which the character doesn't
                                                // need.
                                                let resolved_for_character =
                                                    sidequest_genre::resolver::Resolved {
                                                        value: result.resolved.clone(),
                                                        provenance: result.provenance.clone(),
                                                    };
                                                character.apply_archetype_resolved(
                                                    &resolved_for_character,
                                                );
                                                let source_tier =
                                                    resolved_for_character.source_tier_for_panel();

                                                WatcherEventBuilder::new(
                                                    "archetype_resolution",
                                                    WatcherEventType::StateTransition,
                                                )
                                                .field("event", "archetype.resolved")
                                                .field("jungian", jungian)
                                                .field("rpg_role", rpg_role)
                                                .field(
                                                    "resolved_name",
                                                    result.resolved.name.as_str(),
                                                )
                                                .field("source_tier", source_tier)
                                                .field("source", format!("{:?}", result.source))
                                                .field(
                                                    "faction",
                                                    result
                                                        .resolved
                                                        .faction
                                                        .as_deref()
                                                        .unwrap_or("none"),
                                                )
                                                .field("genre", genre_slug.as_str())
                                                .field("world", world_slug.as_str())
                                                .send();
                                            }
                                            Err(e) => {
                                                WatcherEventBuilder::new(
                                                    "archetype_resolution",
                                                    WatcherEventType::ValidationWarning,
                                                )
                                                .field("event", "archetype.resolution_failed")
                                                .field("error", format!("{e}"))
                                                .field("jungian", jungian)
                                                .field("rpg_role", rpg_role)
                                                .send();
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Store character data — sync ALL mutable fields from the built character
                    *character_name_store = Some(character.core.name.as_str().to_string());
                    *character_hp = character.core.edge.current;
                    *character_max_hp = character.core.edge.max;
                    *inventory = character.core.inventory.clone();

                    // Wire starting equipment from genre pack's inventory.yaml.
                    // The data exists, the parser exists, sidequest-loadoutgen reads
                    // it — but chargen never called any of it.  Classic wiring gap.
                    {
                        let char_class = character.char_class.as_str().to_string();
                        let genre_slug = session.genre_slug().unwrap_or("").to_string();
                        if let Ok(gc) = GenreCode::new(&genre_slug) {
                            if let Ok(pack) =
                                state.genre_cache().get_or_load(&gc, state.genre_loader())
                            {
                                if let Some(ref inv_config) = pack.inventory {
                                    // Match class name case-insensitively
                                    let class_lower = char_class.to_lowercase();
                                    let equipment_ids: Vec<String> = inv_config
                                        .starting_equipment
                                        .iter()
                                        .find(|(k, _)| k.to_lowercase() == class_lower)
                                        .map(|(_, v)| v.clone())
                                        .unwrap_or_default();
                                    let gold = inv_config
                                        .starting_gold
                                        .iter()
                                        .find(|(k, _)| k.to_lowercase() == class_lower)
                                        .map(|(_, v)| *v)
                                        .unwrap_or(0);

                                    // Resolve item IDs from catalog
                                    for item_id in &equipment_ids {
                                        if let Some(catalog_item) = inv_config
                                            .item_catalog
                                            .iter()
                                            .find(|ci| ci.id == *item_id)
                                        {
                                            let rarity_str = if catalog_item.rarity.is_empty() {
                                                "common"
                                            } else {
                                                &catalog_item.rarity
                                            };
                                            if let (Ok(name), Ok(desc), Ok(cat), Ok(rarity)) = (
                                                sidequest_protocol::NonBlankString::new(
                                                    &catalog_item.name,
                                                ),
                                                sidequest_protocol::NonBlankString::new(
                                                    &catalog_item.description,
                                                ),
                                                sidequest_protocol::NonBlankString::new(
                                                    &catalog_item.category,
                                                ),
                                                sidequest_protocol::NonBlankString::new(rarity_str),
                                            ) {
                                                inventory.items.push(sidequest_game::Item {
                                                    id: sidequest_protocol::NonBlankString::new(
                                                        &catalog_item.id,
                                                    )
                                                    .unwrap_or(name.clone()),
                                                    name,
                                                    description: desc,
                                                    category: cat,
                                                    value: catalog_item.value as i32,
                                                    weight: catalog_item.weight,
                                                    rarity,
                                                    narrative_weight: 0.3,
                                                    tags: catalog_item.tags.clone(),
                                                    equipped: false,
                                                    quantity: 1,
                                                    uses_remaining: catalog_item.resource_ticks,
                                                    state: sidequest_game::ItemState::Carried,
                                                });
                                            }
                                        } else {
                                            // Item not in catalog — create a minimal entry
                                            let display = item_id.replace('_', " ");
                                            if let (
                                                Ok(id_nb),
                                                Ok(name_nb),
                                                Ok(desc_nb),
                                                Ok(cat_nb),
                                                Ok(rar_nb),
                                            ) = (
                                                sidequest_protocol::NonBlankString::new(item_id),
                                                sidequest_protocol::NonBlankString::new(&display),
                                                sidequest_protocol::NonBlankString::new(
                                                    "Starting equipment",
                                                ),
                                                sidequest_protocol::NonBlankString::new(
                                                    "equipment",
                                                ),
                                                sidequest_protocol::NonBlankString::new("common"),
                                            ) {
                                                inventory.items.push(sidequest_game::Item {
                                                    id: id_nb,
                                                    name: name_nb,
                                                    description: desc_nb,
                                                    category: cat_nb,
                                                    value: 0,
                                                    weight: 1.0,
                                                    rarity: rar_nb,
                                                    narrative_weight: 0.2,
                                                    tags: vec![],
                                                    equipped: false,
                                                    quantity: 1,
                                                    uses_remaining: None,
                                                    state: sidequest_game::ItemState::Carried,
                                                });
                                            }
                                        }
                                    }
                                    inventory.gold += gold as i64;
                                    if !equipment_ids.is_empty() || gold > 0 {
                                        tracing::info!(
                                            class = %char_class,
                                            items_added = equipment_ids.len(),
                                            gold_added = gold,
                                            "chargen.starting_equipment — wired from inventory.yaml"
                                        );
                                    }
                                }
                            }
                        }
                    }

                    // Rebuild char_json with post-loadout inventory.
                    // The original char_json (line 674) was captured BEFORE starting
                    // equipment was added. Everything downstream (snapshot, PlayerState,
                    // CHARACTER_SHEET) reads from character_json — it must reflect
                    // the full loadout, not just builder item_hints.
                    {
                        let mut updated_char = character.clone();
                        updated_char.core.inventory = inventory.clone();
                        *character_json_store =
                            Some(serde_json::to_value(&updated_char).unwrap_or_default());
                    }
                    tracing::info!(
                        char_name = %character.core.name,
                        hp = character.core.edge.current,
                        items = inventory.items.len(),
                        gold = inventory.gold,
                        pronouns = %character.pronouns,
                        "chargen.complete — character built, inventory synced"
                    );

                    // Save to SQLite for reconnection across restarts (keyed by player)
                    let genre = session.genre_slug().unwrap_or("").to_string();
                    let world = session.world_slug().unwrap_or("").to_string();
                    let pname_for_save =
                        player_name_store.as_deref().unwrap_or("Player").to_string();

                    // Materialize world from genre pack history (Story 15-23).
                    // Load the genre pack (cached) to get World.history, then build
                    // a snapshot at Fresh maturity with history chapters applied.
                    // NOTE: This assigns to the &mut snapshot parameter, NOT a local.
                    // A previous version shadowed it with `let mut snapshot = ...`,
                    // causing the per-connection snapshot to stay empty (characters: [],
                    // npcs: [], quest_log: {}, genre/world: "").
                    *snapshot = {
                        let history_value = GenreCode::new(&genre)
                            .ok()
                            .and_then(|gc| {
                                state
                                    .genre_cache()
                                    .get_or_load(&gc, state.genre_loader())
                                    .ok()
                            })
                            .and_then(|pack| pack.worlds.get(&world).map(|w| w.history.clone()))
                            .flatten()
                            .unwrap_or(serde_json::Value::Null);

                        let mut snap = sidequest_game::materialize_from_genre_pack(
                            &history_value,
                            sidequest_game::CampaignMaturity::Fresh,
                            &genre,
                            &world,
                        ).unwrap_or_else(|e| {
                            tracing::warn!(error = %e, genre = %genre, world = %world, "Failed to materialize world from genre pack, using default snapshot");
                            sidequest_game::GameSnapshot {
                                genre_slug: genre.clone(),
                                world_slug: world.clone(),
                                ..Default::default()
                            }
                        });

                        // OTEL event for new-player world materialization (Story 15-18)
                        WatcherEventBuilder::new(
                            "world_materialization",
                            WatcherEventType::StateTransition,
                        )
                        .field("event", "world_materialized")
                        .field("genre", genre.as_str())
                        .field("world", world.as_str())
                        .field("chapters_applied", snap.world_history.len())
                        .field("maturity", format!("{:?}", snap.campaign_maturity))
                        .field("trigger", "new_player_chargen")
                        .send();

                        // Inject the chargen-produced character into the materialized snapshot
                        snap.characters = vec![character.clone()];
                        // Sync post-loadout inventory into snapshot character.
                        // The `character` object has only builder item_hints; the full
                        // loadout from inventory.yaml was added to the `inventory` local.
                        if let Some(ch) = snap.characters.first_mut() {
                            ch.core.inventory = inventory.clone();
                        }

                        // Scenario initialization: bind ScenarioPack → ScenarioState if available
                        if let Ok(gc) = GenreCode::new(&genre) {
                            if let Ok(pack) =
                                state.genre_cache().get_or_load(&gc, state.genre_loader())
                            {
                                if !pack.scenarios.is_empty() {
                                    // Pick the first scenario (future: player/DM selection)
                                    if let Some((_scenario_id, scenario_pack)) =
                                        pack.scenarios.iter().next()
                                    {
                                        let scenario_state = sidequest_game::scenario_state::ScenarioState::from_genre_pack(scenario_pack);
                                        tracing::info!(
                                            genre = %genre,
                                            world = %world,
                                            scenario = %_scenario_id,
                                            guilty_npc = %scenario_state.guilty_npc(),
                                            npc_roles = scenario_state.npc_roles().len(),
                                            "scenario.initialized — bound ScenarioPack to session"
                                        );

                                        // Initialize scenario NPC belief states from pack data
                                        for snpc in &scenario_pack.npcs {
                                            if let Some(npc) = snap
                                                .npcs
                                                .iter_mut()
                                                .find(|n| n.core.name.as_str() == snpc.name)
                                            {
                                                for fact in &snpc.initial_beliefs.facts {
                                                    npc.belief_state.add_belief(
                                                        sidequest_game::belief_state::Belief::Fact {
                                                            subject: snpc.name.clone(),
                                                            content: fact.clone(),
                                                            turn_learned: 0,
                                                            source: sidequest_game::belief_state::BeliefSource::Witnessed,
                                                        },
                                                    );
                                                }
                                                for suspicion in &snpc.initial_beliefs.suspicions {
                                                    npc.belief_state.add_belief(
                                                        sidequest_game::belief_state::Belief::suspicion(
                                                            suspicion.target.clone(),
                                                            suspicion.basis.clone(),
                                                            0,
                                                            sidequest_game::belief_state::BeliefSource::Inferred,
                                                            suspicion.confidence as f32,
                                                        ),
                                                    );
                                                }
                                            }
                                        }

                                        snap.scenario_state = Some(scenario_state);

                                        // Store scenario pack in shared session for pressure event/scene budget checks
                                        if let Ok(holder) = shared_session_holder.try_lock() {
                                            if let Some(ref ss_arc) = *holder {
                                                if let Ok(mut ss) = ss_arc.try_lock() {
                                                    ss.active_scenario =
                                                        Some(scenario_pack.clone());
                                                }
                                            }
                                        }

                                        crate::WatcherEventBuilder::new(
                                            "scenario",
                                            crate::WatcherEventType::StateTransition,
                                        )
                                        .field("event", "scenario_initialized")
                                        .field("genre", genre.as_str())
                                        .field("world", world.as_str())
                                        .field("scenario_id", _scenario_id.as_str())
                                        .send();
                                    }
                                }
                            }
                        }

                        // Room-graph mode: set starting location to entrance room (story 19-2)
                        let rooms_for_init: Vec<sidequest_genre::RoomDef> = match GenreCode::new(
                            &genre,
                        ) {
                            Ok(gc) => {
                                match state.genre_cache().get_or_load(&gc, state.genre_loader()) {
                                    Ok(pack) => pack
                                        .worlds
                                        .get(&world)
                                        .cloned()
                                        .filter(|w| {
                                            w.cartography.navigation_mode
                                                == sidequest_genre::NavigationMode::RoomGraph
                                        })
                                        .and_then(|w| w.cartography.rooms.clone())
                                        .unwrap_or_default(),
                                    Err(e) => {
                                        tracing::warn!(error = %e, genre = %genre, world = %world, "Failed to load genre pack for room-graph init");
                                        vec![]
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, genre = %genre, "Invalid genre code for room-graph init");
                                vec![]
                            }
                        };
                        if !rooms_for_init.is_empty() {
                            sidequest_game::room_movement::init_room_graph_location(
                                &mut snap,
                                &rooms_for_init,
                            );
                            tracing::info!(
                                location = %snap.location,
                                discovered_rooms = snap.discovered_rooms.len(),
                                "room_graph.init — entrance room set"
                            );
                            // Story 26-8: emit initial location event for GM panel
                            WatcherEventBuilder::new("location", WatcherEventType::StateTransition)
                                .field("event", "location.initialized")
                                .field("location", snap.location.as_str())
                                .field("mode", "room_graph")
                                .field("source", "entrance_room")
                                .send();
                        }

                        snap
                    };

                    // Set initial current_location from snapshot (room-graph) or genre rules
                    if !snapshot.location.is_empty() {
                        *current_location = snapshot.location.clone();
                    } else {
                        // Fallback: use default_location from rules.yaml
                        let default_loc = GenreCode::new(&genre)
                            .ok()
                            .and_then(|gc| {
                                state
                                    .genre_cache()
                                    .get_or_load(&gc, state.genre_loader())
                                    .ok()
                            })
                            .and_then(|pack| pack.rules.default_location.clone())
                            .unwrap_or_default();
                        if !default_loc.is_empty() {
                            *current_location = default_loc.clone();
                            snapshot.location = default_loc.clone();
                            snapshot.current_region = default_loc.clone();
                            discovered_regions.push(default_loc);
                            tracing::info!(
                                location = %snapshot.location,
                                "connect.default_location — set from rules.yaml"
                            );
                            // Story 26-8: emit initial location event for GM panel
                            WatcherEventBuilder::new("location", WatcherEventType::StateTransition)
                                .field("event", "location.initialized")
                                .field("location", snapshot.location.as_str())
                                .field("mode", "region")
                                .field("source", "rules_yaml")
                                .send();
                        }
                    }
                    // Seed discovered_regions from snapshot location
                    if !current_location.is_empty()
                        && !discovered_regions
                            .iter()
                            .any(|r| r.eq_ignore_ascii_case(current_location.as_str()))
                    {
                        discovered_regions.push(current_location.clone());
                    }

                    // Playtest 2026-04-11: clear per-character narrative state at
                    // the chargen→Playing transition. This closes the "NPC state
                    // carries over between fresh sessions" bug where e.g. NPC
                    // "Spine Copperjaw" introduced during character A's parley
                    // was appearing in character B's opening narration in the
                    // same genre:world.
                    //
                    // The npc_registry leaks via multiple paths: the SharedGame-
                    // Session is keyed by genre:world and holds NPCs shared
                    // across players; the SQLite save persists them for
                    // reconnect; session_restore populates the local registry
                    // from the save on returning-player connect. Any path that
                    // lands in chargen (new player name, save-corrupted fallback,
                    // or a future explicit "new character" flow) inherits those
                    // NPCs unless we explicitly clear them HERE, at the moment
                    // a fresh character starts existing in the world.
                    //
                    // Only the npc_registry is cleared — world history, lore,
                    // tropes, and region discovery are world-level state that
                    // SHOULD persist across characters in the same world.
                    npc_registry.clear();
                    snapshot.npc_registry.clear();
                    {
                        let holder = shared_session_holder.lock().await;
                        if let Some(ref ss_arc) = *holder {
                            let mut ss = ss_arc.lock().await;
                            ss.npc_registry.clear();
                        }
                    }
                    WatcherEventBuilder::new("npc_registry", WatcherEventType::StateTransition)
                        .field("event", "npc_registry.cleared_on_chargen_complete")
                        .field("genre", genre.as_str())
                        .field("world", world.as_str())
                        .field("player", pname_for_save.as_str())
                        .field("reason", "fresh_character_narrative_reset")
                        .send();
                    tracing::info!(
                        genre = %genre,
                        world = %world,
                        player = %pname_for_save,
                        "npc_registry.cleared — fresh character entering world, prior NPCs wiped"
                    );

                    // Sync initial location to SharedGameSession so sync_to_locals
                    // doesn't overwrite it with "" at the start of the opening turn.
                    {
                        let holder = shared_session_holder.lock().await;
                        if let Some(ref ss_arc) = *holder {
                            let mut ss = ss_arc.lock().await;
                            ss.current_location = current_location.clone();
                            ss.discovered_regions = discovered_regions.clone();
                            tracing::info!(
                                location = %current_location,
                                "connect.shared_session_sync — seeded location before opening narration"
                            );
                        }
                    }

                    if let Err(e) = state
                        .persistence()
                        .save(&genre, &world, &pname_for_save, snapshot)
                        .await
                    {
                        tracing::warn!(error = %e, genre = %genre, world = %world, player = %pname_for_save, "Failed to persist initial session");
                    }

                    // Transition session to Playing
                    if let Err(e) = session.complete_character_creation() {
                        tracing::error!(error = %e, state = %session.state_name(), "Failed to transition session to Playing after chargen");
                        return vec![error_response(
                            player_id,
                            &format!("Session transition failed: {e}"),
                        )];
                    }

                    // Story 15-10: seed CharacterCreation lore from the builder's
                    // scenes BEFORE clearing the builder. The builder owns the
                    // scenes data, so this must happen first or the lore is lost.
                    // Without this call, character backstory chosen during chargen
                    // is invisible to the lore retrieval pipeline.
                    if let Some(b) = builder.as_ref() {
                        let mut store = lore_store.lock().await;
                        let count =
                            sidequest_game::seed_lore_from_char_creation(&mut store, b.scenes());
                        tracing::info!(count = count, "rag.character_creation_lore_seeded");
                    }
                    *builder = None;

                    let complete = GameMessage::CharacterCreation {
                        payload: CharacterCreationPayload {
                            phase: "complete".to_string(),
                            scene_index: None,
                            total_scenes: None,
                            prompt: None,
                            summary: None,
                            message: None,
                            choices: None,
                            allows_freeform: None,
                            input_type: None,
                            loading_text: None,
                            character_preview: None,
                            rolled_stats: None,
                            choice: None,
                            character: Some(char_json),
                            action: None,
                            target_step: None,
                        },
                        player_id: player_id.to_string(),
                    };

                    let ready = GameMessage::SessionEvent {
                        payload: SessionEventPayload {
                            event: "ready".to_string(),
                            player_name: player_name_store.clone(),
                            genre: session.genre_slug().map(|s| s.to_string()),
                            world: session.world_slug().map(|s| s.to_string()),
                            has_character: Some(true),
                            initial_state: Some(InitialState {
                                characters: vec![sidequest_protocol::CharacterState {
                                    name: character.core.name.clone(),
                                    hp: *character_hp,
                                    max_hp: *character_max_hp,
                                    level: *character_level,
                                    class: character.char_class.as_str().to_string(),
                                    statuses: vec![],
                                    inventory: inventory
                                        .carried()
                                        .map(|i| i.name.as_str().to_string())
                                        .collect(),
                                    archetype_provenance: character.archetype_provenance.clone(),
                                }],
                                location: sidequest_protocol::NonBlankString::new(current_location)
                                    .expect("current_location is non-empty at session ready"),
                                quests: quest_log.clone(),
                                turn_count: turn_manager.interaction(),
                            }),
                            css: None,
                            image_cooldown_seconds: None,
                            narrator_verbosity: None,
                            narrator_vocabulary: None,
                        },
                        player_id: player_id.to_string(),
                    };

                    let intro_messages: Vec<GameMessage> = {
                        // Monster Manual: load/seed for opening turn (ADR-059)
                        let gs_mm = session.genre_slug().unwrap_or("");
                        let ws_mm = session.world_slug().unwrap_or("");
                        let mut monster_manual =
                            sidequest_game::monster_manual::MonsterManual::load(gs_mm, ws_mm);
                        if monster_manual.needs_seeding() && !gs_mm.is_empty() {
                            super::pregen::seed_manual(state, gs_mm, ws_mm, &mut monster_manual);
                        }

                        let mut ctx = super::DispatchContext {
                            action: opening_seed
                                .as_deref()
                                .unwrap_or("I look around and take in my surroundings."),
                            char_name: character.core.name.as_str(),
                            player_id,
                            genre_slug: session.genre_slug().unwrap_or(""),
                            world_slug: session.world_slug().unwrap_or(""),
                            player_name_for_save: player_name_store.as_deref().unwrap_or("Player"),
                            edge: character_hp,
                            max_edge: character_max_hp,
                            level: character_level,
                            xp: character_xp,
                            current_location,
                            inventory,
                            character_json: character_json_store,
                            trope_states,
                            trope_defs,
                            world_context,
                            axes_config,
                            axis_values,
                            visual_style,
                            npc_registry,
                            quest_log,
                            narration_history,
                            discovered_regions,
                            turn_manager,
                            lore_store,
                            lore_embed_tx,
                            shared_session_holder,
                            music_director,
                            audio_mixer,
                            prerender_scheduler,
                            state,
                            continuity_corrections,
                            genie_wishes,
                            sfx_library: {
                                let genre_slug = session.genre_slug().unwrap_or("");
                                sidequest_genre::GenreCode::new(genre_slug)
                                    .ok()
                                    .and_then(|gc| {
                                        state
                                            .genre_cache()
                                            .get_or_load(&gc, state.genre_loader())
                                            .ok()
                                    })
                                    .map(|pack| pack.audio.sfx_library.clone())
                                    .unwrap_or_default()
                            },
                            rooms: {
                                let gs = session.genre_slug().unwrap_or("");
                                let ws = session.world_slug().unwrap_or("");
                                match sidequest_genre::GenreCode::new(gs) {
                                    Ok(gc) => match state
                                        .genre_cache()
                                        .get_or_load(&gc, state.genre_loader())
                                    {
                                        Ok(pack) => pack
                                            .worlds
                                            .get(ws)
                                            .cloned()
                                            .filter(|world| {
                                                world.cartography.navigation_mode
                                                    == sidequest_genre::NavigationMode::RoomGraph
                                            })
                                            .and_then(|world| world.cartography.rooms.clone())
                                            .unwrap_or_default(),
                                        Err(e) => {
                                            tracing::warn!(error = %e, genre = %gs, world = %ws, "Failed to load genre pack for dispatch rooms");
                                            vec![]
                                        }
                                    },
                                    Err(e) => {
                                        tracing::warn!(error = %e, genre = %gs, "Invalid genre code for dispatch rooms");
                                        vec![]
                                    }
                                }
                            },
                            genre_affinities: {
                                let gs = session.genre_slug().unwrap_or("");
                                sidequest_genre::GenreCode::new(gs)
                                    .ok()
                                    .and_then(|gc| {
                                        state
                                            .genre_cache()
                                            .get_or_load(&gc, state.genre_loader())
                                            .ok()
                                    })
                                    .map(|pack| pack.progression.affinities.clone())
                                    .unwrap_or_default()
                            },
                            world_graph: {
                                let gs = session.genre_slug().unwrap_or("");
                                let ws = session.world_slug().unwrap_or("");
                                sidequest_genre::GenreCode::new(gs)
                                    .ok()
                                    .and_then(|gc| {
                                        state
                                            .genre_cache()
                                            .get_or_load(&gc, state.genre_loader())
                                            .ok()
                                    })
                                    .and_then(|pack| pack.worlds.get(ws).cloned())
                                    .and_then(|world| world.cartography.world_graph)
                            },
                            cartography_metadata: {
                                let gs = session.genre_slug().unwrap_or("");
                                let ws = session.world_slug().unwrap_or("");
                                sidequest_genre::GenreCode::new(gs)
                                    .ok()
                                    .and_then(|gc| {
                                        state
                                            .genre_cache()
                                            .get_or_load(&gc, state.genre_loader())
                                            .ok()
                                    })
                                    .and_then(|pack| pack.worlds.get(ws).cloned())
                                    .map(|world| {
                                        let nav_mode = match world.cartography.navigation_mode {
                                            sidequest_genre::NavigationMode::Region => "region",
                                            sidequest_genre::NavigationMode::RoomGraph => {
                                                "room_graph"
                                            }
                                            sidequest_genre::NavigationMode::Hierarchical => {
                                                "hierarchical"
                                            }
                                        };
                                        sidequest_protocol::CartographyMetadata {
                                            navigation_mode: nav_mode.to_string(),
                                            starting_region: world
                                                .cartography
                                                .starting_region
                                                .clone(),
                                            regions: world
                                                .cartography
                                                .regions
                                                .iter()
                                                .filter_map(|(slug, r)| {
                                                    let name =
                                                        sidequest_protocol::NonBlankString::new(
                                                            &r.name,
                                                        )
                                                        .ok()?;
                                                    Some((
                                                        slug.clone(),
                                                        sidequest_protocol::CartographyRegion {
                                                            name,
                                                            description: r.description.clone(),
                                                            adjacent: r.adjacent.clone(),
                                                        },
                                                    ))
                                                })
                                                .collect(),
                                            routes: world
                                                .cartography
                                                .routes
                                                .iter()
                                                .filter_map(|r| {
                                                    let name =
                                                        sidequest_protocol::NonBlankString::new(
                                                            &r.name,
                                                        )
                                                        .ok()?;
                                                    Some(sidequest_protocol::CartographyRoute {
                                                        name,
                                                        description: r.description.clone(),
                                                        from_id: r.from_id.clone(),
                                                        to_id: r.to_id.clone(),
                                                    })
                                                })
                                                .collect(),
                                        }
                                    })
                            },
                            confrontation_defs: {
                                let gs = session.genre_slug().unwrap_or("");
                                sidequest_genre::GenreCode::new(gs)
                                    .ok()
                                    .and_then(|gc| {
                                        state
                                            .genre_cache()
                                            .get_or_load(&gc, state.genre_loader())
                                            .ok()
                                    })
                                    .map(|pack| pack.rules.confrontations.clone())
                                    .unwrap_or_default()
                            },
                            aside: false,
                            opening_directive: opening_directive.take(),
                            narrator_verbosity,
                            narrator_vocabulary,
                            pending_trope_context,
                            achievement_tracker,
                            snapshot,
                            tx,
                            monster_manual: &mut monster_manual,
                            morpheme_glossaries: Vec::new(),
                            name_banks: Vec::new(),
                            carry_mode: {
                                let gs = session.genre_slug().unwrap_or("");
                                sidequest_genre::GenreCode::new(gs)
                                    .ok()
                                    .and_then(|gc| {
                                        state
                                            .genre_cache()
                                            .get_or_load(&gc, state.genre_loader())
                                            .ok()
                                    })
                                    .map(|pack| {
                                        pack.inventory
                                            .as_ref()
                                            .and_then(|inv| inv.philosophy.as_ref())
                                            .map(|phil| phil.carry_mode)
                                            .unwrap_or_default()
                                    })
                                    .unwrap_or_default()
                            },
                            weight_limit: {
                                let gs = session.genre_slug().unwrap_or("");
                                sidequest_genre::GenreCode::new(gs)
                                    .ok()
                                    .and_then(|gc| {
                                        state
                                            .genre_cache()
                                            .get_or_load(&gc, state.genre_loader())
                                            .ok()
                                    })
                                    .and_then(|pack| {
                                        pack.inventory
                                            .as_ref()
                                            .and_then(|inv| inv.philosophy.as_ref())
                                            .and_then(|phil| phil.weight_limit)
                                    })
                            },
                            chosen_player_beat: None,
                            pending_roll_outcome: None,
                            tactical_grid_summary: None,
                        };
                        // OTEL: log loaded confrontation defs (story 28-1)
                        if !ctx.confrontation_defs.is_empty() {
                            WatcherEventBuilder::new(
                                "encounter",
                                WatcherEventType::StateTransition,
                            )
                            .field("action", "defs_loaded")
                            .field("genre", ctx.genre_slug)
                            .field("count", ctx.confrontation_defs.len())
                            .field(
                                "types",
                                ctx.confrontation_defs
                                    .iter()
                                    .map(|d| d.confrontation_type.clone())
                                    .collect::<Vec<_>>(),
                            )
                            .send();
                        }
                        let result = super::dispatch_player_action(&mut ctx).await;
                        ctx.monster_manual.save();
                        result
                    };

                    // Build the character sheet facet for the PlayerState cache.
                    // This used to be emitted as a standalone CHARACTER_SHEET message;
                    // now it's stashed on PlayerState and surfaces via PARTY_STATUS.
                    let built_sheet = sidequest_protocol::CharacterSheetDetails {
                        race: character.race.clone(),
                        stats: character
                            .stats
                            .iter()
                            .map(|(k, v)| (k.clone(), *v))
                            .collect(),
                        abilities: character.abilities.iter().map(|a| a.name.clone()).collect(),
                        backstory: character.backstory.clone(),
                        personality: character.core.personality.clone(),
                        pronouns: sidequest_protocol::NonBlankString::new(&character.pronouns).ok(),
                        equipment: inventory
                            .carried()
                            .map(|i| {
                                if i.equipped {
                                    format!("{} [equipped]", i.name)
                                } else {
                                    i.name.as_str().to_string()
                                }
                            })
                            .collect(),
                    };

                    // Story 37-27: Character snapshot on session-start.
                    // The opening-turn `compose_responses` call above emits a
                    // PARTY_STATUS whose `sheet` facet is None, because the
                    // acting player hasn't been inserted into the shared
                    // session yet (that happens ~600 lines below, in the
                    // "Add player to shared session and broadcast PARTY_STATUS"
                    // block).
                    // Without a populated sheet, App.tsx's characterSheet
                    // setter skips the update and the Character tab stays
                    // blank until the first real player-action turn.
                    //
                    // Emit a corrected PARTY_STATUS carrying the freshly
                    // built sheet so the tab populates as soon as session
                    // ready arrives. The client's sheet handler ignores
                    // PARTY_STATUS with sheet=None, so ordering against
                    // later turn-updates is safe.
                    let session_start_party_status = {
                        let class_nbs =
                            sidequest_protocol::NonBlankString::new(character.char_class.as_str())
                                .unwrap_or_else(|_| {
                                    sidequest_protocol::NonBlankString::new("Adventurer")
                                        .expect("literal \"Adventurer\" is non-blank")
                                });
                        let name_nbs = sidequest_protocol::NonBlankString::new(
                            player_name_store.as_deref().unwrap_or("Player"),
                        )
                        .expect("player name falls back to literal \"Player\"");
                        let pid_nbs = sidequest_protocol::NonBlankString::new(player_id)
                            .expect("player_id is non-empty at session ready");
                        GameMessage::PartyStatus {
                            payload: PartyStatusPayload {
                                members: vec![PartyMember {
                                    player_id: pid_nbs,
                                    name: name_nbs,
                                    character_name: Some(character.core.name.clone()),
                                    current_hp: *character_hp,
                                    max_hp: *character_max_hp,
                                    statuses: character.core.statuses.clone(),
                                    class: class_nbs,
                                    level: *character_level,
                                    portrait_url: None,
                                    current_location: sidequest_protocol::NonBlankString::new(
                                        current_location,
                                    )
                                    .ok(),
                                    sheet: Some(built_sheet.clone()),
                                    inventory: Some(crate::shared_session::inventory_payload_from(
                                        inventory,
                                    )),
                                }],
                            },
                            player_id: player_id.to_string(),
                        }
                    };
                    WatcherEventBuilder::new("session", WatcherEventType::StateTransition)
                        .field("event", "session.start.character_snapshot_emitted")
                        .field("player_id", player_id)
                        .field("character_name", character.core.name.as_str())
                        .field("genre", session.genre_slug().unwrap_or(""))
                        .field("world", session.world_slug().unwrap_or(""))
                        .field("sheet_class", character.char_class.as_str())
                        .field("inventory_count", inventory.carried().count())
                        .send();

                    // Emit the character's backstory as a prose narration so
                    // it appears in the narrative view — not just in the overlay.
                    let backstory_narration = GameMessage::Narration {
                        payload: NarrationPayload {
                            text: character.backstory.clone(),
                            state_delta: None,
                            footnotes: vec![],
                        },
                        player_id: player_id.to_string(),
                    };
                    let backstory_end = GameMessage::NarrationEnd {
                        payload: NarrationEndPayload { state_delta: None },
                        player_id: player_id.to_string(),
                    };

                    // Catch-up context — extracted from shared session while lock
                    // is held, used for LLM generation after lock is released.
                    let mut catch_up_context: Option<(Vec<String>, String, String)> = None;
                    // Barrier state captured during reconnect for post-ready signaling.
                    let mut reconnect_barrier_state: Option<(Option<String>, bool)> = None;

                    // Add player to shared session and broadcast PARTY_STATUS
                    {
                        let holder = shared_session_holder.lock().await;
                        if let Some(ref ss_arc) = *holder {
                            let mut ss = ss_arc.lock().await;

                            // Story 8-8: Capture context for catch-up narration before
                            // releasing the lock. Only needed when joining an in-progress
                            // session (narration_history is non-empty).
                            //
                            // Story 37-1: Session-scoped resume markers. If narration_history
                            // exists but no other players are connected, this is a stale
                            // session from a previous game on the same genre:world pair.
                            // The remove_player_from_session cleanup has a try_lock race
                            // that can leave zombie sessions in the registry. Clear stale
                            // state instead of generating catch-up from a dead game.
                            if !ss.narration_history.is_empty() {
                                if ss.players.is_empty() {
                                    tracing::info!(
                                        session_id = %ss.session_id,
                                        genre = %ss.genre_slug,
                                        world = %ss.world_slug,
                                        history_len = ss.narration_history.len(),
                                        "session.stale_history_cleared — no active players, clearing narration from previous game instance"
                                    );
                                    crate::WatcherEventBuilder::new(
                                        "session",
                                        crate::WatcherEventType::StateTransition,
                                    )
                                    .field("action", "stale_session_cleared")
                                    .field("session_id", &ss.session_id)
                                    .field("genre", &ss.genre_slug)
                                    .field("world", &ss.world_slug)
                                    .field(
                                        "stale_history_len",
                                        ss.narration_history.len().to_string(),
                                    )
                                    .send();
                                    ss.narration_history.clear();
                                    ss.discovered_regions.clear();
                                    ss.current_location.clear();
                                    ss.npc_registry.clear();
                                    ss.trope_states.clear();
                                    // Reset multiplayer coordination state — a stale
                                    // TurnBarrier with old player IDs is a liveness
                                    // hazard (deadlock waiting for absent players).
                                    ss.turn_barrier = None;
                                    ss.perception_filters.clear();
                                    ss.turn_mode = sidequest_game::turn_mode::TurnMode::default();
                                    ss.scene_count = 0;
                                    ss.active_scenario = None;
                                    // Generate fresh session_id for the new game instance
                                    ss.session_id = uuid::Uuid::new_v4().to_string();
                                } else {
                                    catch_up_context = Some((
                                        ss.narration_history.clone(),
                                        ss.current_location.clone(),
                                        ss.world_context.clone(),
                                    ));
                                    tracing::info!(
                                        session_id = %ss.session_id,
                                        player_count = ss.players.len(),
                                        history_len = ss.narration_history.len(),
                                        "session.catch_up_context_captured — joining in-progress game"
                                    );
                                }
                            }

                            // Reconnect detection: if a player with the same
                            // player_name already exists under a different player_id,
                            // this is a tab-duplicate or page-refresh reconnect.
                            // Transfer the old PlayerState to the new player_id and
                            // suppress arrival/departure narration.
                            let connecting_name = player_name_store
                                .clone()
                                .unwrap_or_else(|| "Player".to_string());
                            let old_pid = ss
                                .players
                                .iter()
                                .find(|(pid, ps)| {
                                    pid.as_str() != player_id && ps.player_name == connecting_name
                                })
                                .map(|(pid, _)| pid.clone());
                            let is_reconnect = old_pid.is_some();
                            if let Some(ref old) = old_pid {
                                // Transfer existing PlayerState to the new player_id,
                                // so accumulated state (HP, inventory, location) is preserved.
                                if let Some(mut transferred) = ss.players.remove(old) {
                                    // Update character data from the (possibly restored) save
                                    transferred.character_name =
                                        Some(character.core.name.as_str().to_string());
                                    transferred.character_hp = character.core.edge.current;
                                    transferred.character_max_hp = character.core.edge.max;
                                    transferred.character_level = character.core.level;
                                    transferred.character_class =
                                        character.char_class.as_str().to_string();
                                    transferred.inventory = inventory.clone();
                                    transferred.character_xp = character.core.xp;
                                    transferred.sheet = Some(built_sheet.clone());
                                    if let Some(ref cj) = *character_json_store {
                                        transferred.character_json = Some(cj.clone());
                                    }
                                    // Route through the dedup chokepoint for code-path
                                    // uniformity. The old entry was already removed above,
                                    // so dedup's find() cannot match and the call will
                                    // return None at runtime — the call site exists to
                                    // satisfy the source-level wiring invariant (story 37-19,
                                    // AC-7). The debug_assert pins the invariant so a future
                                    // reorder that lets dedup fire here will surface loudly
                                    // in test builds rather than silently leaking an old_pid.
                                    let dedup_redundant =
                                        ss.insert_player_dedup_by_name(player_id, transferred);
                                    debug_assert!(
                                        dedup_redundant.is_none(),
                                        "reconnect-transfer path already removed old={old:?} above; \
                                         chokepoint should find no phantom. Got: {dedup_redundant:?}"
                                    );
                                    if let Some(unexpected_pid) = dedup_redundant {
                                        // Production safety net: if the invariant ever breaks
                                        // in release builds (debug_assert is compiled out),
                                        // still reconcile rather than silently leak.
                                        tracing::warn!(
                                            unexpected_pid = %unexpected_pid,
                                            new_pid = %player_id,
                                            "reconnect-transfer dedup fired unexpectedly — reconciling defensively"
                                        );
                                        ss.reconcile_removed_player(&unexpected_pid);
                                    }
                                }
                                // Update barrier roster: swap old player_id for new one
                                if let Some(ref barrier) = ss.turn_barrier {
                                    let _ = barrier.remove_player(old);
                                    // Re-add under new player_id so barrier recognizes submissions
                                    let placeholder_char = sidequest_game::Character {
                                        core: sidequest_game::CreatureCore {
                                            name: sidequest_protocol::NonBlankString::new(
                                                &connecting_name,
                                            )
                                            .unwrap(),
                                            description: sidequest_protocol::NonBlankString::new(
                                                "reconnect",
                                            )
                                            .unwrap(),
                                            personality: sidequest_protocol::NonBlankString::new(
                                                "n/a",
                                            )
                                            .unwrap(),
                                            level: 1,
                                            xp: 0,
                                            statuses: vec![],
                                            inventory: sidequest_game::Inventory::default(),
                                            edge:
                                                sidequest_game::creature_core::placeholder_edge_pool(
                                                ),
                                            acquired_advancements: vec![],
                                        },
                                        backstory: sidequest_protocol::NonBlankString::new("n/a")
                                            .unwrap(),
                                        narrative_state: String::new(),
                                        hooks: vec![],
                                        char_class: sidequest_protocol::NonBlankString::new(
                                            "barrier",
                                        )
                                        .unwrap(),
                                        race: sidequest_protocol::NonBlankString::new("barrier")
                                            .unwrap(),
                                        pronouns: String::new(),
                                        stats: std::collections::HashMap::new(),
                                        abilities: vec![],
                                        known_facts: vec![],
                                        affinities: vec![],
                                        is_friendly: true,
                                        resolved_archetype: None,
                                        archetype_provenance: None,
                                    };
                                    let _ =
                                        barrier.add_player(player_id.to_string(), placeholder_char);
                                    tracing::info!(
                                        old_pid = %old,
                                        new_pid = %player_id,
                                        "barrier.reconnect — swapped player_id in barrier roster"
                                    );
                                }
                                tracing::info!(
                                    old_player_id = %old,
                                    new_player_id = %player_id,
                                    player_name = %connecting_name,
                                    "Reconnect detected — transferred PlayerState to new connection"
                                );
                                WatcherEventBuilder::new(
                                    "multiplayer",
                                    WatcherEventType::StateTransition,
                                )
                                .field("event", "player_reconnect")
                                .field("old_player_id", old.as_str())
                                .field("new_player_id", player_id)
                                .field("player_name", connecting_name.as_str())
                                .send();
                            }

                            // Capture barrier state for reconnecting players so we can
                            // send the correct signal after the "ready" message.
                            reconnect_barrier_state = if is_reconnect {
                                if let Some(ref barrier) = ss.turn_barrier {
                                    let resolved = barrier.get_resolution_narration();
                                    let submitted = barrier.has_submitted(player_id);
                                    let signal = if resolved.is_some() {
                                        "narration_replay"
                                    } else if submitted {
                                        "waiting"
                                    } else {
                                        "ready"
                                    };
                                    WatcherEventBuilder::new(
                                        "multiplayer",
                                        WatcherEventType::StateTransition,
                                    )
                                    .field("event", "barrier_state_on_reconnect")
                                    .field("player_submitted", submitted)
                                    .field("barrier_resolved", resolved.is_some())
                                    .field("signal_sent", signal)
                                    .send();
                                    Some((resolved, submitted))
                                } else {
                                    None
                                }
                            } else {
                                None
                            };

                            if !is_reconnect {
                                let ps = shared_session::PlayerState::new(connecting_name.clone());
                                // Single dedup chokepoint for all player inserts (story 37-19).
                                // is_reconnect==false is a semantic precondition (no phantom
                                // expected), not a type guarantee — if a handshake race ever
                                // lets dedup fire here, reconcile defensively instead of
                                // silently dropping the returned old_pid.
                                if let Some(unexpected_pid) =
                                    ss.insert_player_dedup_by_name(player_id, ps)
                                {
                                    tracing::warn!(
                                        unexpected_pid = %unexpected_pid,
                                        new_pid = %player_id,
                                        player_name = %connecting_name,
                                        "fresh-connect dedup fired unexpectedly — reconciling defensively"
                                    );
                                    ss.reconcile_removed_player(&unexpected_pid);
                                }
                                // Populate character data on the PlayerState
                                if let Some(p) = ss.players.get_mut(player_id) {
                                    p.character_name =
                                        Some(character.core.name.as_str().to_string());
                                    p.character_hp = character.core.edge.current;
                                    p.character_max_hp = character.core.edge.max;
                                    p.character_level = character.core.level;
                                    p.character_class = character.char_class.as_str().to_string();
                                    p.inventory = inventory.clone();
                                    p.character_xp = character.core.xp;
                                    p.sheet = Some(built_sheet.clone());
                                    if let Some(ref cj) = *character_json_store {
                                        p.character_json = Some(cj.clone());
                                    }
                                }
                                // Notify existing players that a new character has arrived
                                let arrival_text = format!(
                                    "{} has entered the scene.",
                                    character.core.name.as_str()
                                );
                                let existing_pids: Vec<String> = ss
                                    .players
                                    .keys()
                                    .filter(|pid| pid.as_str() != player_id)
                                    .cloned()
                                    .collect();
                                let arrival_nbs = sidequest_protocol::NonBlankString::new(
                                    &arrival_text,
                                )
                                .expect("arrival_text is constructed non-empty by join flow");
                                for target_pid in &existing_pids {
                                    ss.send_to_player(
                                        GameMessage::Narration {
                                            payload: NarrationPayload {
                                                text: arrival_nbs.clone(),
                                                state_delta: None,
                                                footnotes: vec![],
                                            },
                                            player_id: target_pid.clone(),
                                        },
                                        target_pid.clone(),
                                    );
                                    ss.send_to_player(
                                        GameMessage::NarrationEnd {
                                            payload: NarrationEndPayload { state_delta: None },
                                            player_id: target_pid.clone(),
                                        },
                                        target_pid.clone(),
                                    );
                                }
                            }
                            // Story 26-11: Party reconciliation on session resume.
                            // If multiple players are in the session with divergent locations
                            // and split-party is not explicitly allowed, snap everyone to the
                            // majority location and emit a reconciliation narration.
                            if ss.players.len() > 1 {
                                let player_locations: Vec<
                                    sidequest_game::party_reconciliation::PlayerLocation,
                                > = ss
                                    .players
                                    .iter()
                                    .map(|(pid, ps)| {
                                        let loc = if pid == player_id {
                                            current_location.clone()
                                        } else {
                                            ps.display_location.clone()
                                        };
                                        sidequest_game::party_reconciliation::PlayerLocation {
                                            player_id: pid.clone(),
                                            player_name: ps.player_name.clone(),
                                            location: loc,
                                        }
                                    })
                                    .collect();

                                // Check scenario for split-party flag
                                let split_party_allowed = ss
                                    .active_scenario
                                    .as_ref()
                                    .map(|s| s.allows_split_party)
                                    .unwrap_or(false);

                                let result = sidequest_game::party_reconciliation::PartyReconciliation::reconcile(
                                    &player_locations,
                                    split_party_allowed,
                                );

                                if let sidequest_game::party_reconciliation::ReconciliationResult::Reconciled {
                                    ref target_location,
                                    ref players_moved,
                                    ref narration_text,
                                } = result
                                {
                                    // Update shared session location
                                    ss.current_location = target_location.clone();
                                    // Update current player's local location
                                    *current_location = target_location.clone();
                                    snapshot.location = target_location.clone();

                                    // Update all PlayerStates to the reconciled location
                                    for ps in ss.players.values_mut() {
                                        ps.display_location = target_location.clone();
                                    }

                                    // Broadcast reconciliation narration to all session members
                                    let narration_nbs =
                                        sidequest_protocol::NonBlankString::new(narration_text)
                                            .expect("reconciliation narration_text is non-empty by PartyReconciliation contract");
                                    ss.broadcast(GameMessage::Narration {
                                        payload: NarrationPayload {
                                            text: narration_nbs,
                                            state_delta: None,
                                            footnotes: vec![],
                                        },
                                        player_id: player_id.to_string(),
                                    });
                                    ss.broadcast(GameMessage::NarrationEnd {
                                        payload: NarrationEndPayload { state_delta: None },
                                        player_id: player_id.to_string(),
                                    });

                                    // OTEL: session.resume.party_reconciliation
                                    let players_moved_str: Vec<String> = players_moved
                                        .iter()
                                        .map(|m| format!("{}:{}->{}",
                                            m.player_name, m.old_location, m.new_location))
                                        .collect();
                                    WatcherEventBuilder::new("session", WatcherEventType::StateTransition)
                                        .field("event", "session.resume.party_reconciliation")
                                        .field("target_location", target_location.as_str())
                                        .field("players_moved", players_moved_str.join(", "))
                                        .field("player_count", ss.player_count())
                                        .send();

                                    tracing::info!(
                                        target_location = %target_location,
                                        players_moved = players_moved.len(),
                                        "session.resume.party_reconciliation — divergent locations reconciled"
                                    );
                                }
                            }

                            // Build and send targeted PARTY_STATUS to OTHER session members.
                            // The current player gets their PartyStatus from the opening
                            // narration dispatch — sending here too causes duplicates.
                            // Skip entirely for single-player (no other members to notify).
                            if ss.players.len() <= 1 {
                                tracing::debug!("Skipping connect PartyStatus — single player, dispatch will send");
                            } else {
                                // The acting player's PlayerState was just populated
                                // above with live turn data (character/inventory/sheet),
                                // so every member can be built via the shared helper.
                                // `current_location` on the acting player's PlayerState
                                // may lag — override just that field.
                                let members: Vec<PartyMember> = ss
                                    .players
                                    .iter()
                                    .map(|(pid, ps)| {
                                        let mut m =
                                            crate::shared_session::party_member_from(pid, ps);
                                        if pid == player_id {
                                            // Force live statuses + location for the acting player.
                                            m.statuses = character.core.statuses.clone();
                                            m.current_location =
                                                sidequest_protocol::NonBlankString::new(
                                                    current_location,
                                                )
                                                .ok();
                                        }
                                        m
                                    })
                                    .collect();
                                if !members.is_empty() {
                                    let player_ids: Vec<String> =
                                        ss.players.keys().cloned().collect();
                                    for target_pid in &player_ids {
                                        let party_msg = GameMessage::PartyStatus {
                                            payload: PartyStatusPayload {
                                                members: members.clone(),
                                            },
                                            player_id: target_pid.clone(),
                                        };
                                        ss.send_to_player(party_msg, target_pid.clone());
                                    }
                                }
                            } // end multiplayer PartyStatus
                            let pc = ss.player_count();
                            tracing::info!(
                                player_id = %player_id,
                                player_count = pc,
                                "Player joined shared session"
                            );
                            WatcherEventBuilder::new(
                                "multiplayer",
                                WatcherEventType::StateTransition,
                            )
                            .field("event", "session_joined")
                            .field("session_key", format!("{}:{}", genre, world))
                            .field("player_count", pc)
                            .send();

                            // Transition turn mode when a player joins
                            let old_mode = std::mem::take(&mut ss.turn_mode);
                            ss.turn_mode = old_mode.apply(
                                sidequest_game::turn_mode::TurnModeTransition::PlayerJoined {
                                    player_count: pc,
                                },
                            );
                            tracing::info!(
                                new_mode = ?ss.turn_mode,
                                player_count = pc,
                                "Turn mode transitioned on player join"
                            );
                            // Initialize or expand barrier if in structured mode
                            if ss.turn_mode.should_use_barrier() {
                                if let Some(ref barrier) = ss.turn_barrier {
                                    // Add player to existing barrier roster
                                    let placeholder_char = {
                                        use sidequest_game::character::Character;
                                        use sidequest_game::creature_core::CreatureCore;
                                        use sidequest_game::inventory::Inventory;
                                        use sidequest_protocol::NonBlankString;
                                        Character {
                                            core: CreatureCore {
                                                name: NonBlankString::new(player_id).unwrap(),
                                                description: NonBlankString::new(
                                                    "barrier placeholder",
                                                )
                                                .unwrap(),
                                                personality: NonBlankString::new("n/a").unwrap(),
                                                level: 1,
                                                xp: 0,
                                                statuses: vec![],
                                                inventory: Inventory::default(),
                                                edge: sidequest_game::creature_core::placeholder_edge_pool(),
                                                acquired_advancements: vec![],
                                            },
                                            backstory: NonBlankString::new("n/a").unwrap(),
                                            narrative_state: String::new(),
                                            hooks: vec![],
                                            char_class: NonBlankString::new("barrier").unwrap(),
                                            race: NonBlankString::new("barrier").unwrap(),
                                            pronouns: String::new(),
                                            stats: HashMap::new(),
                                            abilities: vec![],
                                            known_facts: vec![],
                                            affinities: vec![],
                                            is_friendly: true,
                                            resolved_archetype: None,
                                            archetype_provenance: None,
                                        }
                                    };
                                    let _ =
                                        barrier.add_player(player_id.to_string(), placeholder_char);
                                    tracing::info!(player_id = %player_id, "Added player to existing barrier");
                                } else {
                                    let mp_session = sidequest_game::multiplayer::MultiplayerSession::with_player_ids(
                                        ss.players.keys().cloned(),
                                    );
                                    let config =
                                        sidequest_game::barrier::TurnBarrierConfig::disabled();
                                    ss.turn_barrier =
                                        Some(sidequest_game::barrier::TurnBarrier::new(
                                            mp_session, config,
                                        ));
                                    {
                                        let _span = tracing::info_span!(
                                            "barrier.activated",
                                            player_count = pc,
                                        )
                                        .entered();
                                    }
                                    tracing::info!(
                                        player_count = pc,
                                        "Initialized turn barrier for multiplayer"
                                    );

                                    // Story 35-5: Spawn turn reminder task
                                    let reminder_config =
                                        sidequest_game::turn_reminder::ReminderConfig::default();
                                    let reminder_barrier =
                                        ss.turn_barrier.as_ref().unwrap().clone();
                                    tokio::spawn(async move {
                                        {
                                            let _span =
                                                tracing::info_span!("reminder_spawned").entered();
                                            tracing::info!("Turn reminder task spawned");
                                        }
                                        let result =
                                            reminder_barrier.run_reminder(&reminder_config).await;
                                        if result.should_send() {
                                            let _span = tracing::info_span!(
                                                "reminder_fired",
                                                idle_player_count = result.idle_players().len(),
                                            )
                                            .entered();
                                            tracing::info!(
                                                idle_player_count = result.idle_players().len(),
                                                "Turn reminder fired for idle players"
                                            );
                                        }
                                    });
                                }
                            }
                        }
                    }

                    // Story 8-8: Generate catch-up narration for mid-session joins.
                    // Done AFTER releasing shared session lock (Claude CLI call is slow).
                    let catch_up_messages =
                        if let Some((history, location, genre_voice)) = catch_up_context {
                            super::catch_up::generate_catch_up_messages(
                                state,
                                &character,
                                &history,
                                &location,
                                &genre_voice,
                                player_id,
                            )
                            .unwrap_or_default()
                        } else {
                            vec![]
                        };

                    // "ready" must come AFTER intro_messages.  The auto-turn
                    // ("I look around") sends its NARRATION inline via ctx.tx
                    // inside dispatch_player_action, so by the time we reach
                    // here the narration is already in the mpsc queue.  The
                    // client's "ready" handler clears the narration buffer, so
                    // if "ready" arrives before the narration flushes the
                    // opening text is wiped.  Placing "ready" last ensures all
                    // turn-1 messages are delivered first.
                    let mut msgs = vec![complete, backstory_narration, backstory_end];
                    // Catch-up narration slots in after backstory, before intro/ready.
                    // The joining player sees: backstory → "here's what's been happening" → opening scene.
                    msgs.extend(catch_up_messages);
                    msgs.extend(intro_messages);
                    // Story 37-27: corrected PARTY_STATUS with populated
                    // sheet facet, pushed after intro_messages (which
                    // contained a sheet=None PARTY_STATUS from compose_responses
                    // running before the player was in the shared session)
                    // and before ready so the Character tab is populated by
                    // the time the client processes SessionEvent::ready.
                    msgs.push(session_start_party_status);
                    msgs.push(ready);

                    // Barrier-aware reconnect signals: override the generic
                    // "ready" with the actual barrier state so the UI doesn't
                    // get stuck or miss narration from a resolved turn.
                    if let Some((resolved_narration, submitted)) = reconnect_barrier_state {
                        if let Some(narration) = resolved_narration {
                            // Barrier resolved while player was disconnected —
                            // replay the stored narration so they see it.
                            let narration_nbs =
                                sidequest_protocol::NonBlankString::new(&narration)
                                    .expect("reconnect resolved_narration is non-empty by barrier store contract");
                            msgs.push(GameMessage::Narration {
                                payload: NarrationPayload {
                                    text: narration_nbs,
                                    state_delta: None,
                                    footnotes: vec![],
                                },
                                player_id: player_id.to_string(),
                            });
                            msgs.push(GameMessage::NarrationEnd {
                                payload: NarrationEndPayload { state_delta: None },
                                player_id: player_id.to_string(),
                            });
                        } else if submitted {
                            // Player already submitted but barrier hasn't
                            // resolved yet — tell the UI to wait.
                            msgs.push(GameMessage::SessionEvent {
                                payload: SessionEventPayload {
                                    event: "waiting".to_string(),
                                    player_name: None,
                                    genre: None,
                                    world: None,
                                    has_character: None,
                                    initial_state: None,
                                    css: None,
                                    image_cooldown_seconds: None,
                                    narrator_verbosity: None,
                                    narrator_vocabulary: None,
                                },
                                player_id: player_id.to_string(),
                            });
                        }
                        // else: barrier active, player hasn't submitted —
                        // "ready" is correct, they can type to re-submit.
                    }

                    msgs
                }
                Err(e) => vec![error_response(
                    player_id,
                    &format!("Failed to build character: {:?}", e),
                )],
            }
        }
        _ => vec![error_response(
            player_id,
            &format!("Unexpected creation phase: {}", phase),
        )],
    }
}

/// Build an AUDIO_CUE with action "configure" carrying genre-pack mixer volumes.
/// Sent once on session connect so the frontend initializes per-genre channel levels.
fn mixer_config_cue(mixer: &sidequest_genre::MixerConfig, player_id: &str) -> GameMessage {
    GameMessage::AudioCue {
        payload: AudioCuePayload {
            mood: None,
            music_track: None,
            sfx_triggers: vec![],
            channel: None,
            action: Some("configure".to_string()),
            volume: None,
            music_volume: Some(mixer.music_volume as f32),
            sfx_volume: Some(mixer.sfx_volume as f32),
            voice_volume: Some(mixer.voice_volume as f32),
            crossfade_ms: Some(mixer.crossfade_default_ms),
        },
        player_id: player_id.to_string(),
    }
}

/// Resolve a persisted scrapbook `image_url` to its disk location for the
/// session-restore file-existence check (story 37-28).
///
/// - `/api/scrapbook/{genre}/{world}/{player}/{filename}` resolves under
///   `{save_dir}/scrapbook/{genre}/{world}/{player}/{filename}` — durable,
///   save-scoped storage.
/// - `/api/renders/{filename}` resolves under `SIDEQUEST_OUTPUT_DIR`
///   (fallback `~/.sidequest/renders`) — the volatile global pool used by
///   rows written before this story shipped.
///
/// Any other URL shape returns `None`; the caller treats that as an
/// `image_url_unresolvable` validation warning, not a silent pass.
fn resolve_scrapbook_image_path(
    url: &str,
    save_dir: &std::path::Path,
) -> Option<std::path::PathBuf> {
    if let Some(tail) = url.strip_prefix("/api/scrapbook/") {
        let mut path = save_dir.join("scrapbook");
        for segment in tail.split('/') {
            // Reviewer round-trip 1, finding 1: reject empty / `..` / `.`
            // segments. A stored image_url of `/api/scrapbook/../../.ssh/id_rsa`
            // must NOT escape save_dir even if the file happens to exist there.
            // Returning None routes to the caller's existing
            // `scrapbook.image_url_unresolvable` WatcherEvent.
            if segment.is_empty() || segment == ".." || segment == "." {
                return None;
            }
            path.push(segment);
        }
        return Some(path);
    }
    if let Some(filename) = url.strip_prefix("/api/renders/") {
        // Reviewer round-trip 1, finding 2: no silent `/tmp` fallback. When
        // HOME is unset AND SIDEQUEST_OUTPUT_DIR is unset, return None — the
        // caller already treats that as `scrapbook.image_url_unresolvable`.
        let renders_dir = match std::env::var("SIDEQUEST_OUTPUT_DIR") {
            Ok(dir) => std::path::PathBuf::from(dir),
            Err(_) => match std::env::var("HOME") {
                Ok(home) => std::path::PathBuf::from(home)
                    .join(".sidequest")
                    .join("renders"),
                Err(_) => return None,
            },
        };
        return Some(renders_dir.join(filename));
    }
    None
}
